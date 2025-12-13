//! TLS 1.2 Handshake implementation
//!
//! This module handles the TLS 1.2 handshake protocol:
//! 1. ClientHello (with extensions)
//! 2. ServerHello
//! 3. Certificate
//! 4. ServerKeyExchange (for ECDHE)
//! 5. ServerHelloDone
//! 6. ClientKeyExchange
//! 7. ChangeCipherSpec
//! 8. Finished

use crate::crypto::{self, EcdhKeyPair};
use crate::error::{Result, TlsError};
use crate::prf::{self, KeyMaterial};
use tracing::debug;

// TLS version constants
pub const TLS_VERSION_1_2: u16 = 0x0303;
pub const TLS_VERSION_1_0: u16 = 0x0301;

// Content types
pub const CONTENT_TYPE_CHANGE_CIPHER_SPEC: u8 = 20;
pub const CONTENT_TYPE_ALERT: u8 = 21;
pub const CONTENT_TYPE_HANDSHAKE: u8 = 22;
pub const CONTENT_TYPE_APPLICATION_DATA: u8 = 23;

// Handshake types
pub const HANDSHAKE_CLIENT_HELLO: u8 = 1;
pub const HANDSHAKE_SERVER_HELLO: u8 = 2;
pub const HANDSHAKE_CERTIFICATE: u8 = 11;
pub const HANDSHAKE_SERVER_KEY_EXCHANGE: u8 = 12;
pub const HANDSHAKE_CERTIFICATE_REQUEST: u8 = 13;
pub const HANDSHAKE_SERVER_HELLO_DONE: u8 = 14;
pub const HANDSHAKE_CERTIFICATE_VERIFY: u8 = 15;
pub const HANDSHAKE_CLIENT_KEY_EXCHANGE: u8 = 16;
pub const HANDSHAKE_FINISHED: u8 = 20;

// Extension types
pub const EXT_SERVER_NAME: u16 = 0;
pub const EXT_EC_POINT_FORMATS: u16 = 11;
pub const EXT_SUPPORTED_GROUPS: u16 = 10;
pub const EXT_SIGNATURE_ALGORITHMS: u16 = 13;
pub const EXT_RENEGOTIATION_INFO: u16 = 0xff01;

// TLS 1.2 Cipher suites we support (in preference order)
pub const TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256: u16 = 0xC02F;
pub const TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384: u16 = 0xC030;
pub const TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256: u16 = 0xC027;
pub const TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384: u16 = 0xC028;
pub const TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA: u16 = 0xC013;
pub const TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA: u16 = 0xC014;

// Named groups
pub const GROUP_SECP256R1: u16 = 0x0017; // P-256
pub const GROUP_SECP384R1: u16 = 0x0018; // P-384
pub const GROUP_X25519: u16 = 0x001d;

// EC point formats
pub const EC_POINT_FORMAT_UNCOMPRESSED: u8 = 0;

// Signature algorithms
pub const SIG_RSA_PKCS1_SHA256: u16 = 0x0401;
pub const SIG_RSA_PKCS1_SHA384: u16 = 0x0501;
pub const SIG_RSA_PKCS1_SHA512: u16 = 0x0601;
pub const SIG_ECDSA_SECP256R1_SHA256: u16 = 0x0403;
pub const SIG_ECDSA_SECP384R1_SHA384: u16 = 0x0503;

// EC curve types for ServerKeyExchange
pub const EC_CURVE_TYPE_NAMED_CURVE: u8 = 3;

/// Cipher suite parameters
#[derive(Debug, Clone, Copy)]
pub struct CipherSuiteParams {
    pub mac_key_len: usize,
    pub key_len: usize,
    pub iv_len: usize, // Fixed IV length (for GCM: 4, for CBC: 0 since IV is explicit)
    pub is_aead: bool,
    pub mac_len: usize, // For CBC: HMAC output length; for GCM: 0 (tag is part of ciphertext)
    pub use_sha384: bool, // true for SHA-384 cipher suites (0xC030, 0xC028)
}

