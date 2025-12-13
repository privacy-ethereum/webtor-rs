//! TLS 1.3 Record Layer
//!
//! Handles reading and writing TLS records with encryption/decryption.
//! TLS 1.3 uses AEAD (AES-GCM or ChaCha20-Poly1305) for all encryption.

use crate::crypto::Cipher;
use crate::error::{Result, TlsError};
use crate::handshake::{
    CONTENT_TYPE_APPLICATION_DATA, CONTENT_TYPE_HANDSHAKE, TLS_AES_128_GCM_SHA256,
    TLS_AES_256_GCM_SHA384, TLS_CHACHA20_POLY1305_SHA256, TLS_VERSION_1_2,
};
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, trace};

/// Maximum TLS record size (16KB + some overhead)
pub const MAX_RECORD_SIZE: usize = 16384 + 256;
/// Maximum plaintext size per record
pub const MAX_PLAINTEXT_SIZE: usize = 16384;

/// TLS record reader/writer with encryption state
pub struct RecordLayer {
    /// Read cipher (for decrypting incoming records)
    read_cipher: Option<RecordCipher>,
    /// Write cipher (for encrypting outgoing records)
    write_cipher: Option<RecordCipher>,
    /// Negotiated cipher suite
    cipher_suite: u16,
}

/// Encryption state for one direction
struct RecordCipher {
    aead: Cipher,
    iv: Vec<u8>,
    sequence: u64,
}

impl RecordCipher {
    fn new(aead: Cipher, iv: Vec<u8>) -> Self {
        Self {
            aead,
            iv,
            sequence: 0,
        }
    }

    /// Compute the nonce for the current record
    fn compute_nonce(&self) -> Vec<u8> {
        let mut nonce = self.iv.clone();
        // XOR sequence number into the last 8 bytes of IV
        let seq_bytes = self.sequence.to_be_bytes();
        for i in 0..8 {
            nonce[4 + i] ^= seq_bytes[i];
        }
        nonce
    }

    fn increment_sequence(&mut self) {
        self.sequence = self.sequence.wrapping_add(1);
    }
}

impl RecordLayer {
    /// Create a new record layer (initially unencrypted)
    pub fn new() -> Self {
        Self {
            read_cipher: None,
            write_cipher: None,
            cipher_suite: TLS_AES_128_GCM_SHA256,
        }
    }

    /// Set the negotiated cipher suite
    pub fn set_cipher_suite(&mut self, cipher_suite: u16) {
        self.cipher_suite = cipher_suite;
        debug!("Record layer using cipher suite: 0x{:04x}", cipher_suite);
    }

    /// Create a cipher based on the negotiated cipher suite
    async fn create_cipher(&self, key: &[u8]) -> Result<Cipher> {
        match self.cipher_suite {
            TLS_AES_128_GCM_SHA256 => Cipher::aes_128_gcm(key).await,
            TLS_AES_256_GCM_SHA384 => Cipher::aes_256_gcm(key).await,
            TLS_CHACHA20_POLY1305_SHA256 => Cipher::chacha20_poly1305(key),
            _ => Err(TlsError::protocol(format!(
                "Unsupported cipher suite: 0x{:04x}",
                self.cipher_suite
            ))),
        }
    }

    /// Set the read cipher for decrypting incoming records
    pub async fn set_read_cipher(&mut self, key: &[u8], iv: &[u8]) -> Result<()> {
        let aead = self.create_cipher(key).await?;
        self.read_cipher = Some(RecordCipher::new(aead, iv.to_vec()));
        debug!(
            "Read cipher activated (cipher suite: 0x{:04x})",
            self.cipher_suite
        );
        Ok(())
    }

    /// Set the write cipher for encrypting outgoing records
    pub async fn set_write_cipher(&mut self, key: &[u8], iv: &[u8]) -> Result<()> {
        let aead = self.create_cipher(key).await?;
        self.write_cipher = Some(RecordCipher::new(aead, iv.to_vec()));
        debug!(
            "Write cipher activated (cipher suite: 0x{:04x})",
            self.cipher_suite
        );
        Ok(())
    }

