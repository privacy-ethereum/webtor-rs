//! TLS 1.2 Record Layer
//!
//! Handles reading and writing TLS 1.2 records with encryption/decryption.
//! Supports both AEAD (GCM) and non-AEAD (CBC+HMAC) cipher suites.

use crate::crypto::{AesCbc, Cipher};
use crate::error::{Result, TlsError};
use crate::handshake_1_2::{
    CipherSuiteParams, TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA, TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
    TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA, TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384,
    TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384, TLS_VERSION_1_2,
};
use crate::prf;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, trace};

/// Maximum TLS record size
pub const MAX_RECORD_SIZE: usize = 16384 + 2048; // 16KB + overhead for MAC/padding
/// Maximum plaintext size per record
pub const MAX_PLAINTEXT_SIZE: usize = 16384;

/// TLS 1.2 record layer with encryption state
pub struct RecordLayer12 {
    /// Read cipher state
    read_cipher: Option<CipherState>,
    /// Write cipher state
    write_cipher: Option<CipherState>,
    /// Cipher suite parameters
    params: Option<CipherSuiteParams>,
    /// Cipher suite code
    cipher_suite: u16,
}

/// MAC algorithm for CBC cipher suites
#[derive(Clone, Copy)]
enum MacAlgorithm {
    Sha1,
    Sha256,
    Sha384,
}

/// Encryption state for one direction
enum CipherState {
    /// AEAD cipher (GCM)
    Aead {
        cipher: Cipher,
        implicit_iv: Vec<u8>, // 4 bytes for TLS 1.2 GCM
        sequence: u64,
    },
    /// CBC cipher with HMAC
    Cbc {
        cipher: AesCbc,
        mac_key: Vec<u8>,
        sequence: u64,
        mac_len: usize,
        mac_alg: MacAlgorithm,
    },
}

impl RecordLayer12 {
    /// Create a new TLS 1.2 record layer
    pub fn new() -> Self {
        Self {
            read_cipher: None,
            write_cipher: None,
            params: None,
            cipher_suite: 0,
        }
    }

    /// Set the cipher suite
    pub fn set_cipher_suite(&mut self, suite: u16) -> Result<()> {
        self.cipher_suite = suite;
        self.params = Some(CipherSuiteParams::for_suite(suite)?);
        debug!("TLS 1.2 record layer using cipher suite: 0x{:04x}", suite);
        Ok(())
    }

    /// Activate read cipher (server -> client)
    pub async fn set_read_cipher(&mut self, key: &[u8], iv: &[u8], mac_key: &[u8]) -> Result<()> {
        let params = self
            .params
            .as_ref()
            .ok_or_else(|| TlsError::protocol("Cipher suite not set"))?;

        let cipher_state = if params.is_aead {
            let cipher = match self.cipher_suite {
                TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => Cipher::aes_128_gcm(key).await?,
                TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 => Cipher::aes_256_gcm(key).await?,
                _ => return Err(TlsError::protocol("Unsupported AEAD cipher")),
            };
            CipherState::Aead {
                cipher,
                implicit_iv: iv.to_vec(),
                sequence: 0,
            }
        } else {
            let cipher = if key.len() == 16 {
                AesCbc::new_128(key).await?
            } else {
                AesCbc::new_256(key).await?
            };
            let mac_alg = match self.cipher_suite {
                TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA | TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA => {
                    MacAlgorithm::Sha1
                }
                TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384 => MacAlgorithm::Sha384,
                _ => MacAlgorithm::Sha256,
            };
            CipherState::Cbc {
                cipher,
                mac_key: mac_key.to_vec(),
                sequence: 0,
                mac_len: params.mac_len,
                mac_alg,
            }
        };

        self.read_cipher = Some(cipher_state);
        debug!("TLS 1.2 read cipher activated");
        Ok(())
    }

    /// Activate write cipher (client -> server)
    pub async fn set_write_cipher(&mut self, key: &[u8], iv: &[u8], mac_key: &[u8]) -> Result<()> {
        let params = self
            .params
            .as_ref()
            .ok_or_else(|| TlsError::protocol("Cipher suite not set"))?;

        let cipher_state = if params.is_aead {
            let cipher = match self.cipher_suite {
                TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => Cipher::aes_128_gcm(key).await?,
                TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 => Cipher::aes_256_gcm(key).await?,
                _ => return Err(TlsError::protocol("Unsupported AEAD cipher")),
            };
            CipherState::Aead {
                cipher,
                implicit_iv: iv.to_vec(),
                sequence: 0,
            }
        } else {
            let cipher = if key.len() == 16 {
                AesCbc::new_128(key).await?
            } else {
                AesCbc::new_256(key).await?
            };
            let mac_alg = match self.cipher_suite {
                TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA | TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA => {
                    MacAlgorithm::Sha1
                }
                TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384 => MacAlgorithm::Sha384,
                _ => MacAlgorithm::Sha256,
            };
            CipherState::Cbc {
                cipher,
                mac_key: mac_key.to_vec(),
                sequence: 0,
                mac_len: params.mac_len,
                mac_alg,
            }
        };

        self.write_cipher = Some(cipher_state);
        debug!("TLS 1.2 write cipher activated");
        Ok(())
    }