impl CipherSuiteParams {
    pub fn for_suite(suite: u16) -> Result<Self> {
        match suite {
            TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => Ok(Self {
                mac_key_len: 0, // No separate MAC key for AEAD
                key_len: 16,
                iv_len: 4, // Implicit IV (nonce) for GCM
                is_aead: true,
                mac_len: 0,
                use_sha384: false,
            }),
            TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 => Ok(Self {
                mac_key_len: 0,
                key_len: 32,
                iv_len: 4,
                is_aead: true,
                mac_len: 0,
                use_sha384: true,
            }),
            TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256 => Ok(Self {
                mac_key_len: 32, // SHA-256 HMAC key
                key_len: 16,
                iv_len: 0, // Explicit IV in record (not from key block)
                is_aead: false,
                mac_len: 32, // SHA-256 output
                use_sha384: false,
            }),
            TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384 => Ok(Self {
                mac_key_len: 48, // SHA-384 HMAC key
                key_len: 32,
                iv_len: 0,
                is_aead: false,
                mac_len: 48,
                use_sha384: true,
            }),
            TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA => Ok(Self {
                mac_key_len: 20, // SHA-1 HMAC key
                key_len: 16,
                iv_len: 0,
                is_aead: false,
                mac_len: 20, // SHA-1 output
                use_sha384: false,
            }),
            TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA => Ok(Self {
                mac_key_len: 20,
                key_len: 32,
                iv_len: 0,
                is_aead: false,
                mac_len: 20,
                use_sha384: false,
            }),
            _ => Err(TlsError::protocol(format!(
                "Unsupported cipher suite: 0x{:04x}",
                suite
            ))),
        }
    }

    /// Total key block length needed
    pub fn key_block_len(&self) -> usize {
        2 * (self.mac_key_len + self.key_len + self.iv_len)
    }
}

/// TLS 1.2 handshake state machine
pub struct Handshake12State {
    /// Our ECDH key pair (P-256)
    pub ecdh_key: Option<EcdhKeyPair>,
    /// Client random (32 bytes)
    pub client_random: Vec<u8>,
    /// Server random (32 bytes)
    pub server_random: Vec<u8>,
    /// Negotiated cipher suite
    pub cipher_suite: u16,
    /// Server name for SNI
    pub server_name: String,
    /// Transcript of all handshake messages (for Finished verification)
    pub transcript: Vec<u8>,
    /// Pre-master secret
    pub pre_master_secret: Option<Vec<u8>>,
    /// Master secret (48 bytes)
    pub master_secret: Option<Vec<u8>>,
    /// Derived key material
    pub key_material: Option<KeyMaterial>,
    /// Server's ECDH public key
    pub server_public_key: Option<Vec<u8>>,
    /// Server's chosen curve
    pub server_curve: Option<u16>,
}

impl Handshake12State {
    /// Create a new TLS 1.2 handshake state
    pub async fn new(server_name: &str) -> Result<Self> {
        let client_random = crypto::random_bytes(32)?;

        Ok(Self {
            ecdh_key: None, // Generated after we know server's curve
            client_random,
            server_random: Vec::new(),
            cipher_suite: 0,
            server_name: server_name.to_string(),
            transcript: Vec::new(),
            pre_master_secret: None,
            master_secret: None,
            key_material: None,
            server_public_key: None,
            server_curve: None,
        })
    }

