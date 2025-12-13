//! Cryptographic operations using SubtleCrypto API and pure Rust
//!
//! This module provides cryptographic primitives for TLS 1.3:
//! - ECDH key exchange (P-256 via SubtleCrypto, X25519 via pure Rust)
//! - AES-GCM encryption (via SubtleCrypto)
//! - ChaCha20-Poly1305 encryption (via pure Rust - not in SubtleCrypto)
//! - HKDF key derivation
//! - SHA-256/SHA-384 hashing

use crate::error::{Result, TlsError};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{CryptoKey, SubtleCrypto};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey};

/// Get the SubtleCrypto instance from the runtime environment.
/// Supports both browser (window.crypto) and Node.js (globalThis.crypto).
pub fn get_subtle_crypto() -> Result<SubtleCrypto> {
    // First try web_sys::window() for browser environments
    if let Some(window) = web_sys::window() {
        if let Ok(crypto) = window.crypto() {
            return Ok(crypto.subtle());
        }
    }

    // Fall back to globalThis.crypto for Node.js
    let global = js_sys::global();
    let crypto = Reflect::get(&global, &"crypto".into())
        .map_err(|_| TlsError::subtle_crypto("No crypto object in globalThis"))?;

    if crypto.is_undefined() {
        return Err(TlsError::subtle_crypto("globalThis.crypto is undefined"));
    }

    let subtle = Reflect::get(&crypto, &"subtle".into())
        .map_err(|_| TlsError::subtle_crypto("No subtle property on crypto"))?;

    if subtle.is_undefined() {
        return Err(TlsError::subtle_crypto("crypto.subtle is undefined"));
    }

    Ok(subtle.unchecked_into())
}

/// Generate random bytes
pub fn random_bytes(len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    getrandom::getrandom(&mut buf)
        .map_err(|e| TlsError::crypto(format!("Failed to generate random bytes: {}", e)))?;
    Ok(buf)
}

/// ECDH key pair for key exchange
pub struct EcdhKeyPair {
    pub private_key: CryptoKey,
    pub public_key: CryptoKey,
    pub public_key_bytes: Vec<u8>,
}

