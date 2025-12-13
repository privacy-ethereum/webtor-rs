//! TLS 1.2 Browser Integration Tests
//!
//! These tests run in a browser environment (via wasm-pack test) and validate
//! TLS 1.2 functionality with real SubtleCrypto APIs.
//!
//! ## Running tests
//!
//! ```bash
//! # Run in headless Chrome
//! cd subtle-tls && wasm-pack test --headless --chrome --features tls12
//!
//! # Run in headless Firefox
//! cd subtle-tls && wasm-pack test --headless --firefox --features tls12
//! ```
//!
//! Note: These tests require the `tls12` feature to be enabled.

#![cfg(feature = "tls12")]

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

mod tls12_crypto_tests {
    use super::*;
    use subtle_tls::crypto::AesCbc;
    use subtle_tls::prf;

    #[wasm_bindgen_test]
    async fn test_aes_cbc_pkcs7_padding_full_block() {
        let key = vec![0x42u8; 16];
        let cipher = AesCbc::new_128(&key).await.unwrap();

        let iv = vec![0x01u8; 16];
        // 16 bytes exactly - will need a full padding block
        let plaintext = b"Sixteen bytesss!";

        let ciphertext = cipher.encrypt(&iv, plaintext).await.unwrap();
        // Should be 32 bytes (16 data + 16 padding block)
        assert_eq!(ciphertext.len(), 32);

        let decrypted = cipher.decrypt(&iv, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_aes_cbc_various_lengths() {
        let key = vec![0x42u8; 16];
        let cipher = AesCbc::new_128(&key).await.unwrap();
        let iv = vec![0x01u8; 16];

        // Test various plaintext lengths
        for len in [1, 7, 15, 16, 17, 31, 32, 33, 64, 100, 256] {
            let plaintext: Vec<u8> = (0..len).map(|i| i as u8).collect();

            let ciphertext = cipher.encrypt(&iv, &plaintext).await.unwrap();
            // Ciphertext should be padded to multiple of 16
            assert!(ciphertext.len() % 16 == 0);
            assert!(ciphertext.len() >= plaintext.len());

            let decrypted = cipher.decrypt(&iv, &ciphertext).await.unwrap();
            assert_eq!(decrypted, plaintext, "Failed for length {}", len);
        }
    }

    #[wasm_bindgen_test]
    async fn test_prf_with_master_secret_label() {
        let secret = vec![0x42u8; 48];
        let label = b"master secret";
        let seed = vec![0x01u8; 64]; // client_random + server_random

        let result = prf::prf(&secret, label, &seed, 48).await.unwrap();
        assert_eq!(result.len(), 48);
    }

    #[wasm_bindgen_test]
    async fn test_prf_with_key_expansion_label() {
        let master_secret = vec![0x42u8; 48];
        let label = b"key expansion";
        let seed = vec![0x01u8; 64];

        // For AES-128-GCM we need: 2*(16 + 4) = 40 bytes
        let result = prf::prf(&master_secret, label, &seed, 40).await.unwrap();
        assert_eq!(result.len(), 40);
    }

    #[wasm_bindgen_test]
    async fn test_full_key_derivation_flow() {
        // Simulate the full TLS 1.2 key derivation
        let pre_master_secret = vec![0x03u8; 48];
        let client_random = vec![0xaa; 32];
        let server_random = vec![0xbb; 32];

        // Step 1: Derive master secret
        let master_secret =
            prf::derive_master_secret(&pre_master_secret, &client_random, &server_random)
                .await
                .unwrap();

        assert_eq!(master_secret.len(), 48);

        // Step 2: Derive key block for AES-128-GCM (40 bytes needed)
        let key_block = prf::derive_key_block(&master_secret, &client_random, &server_random, 40)
            .await
            .unwrap();

        assert_eq!(key_block.len(), 40);

        // Step 3: Extract key material
        let km = prf::KeyMaterial::from_key_block(&key_block, 0, 16, 4).unwrap();

        assert_eq!(km.client_write_key.len(), 16);
        assert_eq!(km.server_write_key.len(), 16);
        assert_eq!(km.client_write_iv.len(), 4);
        assert_eq!(km.server_write_iv.len(), 4);
    }

    #[wasm_bindgen_test]
    async fn test_verify_data_computation() {
        let master_secret = vec![0x42u8; 48];
        let handshake_hash = vec![0xcc; 32]; // SHA-256 hash of handshake messages

        let client_finished = prf::compute_verify_data(&master_secret, true, &handshake_hash)
            .await
            .unwrap();

        let server_finished = prf::compute_verify_data(&master_secret, false, &handshake_hash)
            .await
            .unwrap();

        // verify_data is 12 bytes in TLS 1.2
        assert_eq!(client_finished.len(), 12);
        assert_eq!(server_finished.len(), 12);

        // Client and server finished values must differ
        assert_ne!(client_finished, server_finished);
    }
}

mod tls12_handshake_tests {
    use super::*;
    use subtle_tls::handshake_1_2::{
        CipherSuiteParams, Handshake12State, TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA,
        TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256, TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
        TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
    };

    #[wasm_bindgen_test]
    async fn test_handshake_state_initialization() {
        let state = Handshake12State::new("httpbin.org").await.unwrap();

        assert_eq!(state.server_name, "httpbin.org");
        assert_eq!(state.client_random.len(), 32);
        // Before server hello, cipher suite is 0
        assert_eq!(state.cipher_suite, 0);
    }

    #[wasm_bindgen_test]
    async fn test_client_hello_structure() {
        let state = Handshake12State::new("example.com").await.unwrap();
        let client_hello = state.build_client_hello();

        // Handshake type 1 = ClientHello
        assert_eq!(client_hello[0], 1);

        // Length check (3 bytes after type)
        let length = ((client_hello[1] as usize) << 16)
            | ((client_hello[2] as usize) << 8)
            | (client_hello[3] as usize);
        assert_eq!(length, client_hello.len() - 4);

        // Version should be TLS 1.2 (0x0303)
        assert_eq!(client_hello[4], 0x03);
        assert_eq!(client_hello[5], 0x03);

        // Random should match state
        assert_eq!(&client_hello[6..38], &state.client_random[..]);
    }

    #[wasm_bindgen_test]
    async fn test_client_hello_contains_extensions() {
        let state = Handshake12State::new("test.example.org").await.unwrap();
        let client_hello = state.build_client_hello();

        // The SNI should be present
        let sni_bytes = b"test.example.org";
        let mut found_sni = false;
        for i in 0..client_hello.len().saturating_sub(sni_bytes.len()) {
            if &client_hello[i..i + sni_bytes.len()] == sni_bytes {
                found_sni = true;
                break;
            }
        }
        assert!(found_sni, "SNI extension not found");
    }

    #[wasm_bindgen_test]
    async fn test_all_supported_cipher_suites() {
        let suites = [
            (TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256, 16, 4, true),
            (TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384, 32, 4, true),
            (TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256, 16, 0, false),
            (TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA, 16, 0, false),
        ];

        for (suite, expected_key_len, expected_iv_len, is_aead) in suites {
            let params = CipherSuiteParams::for_suite(suite)
                .expect(&format!("Suite 0x{:04x} should be supported", suite));

            assert_eq!(
                params.key_len, expected_key_len,
                "Key length mismatch for 0x{:04x}",
                suite
            );
            assert_eq!(
                params.iv_len, expected_iv_len,
                "IV length mismatch for 0x{:04x}",
                suite
            );
            assert_eq!(
                params.is_aead, is_aead,
                "AEAD flag mismatch for 0x{:04x}",
                suite
            );
        }
    }

    #[wasm_bindgen_test]
    async fn test_cipher_suite_key_block_length() {
        // AES-128-GCM: 0 + 16 + 16 + 4 + 4 = 40
        let gcm_params =
            CipherSuiteParams::for_suite(TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256).unwrap();
        assert_eq!(gcm_params.key_block_len(), 40);

        // AES-256-GCM: 0 + 32 + 32 + 4 + 4 = 72
        let gcm256_params =
            CipherSuiteParams::for_suite(TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384).unwrap();
        assert_eq!(gcm256_params.key_block_len(), 72);

        // AES-128-CBC-SHA256: 32 + 32 + 16 + 16 + 0 + 0 = 96
        let cbc_params =
            CipherSuiteParams::for_suite(TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256).unwrap();
        assert_eq!(cbc_params.key_block_len(), 96);
    }
}

mod tls12_record_tests {
    use super::*;
    use futures::io::Cursor;
    use subtle_tls::handshake::{CONTENT_TYPE_APPLICATION_DATA, CONTENT_TYPE_HANDSHAKE};
    use subtle_tls::handshake_1_2::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256;
    use subtle_tls::record_1_2::RecordLayer12;

    #[wasm_bindgen_test]
    async fn test_record_layer_creation() {
        let _layer = RecordLayer12::new();
    }

    #[wasm_bindgen_test]
    async fn test_unencrypted_record_write() {
        let mut layer = RecordLayer12::new();
        let mut output = Vec::new();
        let data = b"Hello TLS 1.2!";

        layer
            .write_record(&mut output, CONTENT_TYPE_HANDSHAKE, data)
            .await
            .unwrap();

        // Record header: type(1) + version(2) + length(2) = 5 bytes
        assert!(output.len() >= 5);
        assert_eq!(output[0], CONTENT_TYPE_HANDSHAKE);
        assert_eq!(output[1], 0x03); // TLS 1.2 major
        assert_eq!(output[2], 0x03); // TLS 1.2 minor

        let length = ((output[3] as usize) << 8) | (output[4] as usize);
        assert_eq!(length, data.len());
    }

    #[wasm_bindgen_test]
    async fn test_unencrypted_record_roundtrip() {
        let mut layer = RecordLayer12::new();
        let data = b"Test record data";

        let mut record = Vec::new();
        layer
            .write_record(&mut record, CONTENT_TYPE_HANDSHAKE, data)
            .await
            .unwrap();

        let mut cursor = Cursor::new(record);
        let (content_type, content) = layer.read_record(&mut cursor).await.unwrap();

        assert_eq!(content_type, CONTENT_TYPE_HANDSHAKE);
        assert_eq!(content, data);
    }

    #[wasm_bindgen_test]
    async fn test_gcm_encrypted_record_roundtrip() {
        let mut layer = RecordLayer12::new();
        layer
            .set_cipher_suite(TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256)
            .unwrap();

        let key = vec![0x42u8; 16];
        let implicit_iv = vec![0x01u8; 4];
        let mac_key = vec![]; // GCM doesn't use MAC key

        layer
            .set_write_cipher(&key, &implicit_iv, &mac_key)
            .await
            .unwrap();
        layer
            .set_read_cipher(&key, &implicit_iv, &mac_key)
            .await
            .unwrap();

        let plaintext = b"GCM encrypted application data for TLS 1.2";
        let mut output = Vec::new();

        layer
            .write_record(&mut output, CONTENT_TYPE_APPLICATION_DATA, plaintext)
            .await
            .unwrap();

        // Should be: header(5) + explicit_iv(8) + ciphertext + tag(16)
        assert!(output.len() > 5 + 8 + plaintext.len());

        let mut cursor = Cursor::new(output);
        let (content_type, decrypted) = layer.read_record(&mut cursor).await.unwrap();

        assert_eq!(content_type, CONTENT_TYPE_APPLICATION_DATA);
        assert_eq!(decrypted, plaintext);
    }

    #[wasm_bindgen_test]
    async fn test_multiple_records_sequence_numbers() {
        let mut layer = RecordLayer12::new();
        layer
            .set_cipher_suite(TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256)
            .unwrap();

        let key = vec![0x42u8; 16];
        let iv = vec![0x01u8; 4];
        let mac_key = vec![];

        layer.set_write_cipher(&key, &iv, &mac_key).await.unwrap();
        layer.set_read_cipher(&key, &iv, &mac_key).await.unwrap();

        // Write and read multiple records - sequence numbers should increment
        let mut outputs = Vec::new();
        for i in 0..5 {
            let mut output = Vec::new();
            let data = format!("Record {}", i);
            layer
                .write_record(&mut output, CONTENT_TYPE_APPLICATION_DATA, data.as_bytes())
                .await
                .unwrap();
            outputs.push((output, data));
        }

        // Each record should be different due to different nonces
        for i in 0..outputs.len() {
            for j in (i + 1)..outputs.len() {
                assert_ne!(
                    outputs[i].0, outputs[j].0,
                    "Records {} and {} should differ",
                    i, j
                );
            }
        }

        // Read them all back
        for (output, expected) in outputs {
            let mut cursor = Cursor::new(output);
            let (content_type, decrypted) = layer.read_record(&mut cursor).await.unwrap();
            assert_eq!(content_type, CONTENT_TYPE_APPLICATION_DATA);
            assert_eq!(String::from_utf8(decrypted).unwrap(), expected);
        }
    }
}

mod tls_version_negotiation_tests {
    use super::*;
    use subtle_tls::{TlsConfig, TlsVersion};

    #[wasm_bindgen_test]
    async fn test_tls_version_enum() {
        // Test that different variants are not equal
        assert_ne!(TlsVersion::Tls13, TlsVersion::Tls12);
        assert_ne!(TlsVersion::Tls13, TlsVersion::Prefer13);
        assert_ne!(TlsVersion::Tls12, TlsVersion::Prefer13);

        // Test clone produces equal values
        let v1 = TlsVersion::Tls13;
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    #[wasm_bindgen_test]
    async fn test_config_with_prefer13() {
        let config = TlsConfig {
            skip_verification: false,
            alpn_protocols: vec!["http/1.1".to_string()],
            version: TlsVersion::Prefer13,
        };

        assert_eq!(config.version, TlsVersion::Prefer13);
    }

    #[wasm_bindgen_test]
    async fn test_config_with_tls12_only() {
        let config = TlsConfig {
            skip_verification: false,
            alpn_protocols: vec!["http/1.1".to_string()],
            version: TlsVersion::Tls12,
        };

        assert_eq!(config.version, TlsVersion::Tls12);
    }
}