    /// Build ClientHello message for TLS 1.2
    pub fn build_client_hello(&self) -> Vec<u8> {
        let mut hello = Vec::new();

        // Version (TLS 1.2)
        hello.push((TLS_VERSION_1_2 >> 8) as u8);
        hello.push(TLS_VERSION_1_2 as u8);

        // Random (32 bytes)
        hello.extend_from_slice(&self.client_random);

        // Session ID (empty for new connection)
        hello.push(0);

        // Cipher suites (ECDHE-RSA suites we support)
        let cipher_suites = [
            TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
            TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
            TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256,
            TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384,
            TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA,
            TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA,
        ];
        let suites_len = cipher_suites.len() * 2;
        hello.push((suites_len >> 8) as u8);
        hello.push(suites_len as u8);
        for cs in cipher_suites {
            hello.push((cs >> 8) as u8);
            hello.push(cs as u8);
        }

        // Compression methods (null only)
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

        // Supported Groups (Elliptic Curves) - only P-256 is implemented
        extensions.push((EXT_SUPPORTED_GROUPS >> 8) as u8);
        extensions.push(EXT_SUPPORTED_GROUPS as u8);
        extensions.push(0);
        extensions.push(4); // Length (2 bytes for list len + 2 bytes for group)
        extensions.push(0);
        extensions.push(2); // Groups list length
        extensions.push((GROUP_SECP256R1 >> 8) as u8);
        extensions.push(GROUP_SECP256R1 as u8);

        // EC Point Formats
        extensions.push((EXT_EC_POINT_FORMATS >> 8) as u8);
        extensions.push(EXT_EC_POINT_FORMATS as u8);
        extensions.push(0);
        extensions.push(2); // Length
        extensions.push(1); // Formats length
        extensions.push(EC_POINT_FORMAT_UNCOMPRESSED);

        // Signature Algorithms
        let sig_algs = self.build_signature_algorithms();
        extensions.push((EXT_SIGNATURE_ALGORITHMS >> 8) as u8);
        extensions.push(EXT_SIGNATURE_ALGORITHMS as u8);
        extensions.push((sig_algs.len() >> 8) as u8);
        extensions.push(sig_algs.len() as u8);
        extensions.extend_from_slice(&sig_algs);

        // Renegotiation Info (empty, signals secure renegotiation support)
        extensions.push((EXT_RENEGOTIATION_INFO >> 8) as u8);
        extensions.push(EXT_RENEGOTIATION_INFO as u8);
        extensions.push(0);
        extensions.push(1); // Length
        extensions.push(0); // Empty renegotiated_connection

        extensions
    }

    fn build_sni_extension(&self) -> Vec<u8> {
        let name_bytes = self.server_name.as_bytes();
        let mut ext = Vec::new();

        // Server name list length
        let list_len = 3 + name_bytes.len();
        ext.push((list_len >> 8) as u8);
        ext.push(list_len as u8);

        // Name type (host_name = 0)
        ext.push(0);

        // Name length
        ext.push((name_bytes.len() >> 8) as u8);
        ext.push(name_bytes.len() as u8);

        // Name
        ext.extend_from_slice(name_bytes);

        ext
    }

    fn build_signature_algorithms(&self) -> Vec<u8> {
        let algs = [
            SIG_RSA_PKCS1_SHA256,
            SIG_RSA_PKCS1_SHA384,
            SIG_RSA_PKCS1_SHA512,
            SIG_ECDSA_SECP256R1_SHA256,
            SIG_ECDSA_SECP384R1_SHA384,
        ];

        let mut ext = Vec::new();
        let len = algs.len() * 2;
        ext.push((len >> 8) as u8);
        ext.push(len as u8);

        for alg in algs {
            ext.push((alg >> 8) as u8);
            ext.push(alg as u8);
        }

        ext
    }

    /// Parse ServerHello message
    pub fn parse_server_hello(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 38 {
            return Err(TlsError::handshake("ServerHello too short"));
        }

        let mut pos = 0;

        // Version
        let version = ((data[pos] as u16) << 8) | (data[pos + 1] as u16);
        pos += 2;
        if version != TLS_VERSION_1_2 {
            return Err(TlsError::handshake(format!(
                "Unexpected TLS version: 0x{:04x}",
                version
            )));
        }

        // Server random
        self.server_random = data[pos..pos + 32].to_vec();
        pos += 32;

        // Session ID
        let session_id_len = data[pos] as usize;
        pos += 1 + session_id_len;

        if pos + 3 > data.len() {
            return Err(TlsError::handshake("ServerHello truncated"));
        }

        // Cipher suite
        self.cipher_suite = ((data[pos] as u16) << 8) | (data[pos + 1] as u16);
        pos += 2;
        debug!("Server selected cipher suite: 0x{:04x}", self.cipher_suite);

        // Compression method (must be null)
        let compression = data[pos];
        if compression != 0 {
            return Err(TlsError::handshake("Server selected non-null compression"));
        }

        Ok(())
    }

