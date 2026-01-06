//! TLS 1.3 Handshake implementation
//!
//! This module handles the TLS 1.3 handshake protocol:
//! 1. ClientHello (with key_share extension)
//! 2. ServerHello
//! 3. EncryptedExtensions
//! 4. Certificate
//! 5. CertificateVerify
//! 6. Finished

use crate::crypto::{self, EcdhKeyPair, Hkdf, X25519KeyPair};
use crate::error::{Result, TlsError};
use tracing::{debug, trace};

// TLS 1.3 constants
pub const TLS_VERSION_1_2: u16 = 0x0303; // Used in record layer for compatibility
pub const TLS_VERSION_1_3: u16 = 0x0304;

// Content types
pub const CONTENT_TYPE_CHANGE_CIPHER_SPEC: u8 = 20;
pub const CONTENT_TYPE_ALERT: u8 = 21;
pub const CONTENT_TYPE_HANDSHAKE: u8 = 22;
pub const CONTENT_TYPE_APPLICATION_DATA: u8 = 23;

// Handshake types
pub const HANDSHAKE_CLIENT_HELLO: u8 = 1;
pub const HANDSHAKE_SERVER_HELLO: u8 = 2;
pub const HANDSHAKE_NEW_SESSION_TICKET: u8 = 4;
pub const HANDSHAKE_END_OF_EARLY_DATA: u8 = 5;
pub const HANDSHAKE_ENCRYPTED_EXTENSIONS: u8 = 8;
pub const HANDSHAKE_CERTIFICATE: u8 = 11;
pub const HANDSHAKE_CERTIFICATE_REQUEST: u8 = 13;
pub const HANDSHAKE_CERTIFICATE_VERIFY: u8 = 15;
pub const HANDSHAKE_FINISHED: u8 = 20;
pub const HANDSHAKE_KEY_UPDATE: u8 = 24;
pub const HANDSHAKE_MESSAGE_HASH: u8 = 254;

// Extension types
pub const EXT_SERVER_NAME: u16 = 0;
pub const EXT_SUPPORTED_GROUPS: u16 = 10;
pub const EXT_SIGNATURE_ALGORITHMS: u16 = 13;
pub const EXT_ALPN: u16 = 16;
pub const EXT_SUPPORTED_VERSIONS: u16 = 43;
pub const EXT_KEY_SHARE: u16 = 51;

// Cipher suites
pub const TLS_AES_128_GCM_SHA256: u16 = 0x1301;
pub const TLS_AES_256_GCM_SHA384: u16 = 0x1302;
pub const TLS_CHACHA20_POLY1305_SHA256: u16 = 0x1303;

// Named groups
pub const GROUP_SECP256R1: u16 = 0x0017; // P-256
pub const GROUP_X25519: u16 = 0x001d;

// Signature algorithms
pub const SIG_ECDSA_SECP256R1_SHA256: u16 = 0x0403;
pub const SIG_RSA_PSS_RSAE_SHA256: u16 = 0x0804;
pub const SIG_RSA_PKCS1_SHA256: u16 = 0x0401;

/// Key exchange state - supports both P-256 and X25519
pub enum KeyExchangeState {
    P256(EcdhKeyPair),
    X25519(X25519KeyPair),
}

/// TLS handshake state machine
pub struct HandshakeState {
    /// Our P-256 ECDH key pair
    pub ecdh_key: EcdhKeyPair,
    /// Our X25519 key pair (for servers that prefer X25519)
    pub x25519_key: Option<X25519KeyPair>,
    /// Which key exchange was selected by the server
    pub selected_group: u16,
    /// Client random (32 bytes)
    pub client_random: Vec<u8>,
    /// Server random (32 bytes)
    pub server_random: Vec<u8>,
    /// Negotiated cipher suite
    pub cipher_suite: u16,
    /// Server name for SNI
    pub server_name: String,
    /// Transcript hash of all handshake messages
    pub transcript: Vec<u8>,
    /// Handshake secret
    pub handshake_secret: Option<Vec<u8>>,
    /// Client handshake traffic secret
    pub client_handshake_secret: Option<Vec<u8>>,
    /// Server handshake traffic secret
    pub server_handshake_secret: Option<Vec<u8>>,
    /// Client application traffic secret
    pub client_app_secret: Option<Vec<u8>>,
    /// Server application traffic secret
    pub server_app_secret: Option<Vec<u8>>,
    /// Exporter master secret for RFC 8446 key export
    pub exporter_master_secret: Option<Vec<u8>>,
}

