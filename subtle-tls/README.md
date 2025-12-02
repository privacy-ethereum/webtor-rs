# subtle-tls

TLS 1.3 implementation using browser SubtleCrypto API for WASM environments.

## Overview

This crate provides TLS 1.3 encryption for WASM environments where native crypto libraries like `ring` cannot be used. It leverages the browser's SubtleCrypto API for cryptographic operations.

## Features

- TLS 1.3 client implementation
- Key exchange: P-256 ECDH (via SubtleCrypto), X25519 (via pure Rust)
- Cipher suites: AES-128-GCM, AES-256-GCM (via SubtleCrypto), ChaCha20-Poly1305 (via pure Rust)
- Certificate chain validation with hostname verification
- Trust store with embedded Let's Encrypt roots (~3.5KB)
- AsyncRead/AsyncWrite interface

## Usage

```rust
use subtle_tls::TlsConnector;

let connector = TlsConnector::new();
let mut tls_stream = connector.connect(tcp_stream, "example.com").await?;

// Read/write using async methods
tls_stream.write(b"GET / HTTP/1.1\r\n\r\n").await?;
let mut response = vec![0u8; 4096];
let n = tls_stream.read(&mut response).await?;
```

## Configuration

```rust
use subtle_tls::{TlsConnector, TlsConfig};

let config = TlsConfig {
    skip_verification: false,  // Set to true for testing (INSECURE!)
    alpn_protocols: vec!["http/1.1".to_string(), "h2".to_string()],
};

let connector = TlsConnector::with_config(config);
```

## Testing

### WASM tests - Node.js (full test suite)

All 53 tests run in Node.js thanks to the `globalThis.crypto` fallback:

```bash
# Requires: cargo install wasm-bindgen-cli
# And .cargo/config.toml with runner = "wasm-bindgen-test-runner"
WASM_BINDGEN_USE_NODE_EXPERIMENTAL=1 cargo test --target wasm32-unknown-unknown -p subtle-tls
```

This runs all tests including:
- P-256 ECDH key exchange (via Node's WebCrypto)
- X25519 key exchange (pure Rust)
- AES-128-GCM and AES-256-GCM encryption (via Node's WebCrypto)
- ChaCha20-Poly1305 encryption (pure Rust)
- SHA-256/SHA-384 hashing (via Node's WebCrypto)
- HKDF key derivation (via Node's WebCrypto)
- Handshake message building/parsing
- Record layer encryption/decryption
- Trust store certificate parsing
- Certificate verification
- Error types and config

### WASM tests - Browser

For browser testing (alternative to Node.js):

```bash
# Using Chrome (requires chromedriver)
wasm-pack test --headless --chrome

# Using Firefox (requires geckodriver)
wasm-pack test --headless --firefox
```

**Note**: Browser tests require WebDriver:
- Chrome: `chromedriver` (install via `brew install chromedriver` on macOS)
- Firefox: `geckodriver` (install via `brew install geckodriver` on macOS)

## Architecture

```
subtle-tls/
├── src/
│   ├── lib.rs           # Public API (TlsConnector, TlsConfig)
│   ├── crypto.rs        # Cryptographic primitives (ECDH, AES-GCM, HKDF, etc.)
│   ├── handshake.rs     # TLS 1.3 handshake protocol
│   ├── record.rs        # TLS record layer with encryption
│   ├── stream.rs        # TlsStream wrapper with async I/O
│   ├── cert.rs          # Certificate validation
│   ├── trust_store.rs   # Root CA certificates
│   └── error.rs         # Error types
└── tests/
    └── integration_tests.rs  # WASM integration tests
```

## Security Considerations

- This implementation is for educational and experimental use
- Certificate validation uses embedded Let's Encrypt roots by default
- The `skip_verification` option should NEVER be used in production
- ChaCha20-Poly1305 uses pure Rust (not SubtleCrypto) since browsers don't support it

## License

MIT
