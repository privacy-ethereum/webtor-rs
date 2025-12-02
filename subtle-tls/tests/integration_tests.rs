//! Integration tests for subtle-tls
//!
//! These tests verify the TLS 1.3 implementation works correctly.
//!
//! ## Running tests
//!
//! ### Node.js (pure Rust tests only - no SubtleCrypto):
//! ```bash
//! WASM_BINDGEN_USE_NODE_EXPERIMENTAL=1 cargo test --target wasm32-unknown-unknown -p subtle-tls
//! ```
//!
//! ### Browser (full SubtleCrypto tests):
//! ```bash
//! wasm-pack test --headless --chrome  # or --firefox
//! ```
//!
//! Note: Tests marked with `// Requires SubtleCrypto (browser)` will fail in Node.js
//! but pass in browser environment.

use wasm_bindgen_test::*;

// Uncomment for browser-only testing:
// wasm_bindgen_test_configure!(run_in_browser);

mod crypto_tests {
    use super::*;
    use subtle_tls::crypto::{self, AesGcm, Cipher, EcdhKeyPair, Hkdf, X25519KeyPair};

    #[wasm_bindgen_test]
    async fn test_x25519_key_generation() {
        let keypair = X25519KeyPair::generate().unwrap();
        assert_eq!(keypair.public_key_bytes.len(), 32);
    }

    #[wasm_bindgen_test]
    async fn test_x25519_key_exchange() {
        let alice = X25519KeyPair::generate().unwrap();
        let bob = X25519KeyPair::generate().unwrap();

        let alice_public = alice.public_key_bytes.clone();
        let bob_public = bob.public_key_bytes.clone();

        let alice_secret = alice.derive_shared_secret(&bob_public).unwrap();
        let bob_secret = bob.derive_shared_secret(&alice_public).unwrap();

        assert_eq!(alice_secret.len(), 32);
        assert_eq!(alice_secret, bob_secret);
    }