impl HandshakeState {
    /// Create a new handshake state
    pub async fn new(server_name: &str) -> Result<Self> {
        let ecdh_key = EcdhKeyPair::generate().await?;
        let x25519_key = X25519KeyPair::generate()?;
        let client_random = crypto::random_bytes(32)?;

        Ok(Self {
            ecdh_key,
            x25519_key: Some(x25519_key),
            selected_group: GROUP_SECP256R1, // Default, will be updated by ServerHello
            client_random,
            server_random: Vec::new(),
            cipher_suite: TLS_AES_128_GCM_SHA256,
            server_name: server_name.to_string(),
            transcript: Vec::new(),
            handshake_secret: None,
            client_handshake_secret: None,
            server_handshake_secret: None,
            client_app_secret: None,
            server_app_secret: None,
            exporter_master_secret: None,
        })
    }

    /// Build ClientHello message
    pub fn build_client_hello(&self) -> Vec<u8> {
        let mut hello = Vec::new();

        // Legacy version (TLS 1.2 for compatibility)
        hello.push((TLS_VERSION_1_2 >> 8) as u8);
        hello.push(TLS_VERSION_1_2 as u8);

        // Random (32 bytes)
        hello.extend_from_slice(&self.client_random);

        // Legacy session ID (empty)
        hello.push(0);

        // Cipher suites (only SHA-256 based suites for now, SHA-384 not implemented)
        let cipher_suites = [TLS_AES_128_GCM_SHA256, TLS_CHACHA20_POLY1305_SHA256];
        hello.push(0);
        hello.push((cipher_suites.len() * 2) as u8);
        for cs in cipher_suites {
            hello.push((cs >> 8) as u8);
            hello.push(cs as u8);
        }

        // Legacy compression methods (null only)
        hello.push(1);
        hello.push(0);

        // Extensions
        let extensions = self.build_extensions();
        hello.push((extensions.len() >> 8) as u8);
        hello.push(extensions.len() as u8);
        hello.extend_from_slice(&extensions);

        // Wrap in handshake header
        let mut message = vec![HANDSHAKE_CLIENT_HELLO];
        let len = hello.len();
        message.push((len >> 16) as u8);
        message.push((len >> 8) as u8);
        message.push(len as u8);
        message.extend_from_slice(&hello);

        message
    }

