//! SubtleTLS - TLS 1.3 implementation using browser SubtleCrypto API
//!
//! This crate provides TLS 1.3 encryption for WASM environments where
//! native crypto libraries like `ring` cannot be used. It leverages the
//! browser's SubtleCrypto API for all cryptographic operations.
//!
//! # Features
//! - TLS 1.3 client implementation
//! - ECDHE key exchange with P-256
//! - AES-128-GCM and AES-256-GCM encryption
//! - Certificate chain validation
//! - AsyncRead/AsyncWrite interface
//!
//! # Example
//! ```ignore
//! use subtle_tls::TlsConnector;
//! use futures::io::{AsyncReadExt, AsyncWriteExt};
//!
//! let connector = TlsConnector::new();
//! let mut tls_stream = connector.connect(tcp_stream, "example.com").await?;
//! tls_stream.write_all(b"GET / HTTP/1.1\r\n\r\n").await?;
//! ```

pub mod cert;
pub mod crypto;
pub mod error;
pub mod handshake;
pub mod record;
pub mod stream;
pub mod trust_store;

pub use error::{TlsError, Result};
pub use stream::TlsStream;

/// TLS connector for establishing secure connections
pub struct TlsConnector {
    config: TlsConfig,
}

/// TLS configuration
#[derive(Clone)]
pub struct TlsConfig {
    /// Skip certificate verification (INSECURE - for testing only)
    pub skip_verification: bool,
    /// Application-Layer Protocol Negotiation protocols
    pub alpn_protocols: Vec<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            skip_verification: false,
            alpn_protocols: vec!["http/1.1".to_string()],
        }
    }
}

impl TlsConnector {
    /// Create a new TLS connector with default configuration
    pub fn new() -> Self {
        Self {
            config: TlsConfig::default(),
        }
    }

    /// Create a TLS connector with custom configuration
    pub fn with_config(config: TlsConfig) -> Self {
        Self { config }
    }

    /// Connect to a server, wrapping the given stream with TLS
    pub async fn connect<S>(
        &self,
        stream: S,
        server_name: &str,
    ) -> Result<TlsStream<S>>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        TlsStream::connect(stream, server_name, self.config.clone()).await
    }
}

impl Default for TlsConnector {
    fn default() -> Self {
        Self::new()
    }
}
