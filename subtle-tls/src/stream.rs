//! TLS Stream implementation with AsyncRead/AsyncWrite
//!
//! Wraps an underlying stream with TLS encryption after handshake.

use crate::cert::CertificateVerifier;
use crate::crypto;
use crate::error::{Result, TlsError};
use crate::handshake::{
    self, HandshakeState, CONTENT_TYPE_ALERT, CONTENT_TYPE_APPLICATION_DATA,
    CONTENT_TYPE_CHANGE_CIPHER_SPEC, CONTENT_TYPE_HANDSHAKE, HANDSHAKE_CERTIFICATE,
    HANDSHAKE_CERTIFICATE_VERIFY, HANDSHAKE_ENCRYPTED_EXTENSIONS, HANDSHAKE_FINISHED,
    HANDSHAKE_SERVER_HELLO,
};
use crate::record::RecordLayer;
use crate::TlsConfig;
use futures::io::{AsyncRead, AsyncWrite};
use std::cell::RefCell;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use tracing::{debug, info, trace, warn};

/// TLS-encrypted stream
pub struct TlsStream<S> {
    /// Underlying transport stream
    inner: Rc<RefCell<S>>,
    /// Record layer for reading/writing TLS records
    record_layer: Rc<RefCell<RecordLayer>>,
    /// Buffered plaintext for reading
    read_buffer: Vec<u8>,
    /// Position in read buffer
    read_pos: usize,
    /// Whether the connection is established
    connected: bool,
    /// DER-encoded peer certificate (for CertifiedConn)
    peer_certificate: Option<Vec<u8>>,
    /// TLS keying material (for export_keying_material)
    keying_material: Option<KeyingMaterial>,
    /// Buffer for accumulating encrypted record data being read
    record_read_buffer: Vec<u8>,
    /// Buffer for pending write data (encrypted, ready to send to transport)
    record_write_buffer: Vec<u8>,
}

/// Stored keying material for RFC 5705 key export
struct KeyingMaterial {
    master_secret: Vec<u8>,
    client_random: Vec<u8>,
    server_random: Vec<u8>,
}