    fn build_extensions(&self) -> Vec<u8> {
        let mut extensions = Vec::new();

        // Server Name Indication (SNI)
        let sni = self.build_sni_extension();
        extensions.push((EXT_SERVER_NAME >> 8) as u8);
        extensions.push(EXT_SERVER_NAME as u8);
        extensions.push((sni.len() >> 8) as u8);
        extensions.push(sni.len() as u8);
        extensions.extend_from_slice(&sni);

        // Supported Versions (TLS 1.3 only)
        extensions.push((EXT_SUPPORTED_VERSIONS >> 8) as u8);
        extensions.push(EXT_SUPPORTED_VERSIONS as u8);
        extensions.push(0);
        extensions.push(3); // Length
        extensions.push(2); // Versions length
        extensions.push((TLS_VERSION_1_3 >> 8) as u8);
        extensions.push(TLS_VERSION_1_3 as u8);

        // Supported Groups (X25519 preferred, then P-256)
        extensions.push((EXT_SUPPORTED_GROUPS >> 8) as u8);
        extensions.push(EXT_SUPPORTED_GROUPS as u8);
        extensions.push(0);
        extensions.push(6); // Length: 2 (list len) + 2 (X25519) + 2 (P-256)
        extensions.push(0);
        extensions.push(4); // Groups length: 2 groups * 2 bytes
        extensions.push((GROUP_X25519 >> 8) as u8);
        extensions.push(GROUP_X25519 as u8);
        extensions.push((GROUP_SECP256R1 >> 8) as u8);
        extensions.push(GROUP_SECP256R1 as u8);

        // Key Share (both X25519 and P-256 public keys)
        let key_share = self.build_key_share_extension();
        extensions.push((EXT_KEY_SHARE >> 8) as u8);
        extensions.push(EXT_KEY_SHARE as u8);
        extensions.push((key_share.len() >> 8) as u8);
        extensions.push(key_share.len() as u8);
        extensions.extend_from_slice(&key_share);

        // Signature Algorithms
        let sig_algs = self.build_signature_algorithms_extension();
        extensions.push((EXT_SIGNATURE_ALGORITHMS >> 8) as u8);
        extensions.push(EXT_SIGNATURE_ALGORITHMS as u8);
        extensions.push((sig_algs.len() >> 8) as u8);
        extensions.push(sig_algs.len() as u8);
        extensions.extend_from_slice(&sig_algs);

        // ALPN (Application-Layer Protocol Negotiation) - advertise http/1.1
        let alpn = self.build_alpn_extension();
        extensions.push((EXT_ALPN >> 8) as u8);
        extensions.push(EXT_ALPN as u8);
        extensions.push((alpn.len() >> 8) as u8);
        extensions.push(alpn.len() as u8);
        extensions.extend_from_slice(&alpn);

        extensions
    }

    fn build_alpn_extension(&self) -> Vec<u8> {
        // ALPN protocol list - only advertise http/1.1
        // We don't support HTTP/2, so advertising h2 causes servers to send
        // HTTP/2 binary frames that we can't parse.
        // Format: length(2) + [ length(1) + protocol_name ]*
        let protocols: &[&[u8]] = &[b"http/1.1"];
        let mut ext = Vec::new();

        // Calculate total list length
        let list_len: usize = protocols.iter().map(|p| 1 + p.len()).sum();
        ext.push((list_len >> 8) as u8);
        ext.push(list_len as u8);

        // Add each protocol entry
        for protocol in protocols {
            ext.push(protocol.len() as u8);
            ext.extend_from_slice(protocol);
        }

        ext
    }

    fn build_sni_extension(&self) -> Vec<u8> {
        let name_bytes = self.server_name.as_bytes();
        let mut ext = Vec::new();

        // Server name list length
        let list_len = 3 + name_bytes.len();
        ext.push((list_len >> 8) as u8);
        ext.push(list_len as u8);

        // Server name type (host_name = 0)
        ext.push(0);

        // Server name length
        ext.push((name_bytes.len() >> 8) as u8);
        ext.push(name_bytes.len() as u8);

        // Server name
        ext.extend_from_slice(name_bytes);

        ext
    }

    fn build_key_share_extension(&self) -> Vec<u8> {
        let p256_key_bytes = &self.ecdh_key.public_key_bytes;
        let mut ext = Vec::new();

        // Calculate total length for both key shares
        let p256_entry_len = 4 + p256_key_bytes.len(); // group(2) + len(2) + key
        let x25519_entry_len = if let Some(ref x25519_key) = self.x25519_key {
            4 + x25519_key.public_key_bytes.len()
        } else {
            0
        };
        let total_len = p256_entry_len + x25519_entry_len;

        // Client key shares length
        ext.push((total_len >> 8) as u8);
        ext.push(total_len as u8);

        // X25519 key share (first, as preferred)
        if let Some(ref x25519_key) = self.x25519_key {
            let key_bytes = &x25519_key.public_key_bytes;
            ext.push((GROUP_X25519 >> 8) as u8);
            ext.push(GROUP_X25519 as u8);
            ext.push((key_bytes.len() >> 8) as u8);
            ext.push(key_bytes.len() as u8);
            ext.extend_from_slice(key_bytes);
        }

        // P-256 key share
        ext.push((GROUP_SECP256R1 >> 8) as u8);
        ext.push(GROUP_SECP256R1 as u8);
        ext.push((p256_key_bytes.len() >> 8) as u8);
        ext.push(p256_key_bytes.len() as u8);
        ext.extend_from_slice(p256_key_bytes);

        ext
    }