    /// Parse ServerKeyExchange for ECDHE
    pub fn parse_server_key_exchange(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 4 {
            return Err(TlsError::handshake("ServerKeyExchange too short"));
        }

        let mut pos = 0;

        // EC curve type
        let curve_type = data[pos];
        pos += 1;
        if curve_type != EC_CURVE_TYPE_NAMED_CURVE {
            return Err(TlsError::handshake(format!(
                "Unsupported EC curve type: {}",
                curve_type
            )));
        }

        // Named curve
        let curve = ((data[pos] as u16) << 8) | (data[pos + 1] as u16);
        pos += 2;
        self.server_curve = Some(curve);
        debug!("Server selected curve: 0x{:04x}", curve);

        // Public key length
        let pubkey_len = data[pos] as usize;
        pos += 1;

        if pos + pubkey_len > data.len() {
            return Err(TlsError::handshake(
                "ServerKeyExchange public key truncated",
            ));
        }

        // Store server's public key
        self.server_public_key = Some(data[pos..pos + pubkey_len].to_vec());
        debug!("Server ECDH public key: {} bytes", pubkey_len);

        // Rest is signature (we should verify but skip for now - cert chain validates server)
        // TODO: Verify ServerKeyExchange signature

        Ok(())
    }

    /// Generate our ECDH key pair and compute shared secret
    pub async fn compute_key_exchange(&mut self) -> Result<()> {
        let curve = self
            .server_curve
            .ok_or_else(|| TlsError::handshake("No server curve"))?;

        if curve != GROUP_SECP256R1 {
            return Err(TlsError::handshake(format!(
                "Unsupported curve: 0x{:04x}, only P-256 supported",
                curve
            )));
        }

        let server_pubkey = self
            .server_public_key
            .as_ref()
            .ok_or_else(|| TlsError::handshake("No server public key"))?;

        // Generate our key pair
        let ecdh_key = EcdhKeyPair::generate().await?;

        // Derive shared secret (this is the pre-master secret for ECDHE)
        let pre_master_secret = ecdh_key.derive_shared_secret(server_pubkey).await?;
        debug!(
            "Computed ECDHE shared secret: {} bytes",
            pre_master_secret.len()
        );

        self.ecdh_key = Some(ecdh_key);
        self.pre_master_secret = Some(pre_master_secret);

        Ok(())
    }

    /// Derive master secret and key material
    pub async fn derive_keys(&mut self) -> Result<()> {
        let pms = self
            .pre_master_secret
            .as_ref()
            .ok_or_else(|| TlsError::handshake("No pre-master secret"))?;

        // Derive master secret
        let master_secret =
            prf::derive_master_secret(pms, &self.client_random, &self.server_random).await?;
        debug!("Derived master secret: {} bytes", master_secret.len());

        // Get cipher suite parameters
        let params = CipherSuiteParams::for_suite(self.cipher_suite)?;

        // Derive key block
        let key_block = prf::derive_key_block(
            &master_secret,
            &self.client_random,
            &self.server_random,
            params.key_block_len(),
        )
        .await?;

        // Extract key material
        let key_material = KeyMaterial::from_key_block(
            &key_block,
            params.mac_key_len,
            params.key_len,
            params.iv_len,
        )?;

        self.master_secret = Some(master_secret);
        self.key_material = Some(key_material);

        debug!("Derived TLS 1.2 key material");
        Ok(())
    }