impl<S> TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Perform TLS handshake and return encrypted stream
    pub async fn connect(mut stream: S, server_name: &str, config: TlsConfig) -> Result<Self> {
        info!("Starting TLS 1.3 handshake with {}", server_name);

        let mut handshake = HandshakeState::new(server_name).await?;
        let mut record_layer = RecordLayer::new();

        // Step 1: Send ClientHello
        let client_hello = handshake.build_client_hello();
        handshake.update_transcript(&client_hello);
        record_layer
            .write_record(&mut stream, CONTENT_TYPE_HANDSHAKE, &client_hello)
            .await?;
        debug!("Sent ClientHello");

        // Step 2: Receive ServerHello
        info!("Waiting for ServerHello...");
        let server_hello_data = Self::read_server_hello(&mut stream, &mut record_layer).await?;
        info!("Got ServerHello: {} bytes", server_hello_data.len());

        // Add raw record to transcript (including handshake header)
        let mut server_hello_msg = vec![HANDSHAKE_SERVER_HELLO];
        let len = server_hello_data.len();
        server_hello_msg.push((len >> 16) as u8);
        server_hello_msg.push((len >> 8) as u8);
        server_hello_msg.push(len as u8);
        server_hello_msg.extend_from_slice(&server_hello_data);
        handshake.update_transcript(&server_hello_msg);

        let server_key_share = handshake.parse_server_hello(&server_hello_data)?;
        info!(
            "Parsed ServerHello, cipher suite: 0x{:04x}",
            handshake.cipher_suite
        );

        // Set cipher suite on record layer
        record_layer.set_cipher_suite(handshake.cipher_suite);

        // Step 3: Derive handshake keys
        info!("Deriving handshake keys...");
        handshake.derive_handshake_keys(&server_key_share).await?;
        info!("Handshake keys derived");

        // Activate server->client encryption
        let (read_key, read_iv) = handshake.get_handshake_keys(false).await?;
        record_layer.set_read_cipher(&read_key, &read_iv).await?;
        info!("Read cipher activated");

        // Step 4: Receive encrypted handshake messages
        // (EncryptedExtensions, Certificate, CertificateVerify, Finished)
        info!("Processing encrypted handshake messages...");
        let peer_certificate = match Self::process_encrypted_handshake(
            &mut stream,
            &mut record_layer,
            &mut handshake,
            &config,
        )
        .await
        {
            Ok(cert) => {
                info!("process_encrypted_handshake returned Ok");
                cert
            }
            Err(e) => {
                tracing::error!("process_encrypted_handshake returned Err: {}", e);
                return Err(e);
            }
        };
        info!("Encrypted handshake complete");

        // Step 5: Derive application keys
        handshake.derive_application_keys().await?;

        // Step 6: Activate client->server encryption and send Finished
        let (write_key, write_iv) = handshake.get_handshake_keys(true).await?;
        record_layer.set_write_cipher(&write_key, &write_iv).await?;

        let client_finished = handshake.build_client_finished().await?;
        handshake.update_transcript(&client_finished);
        record_layer
            .write_record(&mut stream, CONTENT_TYPE_HANDSHAKE, &client_finished)
            .await?;
        debug!("Sent client Finished");

        // Step 7: Switch to application keys
        let (app_read_key, app_read_iv) = handshake.get_application_keys(false).await?;
        let (app_write_key, app_write_iv) = handshake.get_application_keys(true).await?;
        record_layer
            .set_read_cipher(&app_read_key, &app_read_iv)
            .await?;
        record_layer
            .set_write_cipher(&app_write_key, &app_write_iv)
            .await?;

        info!("TLS 1.3 handshake completed with {}", server_name);

        Ok(Self {
            inner: Rc::new(RefCell::new(stream)),
            record_layer: Rc::new(RefCell::new(record_layer)),
            read_buffer: Vec::new(),
            read_pos: 0,
            connected: true,
            peer_certificate,
            keying_material: None, // TODO: implement keying material export
            record_read_buffer: Vec::new(),
            record_write_buffer: Vec::new(),
        })
    }

    /// Get the peer certificate (DER-encoded)
    pub fn peer_certificate(&self) -> Option<&[u8]> {
        self.peer_certificate.as_deref()
    }

    async fn read_server_hello(stream: &mut S, record_layer: &mut RecordLayer) -> Result<Vec<u8>> {
        loop {
            let (content_type, data) = record_layer.read_record(stream).await?;

            match content_type {
                CONTENT_TYPE_HANDSHAKE => {
                    // Parse handshake message
                    if data.len() < 4 {
                        return Err(TlsError::handshake("Handshake message too short"));
                    }
                    let msg_type = data[0];
                    let length =
                        ((data[1] as usize) << 16) | ((data[2] as usize) << 8) | (data[3] as usize);

                    if msg_type == HANDSHAKE_SERVER_HELLO {
                        let total_len = 4 + length;
                        if data.len() < total_len {
                            return Err(TlsError::handshake(format!(
                                "ServerHello message truncated: expected {} bytes, got {}",
                                total_len,
                                data.len()
                            )));
                        }
                        return Ok(data[4..total_len].to_vec());
                    } else {
                        return Err(TlsError::UnexpectedMessage {
                            expected: "ServerHello".to_string(),
                            got: format!("HandshakeType {}", msg_type),
                        });
                    }
                }
                CONTENT_TYPE_ALERT => {
                    return Err(Self::parse_alert(&data));
                }
                CONTENT_TYPE_CHANGE_CIPHER_SPEC => {
                    // Ignore CCS for TLS 1.3 compatibility
                    debug!("Ignoring ChangeCipherSpec");
                    continue;
                }
                _ => {
                    return Err(TlsError::UnexpectedMessage {
                        expected: "Handshake".to_string(),
                        got: format!("ContentType {}", content_type),
                    });
                }
            }
        }
    }

    /// Process encrypted handshake messages and return the peer certificate (if any)
    async fn process_encrypted_handshake(
        stream: &mut S,
        record_layer: &mut RecordLayer,
        handshake: &mut HandshakeState,
        config: &TlsConfig,
    ) -> Result<Option<Vec<u8>>> {
        let mut got_encrypted_extensions = false;
        let mut got_certificate = false;
        let mut got_certificate_verify = false;
        let mut got_finished = false;

        // Store certificate chain and verify data for validation
        let mut cert_chain: Vec<Vec<u8>> = Vec::new();
        let mut cert_verify_algorithm: u16 = 0;
        let mut cert_verify_signature: Vec<u8> = Vec::new();
        // Transcript hash at the point of CertificateVerify (before the message itself)
        let mut transcript_before_cert_verify: Vec<u8> = Vec::new();

        // Buffer for accumulating fragmented handshake messages
        let mut handshake_buffer: Vec<u8> = Vec::new();

        while !got_finished {
            info!("process_encrypted_handshake: reading next record...");
            let (content_type, data) = record_layer.read_record(stream).await?;
            info!(
                "process_encrypted_handshake: got record type={}, len={}",
                content_type,
                data.len()
            );

            match content_type {
                20 => {
                    // CONTENT_TYPE_CHANGE_CIPHER_SPEC
                    debug!("Ignoring ChangeCipherSpec");
                    continue;
                }
                21 => {
                    // CONTENT_TYPE_ALERT
                    return Err(Self::parse_alert(&data));
                }
                22 => {
                    // CONTENT_TYPE_HANDSHAKE
                    // Accumulate handshake data (may be fragmented across records)
                    info!(
                        "Adding {} bytes to handshake buffer (was {} bytes)",
                        data.len(),
                        handshake_buffer.len()
                    );
                    handshake_buffer.extend_from_slice(&data);
                    info!(
                        "Handshake buffer now {} bytes, first 4 bytes: {:02x?}",
                        handshake_buffer.len(),
                        &handshake_buffer[..4.min(handshake_buffer.len())]
                    );

                    // Parse complete handshake messages from the buffer
                    while handshake_buffer.len() >= 4 {
                        let msg_type = handshake_buffer[0];
                        let length = ((handshake_buffer[1] as usize) << 16)
                            | ((handshake_buffer[2] as usize) << 8)
                            | (handshake_buffer[3] as usize);

                        let total_len = 4 + length;
                        trace!(
                            "Parsing handshake: type={}, length={}, buffer_len={}",
                            msg_type,
                            length,
                            handshake_buffer.len()
                        );

                        if handshake_buffer.len() < total_len {
                            // Need more data - wait for next record
                            debug!(
                                "Handshake message fragmented: have {} bytes, need {}",
                                handshake_buffer.len(),
                                total_len
                            );
                            break;
                        }

                        // Extract complete message
                        let msg_data: Vec<u8> = handshake_buffer.drain(..total_len).collect();
                        let msg_body = &msg_data[4..];

                        match msg_type {
                            HANDSHAKE_ENCRYPTED_EXTENSIONS => {
                                debug!("Received EncryptedExtensions");
                                handshake.update_transcript(&msg_data);
                                got_encrypted_extensions = true;
                            }
                            HANDSHAKE_CERTIFICATE => {
                                debug!("Received Certificate ({} bytes)", msg_body.len());
                                handshake.update_transcript(&msg_data);
                                cert_chain = handshake::parse_certificate(msg_body)?;
                                got_certificate = true;
                            }
                            HANDSHAKE_CERTIFICATE_VERIFY => {
                                debug!("Received CertificateVerify");
                                // Save transcript hash before adding CertificateVerify
                                transcript_before_cert_verify =
                                    crypto::sha256(&handshake.transcript).await?;
                                handshake.update_transcript(&msg_data);
                                let (algorithm, signature) =
                                    handshake::parse_certificate_verify(msg_body)?;
                                cert_verify_algorithm = algorithm;
                                cert_verify_signature = signature;
                                got_certificate_verify = true;
                            }
                            HANDSHAKE_FINISHED => {
                                info!("Received server Finished ({} bytes)", msg_body.len());
                                let verify_data = handshake::parse_finished(msg_body)?;
                                info!("Verifying server Finished...");
                                handshake.verify_server_finished(&verify_data).await?;
                                info!("Server Finished verified OK");
                                handshake.update_transcript(&msg_data);
                                got_finished = true;
                                info!("got_finished = true, breaking out of loop");
                            }
                            _ => {
                                warn!("Ignoring unknown handshake message type: {}", msg_type);
                                handshake.update_transcript(&msg_data);
                            }
                        }
                    }
                }
                _ => {
                    return Err(TlsError::UnexpectedMessage {
                        expected: "Handshake".to_string(),
                        got: format!("ContentType {}", content_type),
                    });
                }
            }
        }

        if !got_encrypted_extensions {
            return Err(TlsError::handshake("Missing EncryptedExtensions"));
        }

        // Validate certificates if not skipping verification
        if !config.skip_verification {
            if !got_certificate {
                return Err(TlsError::handshake("Missing Certificate message"));
            }
            if !got_certificate_verify {
                return Err(TlsError::handshake("Missing CertificateVerify message"));
            }

            // Verify certificate chain
            let verifier = CertificateVerifier::new(&handshake.server_name, false);
            verifier.verify_chain(&cert_chain).await?;

            // Verify CertificateVerify signature
            // We need the server's public key from the leaf certificate
            if let Some(leaf_der) = cert_chain.first() {
                // Extract SubjectPublicKeyInfo from the certificate
                let server_public_key = extract_public_key_spki(leaf_der)?;

                crate::cert::verify_certificate_verify(
                    cert_verify_algorithm,
                    &cert_verify_signature,
                    &transcript_before_cert_verify,
                    &server_public_key,
                )
                .await?;
            }
        }

        debug!("Encrypted handshake phase completed");
        // Return the leaf certificate (first in chain)
        Ok(cert_chain.into_iter().next())
    }

    fn parse_alert(data: &[u8]) -> TlsError {
        if data.len() >= 2 {
            let level = data[0];
            let description = data[1];
            let level_str = if level == 1 { "warning" } else { "fatal" };
            let desc_str = match description {
                0 => "close_notify",
                10 => "unexpected_message",
                20 => "bad_record_mac",
                40 => "handshake_failure",
                42 => "bad_certificate",
                43 => "unsupported_certificate",
                44 => "certificate_revoked",
                45 => "certificate_expired",
                46 => "certificate_unknown",
                47 => "illegal_parameter",
                48 => "unknown_ca",
                49 => "access_denied",
                50 => "decode_error",
                51 => "decrypt_error",
                70 => "protocol_version",
                71 => "insufficient_security",
                80 => "internal_error",
                86 => "inappropriate_fallback",
                90 => "user_canceled",
                109 => "missing_extension",
                110 => "unsupported_extension",
                112 => "unrecognized_name",
                113 => "bad_certificate_status_response",
                115 => "unknown_psk_identity",
                116 => "certificate_required",
                120 => "no_application_protocol",
                _ => "unknown",
            };
            TlsError::Alert(format!(
                "{} alert: {} ({})",
                level_str, desc_str, description
            ))
        } else {
            TlsError::Alert("Unknown alert".to_string())
        }
    }

    /// Read application data (blocking)
    pub async fn read_app_data(&mut self) -> Result<Vec<u8>> {
        let mut inner = self.inner.borrow_mut();
        let mut record_layer = self.record_layer.borrow_mut();

        loop {
            let (content_type, data) = record_layer.read_record(&mut *inner).await?;

            match content_type {
                CONTENT_TYPE_APPLICATION_DATA => {
                    return Ok(data);
                }
                CONTENT_TYPE_ALERT => {
                    if data.len() >= 2 && data[0] == 1 && data[1] == 0 {
                        // close_notify
                        return Err(TlsError::ConnectionClosed);
                    }
                    return Err(Self::parse_alert(&data));
                }
                CONTENT_TYPE_HANDSHAKE => {
                    // Post-handshake messages (e.g., NewSessionTicket)
                    trace!("Ignoring post-handshake message");
                    continue;
                }
                _ => {
                    warn!("Ignoring unexpected content type: {}", content_type);
                    continue;
                }
            }
        }
    }

    /// Write application data (blocking)
    pub async fn write_app_data(&mut self, data: &[u8]) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        let mut record_layer = self.record_layer.borrow_mut();
        record_layer
            .write_record(&mut *inner, CONTENT_TYPE_APPLICATION_DATA, data)
            .await
    }

    /// Close the TLS connection gracefully
    pub async fn close(&mut self) -> Result<()> {
        // Send close_notify alert
        let alert = [1, 0]; // warning, close_notify
        let mut inner = self.inner.borrow_mut();
        let mut record_layer = self.record_layer.borrow_mut();
        record_layer
            .write_record(&mut *inner, CONTENT_TYPE_ALERT, &alert)
            .await?;
        self.connected = false;
        Ok(())
    }
}