    fn build_signature_algorithms_extension(&self) -> Vec<u8> {
        let algorithms = [
            SIG_ECDSA_SECP256R1_SHA256,
            SIG_RSA_PSS_RSAE_SHA256,
            SIG_RSA_PKCS1_SHA256,
        ];

        let mut ext = Vec::new();
        ext.push(0);
        ext.push((algorithms.len() * 2) as u8);

        for alg in algorithms {
            ext.push((alg >> 8) as u8);
            ext.push(alg as u8);
        }

        ext
    }

    /// Parse ServerHello message and extract key share
    pub fn parse_server_hello(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 38 {
            return Err(TlsError::handshake("ServerHello too short"));
        }

        let mut pos = 0;

        // Legacy version (ignore)
        pos += 2;

        // Random
        self.server_random = data[pos..pos + 32].to_vec();
        pos += 32;

        // Session ID (skip)
        let session_id_len = data[pos] as usize;
        pos += 1 + session_id_len;

        if pos + 4 > data.len() {
            return Err(TlsError::handshake("ServerHello truncated"));
        }

        // Cipher suite
        self.cipher_suite = ((data[pos] as u16) << 8) | (data[pos + 1] as u16);
        pos += 2;
        debug!("Negotiated cipher suite: 0x{:04x}", self.cipher_suite);

        // Compression method (skip)
        pos += 1;

        // Extensions
        if pos + 2 > data.len() {
            return Err(TlsError::handshake("ServerHello missing extensions"));
        }
        let ext_len = ((data[pos] as usize) << 8) | (data[pos + 1] as usize);
        pos += 2;

        let ext_end = pos + ext_len;
        let mut server_key_share = None;

        while pos + 4 <= ext_end {
            let ext_type = ((data[pos] as u16) << 8) | (data[pos + 1] as u16);
            let ext_data_len = ((data[pos + 2] as usize) << 8) | (data[pos + 3] as usize);
            pos += 4;

            if pos + ext_data_len > ext_end {
                return Err(TlsError::handshake("Extension data overflow"));
            }

            let ext_data = &data[pos..pos + ext_data_len];
            pos += ext_data_len;

            match ext_type {
                EXT_SUPPORTED_VERSIONS => {
                    if ext_data.len() >= 2 {
                        let version = ((ext_data[0] as u16) << 8) | (ext_data[1] as u16);
                        if version != TLS_VERSION_1_3 {
                            return Err(TlsError::handshake(format!(
                                "Unsupported TLS version: 0x{:04x}",
                                version
                            )));
                        }
                    }
                }
                EXT_KEY_SHARE => {
                    // Parse key share: group (2) + key_len (2) + key
                    if ext_data.len() >= 4 {
                        let group = ((ext_data[0] as u16) << 8) | (ext_data[1] as u16);
                        let key_len = ((ext_data[2] as usize) << 8) | (ext_data[3] as usize);

                        // We support both P-256 and X25519
                        if group != GROUP_SECP256R1 && group != GROUP_X25519 {
                            return Err(TlsError::handshake(format!(
                                "Unsupported key share group: 0x{:04x}",
                                group
                            )));
                        }

                        self.selected_group = group;
                        debug!("Server selected key exchange group: 0x{:04x}", group);

                        if ext_data.len() >= 4 + key_len {
                            server_key_share = Some(ext_data[4..4 + key_len].to_vec());
                        }
                    }
                }
                _ => {
                    trace!("Ignoring extension 0x{:04x}", ext_type);
                }
            }
        }

        server_key_share.ok_or_else(|| TlsError::handshake("No key_share in ServerHello"))
    }