impl EcdhKeyPair {
    /// Generate a new P-256 ECDH key pair
    pub async fn generate() -> Result<Self> {
        let subtle = get_subtle_crypto()?;

        // Create algorithm object for P-256 ECDH
        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"ECDH".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(&algorithm, &"namedCurve".into(), &"P-256".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set curve"))?;

        // Key usages
        let key_usages = Array::new();
        key_usages.push(&"deriveBits".into());

        // Generate key pair
        let key_pair = JsFuture::from(
            subtle
                .generate_key_with_object(&algorithm, true, &key_usages)
                .map_err(|e| TlsError::subtle_crypto(format!("Failed to generate key: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("Key generation failed: {:?}", e)))?;

        // Extract public and private keys
        let private_key: CryptoKey = Reflect::get(&key_pair, &"privateKey".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to get private key"))?
            .unchecked_into();

        let public_key: CryptoKey = Reflect::get(&key_pair, &"publicKey".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to get public key"))?
            .unchecked_into();

        // Export public key in uncompressed format for TLS
        let public_key_bytes = Self::export_public_key(&subtle, &public_key).await?;

        Ok(Self {
            private_key,
            public_key,
            public_key_bytes,
        })
    }

    /// Export public key as uncompressed point (0x04 || x || y)
    async fn export_public_key(subtle: &SubtleCrypto, key: &CryptoKey) -> Result<Vec<u8>> {
        let exported = JsFuture::from(
            subtle
                .export_key("raw", key)
                .map_err(|e| TlsError::subtle_crypto(format!("Failed to export key: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("Key export failed: {:?}", e)))?;

        let array_buffer: ArrayBuffer = exported.unchecked_into();
        let uint8_array = Uint8Array::new(&array_buffer);
        Ok(uint8_array.to_vec())
    }

    /// Derive shared secret from peer's public key
    pub async fn derive_shared_secret(&self, peer_public_key_bytes: &[u8]) -> Result<Vec<u8>> {
        let subtle = get_subtle_crypto()?;

        // Import peer's public key
        let peer_key = Self::import_public_key(&subtle, peer_public_key_bytes).await?;

        // Create ECDH derive params
        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"ECDH".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(&algorithm, &"public".into(), &peer_key)
            .map_err(|_| TlsError::subtle_crypto("Failed to set public key"))?;

        // Derive 32 bytes (256 bits) shared secret
        let shared_secret = JsFuture::from(
            subtle
                .derive_bits_with_object(&algorithm, &self.private_key, 256)
                .map_err(|e| TlsError::subtle_crypto(format!("Failed to derive bits: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("Derive bits failed: {:?}", e)))?;

        let array_buffer: ArrayBuffer = shared_secret.unchecked_into();
        let uint8_array = Uint8Array::new(&array_buffer);
        Ok(uint8_array.to_vec())
    }

    /// Import a public key from raw bytes
    async fn import_public_key(subtle: &SubtleCrypto, bytes: &[u8]) -> Result<CryptoKey> {
        let key_data = Uint8Array::from(bytes);

        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"ECDH".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(&algorithm, &"namedCurve".into(), &"P-256".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set curve"))?;

        let key_usages = Array::new();

        let key = JsFuture::from(
            subtle
                .import_key_with_object("raw", &key_data.buffer(), &algorithm, true, &key_usages)
                .map_err(|e| TlsError::subtle_crypto(format!("Failed to import key: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("Key import failed: {:?}", e)))?;

        Ok(key.unchecked_into())
    }
}

/// X25519 key pair for key exchange (pure Rust, no SubtleCrypto needed)
pub struct X25519KeyPair {
    secret: EphemeralSecret,
    pub public_key_bytes: Vec<u8>,
}

impl X25519KeyPair {
    /// Generate a new X25519 key pair
    pub fn generate() -> Result<Self> {
        let mut rng_bytes = [0u8; 32];
        getrandom::getrandom(&mut rng_bytes)
            .map_err(|e| TlsError::crypto(format!("Failed to generate random bytes: {}", e)))?;

        // Create secret from random bytes
        let secret = EphemeralSecret::random_from_rng(&mut rand::rngs::OsRng);
        let public = X25519PublicKey::from(&secret);

        Ok(Self {
            secret,
            public_key_bytes: public.as_bytes().to_vec(),
        })
    }

    /// Derive shared secret from peer's public key
    pub fn derive_shared_secret(self, peer_public_key_bytes: &[u8]) -> Result<Vec<u8>> {
        tracing::info!(
            "X25519 derive_shared_secret: peer key {} bytes",
            peer_public_key_bytes.len()
        );
        if peer_public_key_bytes.len() != 32 {
            return Err(TlsError::crypto("X25519 public key must be 32 bytes"));
        }

        let mut peer_bytes = [0u8; 32];
        peer_bytes.copy_from_slice(peer_public_key_bytes);
        tracing::info!("X25519: creating peer public key");
        let peer_public = X25519PublicKey::from(peer_bytes);

        tracing::info!("X25519: computing diffie_hellman");
        let shared_secret = self.secret.diffie_hellman(&peer_public);
        tracing::info!(
            "X25519: diffie_hellman complete, {} bytes",
            shared_secret.as_bytes().len()
        );
        Ok(shared_secret.as_bytes().to_vec())
    }
}

/// Unified key exchange that supports both P-256 and X25519
pub enum KeyExchange {
    P256(EcdhKeyPair),
    X25519(X25519KeyPair),
}

impl KeyExchange {
    /// Generate a P-256 key pair
    pub async fn generate_p256() -> Result<Self> {
        Ok(KeyExchange::P256(EcdhKeyPair::generate().await?))
    }

    /// Generate an X25519 key pair
    pub fn generate_x25519() -> Result<Self> {
        Ok(KeyExchange::X25519(X25519KeyPair::generate()?))
    }

    /// Get the public key bytes
    pub fn public_key_bytes(&self) -> &[u8] {
        match self {
            KeyExchange::P256(kp) => &kp.public_key_bytes,
            KeyExchange::X25519(kp) => &kp.public_key_bytes,
        }
    }

    /// Get the named group code for TLS
    pub fn named_group(&self) -> u16 {
        match self {
            KeyExchange::P256(_) => 0x0017,   // secp256r1
            KeyExchange::X25519(_) => 0x001d, // x25519
        }
    }
}

/// AES-GCM cipher for record encryption
pub struct AesGcm {
    key: CryptoKey,
    #[allow(dead_code)]
    key_size: usize,
}

impl AesGcm {
    /// Create AES-128-GCM cipher from key bytes
    pub async fn new_128(key_bytes: &[u8]) -> Result<Self> {
        if key_bytes.len() != 16 {
            return Err(TlsError::crypto("AES-128-GCM requires 16-byte key"));
        }
        Self::new(key_bytes, 128).await
    }

    /// Create AES-256-GCM cipher from key bytes
    pub async fn new_256(key_bytes: &[u8]) -> Result<Self> {
        if key_bytes.len() != 32 {
            return Err(TlsError::crypto("AES-256-GCM requires 32-byte key"));
        }
        Self::new(key_bytes, 256).await
    }

    async fn new(key_bytes: &[u8], bits: usize) -> Result<Self> {
        let subtle = get_subtle_crypto()?;
        let key_data = Uint8Array::from(key_bytes);

        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"AES-GCM".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(
            &algorithm,
            &"length".into(),
            &JsValue::from_f64(bits as f64),
        )
        .map_err(|_| TlsError::subtle_crypto("Failed to set key length"))?;

        let key_usages = Array::new();
        key_usages.push(&"encrypt".into());
        key_usages.push(&"decrypt".into());

        let key = JsFuture::from(
            subtle
                .import_key_with_object("raw", &key_data.buffer(), &algorithm, false, &key_usages)
                .map_err(|e| TlsError::subtle_crypto(format!("Failed to import key: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("Key import failed: {:?}", e)))?;

        Ok(Self {
            key: key.unchecked_into(),
            key_size: bits / 8,
        })
    }

    /// Encrypt plaintext with the given nonce and additional data
    /// Returns ciphertext with 16-byte authentication tag appended
    pub async fn encrypt(&self, nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        if nonce.len() != 12 {
            return Err(TlsError::crypto("AES-GCM requires 12-byte nonce"));
        }

        let subtle = get_subtle_crypto()?;
        let nonce_array = Uint8Array::from(nonce);
        let aad_array = Uint8Array::from(aad);
        let plaintext_array = Uint8Array::from(plaintext);

        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"AES-GCM".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(&algorithm, &"iv".into(), &nonce_array)
            .map_err(|_| TlsError::subtle_crypto("Failed to set iv"))?;
        Reflect::set(&algorithm, &"additionalData".into(), &aad_array)
            .map_err(|_| TlsError::subtle_crypto("Failed to set additionalData"))?;
        Reflect::set(&algorithm, &"tagLength".into(), &JsValue::from_f64(128.0))
            .map_err(|_| TlsError::subtle_crypto("Failed to set tagLength"))?;

        let ciphertext = JsFuture::from(
            subtle
                .encrypt_with_object_and_buffer_source(
                    &algorithm,
                    &self.key,
                    &plaintext_array.buffer(),
                )
                .map_err(|e| TlsError::subtle_crypto(format!("Encryption failed: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("Encryption failed: {:?}", e)))?;

        let array_buffer: ArrayBuffer = ciphertext.unchecked_into();
        let uint8_array = Uint8Array::new(&array_buffer);
        Ok(uint8_array.to_vec())
    }

    /// Decrypt ciphertext (with tag appended) using the given nonce and additional data
    pub async fn decrypt(&self, nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        tracing::info!(
            "AesGcm::decrypt called, ciphertext len={}",
            ciphertext.len()
        );
        if nonce.len() != 12 {
            return Err(TlsError::crypto("AES-GCM requires 12-byte nonce"));
        }
        if ciphertext.len() < 16 {
            return Err(TlsError::crypto("Ciphertext too short (missing tag)"));
        }

        tracing::info!("AesGcm::decrypt: getting SubtleCrypto");
        let subtle = get_subtle_crypto()?;
        let nonce_array = Uint8Array::from(nonce);
        let aad_array = Uint8Array::from(aad);
        let ciphertext_array = Uint8Array::from(ciphertext);

        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"AES-GCM".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(&algorithm, &"iv".into(), &nonce_array)
            .map_err(|_| TlsError::subtle_crypto("Failed to set iv"))?;
        Reflect::set(&algorithm, &"additionalData".into(), &aad_array)
            .map_err(|_| TlsError::subtle_crypto("Failed to set additionalData"))?;
        Reflect::set(&algorithm, &"tagLength".into(), &JsValue::from_f64(128.0))
            .map_err(|_| TlsError::subtle_crypto("Failed to set tagLength"))?;

        tracing::info!("AesGcm::decrypt: calling subtle.decrypt");
        let promise: js_sys::Promise = subtle
            .decrypt_with_object_and_buffer_source(
                &algorithm,
                &self.key,
                &ciphertext_array.buffer(),
            )
            .map_err(|e| TlsError::subtle_crypto(format!("Decryption failed: {:?}", e)))?;

        tracing::info!("AesGcm::decrypt: awaiting promise via JsFuture");

        // The key insight: we need to let the JavaScript event loop run
        // JsFuture::from(promise).await should work if the executor properly yields
        let plaintext = JsFuture::from(promise).await.map_err(|e| {
            tracing::error!("AesGcm::decrypt: JsFuture error: {:?}", e);
            TlsError::crypto(format!("Decryption failed (bad tag?): {:?}", e))
        })?;
        tracing::info!("AesGcm::decrypt: decryption completed");

        let array_buffer: ArrayBuffer = plaintext.unchecked_into();
        let uint8_array = Uint8Array::new(&array_buffer);
        tracing::info!("AesGcm::decrypt: returning {} bytes", uint8_array.length());
        Ok(uint8_array.to_vec())
    }
}

/// AES-CBC cipher for TLS 1.2 record encryption
/// Note: CBC mode requires separate MAC (not AEAD)
#[cfg(feature = "tls12")]
pub struct AesCbc {
    key: CryptoKey,
    #[allow(dead_code)]
    key_size: usize,
}

#[cfg(feature = "tls12")]
impl AesCbc {
    /// Create AES-128-CBC cipher from key bytes
    pub async fn new_128(key_bytes: &[u8]) -> Result<Self> {
        if key_bytes.len() != 16 {
            return Err(TlsError::crypto("AES-128-CBC requires 16-byte key"));
        }
        Self::new(key_bytes, 128).await
    }

    /// Create AES-256-CBC cipher from key bytes
    pub async fn new_256(key_bytes: &[u8]) -> Result<Self> {
        if key_bytes.len() != 32 {
            return Err(TlsError::crypto("AES-256-CBC requires 32-byte key"));
        }
        Self::new(key_bytes, 256).await
    }

    async fn new(key_bytes: &[u8], bits: usize) -> Result<Self> {
        let subtle = get_subtle_crypto()?;
        let key_data = Uint8Array::from(key_bytes);

        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"AES-CBC".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(
            &algorithm,
            &"length".into(),
            &JsValue::from_f64(bits as f64),
        )
        .map_err(|_| TlsError::subtle_crypto("Failed to set key length"))?;

        let key_usages = Array::new();
        key_usages.push(&"encrypt".into());
        key_usages.push(&"decrypt".into());

        let key = JsFuture::from(
            subtle
                .import_key_with_object("raw", &key_data.buffer(), &algorithm, false, &key_usages)
                .map_err(|e| TlsError::subtle_crypto(format!("Failed to import key: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("Key import failed: {:?}", e)))?;

        Ok(Self {
            key: key.unchecked_into(),
            key_size: bits / 8,
        })
    }

    /// Encrypt plaintext with the given IV
    /// SubtleCrypto AES-CBC uses PKCS#7 padding automatically
    pub async fn encrypt(&self, iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        if iv.len() != 16 {
            return Err(TlsError::crypto("AES-CBC requires 16-byte IV"));
        }

        let subtle = get_subtle_crypto()?;
        let iv_array = Uint8Array::from(iv);
        let plaintext_array = Uint8Array::from(plaintext);

        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"AES-CBC".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(&algorithm, &"iv".into(), &iv_array)
            .map_err(|_| TlsError::subtle_crypto("Failed to set iv"))?;

        let ciphertext = JsFuture::from(
            subtle
                .encrypt_with_object_and_buffer_source(
                    &algorithm,
                    &self.key,
                    &plaintext_array.buffer(),
                )
                .map_err(|e| TlsError::subtle_crypto(format!("Encryption failed: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("Encryption failed: {:?}", e)))?;

        let array_buffer: ArrayBuffer = ciphertext.unchecked_into();
        let uint8_array = Uint8Array::new(&array_buffer);
        Ok(uint8_array.to_vec())
    }

    /// Decrypt ciphertext with the given IV
    /// SubtleCrypto AES-CBC removes PKCS#7 padding automatically
    pub async fn decrypt(&self, iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        if iv.len() != 16 {
            return Err(TlsError::crypto("AES-CBC requires 16-byte IV"));
        }
        if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
            return Err(TlsError::crypto(
                "AES-CBC ciphertext must be multiple of 16 bytes",
            ));
        }

        let subtle = get_subtle_crypto()?;
        let iv_array = Uint8Array::from(iv);
        let ciphertext_array = Uint8Array::from(ciphertext);

        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"AES-CBC".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
        Reflect::set(&algorithm, &"iv".into(), &iv_array)
            .map_err(|_| TlsError::subtle_crypto("Failed to set iv"))?;

        let plaintext = JsFuture::from(
            subtle
                .decrypt_with_object_and_buffer_source(
                    &algorithm,
                    &self.key,
                    &ciphertext_array.buffer(),
                )
                .map_err(|e| TlsError::subtle_crypto(format!("Decryption failed: {:?}", e)))?,
        )
        .await
        .map_err(|e| {
            TlsError::subtle_crypto(format!("Decryption failed (bad padding?): {:?}", e))
        })?;

        let array_buffer: ArrayBuffer = plaintext.unchecked_into();
        let uint8_array = Uint8Array::new(&array_buffer);
        Ok(uint8_array.to_vec())
    }
}

/// ChaCha20-Poly1305 cipher for record encryption (pure Rust)
/// This is used when the server negotiates TLS_CHACHA20_POLY1305_SHA256
pub struct ChaCha20Poly1305Cipher {
    cipher: ChaCha20Poly1305,
}

impl ChaCha20Poly1305Cipher {
    /// Create a new ChaCha20-Poly1305 cipher from a 32-byte key
    pub fn new(key_bytes: &[u8]) -> Result<Self> {
        if key_bytes.len() != 32 {
            return Err(TlsError::crypto("ChaCha20-Poly1305 requires 32-byte key"));
        }

        let key = chacha20poly1305::Key::from_slice(key_bytes);
        let cipher = ChaCha20Poly1305::new(key);

        Ok(Self { cipher })
    }

    /// Encrypt plaintext with the given nonce and additional data
    /// Returns ciphertext with 16-byte authentication tag appended
    pub fn encrypt(&self, nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        if nonce.len() != 12 {
            return Err(TlsError::crypto("ChaCha20-Poly1305 requires 12-byte nonce"));
        }

        let nonce = Nonce::from_slice(nonce);
        let payload = Payload {
            msg: plaintext,
            aad,
        };

        self.cipher
            .encrypt(nonce, payload)
            .map_err(|e| TlsError::crypto(format!("ChaCha20-Poly1305 encryption failed: {}", e)))
    }

    /// Decrypt ciphertext (with tag appended) using the given nonce and additional data
    pub fn decrypt(&self, nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        if nonce.len() != 12 {
            return Err(TlsError::crypto("ChaCha20-Poly1305 requires 12-byte nonce"));
        }
        if ciphertext.len() < 16 {
            return Err(TlsError::crypto("Ciphertext too short (missing tag)"));
        }

        let nonce = Nonce::from_slice(nonce);
        let payload = Payload {
            msg: ciphertext,
            aad,
        };

        self.cipher
            .decrypt(nonce, payload)
            .map_err(|e| TlsError::crypto(format!("ChaCha20-Poly1305 decryption failed: {}", e)))
    }
}

/// Unified cipher interface supporting multiple AEAD algorithms
pub enum Cipher {
    Aes128Gcm(AesGcm),
    Aes256Gcm(AesGcm),
    ChaCha20Poly1305(ChaCha20Poly1305Cipher),
}

impl Cipher {
    /// Create AES-128-GCM cipher
    pub async fn aes_128_gcm(key: &[u8]) -> Result<Self> {
        Ok(Cipher::Aes128Gcm(AesGcm::new_128(key).await?))
    }

    /// Create AES-256-GCM cipher
    pub async fn aes_256_gcm(key: &[u8]) -> Result<Self> {
        Ok(Cipher::Aes256Gcm(AesGcm::new_256(key).await?))
    }

    /// Create ChaCha20-Poly1305 cipher
    pub fn chacha20_poly1305(key: &[u8]) -> Result<Self> {
        Ok(Cipher::ChaCha20Poly1305(ChaCha20Poly1305Cipher::new(key)?))
    }

    /// Encrypt with the cipher
    pub async fn encrypt(&self, nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        match self {
            Cipher::Aes128Gcm(c) | Cipher::Aes256Gcm(c) => c.encrypt(nonce, aad, plaintext).await,
            Cipher::ChaCha20Poly1305(c) => c.encrypt(nonce, aad, plaintext),
        }
    }

    /// Decrypt with the cipher
    pub async fn decrypt(&self, nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        match self {
            Cipher::Aes128Gcm(c) | Cipher::Aes256Gcm(c) => c.decrypt(nonce, aad, ciphertext).await,
            Cipher::ChaCha20Poly1305(c) => c.decrypt(nonce, aad, ciphertext),
        }
    }

    /// Encrypt synchronously (only works for ChaCha20-Poly1305)
    pub fn encrypt_sync(&self, nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        match self {
            Cipher::ChaCha20Poly1305(c) => c.encrypt(nonce, aad, plaintext),
            _ => Err(TlsError::crypto(
                "Synchronous encryption only supported for ChaCha20-Poly1305",
            )),
        }
    }

    /// Decrypt synchronously (only works for ChaCha20-Poly1305)
    pub fn decrypt_sync(&self, nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        match self {
            Cipher::ChaCha20Poly1305(c) => c.decrypt(nonce, aad, ciphertext),
            _ => Err(TlsError::crypto(
                "Synchronous decryption only supported for ChaCha20-Poly1305",
            )),
        }
    }

    /// Check if this cipher supports synchronous operations
    pub fn supports_sync(&self) -> bool {
        matches!(self, Cipher::ChaCha20Poly1305(_))
    }

    /// Get the key size for this cipher
    pub fn key_size(&self) -> usize {
        match self {
            Cipher::Aes128Gcm(_) => 16,
            Cipher::Aes256Gcm(_) => 32,
            Cipher::ChaCha20Poly1305(_) => 32,
        }
    }

    /// Get the IV/nonce size for this cipher
    pub fn iv_size(&self) -> usize {
        12 // All TLS 1.3 AEAD ciphers use 12-byte nonce
    }
}

/// HKDF for key derivation
pub struct Hkdf;

impl Hkdf {
    /// HKDF-Extract: Extract a pseudorandom key from input keying material
    pub async fn extract(salt: &[u8], ikm: &[u8]) -> Result<Vec<u8>> {
        // HKDF-Extract is HMAC(salt, ikm)
        // Per RFC 5869: if salt is not provided, it is set to a string of HashLen zeros
        let effective_salt = if salt.is_empty() {
            vec![0u8; 32] // SHA-256 hash length
        } else {
            salt.to_vec()
        };
        tracing::trace!(
            "HKDF-Extract: salt_len={}, ikm_len={}",
            effective_salt.len(),
            ikm.len()
        );
        Self::hmac_sha256(&effective_salt, ikm).await
    }

    /// HKDF-Expand: Expand a pseudorandom key to desired length
    pub async fn expand(prk: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>> {
        let hash_len = 32; // SHA-256
        let n = (length + hash_len - 1) / hash_len;

        if n > 255 {
            return Err(TlsError::crypto("HKDF output too long"));
        }

        let mut okm = Vec::with_capacity(length);
        let mut t = Vec::new();

        for i in 1..=n {
            let mut data = t.clone();
            data.extend_from_slice(info);
            data.push(i as u8);
            t = Self::hmac_sha256(prk, &data).await?;
            okm.extend_from_slice(&t);
        }

        okm.truncate(length);
        Ok(okm)
    }

    /// HKDF-Expand-Label for TLS 1.3
    pub async fn expand_label(
        secret: &[u8],
        label: &str,
        context: &[u8],
        length: usize,
    ) -> Result<Vec<u8>> {
        // HkdfLabel = struct {
        //     uint16 length = Length;
        //     opaque label<7..255> = "tls13 " + Label;
        //     opaque context<0..255> = Context;
        // }
        let full_label = format!("tls13 {}", label);
        let label_bytes = full_label.as_bytes();

        let mut hkdf_label = Vec::new();
        // Length (2 bytes, big endian)
        hkdf_label.push((length >> 8) as u8);
        hkdf_label.push(length as u8);
        // Label length + label
        hkdf_label.push(label_bytes.len() as u8);
        hkdf_label.extend_from_slice(label_bytes);
        // Context length + context
        hkdf_label.push(context.len() as u8);
        hkdf_label.extend_from_slice(context);

        Self::expand(secret, &hkdf_label, length).await
    }

    /// Derive-Secret for TLS 1.3
    pub async fn derive_secret(
        secret: &[u8],
        label: &str,
        messages_hash: &[u8],
    ) -> Result<Vec<u8>> {
        Self::expand_label(secret, label, messages_hash, 32).await
    }

    /// HMAC-SHA256
    async fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
        let subtle = get_subtle_crypto()?;

        // Import key for HMAC
        let key_data = Uint8Array::from(key);
        let algorithm = Object::new();
        Reflect::set(&algorithm, &"name".into(), &"HMAC".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

        let hash_obj = Object::new();
        Reflect::set(&hash_obj, &"name".into(), &"SHA-256".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
        Reflect::set(&algorithm, &"hash".into(), &hash_obj)
            .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;

        let key_usages = Array::new();
        key_usages.push(&"sign".into());

        let crypto_key = JsFuture::from(
            subtle
                .import_key_with_object("raw", &key_data.buffer(), &algorithm, false, &key_usages)
                .map_err(|e| {
                    TlsError::subtle_crypto(format!("Failed to import HMAC key: {:?}", e))
                })?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("HMAC key import failed: {:?}", e)))?;

        let crypto_key: CryptoKey = crypto_key.unchecked_into();

        // Sign (compute HMAC)
        let data_array = Uint8Array::from(data);
        let signature = JsFuture::from(
            subtle
                .sign_with_str_and_buffer_source("HMAC", &crypto_key, &data_array.buffer())
                .map_err(|e| TlsError::subtle_crypto(format!("HMAC sign failed: {:?}", e)))?,
        )
        .await
        .map_err(|e| TlsError::subtle_crypto(format!("HMAC computation failed: {:?}", e)))?;

        let array_buffer: ArrayBuffer = signature.unchecked_into();
        let uint8_array = Uint8Array::new(&array_buffer);
        Ok(uint8_array.to_vec())
    }
}

/// SHA-256 hash function
pub async fn sha256(data: &[u8]) -> Result<Vec<u8>> {
    let subtle = get_subtle_crypto()?;
    let data_array = Uint8Array::from(data);

    let hash = JsFuture::from(
        subtle
            .digest_with_str_and_buffer_source("SHA-256", &data_array.buffer())
            .map_err(|e| TlsError::subtle_crypto(format!("SHA-256 failed: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("SHA-256 failed: {:?}", e)))?;

    let array_buffer: ArrayBuffer = hash.unchecked_into();
    let uint8_array = Uint8Array::new(&array_buffer);
    Ok(uint8_array.to_vec())
}

/// SHA-384 hash function
pub async fn sha384(data: &[u8]) -> Result<Vec<u8>> {
    let subtle = get_subtle_crypto()?;
    let data_array = Uint8Array::from(data);

    let hash = JsFuture::from(
        subtle
            .digest_with_str_and_buffer_source("SHA-384", &data_array.buffer())
            .map_err(|e| TlsError::subtle_crypto(format!("SHA-384 failed: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("SHA-384 failed: {:?}", e)))?;

    let array_buffer: ArrayBuffer = hash.unchecked_into();
    let uint8_array = Uint8Array::new(&array_buffer);
    Ok(uint8_array.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_random_bytes() {
        let bytes = random_bytes(32).unwrap();
        assert_eq!(bytes.len(), 32);
    }

    #[wasm_bindgen_test]
    async fn test_ecdh_key_generation() {
        let key_pair = EcdhKeyPair::generate().await.unwrap();
        // P-256 uncompressed point is 65 bytes (0x04 || x || y)
        assert_eq!(key_pair.public_key_bytes.len(), 65);
        assert_eq!(key_pair.public_key_bytes[0], 0x04);
    }

    #[wasm_bindgen_test]
    async fn test_ecdh_key_exchange() {
        let alice = EcdhKeyPair::generate().await.unwrap();
        let bob = EcdhKeyPair::generate().await.unwrap();

        let alice_secret = alice
            .derive_shared_secret(&bob.public_key_bytes)
            .await
            .unwrap();
        let bob_secret = bob
            .derive_shared_secret(&alice.public_key_bytes)
            .await
            .unwrap();

        assert_eq!(alice_secret, bob_secret);
        assert_eq!(alice_secret.len(), 32);
    }

    #[wasm_bindgen_test]
    async fn test_aes_gcm_128() {
        let key = vec![0u8; 16];
        let cipher = AesGcm::new_128(&key).await.unwrap();

        let nonce = vec![0u8; 12];
        let aad = b"additional data";
        let plaintext = b"Hello, World!";

        let ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        assert!(ciphertext.len() > plaintext.len()); // Has tag appended

        let decrypted = cipher.decrypt(&nonce, aad, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_sha256() {
        let hash = sha256(b"hello").await.unwrap();
        assert_eq!(hash.len(), 32);
    }
}
