//! TLS 1.2 Pseudo-Random Function (PRF)
//!
//! The TLS 1.2 PRF is defined in RFC 5246 Section 5:
//! PRF(secret, label, seed) = P_SHA256(secret, label + seed)
//!
//! P_SHA256(secret, seed) = HMAC_SHA256(secret, A(1) + seed) +
//!                          HMAC_SHA256(secret, A(2) + seed) + ...
//! where A(0) = seed, A(i) = HMAC_SHA256(secret, A(i-1))

use crate::crypto::get_subtle_crypto;
use crate::error::{Result, TlsError};
use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::CryptoKey;

/// HMAC-SHA256 for PRF
async fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let subtle = get_subtle_crypto()?;

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
            .map_err(|e| TlsError::subtle_crypto(format!("Failed to import HMAC key: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("HMAC key import failed: {:?}", e)))?;

    let crypto_key: CryptoKey = crypto_key.unchecked_into();

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

/// HMAC-SHA384 for SHA-384 cipher suites
async fn hmac_sha384(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let subtle = get_subtle_crypto()?;

    let key_data = Uint8Array::from(key);
    let algorithm = Object::new();
    Reflect::set(&algorithm, &"name".into(), &"HMAC".into())
        .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

    let hash_obj = Object::new();
    Reflect::set(&hash_obj, &"name".into(), &"SHA-384".into())
        .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
    Reflect::set(&algorithm, &"hash".into(), &hash_obj)
        .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;

    let key_usages = Array::new();
    key_usages.push(&"sign".into());

    let crypto_key = JsFuture::from(
        subtle
            .import_key_with_object("raw", &key_data.buffer(), &algorithm, false, &key_usages)
            .map_err(|e| TlsError::subtle_crypto(format!("Failed to import HMAC key: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("HMAC key import failed: {:?}", e)))?;

    let crypto_key: CryptoKey = crypto_key.unchecked_into();

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

/// HMAC-SHA1 for older cipher suites
async fn hmac_sha1(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let subtle = get_subtle_crypto()?;

    let key_data = Uint8Array::from(key);
    let algorithm = Object::new();
    Reflect::set(&algorithm, &"name".into(), &"HMAC".into())
        .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

    let hash_obj = Object::new();
    Reflect::set(&hash_obj, &"name".into(), &"SHA-1".into())
        .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
    Reflect::set(&algorithm, &"hash".into(), &hash_obj)
        .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;

    let key_usages = Array::new();
    key_usages.push(&"sign".into());

    let crypto_key = JsFuture::from(
        subtle
            .import_key_with_object("raw", &key_data.buffer(), &algorithm, false, &key_usages)
            .map_err(|e| TlsError::subtle_crypto(format!("Failed to import HMAC key: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("HMAC key import failed: {:?}", e)))?;

    let crypto_key: CryptoKey = crypto_key.unchecked_into();

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

/// P_SHA256 expansion function
/// P_SHA256(secret, seed) = HMAC(secret, A(1) + seed) + HMAC(secret, A(2) + seed) + ...
/// where A(0) = seed, A(i) = HMAC(secret, A(i-1))
async fn p_sha256(secret: &[u8], seed: &[u8], length: usize) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(length);
    let mut a = hmac_sha256(secret, seed).await?; // A(1)

    while result.len() < length {
        // HMAC(secret, A(i) + seed)
        let mut data = a.clone();
        data.extend_from_slice(seed);
        let p = hmac_sha256(secret, &data).await?;
        result.extend_from_slice(&p);

        // A(i+1) = HMAC(secret, A(i))
        a = hmac_sha256(secret, &a).await?;
    }

    result.truncate(length);
    Ok(result)
}

/// TLS 1.2 PRF function
/// PRF(secret, label, seed) = P_SHA256(secret, label + seed)
pub async fn prf(secret: &[u8], label: &[u8], seed: &[u8], length: usize) -> Result<Vec<u8>> {
    let mut label_seed = Vec::with_capacity(label.len() + seed.len());
    label_seed.extend_from_slice(label);
    label_seed.extend_from_slice(seed);

    p_sha256(secret, &label_seed, length).await
}

/// Derive master secret from pre-master secret
/// master_secret = PRF(pre_master_secret, "master secret", ClientHello.random + ServerHello.random)[0..47]
pub async fn derive_master_secret(
    pre_master_secret: &[u8],
    client_random: &[u8],
    server_random: &[u8],
) -> Result<Vec<u8>> {
    let mut seed = Vec::with_capacity(64);
    seed.extend_from_slice(client_random);
    seed.extend_from_slice(server_random);

    prf(pre_master_secret, b"master secret", &seed, 48).await
}

/// Derive key block from master secret
/// key_block = PRF(master_secret, "key expansion", ServerHello.random + ClientHello.random)
/// Note: order is reversed compared to master secret derivation
pub async fn derive_key_block(
    master_secret: &[u8],
    client_random: &[u8],
    server_random: &[u8],
    length: usize,
) -> Result<Vec<u8>> {
    let mut seed = Vec::with_capacity(64);
    seed.extend_from_slice(server_random);
    seed.extend_from_slice(client_random);

    prf(master_secret, b"key expansion", &seed, length).await
}

/// Key material structure for TLS 1.2
#[derive(Debug, Clone, Default)]
pub struct KeyMaterial {
    pub client_write_mac_key: Vec<u8>,
    pub server_write_mac_key: Vec<u8>,
    pub client_write_key: Vec<u8>,
    pub server_write_key: Vec<u8>,
    pub client_write_iv: Vec<u8>,
    pub server_write_iv: Vec<u8>,
}

impl KeyMaterial {
    /// Extract key material from key block
    /// For AES-128-GCM: mac_key_len=0, key_len=16, iv_len=4 (implicit IV)
    /// For AES-128-CBC-SHA256: mac_key_len=32, key_len=16, iv_len=16
    pub fn from_key_block(
        key_block: &[u8],
        mac_key_len: usize,
        key_len: usize,
        iv_len: usize,
    ) -> Result<Self> {
        let total = 2 * (mac_key_len + key_len + iv_len);
        if key_block.len() < total {
            return Err(TlsError::crypto("Key block too short"));
        }

        let mut pos = 0;

        let client_write_mac_key = key_block[pos..pos + mac_key_len].to_vec();
        pos += mac_key_len;

        let server_write_mac_key = key_block[pos..pos + mac_key_len].to_vec();
        pos += mac_key_len;

        let client_write_key = key_block[pos..pos + key_len].to_vec();
        pos += key_len;

        let server_write_key = key_block[pos..pos + key_len].to_vec();
        pos += key_len;

        let client_write_iv = key_block[pos..pos + iv_len].to_vec();
        pos += iv_len;

        let server_write_iv = key_block[pos..pos + iv_len].to_vec();

        Ok(Self {
            client_write_mac_key,
            server_write_mac_key,
            client_write_key,
            server_write_key,
            client_write_iv,
            server_write_iv,
        })
    }
}

/// Compute TLS 1.2 Finished verify_data
/// verify_data = PRF(master_secret, finished_label, Hash(handshake_messages))[0..11]
pub async fn compute_verify_data(
    master_secret: &[u8],
    is_client: bool,
    handshake_hash: &[u8],
) -> Result<Vec<u8>> {
    let label = if is_client {
        b"client finished"
    } else {
        b"server finished"
    };

    prf(master_secret, label, handshake_hash, 12).await
}

/// Compute MAC for TLS 1.2 CBC cipher suites
/// MAC = HMAC(mac_key, seq_num + type + version + length + fragment)
pub async fn compute_mac_sha256(
    mac_key: &[u8],
    seq_num: u64,
    content_type: u8,
    version: u16,
    fragment: &[u8],
) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(13 + fragment.len());
    data.extend_from_slice(&seq_num.to_be_bytes());
    data.push(content_type);
    data.push((version >> 8) as u8);
    data.push(version as u8);
    data.push((fragment.len() >> 8) as u8);
    data.push(fragment.len() as u8);
    data.extend_from_slice(fragment);

    hmac_sha256(mac_key, &data).await
}

/// Compute MAC with SHA-1 for older cipher suites
pub async fn compute_mac_sha1(
    mac_key: &[u8],
    seq_num: u64,
    content_type: u8,
    version: u16,
    fragment: &[u8],
) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(13 + fragment.len());
    data.extend_from_slice(&seq_num.to_be_bytes());
    data.push(content_type);
    data.push((version >> 8) as u8);
    data.push(version as u8);
    data.push((fragment.len() >> 8) as u8);
    data.push(fragment.len() as u8);
    data.extend_from_slice(fragment);

    hmac_sha1(mac_key, &data).await
}

/// Compute MAC with SHA-384 for SHA-384 cipher suites
pub async fn compute_mac_sha384(
    mac_key: &[u8],
    seq_num: u64,
    content_type: u8,
    version: u16,
    fragment: &[u8],
) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(13 + fragment.len());
    data.extend_from_slice(&seq_num.to_be_bytes());
    data.push(content_type);
    data.push((version >> 8) as u8);
    data.push(version as u8);
    data.push((fragment.len() >> 8) as u8);
    data.push(fragment.len() as u8);
    data.extend_from_slice(fragment);

    hmac_sha384(mac_key, &data).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_prf_basic() {
        let secret = vec![0u8; 32];
        let label = b"test label";
        let seed = vec![0u8; 32];

        let result = prf(&secret, label, &seed, 48).await.unwrap();
        assert_eq!(result.len(), 48);
    }

    #[wasm_bindgen_test]
    async fn test_master_secret_derivation() {
        let pms = vec![0u8; 48];
        let client_random = vec![0u8; 32];
        let server_random = vec![0u8; 32];

        let ms = derive_master_secret(&pms, &client_random, &server_random)
            .await
            .unwrap();
        assert_eq!(ms.len(), 48);
    }

    #[wasm_bindgen_test]
    async fn test_key_block_extraction() {
        let key_block = vec![0u8; 104]; // Enough for CBC with SHA-256 MAC

        let km = KeyMaterial::from_key_block(&key_block, 32, 16, 16).unwrap();
        assert_eq!(km.client_write_mac_key.len(), 32);
        assert_eq!(km.server_write_mac_key.len(), 32);
        assert_eq!(km.client_write_key.len(), 16);
        assert_eq!(km.server_write_key.len(), 16);
        assert_eq!(km.client_write_iv.len(), 16);
        assert_eq!(km.server_write_iv.len(), 16);
    }
}