    /// Derive handshake keys after receiving ServerHello
    pub async fn derive_handshake_keys(&mut self, server_key_share: &[u8]) -> Result<()> {
        tracing::info!(
            "derive_handshake_keys: selected_group=0x{:04x}, key_share_len={}",
            self.selected_group,
            server_key_share.len()
        );
        // Compute shared secret based on selected key exchange group
        let shared_secret = match self.selected_group {
            GROUP_SECP256R1 => {
                // P-256 ECDH via SubtleCrypto
                tracing::info!("Using P-256 ECDH");
                self.ecdh_key.derive_shared_secret(server_key_share).await?
            }
            GROUP_X25519 => {
                // X25519 via pure Rust
                tracing::info!("Using X25519 ECDH");
                let x25519_key = self
                    .x25519_key
                    .take()
                    .ok_or_else(|| TlsError::handshake("X25519 key not available"))?;
                x25519_key.derive_shared_secret(server_key_share)?
            }
            _ => {
                return Err(TlsError::handshake(format!(
                    "Unsupported key exchange group: 0x{:04x}",
                    self.selected_group
                )));
            }
        };
        tracing::info!(
            "Derived shared secret ({} bytes) using group 0x{:04x}",
            shared_secret.len(),
            self.selected_group
        );

        // Compute transcript hash up to this point
        tracing::info!(
            "Computing transcript hash ({} bytes of transcript)",
            self.transcript.len()
        );
        let transcript_hash = crypto::sha256(&self.transcript).await?;
        tracing::info!("Transcript hash computed: {} bytes", transcript_hash.len());

        // TLS 1.3 key schedule
        // Early Secret = HKDF-Extract(salt=0, IKM=0)
        tracing::info!("Starting TLS 1.3 key schedule");
        let zero_key = vec![0u8; 32];
        let early_secret = Hkdf::extract(&[], &zero_key).await?;
        tracing::info!("Early secret derived");

        // Derive-Secret(early_secret, "derived", "")
        let empty_hash = crypto::sha256(&[]).await?;
        let derived_secret = Hkdf::derive_secret(&early_secret, "derived", &empty_hash).await?;
        tracing::info!("Derived secret computed");

        // Handshake Secret = HKDF-Extract(derived_secret, shared_secret)
        let handshake_secret = Hkdf::extract(&derived_secret, &shared_secret).await?;
        tracing::info!("Handshake secret derived");

        // Client/Server handshake traffic secrets
        let client_hs_secret =
            Hkdf::derive_secret(&handshake_secret, "c hs traffic", &transcript_hash).await?;
        tracing::info!("Client handshake secret derived");
        let server_hs_secret =
            Hkdf::derive_secret(&handshake_secret, "s hs traffic", &transcript_hash).await?;
        tracing::info!("Server handshake secret derived");

        self.handshake_secret = Some(handshake_secret);
        self.client_handshake_secret = Some(client_hs_secret);
        self.server_handshake_secret = Some(server_hs_secret);

        tracing::info!("Derived handshake traffic secrets complete");
        Ok(())
    }