/// Maximum TLS record size
const MAX_RECORD_SIZE: usize = 16384 + 256;

// AsyncRead implementation - properly polls underlying stream
impl<S> AsyncRead for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin + 'static,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        // First, drain any buffered plaintext
        if self.read_pos < self.read_buffer.len() {
            let available = &self.read_buffer[self.read_pos..];
            let to_copy = std::cmp::min(buf.len(), available.len());
            buf[..to_copy].copy_from_slice(&available[..to_copy]);
            self.read_pos += to_copy;

            if self.read_pos >= self.read_buffer.len() {
                self.read_buffer.clear();
                self.read_pos = 0;
            }

            return Poll::Ready(Ok(to_copy));
        }

        // Try to read and decrypt a TLS record
        loop {
            // Check if we have enough data for a record header
            if self.record_read_buffer.len() >= 5 {
                let content_type = self.record_read_buffer[0];
                let length = ((self.record_read_buffer[3] as usize) << 8)
                    | (self.record_read_buffer[4] as usize);

                if length > MAX_RECORD_SIZE {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("TLS record too large: {}", length),
                    )));
                }

                let total_len = 5 + length;

                // Check if we have the full record
                if self.record_read_buffer.len() >= total_len {
                    // Extract the record
                    let record_data: Vec<u8> = self.record_read_buffer.drain(..total_len).collect();
                    let body = &record_data[5..];

                    // Decrypt if cipher is active
                    let mut record_layer = self.record_layer.borrow_mut();

                    let (actual_type, plaintext) = if record_layer.has_read_cipher()
                        && content_type == CONTENT_TYPE_APPLICATION_DATA
                    {
                        // Build header for AAD
                        let header = [
                            record_data[0],
                            record_data[1],
                            record_data[2],
                            record_data[3],
                            record_data[4],
                        ];

                        // Decrypt synchronously using the record layer's cipher
                        match record_layer.decrypt_record_sync(&header, body) {
                            Ok((ct, pt)) => (ct, pt),
                            Err(e) => {
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("Decryption failed: {}", e),
                                )));
                            }
                        }
                    } else {
                        (content_type, body.to_vec())
                    };

                    drop(record_layer);

                    match actual_type {
                        CONTENT_TYPE_APPLICATION_DATA => {
                            // Copy to output buffer
                            let to_copy = std::cmp::min(buf.len(), plaintext.len());
                            buf[..to_copy].copy_from_slice(&plaintext[..to_copy]);

                            // Buffer any remaining
                            if to_copy < plaintext.len() {
                                self.read_buffer = plaintext[to_copy..].to_vec();
                                self.read_pos = 0;
                            }

                            return Poll::Ready(Ok(to_copy));
                        }
                        CONTENT_TYPE_ALERT => {
                            if plaintext.len() >= 2 && plaintext[0] == 1 && plaintext[1] == 0 {
                                // close_notify - signal EOF
                                return Poll::Ready(Ok(0));
                            }
                            let err = Self::parse_alert(&plaintext);
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::ConnectionReset,
                                err.to_string(),
                            )));
                        }
                        CONTENT_TYPE_HANDSHAKE => {
                            // Post-handshake message (e.g., NewSessionTicket), skip it
                            trace!("Ignoring post-handshake message");
                            continue;
                        }
                        CONTENT_TYPE_CHANGE_CIPHER_SPEC => {
                            // Ignore CCS
                            continue;
                        }
                        _ => {
                            warn!("Ignoring unexpected content type: {}", actual_type);
                            continue;
                        }
                    }
                }
            }

            // Need more data from the underlying transport
            let mut temp = [0u8; 4096];
            let mut inner = self.inner.borrow_mut();

            match Pin::new(&mut *inner).poll_read(cx, &mut temp) {
                Poll::Ready(Ok(0)) => {
                    // EOF from underlying stream
                    return Poll::Ready(Ok(0));
                }
                Poll::Ready(Ok(n)) => {
                    drop(inner);
                    trace!("TlsStream poll_read: got {} bytes from transport", n);
                    self.record_read_buffer.extend_from_slice(&temp[..n]);
                    // Continue loop to try parsing
                }
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Err(e));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