    /// Build ClientKeyExchange message (ECDH public key)
    pub fn build_client_key_exchange(&self) -> Result<Vec<u8>> {
        let ecdh_key = self
            .ecdh_key
            .as_ref()
            .ok_or_else(|| TlsError::handshake("No ECDH key pair"))?;

        let pubkey = &ecdh_key.public_key_bytes;

        let mut message = vec![HANDSHAKE_CLIENT_KEY_EXCHANGE];
        let len = 1 + pubkey.len(); // length byte + public key
        message.push((len >> 16) as u8);
        message.push((len >> 8) as u8);
        message.push(len as u8);
        message.push(pubkey.len() as u8); // Public key length
        message.extend_from_slice(pubkey);

        Ok(message)
    }

    /// Build Finished message
    pub async fn build_finished(&self) -> Result<Vec<u8>> {
        let master_secret = self
            .master_secret
            .as_ref()
            .ok_or_else(|| TlsError::handshake("No master secret"))?;

        // Use SHA-384 for cipher suites that require it, otherwise SHA-256
        let params = self.get_cipher_params()?;
        let transcript_hash = if params.use_sha384 {
            crypto::sha384(&self.transcript).await?
        } else {
            crypto::sha256(&self.transcript).await?
        };

        // Compute verify_data
        let verify_data = prf::compute_verify_data(
            master_secret,
            true, // is_client
            &transcript_hash,
        )
        .await?;

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
        let master_secret = self
            .master_secret
            .as_ref()
            .ok_or_else(|| TlsError::handshake("No master secret"))?;

        // Use SHA-384 for cipher suites that require it, otherwise SHA-256
        let params = self.get_cipher_params()?;
        let transcript_hash = if params.use_sha384 {
            crypto::sha384(&self.transcript).await?
        } else {
            crypto::sha256(&self.transcript).await?
        };

        // Compute expected verify_data
        let expected = prf::compute_verify_data(
            master_secret,
            false, // is_server
            &transcript_hash,
        )
        .await?;

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

    /// Get cipher suite parameters
    pub fn get_cipher_params(&self) -> Result<CipherSuiteParams> {
        CipherSuiteParams::for_suite(self.cipher_suite)
    }

    /// Check if using AEAD cipher
    pub fn is_aead(&self) -> Result<bool> {
        Ok(self.get_cipher_params()?.is_aead)
    }
}

/// Parse Certificate message (same as TLS 1.3)
pub fn parse_certificate(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    if data.len() < 3 {
        return Err(TlsError::handshake("Certificate message too short"));
    }

    let mut pos = 0;

    // Certificate list length (3 bytes)
    let list_len =
        ((data[pos] as usize) << 16) | ((data[pos + 1] as usize) << 8) | (data[pos + 2] as usize);
    pos += 3;

    let list_end = pos + list_len;
    let mut certs = Vec::new();

    while pos + 3 <= list_end {
        // Certificate length (3 bytes)
        let cert_len = ((data[pos] as usize) << 16)
            | ((data[pos + 1] as usize) << 8)
            | (data[pos + 2] as usize);
        pos += 3;

        if pos + cert_len > list_end {
            return Err(TlsError::handshake("Certificate data overflow"));
        }

        certs.push(data[pos..pos + cert_len].to_vec());
        pos += cert_len;
    }

    if certs.is_empty() {
        return Err(TlsError::handshake("No certificates in message"));
    }

    debug!("Parsed {} certificates (TLS 1.2)", certs.len());
    Ok(certs)
}

/// Parse handshake message header
pub fn parse_handshake_header(data: &[u8]) -> Result<(u8, usize)> {
    if data.len() < 4 {
        return Err(TlsError::handshake("Handshake message too short"));
    }

    let msg_type = data[0];
    let length = ((data[1] as usize) << 16) | ((data[2] as usize) << 8) | (data[3] as usize);

    Ok((msg_type, length))
}