    /// Read a TLS record from the stream
    pub async fn read_record<S>(&mut self, stream: &mut S) -> Result<(u8, Vec<u8>)>
    where
        S: AsyncRead + Unpin,
    {
        // Read record header (5 bytes)
        let mut header = [0u8; 5];
        stream.read_exact(&mut header).await.map_err(TlsError::Io)?;

        let content_type = header[0];
        let _version = ((header[1] as u16) << 8) | (header[2] as u16);
        let length = ((header[3] as usize) << 8) | (header[4] as usize);

        if length > MAX_RECORD_SIZE {
            return Err(TlsError::record(format!("Record too large: {}", length)));
        }

        // Read record body
        let mut body = vec![0u8; length];
        stream.read_exact(&mut body).await.map_err(TlsError::Io)?;

        trace!("TLS 1.2 read record: type={}, len={}", content_type, length);

        // Decrypt if cipher is active
        if let Some(ref mut cipher) = self.read_cipher {
            let plaintext = Self::decrypt_record(cipher, content_type, &body).await?;
            return Ok((content_type, plaintext));
        }

        Ok((content_type, body))
    }

    /// Decrypt a record based on cipher type
    async fn decrypt_record(
        cipher: &mut CipherState,
        content_type: u8,
        body: &[u8],
    ) -> Result<Vec<u8>> {
        match cipher {
            CipherState::Aead {
                cipher: aead,
                implicit_iv,
                sequence,
            } => {
                // TLS 1.2 GCM: body = explicit_nonce (8 bytes) + ciphertext + tag (16 bytes)
                if body.len() < 8 + 16 {
                    return Err(TlsError::record("AEAD record too short"));
                }

                let explicit_nonce = &body[..8];
                let ciphertext = &body[8..];

                // Construct full nonce: implicit_iv (4) + explicit_nonce (8)
                let mut nonce = Vec::with_capacity(12);
                nonce.extend_from_slice(implicit_iv);
                nonce.extend_from_slice(explicit_nonce);

                // Additional data: seq_num (8) + type (1) + version (2) + length (2)
                let plaintext_len = ciphertext.len() - 16; // Subtract tag
                let mut aad = Vec::with_capacity(13);
                aad.extend_from_slice(&sequence.to_be_bytes());
                aad.push(content_type);
                aad.push((TLS_VERSION_1_2 >> 8) as u8);
                aad.push(TLS_VERSION_1_2 as u8);
                aad.push((plaintext_len >> 8) as u8);
                aad.push(plaintext_len as u8);

                let plaintext = aead.decrypt(&nonce, &aad, ciphertext).await?;
                *sequence = sequence.wrapping_add(1);

                Ok(plaintext)
            }
            CipherState::Cbc {
                cipher: cbc,
                mac_key,
                sequence,
                mac_len,
                mac_alg,
            } => {
                // TLS 1.2 CBC: body = IV (16) + ciphertext (includes MAC + padding)
                if body.len() < 16 + *mac_len + 1 {
                    return Err(TlsError::record("CBC record too short"));
                }

                let iv = &body[..16];
                let ciphertext = &body[16..];

                // Decrypt
                let decrypted = cbc.decrypt(iv, ciphertext).await?;

                // Remove padding (PKCS#7 - already handled by SubtleCrypto)
                // But we need to extract and verify MAC

                if decrypted.len() < *mac_len {
                    return Err(TlsError::record("Decrypted data too short for MAC"));
                }

                let mac_start = decrypted.len() - *mac_len;
                let plaintext = &decrypted[..mac_start];
                let received_mac = &decrypted[mac_start..];

                // Verify MAC using appropriate hash algorithm
                let computed_mac = match mac_alg {
                    MacAlgorithm::Sha1 => {
                        prf::compute_mac_sha1(
                            mac_key,
                            *sequence,
                            content_type,
                            TLS_VERSION_1_2,
                            plaintext,
                        )
                        .await?
                    }
                    MacAlgorithm::Sha256 => {
                        prf::compute_mac_sha256(
                            mac_key,
                            *sequence,
                            content_type,
                            TLS_VERSION_1_2,
                            plaintext,
                        )
                        .await?
                    }
                    MacAlgorithm::Sha384 => {
                        prf::compute_mac_sha384(
                            mac_key,
                            *sequence,
                            content_type,
                            TLS_VERSION_1_2,
                            plaintext,
                        )
                        .await?
                    }
                };

                // Truncate computed MAC to expected length
                if received_mac != &computed_mac[..*mac_len] {
                    return Err(TlsError::record("MAC verification failed"));
                }

                *sequence = sequence.wrapping_add(1);

                Ok(plaintext.to_vec())
            }
        }
    }