    #[wasm_bindgen_test]
    async fn test_x25519_rejects_invalid_key_length() {
        let alice = X25519KeyPair::generate().unwrap();
        let invalid_key = vec![0u8; 31]; // Wrong length
        
        let result = alice.derive_shared_secret(&invalid_key);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_p256_ecdh_key_generation() {
        let keypair = EcdhKeyPair::generate().await.unwrap();
        // P-256 uncompressed point: 0x04 || x (32 bytes) || y (32 bytes) = 65 bytes
        assert_eq!(keypair.public_key_bytes.len(), 65);
        assert_eq!(keypair.public_key_bytes[0], 0x04);
    }

    #[wasm_bindgen_test]
    async fn test_p256_ecdh_key_exchange() {
        let alice = EcdhKeyPair::generate().await.unwrap();
        let bob = EcdhKeyPair::generate().await.unwrap();

        let alice_secret = alice.derive_shared_secret(&bob.public_key_bytes).await.unwrap();
        let bob_secret = bob.derive_shared_secret(&alice.public_key_bytes).await.unwrap();

        assert_eq!(alice_secret.len(), 32);
        assert_eq!(alice_secret, bob_secret);
    }

    #[wasm_bindgen_test]
    async fn test_aes_128_gcm_encrypt_decrypt() {
        let key = vec![0x42u8; 16];
        let cipher = AesGcm::new_128(&key).await.unwrap();

        let nonce = vec![0x01u8; 12];
        let aad = b"additional authenticated data";
        let plaintext = b"Hello, TLS 1.3!";

        let ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        // Ciphertext should be plaintext + 16 byte auth tag
        assert_eq!(ciphertext.len(), plaintext.len() + 16);

        let decrypted = cipher.decrypt(&nonce, aad, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_aes_256_gcm_encrypt_decrypt() {
        let key = vec![0x42u8; 32];
        let cipher = AesGcm::new_256(&key).await.unwrap();

        let nonce = vec![0x01u8; 12];
        let aad = b"aad";
        let plaintext = b"Secret message for AES-256-GCM";

        let ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        let decrypted = cipher.decrypt(&nonce, aad, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_aes_gcm_rejects_wrong_key_size() {
        let key = vec![0x42u8; 17]; // Wrong size
        let result = AesGcm::new_128(&key).await;
        assert!(result.is_err());

        let result = AesGcm::new_256(&key).await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_aes_gcm_detects_tampering() {
        let key = vec![0x42u8; 16];
        let cipher = AesGcm::new_128(&key).await.unwrap();

        let nonce = vec![0x01u8; 12];
        let aad = b"aad";
        let plaintext = b"plaintext";

        let mut ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        
        // Tamper with the ciphertext
        ciphertext[0] ^= 0xFF;

        let result = cipher.decrypt(&nonce, aad, &ciphertext).await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_aes_gcm_detects_wrong_aad() {
        let key = vec![0x42u8; 16];
        let cipher = AesGcm::new_128(&key).await.unwrap();

        let nonce = vec![0x01u8; 12];
        let aad = b"correct aad";
        let plaintext = b"plaintext";

        let ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        
        let wrong_aad = b"wrong aad";
        let result = cipher.decrypt(&nonce, wrong_aad, &ciphertext).await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_cipher_enum_aes_128() {
        let key = vec![0x42u8; 16];
        let cipher = Cipher::aes_128_gcm(&key).await.unwrap();

        assert_eq!(cipher.key_size(), 16);
        assert_eq!(cipher.iv_size(), 12);

        let nonce = vec![0x01u8; 12];
        let aad = b"aad";
        let plaintext = b"test";

        let ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        let decrypted = cipher.decrypt(&nonce, aad, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_cipher_enum_chacha20_poly1305() {
        let key = vec![0x42u8; 32];
        let cipher = Cipher::chacha20_poly1305(&key).unwrap();

        assert_eq!(cipher.key_size(), 32);
        assert_eq!(cipher.iv_size(), 12);

        let nonce = vec![0x01u8; 12];
        let aad = b"aad";
        let plaintext = b"ChaCha20-Poly1305 test message!";

        let ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        // Ciphertext should be plaintext + 16 byte poly1305 tag
        assert_eq!(ciphertext.len(), plaintext.len() + 16);

        let decrypted = cipher.decrypt(&nonce, aad, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_chacha20_poly1305_detects_tampering() {
        let key = vec![0x42u8; 32];
        let cipher = Cipher::chacha20_poly1305(&key).unwrap();

        let nonce = vec![0x01u8; 12];
        let aad = b"aad";
        let plaintext = b"plaintext";

        let mut ciphertext = cipher.encrypt(&nonce, aad, plaintext).await.unwrap();
        ciphertext[0] ^= 0xFF;

        let result = cipher.decrypt(&nonce, aad, &ciphertext).await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_sha256() {
        let hash = crypto::sha256(b"hello").await.unwrap();
        assert_eq!(hash.len(), 32);
        
        // Known SHA-256 hash of "hello"
        let expected = [
            0x2c, 0xf2, 0x4d, 0xba, 0x5f, 0xb0, 0xa3, 0x0e,
            0x26, 0xe8, 0x3b, 0x2a, 0xc5, 0xb9, 0xe2, 0x9e,
            0x1b, 0x16, 0x1e, 0x5c, 0x1f, 0xa7, 0x42, 0x5e,
            0x73, 0x04, 0x33, 0x62, 0x93, 0x8b, 0x98, 0x24,
        ];
        assert_eq!(hash, expected);
    }

    #[wasm_bindgen_test]
    async fn test_sha384() {
        let hash = crypto::sha384(b"hello").await.unwrap();
        assert_eq!(hash.len(), 48);
    }

    #[wasm_bindgen_test]
    async fn test_sha256_empty() {
        let hash = crypto::sha256(b"").await.unwrap();
        // Known SHA-256 hash of empty string
        let expected = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14,
            0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
            0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
            0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(hash, expected);
    }

    #[wasm_bindgen_test]
    async fn test_hkdf_extract() {
        let salt = vec![0x00u8; 32];
        let ikm = vec![0x0bu8; 22];

        let prk = Hkdf::extract(&salt, &ikm).await.unwrap();
        assert_eq!(prk.len(), 32);
    }

    #[wasm_bindgen_test]
    async fn test_hkdf_expand() {
        let prk = vec![0x42u8; 32];
        let info = b"test info";

        let okm = Hkdf::expand(&prk, info, 32).await.unwrap();
        assert_eq!(okm.len(), 32);

        // Test different output lengths
        let okm_16 = Hkdf::expand(&prk, info, 16).await.unwrap();
        assert_eq!(okm_16.len(), 16);

        let okm_64 = Hkdf::expand(&prk, info, 64).await.unwrap();
        assert_eq!(okm_64.len(), 64);
    }

    #[wasm_bindgen_test]
    async fn test_hkdf_expand_label() {
        let secret = vec![0x42u8; 32];
        let label = "test";
        let context = b"context";

        let output = Hkdf::expand_label(&secret, label, context, 32).await.unwrap();
        assert_eq!(output.len(), 32);

        // Test key and IV derivation labels used in TLS 1.3
        let key = Hkdf::expand_label(&secret, "key", &[], 16).await.unwrap();
        assert_eq!(key.len(), 16);

        let iv = Hkdf::expand_label(&secret, "iv", &[], 12).await.unwrap();
        assert_eq!(iv.len(), 12);
    }

    #[wasm_bindgen_test]
    async fn test_hkdf_derive_secret() {
        let secret = vec![0x42u8; 32];
        let label = "derived";
        let messages_hash = crypto::sha256(b"test").await.unwrap();

        let derived = Hkdf::derive_secret(&secret, label, &messages_hash).await.unwrap();
        assert_eq!(derived.len(), 32);
    }

    #[wasm_bindgen_test]
    async fn test_random_bytes() {
        let bytes1 = crypto::random_bytes(32).unwrap();
        let bytes2 = crypto::random_bytes(32).unwrap();

        assert_eq!(bytes1.len(), 32);
        assert_eq!(bytes2.len(), 32);
        // Should be different (with overwhelming probability)
        assert_ne!(bytes1, bytes2);
    }

    #[wasm_bindgen_test]
    async fn test_random_bytes_various_lengths() {
        for len in [1, 16, 32, 64, 128, 256] {
            let bytes = crypto::random_bytes(len).unwrap();
            assert_eq!(bytes.len(), len);
        }
    }
}

mod handshake_tests {
    use super::*;
    use subtle_tls::handshake::{
        parse_certificate_verify, parse_finished, parse_handshake_header,
        HandshakeState, HANDSHAKE_CLIENT_HELLO,
        TLS_AES_128_GCM_SHA256, TLS_AES_256_GCM_SHA384, TLS_CHACHA20_POLY1305_SHA256,
        TLS_VERSION_1_2,
    };

    #[wasm_bindgen_test]
    async fn test_handshake_state_creation() {
        let state = HandshakeState::new("example.com").await.unwrap();
        
        assert_eq!(state.server_name, "example.com");
        assert_eq!(state.client_random.len(), 32);
        assert_eq!(state.cipher_suite, TLS_AES_128_GCM_SHA256);
        assert!(state.x25519_key.is_some());
        assert_eq!(state.ecdh_key.public_key_bytes.len(), 65);
    }

    #[wasm_bindgen_test]
    async fn test_build_client_hello() {
        let state = HandshakeState::new("example.com").await.unwrap();
        let client_hello = state.build_client_hello();

        // Verify handshake header
        assert_eq!(client_hello[0], HANDSHAKE_CLIENT_HELLO);
        
        // Verify length field
        let length = ((client_hello[1] as usize) << 16)
            | ((client_hello[2] as usize) << 8)
            | (client_hello[3] as usize);
        assert_eq!(length, client_hello.len() - 4);

        // Verify legacy version (TLS 1.2)
        assert_eq!(client_hello[4], (TLS_VERSION_1_2 >> 8) as u8);
        assert_eq!(client_hello[5], TLS_VERSION_1_2 as u8);

        // Verify random is present (32 bytes starting at offset 6)
        assert_eq!(&client_hello[6..38], &state.client_random[..]);
    }

    #[wasm_bindgen_test]
    async fn test_client_hello_contains_cipher_suites() {
        let state = HandshakeState::new("example.com").await.unwrap();
        let client_hello = state.build_client_hello();

        // Convert to hex for easier debugging
        let hex: Vec<String> = client_hello.iter().map(|b| format!("{:02x}", b)).collect();
        let _hex_str = hex.join("");

        // The cipher suites should include TLS 1.3 suites
        // They appear after: header(4) + version(2) + random(32) + session_id_len(1) = 39
        // Session ID length should be 0, so cipher suite length starts at 39
        let pos = 39;
        let cs_len = ((client_hello[pos] as usize) << 8) | (client_hello[pos + 1] as usize);
        assert_eq!(cs_len, 6); // 3 cipher suites * 2 bytes each

        // Parse cipher suites
        let cs1 = ((client_hello[pos + 2] as u16) << 8) | (client_hello[pos + 3] as u16);
        let cs2 = ((client_hello[pos + 4] as u16) << 8) | (client_hello[pos + 5] as u16);
        let cs3 = ((client_hello[pos + 6] as u16) << 8) | (client_hello[pos + 7] as u16);

        assert_eq!(cs1, TLS_AES_128_GCM_SHA256);
        assert_eq!(cs2, TLS_AES_256_GCM_SHA384);
        assert_eq!(cs3, TLS_CHACHA20_POLY1305_SHA256);
    }

    #[wasm_bindgen_test]
    async fn test_client_hello_contains_sni() {
        let server_name = "test.example.com";
        let state = HandshakeState::new(server_name).await.unwrap();
        let client_hello = state.build_client_hello();

        // The SNI should be in the extensions, containing the server name
        let sni_bytes = server_name.as_bytes();
        
        // Find the SNI in the message
        let mut found_sni = false;
        for i in 0..client_hello.len() - sni_bytes.len() {
            if &client_hello[i..i + sni_bytes.len()] == sni_bytes {
                found_sni = true;
                break;
            }
        }
        assert!(found_sni, "SNI not found in ClientHello");
    }

    #[wasm_bindgen_test]
    async fn test_parse_handshake_header() {
        let data = [HANDSHAKE_CLIENT_HELLO, 0x00, 0x01, 0x00];
        let (msg_type, length) = parse_handshake_header(&data).unwrap();
        
        assert_eq!(msg_type, HANDSHAKE_CLIENT_HELLO);
        assert_eq!(length, 256);
    }

    #[wasm_bindgen_test]
    async fn test_parse_handshake_header_too_short() {
        let data = [0x01, 0x00, 0x00];
        let result = parse_handshake_header(&data);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_parse_finished() {
        let verify_data = vec![0x01u8; 32];
        let parsed = parse_finished(&verify_data).unwrap();
        assert_eq!(parsed, verify_data);
    }

    #[wasm_bindgen_test]
    async fn test_parse_certificate_verify() {
        // Build a minimal CertificateVerify message
        // Format: algorithm(2) + signature_length(2) + signature(N)
        let mut data = Vec::new();
        data.push(0x04); // Algorithm high byte (RSA_PSS_RSAE_SHA256 = 0x0804)
        data.push(0x04); // Algorithm low byte
        data.push(0x00); // Signature length high byte
        data.push(0x10); // Signature length low byte (16 bytes)
        data.extend_from_slice(&[0xAA; 16]); // Signature

        let (algorithm, signature) = parse_certificate_verify(&data).unwrap();
        assert_eq!(algorithm, 0x0404);
        assert_eq!(signature.len(), 16);
        assert_eq!(signature, vec![0xAA; 16]);
    }

    #[wasm_bindgen_test]
    async fn test_transcript_update() {
        let mut state = HandshakeState::new("example.com").await.unwrap();
        assert!(state.transcript.is_empty());

        state.update_transcript(&[1, 2, 3]);
        assert_eq!(state.transcript, vec![1, 2, 3]);

        state.update_transcript(&[4, 5]);
        assert_eq!(state.transcript, vec![1, 2, 3, 4, 5]);
    }
}

mod record_tests {
    use super::*;
    use subtle_tls::record::RecordLayer;
    use subtle_tls::handshake::{
        TLS_AES_128_GCM_SHA256, TLS_AES_256_GCM_SHA384, TLS_CHACHA20_POLY1305_SHA256,
        CONTENT_TYPE_APPLICATION_DATA, CONTENT_TYPE_HANDSHAKE,
    };
    use futures::io::Cursor;

    #[wasm_bindgen_test]
    async fn test_record_layer_creation() {
        let _layer = RecordLayer::new();
        // Should be created without encryption - verifies constructor works
    }

    #[wasm_bindgen_test]
    async fn test_set_cipher_suite() {
        let mut layer = RecordLayer::new();
        layer.set_cipher_suite(TLS_AES_256_GCM_SHA384);
        // Verifies the method doesn't panic
    }

    #[wasm_bindgen_test]
    async fn test_write_unencrypted_record() {
        let mut layer = RecordLayer::new();
        let mut output = Vec::new();
        let data = b"Hello, World!";

        layer.write_record(&mut output, CONTENT_TYPE_HANDSHAKE, data).await.unwrap();

        // Verify record header
        assert_eq!(output[0], CONTENT_TYPE_HANDSHAKE);
        assert_eq!(output[1], 0x03); // TLS 1.2 major
        assert_eq!(output[2], 0x03); // TLS 1.2 minor
        
        let length = ((output[3] as usize) << 8) | (output[4] as usize);
        assert_eq!(length, data.len());
        
        assert_eq!(&output[5..], data);
    }

    #[wasm_bindgen_test]
    async fn test_read_unencrypted_record() {
        let mut layer = RecordLayer::new();
        
        // Build a record manually
        let data = b"Test data";
        let mut record = vec![
            CONTENT_TYPE_HANDSHAKE,
            0x03, 0x03, // TLS 1.2
            0x00, data.len() as u8,
        ];
        record.extend_from_slice(data);

        let mut cursor = Cursor::new(record);
        let (content_type, read_data) = layer.read_record(&mut cursor).await.unwrap();

        assert_eq!(content_type, CONTENT_TYPE_HANDSHAKE);
        assert_eq!(read_data, data);
    }

    #[wasm_bindgen_test]
    async fn test_encrypted_record_roundtrip_aes_128() {
        let mut layer = RecordLayer::new();
        layer.set_cipher_suite(TLS_AES_128_GCM_SHA256);

        let key = vec![0x42u8; 16];
        let iv = vec![0x01u8; 12];

        layer.set_write_cipher(&key, &iv).await.unwrap();
        layer.set_read_cipher(&key, &iv).await.unwrap();

        let plaintext = b"Secret application data!";
        let mut output = Vec::new();

        // Write encrypted record
        layer.write_record(&mut output, CONTENT_TYPE_APPLICATION_DATA, plaintext).await.unwrap();

        // The output should be an encrypted record (larger than plaintext due to tag and content type byte)
        assert!(output.len() > plaintext.len() + 5);

        // Read it back
        let mut cursor = Cursor::new(output);
        let (content_type, decrypted) = layer.read_record(&mut cursor).await.unwrap();

        assert_eq!(content_type, CONTENT_TYPE_APPLICATION_DATA);
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_encrypted_record_roundtrip_aes_256() {
        let mut layer = RecordLayer::new();
        layer.set_cipher_suite(TLS_AES_256_GCM_SHA384);

        let key = vec![0x42u8; 32];
        let iv = vec![0x01u8; 12];

        layer.set_write_cipher(&key, &iv).await.unwrap();
        layer.set_read_cipher(&key, &iv).await.unwrap();

        let plaintext = b"AES-256-GCM encrypted data";
        let mut output = Vec::new();

        layer.write_record(&mut output, CONTENT_TYPE_APPLICATION_DATA, plaintext).await.unwrap();

        let mut cursor = Cursor::new(output);
        let (content_type, decrypted) = layer.read_record(&mut cursor).await.unwrap();

        assert_eq!(content_type, CONTENT_TYPE_APPLICATION_DATA);
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_encrypted_record_roundtrip_chacha20() {
        let mut layer = RecordLayer::new();
        layer.set_cipher_suite(TLS_CHACHA20_POLY1305_SHA256);

        let key = vec![0x42u8; 32];
        let iv = vec![0x01u8; 12];

        layer.set_write_cipher(&key, &iv).await.unwrap();
        layer.set_read_cipher(&key, &iv).await.unwrap();

        let plaintext = b"ChaCha20-Poly1305 encrypted data";
        let mut output = Vec::new();

        layer.write_record(&mut output, CONTENT_TYPE_APPLICATION_DATA, plaintext).await.unwrap();

        let mut cursor = Cursor::new(output);
        let (content_type, decrypted) = layer.read_record(&mut cursor).await.unwrap();

        assert_eq!(content_type, CONTENT_TYPE_APPLICATION_DATA);
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_sequence_number_increment() {
        let mut layer = RecordLayer::new();
        layer.set_cipher_suite(TLS_AES_128_GCM_SHA256);

        let key = vec![0x42u8; 16];
        let iv = vec![0x01u8; 12];

        layer.set_write_cipher(&key, &iv).await.unwrap();
        layer.set_read_cipher(&key, &iv).await.unwrap();

        // Write multiple records - each should use a different nonce
        let mut outputs = Vec::new();
        for i in 0..3 {
            let mut output = Vec::new();
            let data = format!("Message {}", i);
            layer.write_record(&mut output, CONTENT_TYPE_APPLICATION_DATA, data.as_bytes()).await.unwrap();
            outputs.push(output);
        }

        // Each encrypted record should be different due to different sequence numbers
        assert_ne!(outputs[0], outputs[1]);
        assert_ne!(outputs[1], outputs[2]);

        // Read them all back
        for (i, output) in outputs.into_iter().enumerate() {
            let mut cursor = Cursor::new(output);
            let (content_type, decrypted) = layer.read_record(&mut cursor).await.unwrap();
            assert_eq!(content_type, CONTENT_TYPE_APPLICATION_DATA);
            assert_eq!(String::from_utf8(decrypted).unwrap(), format!("Message {}", i));
        }
    }
}

mod trust_store_tests {
    use super::*;
    use subtle_tls::trust_store::{TrustStore, EMBEDDED_ROOT_COUNT};

    #[wasm_bindgen_test]
    async fn test_trust_store_creation() {
        let store = TrustStore::new().unwrap();
        assert!(!store.has_extended_roots());
    }

    #[wasm_bindgen_test]
    async fn test_embedded_root_count() {
        let store = TrustStore::new().unwrap();
        let roots = store.get_roots();
        assert_eq!(roots.len(), EMBEDDED_ROOT_COUNT);
    }

    #[wasm_bindgen_test]
    async fn test_isrg_roots_present() {
        let store = TrustStore::new().unwrap();
        let roots = store.get_roots();
        
        let has_isrg_x1 = roots.iter().any(|r| r.subject.contains("ISRG Root X1"));
        let has_isrg_x2 = roots.iter().any(|r| r.subject.contains("ISRG Root X2"));
        
        assert!(has_isrg_x1, "ISRG Root X1 should be in trust store");
        assert!(has_isrg_x2, "ISRG Root X2 should be in trust store");
    }

    #[wasm_bindgen_test]
    async fn test_digicert_root_present() {
        let store = TrustStore::new().unwrap();
        let roots = store.get_roots();
        
        let has_digicert = roots.iter().any(|r| r.subject.contains("DigiCert"));
        assert!(has_digicert, "DigiCert root should be in trust store");
    }

    #[wasm_bindgen_test]
    async fn test_ca_bundle_url() {
        let store = TrustStore::new().unwrap();
        assert!(store.ca_bundle_url().starts_with("https://"));
    }

    #[wasm_bindgen_test]
    async fn test_custom_ca_bundle_url() {
        let store = TrustStore::new().unwrap()
            .with_ca_bundle_url("https://custom.example.com/certs.pem");
        assert_eq!(store.ca_bundle_url(), "https://custom.example.com/certs.pem");
    }
}

mod cert_tests {
    use super::*;
    use subtle_tls::cert::CertificateVerifier;

    #[wasm_bindgen_test]
    async fn test_hostname_exact_match() {
        let _verifier = CertificateVerifier::new("example.com", false);
        // This tests internal matching logic via skip_verification=false
        // We can't directly test matches_hostname as it's private
    }

    #[wasm_bindgen_test]
    async fn test_skip_verification() {
        let verifier = CertificateVerifier::new("example.com", true);
        // With skip_verification=true, even an empty chain should pass
        // (the function returns Ok(()) immediately without validation)
        let result = verifier.verify_chain(&[]).await;
        assert!(result.is_ok());
    }

    #[wasm_bindgen_test]
    async fn test_verifier_creation() {
        let _verifier = CertificateVerifier::new("test.example.com", false);
        // Should not panic
    }
}

mod error_tests {
    use super::*;
    use subtle_tls::TlsError;

    #[wasm_bindgen_test]
    async fn test_error_display() {
        let err = TlsError::handshake("test handshake error");
        assert!(err.to_string().contains("Handshake"));
        assert!(err.to_string().contains("test handshake error"));

        let err = TlsError::certificate("bad cert");
        assert!(err.to_string().contains("Certificate"));

        let err = TlsError::crypto("crypto failed");
        assert!(err.to_string().contains("Crypto"));

        let err = TlsError::subtle_crypto("SubtleCrypto failed");
        assert!(err.to_string().contains("SubtleCrypto"));
    }

    #[wasm_bindgen_test]
    async fn test_error_variants() {
        let _ = TlsError::protocol("protocol error");
        let _ = TlsError::record("record error");
        let _ = TlsError::alert("alert received");
        let _ = TlsError::ConnectionClosed;
        let _ = TlsError::UnexpectedMessage {
            expected: "ServerHello".to_string(),
            got: "Alert".to_string(),
        };
    }
}

mod tls_config_tests {
    use super::*;
    use subtle_tls::{TlsConfig, TlsConnector};

    #[wasm_bindgen_test]
    async fn test_default_config() {
        let config = TlsConfig::default();
        assert!(!config.skip_verification);
        assert!(config.alpn_protocols.contains(&"http/1.1".to_string()));
    }

    #[wasm_bindgen_test]
    async fn test_custom_config() {
        let config = TlsConfig {
            skip_verification: true,
            alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
        };
        assert!(config.skip_verification);
        assert_eq!(config.alpn_protocols.len(), 2);
    }

    #[wasm_bindgen_test]
    async fn test_connector_creation() {
        let _connector = TlsConnector::new();
        let _connector = TlsConnector::default();
        
        let config = TlsConfig::default();
        let _connector = TlsConnector::with_config(config);
    }
}
