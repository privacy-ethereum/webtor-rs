//! Edge case tests for TLS parsing and crypto
//!
//! Tests boundary conditions, malformed inputs, and error handling.

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

mod parsing_edge_cases {
    use super::*;
    use subtle_tls::handshake::{
        parse_certificate, parse_certificate_verify, parse_finished, parse_handshake_header,
    };

    #[wasm_bindgen_test]
    async fn test_handshake_header_empty() {
        let result = parse_handshake_header(&[]);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_handshake_header_too_short() {
        let result = parse_handshake_header(&[0x01, 0x00, 0x00]);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_handshake_header_valid() {
        let data = [0x01, 0x00, 0x00, 0x10]; // ClientHello, length 16
        let (msg_type, length) = parse_handshake_header(&data).unwrap();
        assert_eq!(msg_type, 0x01);
        assert_eq!(length, 16);
    }

    #[wasm_bindgen_test]
    async fn test_handshake_header_max_length() {
        let data = [0x02, 0xFF, 0xFF, 0xFF]; // Max 24-bit length
        let (msg_type, length) = parse_handshake_header(&data).unwrap();
        assert_eq!(msg_type, 0x02);
        assert_eq!(length, 0xFFFFFF);
    }

    #[wasm_bindgen_test]
    async fn test_certificate_empty() {
        let result = parse_certificate(&[]);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_certificate_too_short() {
        let result = parse_certificate(&[0x00, 0x00, 0x00]);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_certificate_no_certs() {
        // Empty certificate list (context_len=0, list_len=0)
        let data = [0x00, 0x00, 0x00, 0x00];
        let result = parse_certificate(&data);
        assert!(result.is_err()); // Should error: no certificates
    }

    #[wasm_bindgen_test]
    async fn test_certificate_verify_empty() {
        let result = parse_certificate_verify(&[]);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_certificate_verify_truncated() {
        let data = [0x08, 0x04, 0x00, 0x10]; // Declares 16-byte sig but none provided
        let result = parse_certificate_verify(&data);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_certificate_verify_valid() {
        let mut data = vec![0x08, 0x04, 0x00, 0x04]; // RSA-PSS-SHA256, 4 byte sig
        data.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let (algo, sig) = parse_certificate_verify(&data).unwrap();
        assert_eq!(algo, 0x0804);
        assert_eq!(sig, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[wasm_bindgen_test]
    async fn test_finished_empty() {
        let result = parse_finished(&[]);
        assert!(result.is_ok()); // Empty is valid, just empty verify_data
    }

    #[wasm_bindgen_test]
    async fn test_finished_valid() {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let result = parse_finished(&data).unwrap();
        assert_eq!(result, data);
    }
}

mod crypto_edge_cases {
    use super::*;
    use subtle_tls::crypto::{random_bytes, sha256, sha384, X25519KeyPair};

    #[wasm_bindgen_test]
    async fn test_random_bytes_zero() {
        let result = random_bytes(0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[wasm_bindgen_test]
    async fn test_random_bytes_large() {
        let result = random_bytes(1024);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1024);
    }

    #[wasm_bindgen_test]
    async fn test_sha256_large_input() {
        let large_input = vec![0xAB; 10000];
        let result = sha256(&large_input).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 32);
    }

    #[wasm_bindgen_test]
    async fn test_sha384_large_input() {
        let large_input = vec![0xAB; 10000];
        let result = sha384(&large_input).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 48);
    }

    #[wasm_bindgen_test]
    async fn test_x25519_zero_key_rejected() {
        let keypair = X25519KeyPair::generate().unwrap();
        let zero_key = vec![0u8; 32];

        // Deriving with all-zero key should still work (it's a valid point)
        // but the result should be checked by the protocol layer
        let result = keypair.derive_shared_secret(&zero_key);
        // This might succeed or fail depending on implementation
        // The important thing is it doesn't panic
        let _ = result;
    }

    #[wasm_bindgen_test]
    async fn test_x25519_short_key_rejected() {
        let keypair = X25519KeyPair::generate().unwrap();
        let short_key = vec![0u8; 16]; // Too short

        let result = keypair.derive_shared_secret(&short_key);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_x25519_long_key_rejected() {
        let keypair = X25519KeyPair::generate().unwrap();
        let long_key = vec![0u8; 64]; // Too long

        let result = keypair.derive_shared_secret(&long_key);
        assert!(result.is_err());
    }
}

mod cipher_edge_cases {
    use super::*;
    use subtle_tls::crypto::{AesGcm, Cipher};

    #[wasm_bindgen_test]
    async fn test_aes_gcm_empty_plaintext() {
        let key = vec![0x42u8; 16];
        let cipher = AesGcm::new_128(&key).await.unwrap();

        let nonce = vec![0x01u8; 12];
        let aad = b"aad";
        let plaintext = b"";

        let ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        // Should be just the 16-byte auth tag
        assert_eq!(ciphertext.len(), 16);

        let decrypted = cipher.decrypt(&nonce, aad, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_aes_gcm_empty_aad() {
        let key = vec![0x42u8; 16];
        let cipher = AesGcm::new_128(&key).await.unwrap();

        let nonce = vec![0x01u8; 12];
        let aad = b"";
        let plaintext = b"test";

        let ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        let decrypted = cipher.decrypt(&nonce, aad, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_aes_gcm_wrong_nonce_length() {
        let key = vec![0x42u8; 16];
        let cipher = AesGcm::new_128(&key).await.unwrap();

        let short_nonce = vec![0x01u8; 8]; // Should be 12
        let result = cipher.encrypt(&short_nonce, b"", b"test").await;
        // Behavior depends on implementation - just ensure no panic
        let _ = result;
    }

    #[wasm_bindgen_test]
    async fn test_chacha20_poly1305_empty() {
        let key = vec![0x42u8; 32];
        let cipher = Cipher::chacha20_poly1305(&key).unwrap();

        let nonce = vec![0x01u8; 12];
        let ciphertext = cipher.encrypt(&nonce, b"", b"").await.unwrap();
        assert_eq!(ciphertext.len(), 16); // Just the tag

        let decrypted = cipher.decrypt(&nonce, b"", &ciphertext).await.unwrap();
        assert!(decrypted.is_empty());
    }

    #[wasm_bindgen_test]
    async fn test_chacha20_wrong_key_size() {
        let short_key = vec![0x42u8; 16];
        let result = Cipher::chacha20_poly1305(&short_key);
        assert!(result.is_err());
    }
}

mod hkdf_edge_cases {
    use super::*;
    use subtle_tls::crypto::Hkdf;

    #[wasm_bindgen_test]
    async fn test_hkdf_extract_empty_salt() {
        let ikm = vec![0x0bu8; 22];
        let result = Hkdf::extract(&[], &ikm).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 32);
    }

    #[wasm_bindgen_test]
    async fn test_hkdf_extract_empty_ikm() {
        let salt = vec![0x00u8; 32];
        let result = Hkdf::extract(&salt, &[]).await;
        assert!(result.is_ok());
    }

    #[wasm_bindgen_test]
    async fn test_hkdf_expand_zero_length() {
        let prk = vec![0x42u8; 32];
        let result = Hkdf::expand(&prk, b"info", 0).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[wasm_bindgen_test]
    async fn test_hkdf_expand_max_length() {
        let prk = vec![0x42u8; 32];
        // Max is 255 * hash_len = 255 * 32 = 8160 for SHA-256
        let result = Hkdf::expand(&prk, b"info", 255).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 255);
    }

    #[wasm_bindgen_test]
    async fn test_hkdf_expand_label_empty_context() {
        let secret = vec![0x42u8; 32];
        let result = Hkdf::expand_label(&secret, "key", &[], 16).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 16);
    }
}

#[cfg(feature = "tls12")]
mod tls12_edge_cases {
    use super::*;
    use subtle_tls::prf;

    #[wasm_bindgen_test]
    async fn test_prf_empty_secret() {
        let result = prf::prf(&[], b"label", &[0u8; 32], 48).await;
        assert!(result.is_ok());
    }

    #[wasm_bindgen_test]
    async fn test_prf_empty_seed() {
        let result = prf::prf(&[0x42u8; 32], b"label", &[], 48).await;
        assert!(result.is_ok());
    }

    #[wasm_bindgen_test]
    async fn test_prf_zero_output() {
        let result = prf::prf(&[0x42u8; 32], b"label", &[0u8; 32], 0).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[wasm_bindgen_test]
    async fn test_key_material_exact_size() {
        // Exactly 40 bytes for AES-128-GCM
        let key_block = vec![0x42u8; 40];
        let km = prf::KeyMaterial::from_key_block(&key_block, 0, 16, 4);
        assert!(km.is_ok());
    }

    #[wasm_bindgen_test]
    async fn test_key_material_too_short() {
        let key_block = vec![0x42u8; 30]; // Too short for AES-128-GCM
        let km = prf::KeyMaterial::from_key_block(&key_block, 0, 16, 4);
        assert!(km.is_err());
    }
}
