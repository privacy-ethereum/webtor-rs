//! TLS 1.2 Stream implementation with AsyncRead/AsyncWrite
//!
//! Wraps an underlying stream with TLS 1.2 encryption after handshake.

use crate::cert::CertificateVerifier;
use crate::error::{Result, TlsError};
use crate::handshake_1_2::{
    self, Handshake12State, CONTENT_TYPE_ALERT, CONTENT_TYPE_APPLICATION_DATA,
    CONTENT_TYPE_CHANGE_CIPHER_SPEC, CONTENT_TYPE_HANDSHAKE, HANDSHAKE_CERTIFICATE,
    HANDSHAKE_FINISHED, HANDSHAKE_SERVER_HELLO, HANDSHAKE_SERVER_HELLO_DONE,
    HANDSHAKE_SERVER_KEY_EXCHANGE, TLS_VERSION_1_2,
};
use crate::record_1_2::RecordLayer12;
use crate::TlsConfig;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::io;
use tracing::{debug, info, trace};

/// TLS 1.2 encrypted stream
pub struct TlsStream12<S> {
    /// Underlying transport stream
    inner: S,
    /// Record layer for reading/writing TLS records
    record_layer: RecordLayer12,
    /// Buffered plaintext for reading
    read_buffer: Vec<u8>,
    /// Position in read buffer
    read_pos: usize,
    /// Whether the connection is established
    connected: bool,
    /// DER-encoded peer certificate
    peer_certificate: Option<Vec<u8>>,
}