    /// Read a single TLS record from the stream
    pub async fn read_record<S>(&mut self, stream: &mut S) -> Result<(u8, Vec<u8>)>
    where
        S: AsyncRead + Unpin,
    {
        // Read record header (5 bytes)
        tracing::debug!(
            "read_record: waiting for 5-byte header (cipher active: {})",
            self.read_cipher.is_some()
        );
        let mut header = [0u8; 5];
        stream.read_exact(&mut header).await.map_err(|e| {
            tracing::error!("read_record: read_exact for header failed: {}", e);
            TlsError::Io(e)
        })?;
        tracing::debug!("read_record: got header: {:02x?}", header);

        let content_type = header[0];
        let _version = ((header[1] as u16) << 8) | (header[2] as u16);
        let length = ((header[3] as usize) << 8) | (header[4] as usize);

        if length > MAX_RECORD_SIZE {
            return Err(TlsError::record(format!("Record too large: {}", length)));
        }

        // Read record body
        tracing::debug!("read_record: reading {} byte body", length);
        let mut body = vec![0u8; length];
        stream.read_exact(&mut body).await.map_err(|e| {
            tracing::error!("read_record: body read failed: {}", e);
            TlsError::Io(e)
        })?;
        tracing::debug!(
            "read_record: got body, type={}, len={}",
            content_type,
            length
        );

        trace!("Read record: type={}, len={}", content_type, length);

        // Decrypt if cipher is active
        if let Some(ref mut cipher) = self.read_cipher {
            if content_type == CONTENT_TYPE_APPLICATION_DATA {
                tracing::debug!("read_record: decrypting APPLICATION_DATA record");
                let nonce = cipher.compute_nonce();
                tracing::debug!(
                    "read_record: nonce={:02x?}, body_len={}",
                    &nonce,
                    body.len()
                );
                // Additional data is the record header with encrypted length
                let aad = &header;

                tracing::debug!(
                    "read_record: calling aead.decrypt (key_size={}, aad={:02x?})",
                    cipher.aead.key_size(),
                    aad
                );
                let plaintext = cipher.aead.decrypt(&nonce, aad, &body).await?;
                cipher.increment_sequence();

                // TLS 1.3: plaintext format is [content][content_type][zeros...]
                // The content type is the last byte. We don't strip padding zeros because
                // legitimate content data can end with zeros (e.g., DER-encoded certificates).
                if plaintext.is_empty() {
                    return Err(TlsError::record("Empty decrypted record"));
                }

                let actual_content_type = plaintext[plaintext.len() - 1];
                let data = plaintext[..plaintext.len() - 1].to_vec();

                trace!(
                    "Decrypted record: type={}, len={}",
                    actual_content_type,
                    data.len()
                );
                return Ok((actual_content_type, data));
            }
        }

        Ok((content_type, body))
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
        let (record_type, body) = if let Some(ref mut cipher) = self.write_cipher {
            // TLS 1.3: encrypt with content type appended to plaintext
            let mut plaintext = data.to_vec();
            plaintext.push(content_type);

            let nonce = cipher.compute_nonce();

            // Build header for AAD (we need to know ciphertext length first)
            let ciphertext_len = plaintext.len() + 16; // +16 for auth tag
            let aad = [
                CONTENT_TYPE_APPLICATION_DATA,
                (TLS_VERSION_1_2 >> 8) as u8,
                TLS_VERSION_1_2 as u8,
                (ciphertext_len >> 8) as u8,
                ciphertext_len as u8,
            ];

            let ciphertext = cipher.aead.encrypt(&nonce, &aad, &plaintext).await?;
            cipher.increment_sequence();

            (CONTENT_TYPE_APPLICATION_DATA, ciphertext)
        } else {
            (content_type, data.to_vec())
        };

        // Build record header
        let mut record = Vec::with_capacity(5 + body.len());
        record.push(record_type);
        record.push((TLS_VERSION_1_2 >> 8) as u8);
        record.push(TLS_VERSION_1_2 as u8);
        record.push((body.len() >> 8) as u8);
        record.push(body.len() as u8);
        record.extend_from_slice(&body);

        trace!("Write record: type={}, len={}", record_type, body.len());

        stream
            .write_all(&record)
            .await
            .map_err(|e| TlsError::Io(e))?;
        stream.flush().await.map_err(|e| TlsError::Io(e))?;

        Ok(())
    }