impl<S> AsyncWrite for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin + 'static,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // First, flush any pending write buffer
        while !self.record_write_buffer.is_empty() {
            let mut inner = self.inner.borrow_mut();
            match Pin::new(&mut *inner).poll_write(cx, &self.record_write_buffer) {
                Poll::Ready(Ok(n)) => {
                    drop(inner);
                    self.record_write_buffer.drain(..n);
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        // Encrypt the new data and write it
        let mut record_layer = self.record_layer.borrow_mut();

        // Encrypt using the record layer
        let encrypted = match record_layer.encrypt_record_sync(CONTENT_TYPE_APPLICATION_DATA, buf) {
            Ok(data) => data,
            Err(e) => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Encryption failed: {}", e),
                )));
            }
        };

        drop(record_layer);

        // Try to write the encrypted record
        let mut inner = self.inner.borrow_mut();
        match Pin::new(&mut *inner).poll_write(cx, &encrypted) {
            Poll::Ready(Ok(n)) => {
                if n < encrypted.len() {
                    // Partial write - buffer the rest
                    drop(inner);
                    self.record_write_buffer.extend_from_slice(&encrypted[n..]);
                }
                // Report how many plaintext bytes were written
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => {
                // Buffer all for later
                drop(inner);
                self.record_write_buffer = encrypted;
                Poll::Pending
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Flush pending write buffer
        while !self.record_write_buffer.is_empty() {
            let mut inner = self.inner.borrow_mut();
            match Pin::new(&mut *inner).poll_write(cx, &self.record_write_buffer) {
                Poll::Ready(Ok(n)) => {
                    drop(inner);
                    self.record_write_buffer.drain(..n);
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        // Flush underlying stream
        let mut inner = self.inner.borrow_mut();
        Pin::new(&mut *inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // First flush pending data
        match self.as_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }

        self.connected = false;
        let mut inner = self.inner.borrow_mut();
        Pin::new(&mut *inner).poll_close(cx)
    }
}

/// Wrapper that provides blocking-style async methods
/// This is the recommended way to use TlsStream in WASM
impl<S> TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Read data using async/await (recommended for WASM)
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
        match self.read_app_data().await {
            Ok(data) => {
                let to_copy = std::cmp::min(buf.len(), data.len());
                buf[..to_copy].copy_from_slice(&data[..to_copy]);

                if to_copy < data.len() {
                    self.read_buffer = data[to_copy..].to_vec();
                    self.read_pos = 0;
                }

                Ok(to_copy)
            }
            Err(TlsError::ConnectionClosed) => Ok(0),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    }

    /// Write data using async/await (recommended for WASM)
    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.write_app_data(buf).await {
            Ok(()) => Ok(buf.len()),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    }

    /// Flush the stream
    pub async fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.borrow_mut();
        use futures::io::AsyncWriteExt;
        inner.flush().await
    }
}

/// Extract SubjectPublicKeyInfo (SPKI) from a DER-encoded X.509 certificate
fn extract_public_key_spki(cert_der: &[u8]) -> Result<Vec<u8>> {
    use x509_parser::prelude::*;

    let (_, cert) = X509Certificate::from_der(cert_der)
        .map_err(|e| TlsError::certificate(format!("Failed to parse certificate: {}", e)))?;

    // The raw field contains the DER-encoded SubjectPublicKeyInfo
    Ok(cert.public_key().raw.to_vec())
}

// Implement tor_rtcompat traits for TlsStream

impl<S> tor_rtcompat::StreamOps for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // Use default implementation
}

impl<S> tor_rtcompat::CertifiedConn for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn peer_certificate(&self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.peer_certificate.clone())
    }

    fn export_keying_material(
        &self,
        len: usize,
        _label: &[u8],
        _context: Option<&[u8]>,
    ) -> io::Result<Vec<u8>> {
        // TLS 1.3 keying material export is complex and requires storing
        // the master secret. For now, return zeros as a placeholder.
        // TODO: Implement proper RFC 5705 key export
        warn!("export_keying_material called but not fully implemented");
        Ok(vec![0u8; len])
    }
}