    /// Derive application keys after receiving server Finished
    pub async fn derive_application_keys(&mut self) -> Result<()> {
        let handshake_secret = self
            .handshake_secret
            .as_ref()
            .ok_or_else(|| TlsError::handshake("Missing handshake secret"))?;

        // Transcript hash up to server Finished
        let transcript_hash = crypto::sha256(&self.transcript).await?;

        // Derive-Secret(handshake_secret, "derived", "")
        let empty_hash = crypto::sha256(&[]).await?;
        let derived_secret = Hkdf::derive_secret(handshake_secret, "derived", &empty_hash).await?;

        // Master Secret = HKDF-Extract(derived_secret, 0)
        let zero_key = vec![0u8; 32];
        let master_secret = Hkdf::extract(&derived_secret, &zero_key).await?;

        // Client/Server application traffic secrets
        let client_app_secret =
            Hkdf::derive_secret(&master_secret, "c ap traffic", &transcript_hash).await?;
        let server_app_secret =
            Hkdf::derive_secret(&master_secret, "s ap traffic", &transcript_hash).await?;

        // Exporter master secret (RFC 8446 Section 7.5)
        // exporter_master_secret = Derive-Secret(Master Secret, "exp master", Transcript-Hash)
        let exporter_master_secret =
            Hkdf::derive_secret(&master_secret, "exp master", &transcript_hash).await?;

        self.client_app_secret = Some(client_app_secret);
        self.server_app_secret = Some(server_app_secret);
        self.exporter_master_secret = Some(exporter_master_secret);

        debug!("Derived application traffic secrets and exporter master secret");
        Ok(())
    }

    /// Compute Finished message verify data
    pub async fn compute_finished(&self, is_client: bool) -> Result<Vec<u8>> {
        let base_key = if is_client {
            self.client_handshake_secret.as_ref()
        } else {
            self.server_handshake_secret.as_ref()
        }
        .ok_or_else(|| TlsError::handshake("Missing handshake secret"))?;

        // finished_key = HKDF-Expand-Label(base_key, "finished", "", Hash.length)
        let finished_key = Hkdf::expand_label(base_key, "finished", &[], 32).await?;

        // verify_data = HMAC(finished_key, Transcript-Hash)
        let transcript_hash = crypto::sha256(&self.transcript).await?;

        // Compute HMAC using the crypto module
        let verify_data = hmac_sha256(&finished_key, &transcript_hash).await?;

        Ok(verify_data)
    }

    /// Build client Finished message
    pub async fn build_client_finished(&self) -> Result<Vec<u8>> {
        let verify_data = self.compute_finished(true).await?;

        let mut message = vec![HANDSHAKE_FINISHED];
        let len = verify_data.len();
        message.push((len >> 16) as u8);
        message.push((len >> 8) as u8);
        message.push(len as u8);
        message.extend_from_slice(&verify_data);

        Ok(message)
    }

    /// Verify server Finished message
    pub async fn verify_server_finished(&self, received_verify_data: &[u8]) -> Result<()> {
        let expected = self.compute_finished(false).await?;

        if received_verify_data != expected {
            return Err(TlsError::handshake("Server Finished verification failed"));
        }

        debug!("Server Finished verified successfully");
        Ok(())
    }

    /// Add handshake message to transcript
    pub fn update_transcript(&mut self, data: &[u8]) {
        self.transcript.extend_from_slice(data);
    }

    /// Get the key size for the negotiated cipher suite
    fn get_key_size(&self) -> usize {
        match self.cipher_suite {
            TLS_AES_128_GCM_SHA256 => 16,
            TLS_AES_256_GCM_SHA384 => 32,
            TLS_CHACHA20_POLY1305_SHA256 => 32,
            _ => 16, // Default
        }
    }

    /// Get key and IV for handshake encryption
    pub async fn get_handshake_keys(&self, is_client: bool) -> Result<(Vec<u8>, Vec<u8>)> {
        let secret = if is_client {
            self.client_handshake_secret.as_ref()
        } else {
            self.server_handshake_secret.as_ref()
        }
        .ok_or_else(|| TlsError::handshake("Missing handshake secret"))?;

        let key_size = self.get_key_size();
        let key = Hkdf::expand_label(secret, "key", &[], key_size).await?;
        let iv = Hkdf::expand_label(secret, "iv", &[], 12).await?;

        Ok((key, iv))
    }