impl<S> TlsStream12<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Perform TLS 1.2 handshake and return encrypted stream
    pub async fn connect(mut stream: S, server_name: &str, config: TlsConfig) -> Result<Self> {
        info!("Starting TLS 1.2 handshake with {}", server_name);

        let mut handshake = Handshake12State::new(server_name).await?;
        let mut record_layer = RecordLayer12::new();

        // Step 1: Send ClientHello
        let client_hello = handshake.build_client_hello();
        handshake.update_transcript(&client_hello);
        Self::write_record(&mut stream, CONTENT_TYPE_HANDSHAKE, &client_hello).await?;
        debug!("Sent TLS 1.2 ClientHello");

        // Step 2: Receive ServerHello, Certificate, ServerKeyExchange, ServerHelloDone
        let peer_certificate =
            Self::process_server_messages(&mut stream, &mut handshake, &config).await?;

        // Step 3: Generate ECDH key pair and compute shared secret
        handshake.compute_key_exchange().await?;

        // Step 4: Derive keys
        handshake.derive_keys().await?;

        // Set up record layer cipher suite
        record_layer.set_cipher_suite(handshake.cipher_suite)?;

        // Step 5: Send ClientKeyExchange
        let client_key_exchange = handshake.build_client_key_exchange()?;
        handshake.update_transcript(&client_key_exchange);
        Self::write_record(&mut stream, CONTENT_TYPE_HANDSHAKE, &client_key_exchange).await?;
        debug!("Sent ClientKeyExchange");

        // Step 6: Send ChangeCipherSpec
        Self::write_record(&mut stream, CONTENT_TYPE_CHANGE_CIPHER_SPEC, &[1]).await?;
        debug!("Sent ChangeCipherSpec");

        // Extract key material (clone to avoid borrow issues)
        let km = handshake
            .key_material
            .clone()
            .ok_or_else(|| TlsError::handshake("No key material"))?;

        // Activate write cipher
        record_layer
            .set_write_cipher(
                &km.client_write_key,
                &km.client_write_iv,
                &km.client_write_mac_key,
            )
            .await?;

        // Step 7: Send Finished (encrypted)
        let finished = handshake.build_finished().await?;
        handshake.update_transcript(&finished);
        record_layer
            .write_record(&mut stream, CONTENT_TYPE_HANDSHAKE, &finished)
            .await?;
        debug!("Sent TLS 1.2 Finished");

        // Step 8: Receive ChangeCipherSpec
        let (content_type, data) = Self::read_record_unencrypted(&mut stream).await?;
        match content_type {
            CONTENT_TYPE_CHANGE_CIPHER_SPEC => {
                debug!("Received ChangeCipherSpec");
            }
            CONTENT_TYPE_ALERT => {
                return Err(Self::parse_alert(&data));
            }
            _ => {
                return Err(TlsError::UnexpectedMessage {
                    expected: "ChangeCipherSpec".to_string(),
                    got: format!("ContentType {}", content_type),
                });
            }
        }

        // Activate read cipher
        record_layer
            .set_read_cipher(
                &km.server_write_key,
                &km.server_write_iv,
                &km.server_write_mac_key,
            )
            .await?;

        // Step 9: Receive Finished (encrypted)
        let (content_type, data) = record_layer.read_record(&mut stream).await?;
        if content_type != CONTENT_TYPE_HANDSHAKE {
            return Err(TlsError::UnexpectedMessage {
                expected: "Finished".to_string(),
                got: format!("ContentType {}", content_type),
            });
        }

        // Parse Finished message
        if data.len() < 4 {
            return Err(TlsError::handshake("Finished message too short"));
        }
        let msg_type = data[0];
        let length = ((data[1] as usize) << 16) | ((data[2] as usize) << 8) | (data[3] as usize);
        if msg_type != HANDSHAKE_FINISHED {
            return Err(TlsError::handshake(format!(
                "Expected Finished, got type {}",
                msg_type
            )));
        }
        let verify_data = &data[4..4 + length];

        // Verify server Finished
        handshake.verify_server_finished(verify_data).await?;
        info!("TLS 1.2 handshake completed with {}", server_name);

        Ok(Self {
            inner: stream,
            record_layer,
            read_buffer: Vec::new(),
            read_pos: 0,
            connected: true,
            peer_certificate,
        })
    }

    /// Process server messages (ServerHello, Certificate, ServerKeyExchange, ServerHelloDone)
    async fn process_server_messages(
        stream: &mut S,
        handshake: &mut Handshake12State,
        config: &TlsConfig,
    ) -> Result<Option<Vec<u8>>> {
        let mut peer_certificate = None;
        let mut cert_chain: Vec<Vec<u8>> = Vec::new();
        let mut _got_server_hello = false;
        let mut _got_certificate = false;
        let mut _got_server_key_exchange = false;
        let mut got_server_hello_done = false;

        while !got_server_hello_done {
            let (content_type, data) = Self::read_record_unencrypted(stream).await?;

            match content_type {
                CONTENT_TYPE_HANDSHAKE => {
                    // Parse multiple handshake messages from one record
                    let mut pos = 0;
                    while pos + 4 <= data.len() {
                        let msg_type = data[pos];
                        let length = ((data[pos + 1] as usize) << 16)
                            | ((data[pos + 2] as usize) << 8)
                            | (data[pos + 3] as usize);

                        let msg_end = pos + 4 + length;
                        if msg_end > data.len() {
                            return Err(TlsError::handshake("Handshake message truncated"));
                        }

                        let msg_data = &data[pos + 4..msg_end];
                        let full_msg = &data[pos..msg_end];

                        match msg_type {
                            HANDSHAKE_SERVER_HELLO => {
                                handshake.parse_server_hello(msg_data)?;
                                handshake.update_transcript(full_msg);
                                _got_server_hello = true;
                                debug!("Received ServerHello");
                            }
                            HANDSHAKE_CERTIFICATE => {
                                cert_chain = handshake_1_2::parse_certificate(msg_data)?;
                                if !cert_chain.is_empty() {
                                    peer_certificate = Some(cert_chain[0].clone());
                                }
                                handshake.update_transcript(full_msg);
                                _got_certificate = true;
                                debug!("Received Certificate ({} certs)", cert_chain.len());
                            }
                            HANDSHAKE_SERVER_KEY_EXCHANGE => {
                                handshake.parse_server_key_exchange(msg_data)?;
                                handshake.update_transcript(full_msg);
                                _got_server_key_exchange = true;
                                debug!("Received ServerKeyExchange");
                            }
                            HANDSHAKE_SERVER_HELLO_DONE => {
                                handshake.update_transcript(full_msg);
                                got_server_hello_done = true;
                                debug!("Received ServerHelloDone");
                            }
                            _ => {
                                debug!("Ignoring handshake message type {}", msg_type);
                                handshake.update_transcript(full_msg);
                            }
                        }

                        pos = msg_end;
                    }
                }
                CONTENT_TYPE_ALERT => {
                    return Err(Self::parse_alert(&data));
                }
                _ => {
                    return Err(TlsError::UnexpectedMessage {
                        expected: "Handshake".to_string(),
                        got: format!("ContentType {}", content_type),
                    });
                }
            }
        }

        // Verify certificate chain
        if !config.skip_verification && !cert_chain.is_empty() {
            let verifier = CertificateVerifier::new(&handshake.server_name, false);
            verifier.verify_chain(&cert_chain).await?;
            debug!("Certificate chain verified");
        }

        Ok(peer_certificate)
    }

    /// Read a TLS record (unencrypted, for handshake)
    async fn read_record_unencrypted(stream: &mut S) -> Result<(u8, Vec<u8>)> {
        let mut header = [0u8; 5];
        stream.read_exact(&mut header).await.map_err(TlsError::Io)?;

        let content_type = header[0];
        let length = ((header[3] as usize) << 8) | (header[4] as usize);

        let mut body = vec![0u8; length];
        stream.read_exact(&mut body).await.map_err(TlsError::Io)?;

        Ok((content_type, body))
    }

    /// Write a TLS record (unencrypted, for handshake)
    async fn write_record(stream: &mut S, content_type: u8, data: &[u8]) -> Result<()> {
        let mut record = Vec::with_capacity(5 + data.len());
        record.push(content_type);
        record.push((TLS_VERSION_1_2 >> 8) as u8);
        record.push(TLS_VERSION_1_2 as u8);
        record.push((data.len() >> 8) as u8);
        record.push(data.len() as u8);
        record.extend_from_slice(data);

        stream.write_all(&record).await.map_err(TlsError::Io)?;
        stream.flush().await.map_err(TlsError::Io)?;

        Ok(())
    }

    fn parse_alert(data: &[u8]) -> TlsError {
        if data.len() >= 2 {
            let level = data[0];
            let description = data[1];
            TlsError::alert(format!("level={}, description={}", level, description))
        } else {
            TlsError::alert("Unknown TLS alert")
        }
    }

    /// Get the peer certificate (DER-encoded)
    pub fn peer_certificate(&self) -> Option<&[u8]> {
        self.peer_certificate.as_deref()
    }

    /// Read application data
    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // First drain buffer
        if self.read_pos < self.read_buffer.len() {
            let available = &self.read_buffer[self.read_pos..];
            let to_copy = std::cmp::min(buf.len(), available.len());
            buf[..to_copy].copy_from_slice(&available[..to_copy]);
            self.read_pos += to_copy;

            if self.read_pos >= self.read_buffer.len() {
                self.read_buffer.clear();
                self.read_pos = 0;
            }

            return Ok(to_copy);
        }

        // Read more data
        loop {
            let (content_type, data) = match self.record_layer.read_record(&mut self.inner).await {
                Ok(r) => r,
                Err(TlsError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(0);
                }
                Err(e) => {
                    return Err(io::Error::new(io::ErrorKind::Other, e.to_string()));
                }
            };

            match content_type {
                CONTENT_TYPE_APPLICATION_DATA => {
                    let to_copy = std::cmp::min(buf.len(), data.len());
                    buf[..to_copy].copy_from_slice(&data[..to_copy]);

                    if to_copy < data.len() {
                        self.read_buffer = data[to_copy..].to_vec();
                        self.read_pos = 0;
                    }

                    return Ok(to_copy);
                }
                CONTENT_TYPE_ALERT => {
                    if data.len() >= 2 && data[0] == 1 && data[1] == 0 {
                        // close_notify
                        return Ok(0);
                    }
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        Self::parse_alert(&data).to_string(),
                    ));
                }
                _ => {
                    trace!("Ignoring content type {}", content_type);
                    continue;
                }
            }
        }
    }

    /// Write application data
    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.record_layer
            .write_record(&mut self.inner, CONTENT_TYPE_APPLICATION_DATA, buf)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        Ok(buf.len())
    }

    /// Flush the stream
    pub async fn flush(&mut self) -> io::Result<()> {
        self.inner.flush().await
    }
}

// Implement tor_rtcompat traits
impl<S> tor_rtcompat::StreamOps for TlsStream12<S> where S: AsyncRead + AsyncWrite + Unpin {}

impl<S> tor_rtcompat::CertifiedConn for TlsStream12<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn peer_certificate(&self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.peer_certificate.clone())
    }

    fn export_keying_material(
        &self,
        _len: usize,
        _label: &[u8],
        _context: Option<&[u8]>,
    ) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "export_keying_material not implemented for TLS 1.2",
        ))
    }
}