    /// Write a TLS record to the stream
    pub async fn write_record<S>(
        &mut self,
        stream: &mut S,
        content_type: u8,
        data: &[u8],
    ) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        // Split into multiple records if needed
        for chunk in data.chunks(MAX_PLAINTEXT_SIZE) {
            self.write_single_record(stream, content_type, chunk)
                .await?;
        }
        Ok(())
    }

    async fn write_single_record<S>(
        &mut self,
        stream: &mut S,
        content_type: u8,
        data: &[u8],
    ) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        let body = if let Some(ref mut cipher) = self.write_cipher {
            Self::encrypt_record(cipher, content_type, data).await?
        } else {
            data.to_vec()
        };

        // Build record
        let mut record = Vec::with_capacity(5 + body.len());
        record.push(content_type);
        record.push((TLS_VERSION_1_2 >> 8) as u8);
        record.push(TLS_VERSION_1_2 as u8);
        record.push((body.len() >> 8) as u8);
        record.push(body.len() as u8);
        record.extend_from_slice(&body);

        trace!(
            "TLS 1.2 write record: type={}, len={}",
            content_type,
            body.len()
        );

        stream.write_all(&record).await.map_err(TlsError::Io)?;
        stream.flush().await.map_err(TlsError::Io)?;

        Ok(())
    }

    /// Encrypt a record based on cipher type
    async fn encrypt_record(
        cipher: &mut CipherState,
        content_type: u8,
        plaintext: &[u8],
    ) -> Result<Vec<u8>> {
        match cipher {
            CipherState::Aead {
                cipher: aead,
                implicit_iv,
                sequence,
            } => {
                // Generate explicit nonce from sequence number
                let explicit_nonce = sequence.to_be_bytes();

                // Construct full nonce: implicit_iv (4) + explicit_nonce (8)
                let mut nonce = Vec::with_capacity(12);
                nonce.extend_from_slice(implicit_iv);
                nonce.extend_from_slice(&explicit_nonce);

                // Additional data: seq_num (8) + type (1) + version (2) + length (2)
                let mut aad = Vec::with_capacity(13);
                aad.extend_from_slice(&sequence.to_be_bytes());
                aad.push(content_type);
                aad.push((TLS_VERSION_1_2 >> 8) as u8);
                aad.push(TLS_VERSION_1_2 as u8);
                aad.push((plaintext.len() >> 8) as u8);
                aad.push(plaintext.len() as u8);

                let ciphertext = aead.encrypt(&nonce, &aad, plaintext).await?;

                // Result: explicit_nonce (8) + ciphertext + tag (16)
                let mut result = Vec::with_capacity(8 + ciphertext.len());
                result.extend_from_slice(&explicit_nonce);
                result.extend_from_slice(&ciphertext);

                *sequence = sequence.wrapping_add(1);

                Ok(result)
            }
            CipherState::Cbc {
                cipher: cbc,
                mac_key,
                sequence,
                mac_len,
                mac_alg,
            } => {
                // Compute MAC using appropriate hash algorithm
                let mac = match mac_alg {
                    MacAlgorithm::Sha1 => {
                        prf::compute_mac_sha1(
                            mac_key,
                            *sequence,
                            content_type,
                            TLS_VERSION_1_2,
                            plaintext,
                        )
                        .await?
                    }
                    MacAlgorithm::Sha256 => {
                        prf::compute_mac_sha256(
                            mac_key,
                            *sequence,
                            content_type,
                            TLS_VERSION_1_2,
                            plaintext,
                        )
                        .await?
                    }
                    MacAlgorithm::Sha384 => {
                        prf::compute_mac_sha384(
                            mac_key,
                            *sequence,
                            content_type,
                            TLS_VERSION_1_2,
                            plaintext,
                        )
                        .await?
                    }
                };

                // Truncate MAC to expected length
                let mac = &mac[..*mac_len];

                // Build plaintext with MAC
                let mut data_with_mac = Vec::with_capacity(plaintext.len() + *mac_len);
                data_with_mac.extend_from_slice(plaintext);
                data_with_mac.extend_from_slice(mac);

                // Generate random IV
                let iv = crate::crypto::random_bytes(16)?;

                // Encrypt (SubtleCrypto adds PKCS#7 padding)
                let ciphertext = cbc.encrypt(&iv, &data_with_mac).await?;

                // Result: IV (16) + ciphertext
                let mut result = Vec::with_capacity(16 + ciphertext.len());
                result.extend_from_slice(&iv);
                result.extend_from_slice(&ciphertext);

                *sequence = sequence.wrapping_add(1);

                Ok(result)
            }
        }
    }

    /// Check if read cipher is active
    pub fn has_read_cipher(&self) -> bool {
        self.read_cipher.is_some()
    }

    /// Check if write cipher is active
    pub fn has_write_cipher(&self) -> bool {
        self.write_cipher.is_some()
    }
}

impl Default for RecordLayer12 {
    fn default() -> Self {
        Self::new()
    }
}