    /// Get key and IV for application data encryption
    pub async fn get_application_keys(&self, is_client: bool) -> Result<(Vec<u8>, Vec<u8>)> {
        let secret = if is_client {
            self.client_app_secret.as_ref()
        } else {
            self.server_app_secret.as_ref()
        }
        .ok_or_else(|| TlsError::handshake("Missing application secret"))?;

        let key_size = self.get_key_size();
        let key = Hkdf::expand_label(secret, "key", &[], key_size).await?;
        let iv = Hkdf::expand_label(secret, "iv", &[], 12).await?;

        Ok((key, iv))
    }

    /// Get the exporter master secret for RFC 8446 key export
    pub fn get_exporter_master_secret(&self) -> Option<&[u8]> {
        self.exporter_master_secret.as_deref()
    }
}

/// HMAC-SHA256 helper (duplicated from crypto module for internal use)
async fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::CryptoKey;

    let window = web_sys::window().ok_or_else(|| TlsError::subtle_crypto("No window"))?;
    let subtle = window
        .crypto()
        .map_err(|_| TlsError::subtle_crypto("No crypto"))?
        .subtle();

    let key_data = Uint8Array::from(key);
    let algorithm = Object::new();
    Reflect::set(&algorithm, &"name".into(), &"HMAC".into())
        .map_err(|_| TlsError::subtle_crypto("Failed to set HMAC algorithm name"))?;
    let hash_obj = Object::new();
    Reflect::set(&hash_obj, &"name".into(), &"SHA-256".into())
        .map_err(|_| TlsError::subtle_crypto("Failed to set hash algorithm name"))?;
    Reflect::set(&algorithm, &"hash".into(), &hash_obj)
        .map_err(|_| TlsError::subtle_crypto("Failed to set hash in algorithm"))?;

    let key_usages = Array::new();
    key_usages.push(&"sign".into());

    let crypto_key = JsFuture::from(
        subtle
            .import_key_with_object("raw", &key_data.buffer(), &algorithm, false, &key_usages)
            .map_err(|_| TlsError::subtle_crypto("HMAC key import failed"))?,
    )
    .await
    .map_err(|_| TlsError::subtle_crypto("HMAC key import failed"))?;

    let crypto_key: CryptoKey = crypto_key.unchecked_into();
    let data_array = Uint8Array::from(data);

    let signature = JsFuture::from(
        subtle
            .sign_with_str_and_buffer_source("HMAC", &crypto_key, &data_array.buffer())
            .map_err(|_| TlsError::subtle_crypto("HMAC sign failed"))?,
    )
    .await
    .map_err(|_| TlsError::subtle_crypto("HMAC failed"))?;

    let array_buffer: ArrayBuffer = signature.unchecked_into();
    let uint8_array = Uint8Array::new(&array_buffer);
    Ok(uint8_array.to_vec())
}

/// Parse a handshake message header
pub fn parse_handshake_header(data: &[u8]) -> Result<(u8, usize)> {
    if data.len() < 4 {
        return Err(TlsError::handshake("Handshake message too short"));
    }

    let msg_type = data[0];
    let length = ((data[1] as usize) << 16) | ((data[2] as usize) << 8) | (data[3] as usize);

    Ok((msg_type, length))
}

/// Parse Certificate message
pub fn parse_certificate(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    if data.len() < 4 {
        return Err(TlsError::handshake("Certificate message too short"));
    }

    let mut pos = 0;

    // Certificate request context (should be empty for server cert)
    let context_len = data[pos] as usize;
    pos += 1;

    if pos + context_len > data.len() {
        return Err(TlsError::handshake("Certificate context overflow"));
    }
    pos += context_len;

    if pos + 3 > data.len() {
        return Err(TlsError::handshake("Certificate list length missing"));
    }

    // Certificate list length
    let list_len =
        ((data[pos] as usize) << 16) | ((data[pos + 1] as usize) << 8) | (data[pos + 2] as usize);
    pos += 3;

    let list_end = pos.saturating_add(list_len).min(data.len());
    let mut certs = Vec::new();

    while pos + 3 <= list_end && pos + 3 <= data.len() {
        // Certificate length
        let cert_len = ((data[pos] as usize) << 16)
            | ((data[pos + 1] as usize) << 8)
            | (data[pos + 2] as usize);
        pos += 3;

        if pos + cert_len > list_end || pos + cert_len > data.len() {
            return Err(TlsError::handshake("Certificate data overflow"));
        }

        certs.push(data[pos..pos + cert_len].to_vec());
        pos += cert_len;

        // Skip extensions
        if pos + 2 <= list_end && pos + 2 <= data.len() {
            let ext_len = ((data[pos] as usize) << 8) | (data[pos + 1] as usize);
            if pos + 2 + ext_len > data.len() {
                break; // Truncated extensions, stop parsing
            }
            pos += 2 + ext_len;
        }
    }

    if certs.is_empty() {
        return Err(TlsError::handshake("No certificates in message"));
    }

    debug!("Parsed {} certificates", certs.len());
    Ok(certs)
}