    /// Read multiple handshake messages from the stream
    /// Returns handshake messages as a vector
    pub async fn read_handshake_messages<S>(&mut self, stream: &mut S) -> Result<Vec<(u8, Vec<u8>)>>
    where
        S: AsyncRead + Unpin,
    {
        let mut messages = Vec::new();

        let (content_type, data) = self.read_record(stream).await?;

        if content_type != CONTENT_TYPE_HANDSHAKE {
            return Err(TlsError::UnexpectedMessage {
                expected: "Handshake".to_string(),
                got: format!("ContentType {}", content_type),
            });
        }

        // Parse handshake messages from the data
        let mut pos = 0;
        while pos + 4 <= data.len() {
            let msg_type = data[pos];
            let length = ((data[pos + 1] as usize) << 16)
                | ((data[pos + 2] as usize) << 8)
                | (data[pos + 3] as usize);
            pos += 4;

            if pos + length > data.len() {
                return Err(TlsError::record("Handshake message extends beyond record"));
            }

            messages.push((msg_type, data[pos..pos + length].to_vec()));
            pos += length;
        }

        Ok(messages)
    }
}

impl Default for RecordLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordLayer {
    /// Check if read cipher is active
    pub fn has_read_cipher(&self) -> bool {
        self.read_cipher.is_some()
    }

    /// Check if write cipher is active
    pub fn has_write_cipher(&self) -> bool {
        self.write_cipher.is_some()
    }

    /// Decrypt a record synchronously (for use in poll_read)
    /// Returns (content_type, plaintext)
    pub fn decrypt_record_sync(&mut self, header: &[u8; 5], body: &[u8]) -> Result<(u8, Vec<u8>)> {
        if let Some(ref mut cipher) = self.read_cipher {
            let nonce = cipher.compute_nonce();

            // Decrypt using synchronous API
            let plaintext = cipher.aead.decrypt_sync(&nonce, header, body)?;
            cipher.increment_sequence();

            // TLS 1.3 inner plaintext: last byte is content type
            if plaintext.is_empty() {
                return Err(TlsError::record("Empty decrypted record"));
            }
            let actual_content_type = plaintext[plaintext.len() - 1];
            let data = plaintext[..plaintext.len() - 1].to_vec();

            Ok((actual_content_type, data))
        } else {
            // No cipher active, return as-is
            Ok((header[0], body.to_vec()))
        }
    }

    /// Encrypt a record synchronously (for use in poll_write)
    /// Returns the full encrypted record including header
    pub fn encrypt_record_sync(&mut self, content_type: u8, data: &[u8]) -> Result<Vec<u8>> {
        if let Some(ref mut cipher) = self.write_cipher {
            // TLS 1.3: encrypt with content type appended to plaintext
            let mut plaintext = data.to_vec();
            plaintext.push(content_type);

            let nonce = cipher.compute_nonce();

            // Build header for AAD (we need to know ciphertext length first)
            let ciphertext_len = plaintext.len() + 16; // +16 for auth tag
            let header = [
                CONTENT_TYPE_APPLICATION_DATA,
                (TLS_VERSION_1_2 >> 8) as u8,
                TLS_VERSION_1_2 as u8,
                (ciphertext_len >> 8) as u8,
                ciphertext_len as u8,
            ];

            let ciphertext = cipher.aead.encrypt_sync(&nonce, &header, &plaintext)?;
            cipher.increment_sequence();

            // Build full record
            let mut record = Vec::with_capacity(5 + ciphertext.len());
            record.extend_from_slice(&header);
            record.extend_from_slice(&ciphertext);

            Ok(record)
        } else {
            // No cipher active, send unencrypted
            let mut record = Vec::with_capacity(5 + data.len());
            record.push(content_type);
            record.push((TLS_VERSION_1_2 >> 8) as u8);
            record.push(TLS_VERSION_1_2 as u8);
            record.push((data.len() >> 8) as u8);
            record.push(data.len() as u8);
            record.extend_from_slice(data);
            Ok(record)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_layer_new() {
        let layer = RecordLayer::new();
        assert!(!layer.has_read_cipher());
        assert!(!layer.has_write_cipher());
    }
}