/// Parse CertificateVerify message
pub fn parse_certificate_verify(data: &[u8]) -> Result<(u16, Vec<u8>)> {
    if data.len() < 4 {
        return Err(TlsError::handshake("CertificateVerify too short"));
    }

    let algorithm = ((data[0] as u16) << 8) | (data[1] as u16);
    let sig_len = ((data[2] as usize) << 8) | (data[3] as usize);

    if data.len() < 4 + sig_len {
        return Err(TlsError::handshake("CertificateVerify signature truncated"));
    }

    let signature = data[4..4 + sig_len].to_vec();

    Ok((algorithm, signature))
}

/// Parse Finished message
pub fn parse_finished(data: &[u8]) -> Result<Vec<u8>> {
    // Finished message is just the verify_data
    Ok(data.to_vec())
}

/// Maximum handshake message size (matches TLS spec 24-bit length field: 0 to 2^24-1)
pub const MAX_HANDSHAKE_MESSAGE_SIZE: usize = (1 << 24) - 1;

#[cfg(kani)]
mod verification {
    use super::*;

    #[kani::proof]
    #[kani::unwind(10)]
    fn parse_handshake_header_never_panics() {
        let data: [u8; 8] = kani::any();
        let _ = parse_handshake_header(&data);
    }

    #[kani::proof]
    fn parse_handshake_header_empty_never_panics() {
        let data: [u8; 0] = [];
        let _ = parse_handshake_header(&data);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn parse_handshake_header_short_never_panics() {
        let data: [u8; 3] = kani::any();
        let _ = parse_handshake_header(&data);
    }

    #[kani::proof]
    #[kani::unwind(20)]
    fn parse_certificate_never_panics() {
        // Use smaller input to keep verification tractable
        let data: [u8; 16] = kani::any();
        let _ = parse_certificate(&data);
    }

    #[kani::proof]
    #[kani::unwind(20)]
    fn parse_certificate_verify_never_panics() {
        let data: [u8; 16] = kani::any();
        let _ = parse_certificate_verify(&data);
    }

    #[kani::proof]
    #[kani::unwind(10)]
    fn parse_finished_never_panics() {
        let data: [u8; 8] = kani::any();
        let _ = parse_finished(&data);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn handshake_header_length_bounded() {
        let data: [u8; 4] = kani::any();
        if let Ok((_ty, len)) = parse_handshake_header(&data) {
            kani::assert(len < (1 << 24), "length exceeds 24-bit max");
        }
    }

    #[kani::proof]
    #[kani::unwind(20)]
    fn certificate_parse_non_empty_on_ok() {
        // Use smaller input to keep verification tractable
        let data: [u8; 16] = kani::any();
        if let Ok(certs) = parse_certificate(&data) {
            kani::assert(!certs.is_empty(), "certs should be non-empty on Ok");
        }
    }

    #[kani::proof]
    #[kani::unwind(10)]
    fn finished_parse_preserves_length() {
        let data: [u8; 8] = kani::any();
        if let Ok(result) = parse_finished(&data) {
            kani::assert(result.len() == data.len(), "finished preserves length");
        }
    }
}
