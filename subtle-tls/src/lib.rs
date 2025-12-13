//! SubtleTLS - TLS 1.2/1.3 implementation using browser SubtleCrypto API
//!
//! This crate provides TLS encryption for WASM environments where
//! native crypto libraries like `ring` cannot be used. It leverages the
//! browser's SubtleCrypto API for all cryptographic operations.
//!
//! # Features
//! - TLS 1.3 client implementation (default)
//! - TLS 1.2 client implementation (with `tls12` feature)
//! - ECDHE key exchange with P-256 and X25519
//! - AES-128-GCM, AES-256-GCM, and ChaCha20-Poly1305 encryption
//! - AES-CBC encryption (TLS 1.2 only)
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

#[cfg(feature = "tls12")]
pub mod handshake_1_2;
#[cfg(feature = "tls12")]
pub mod prf;
#[cfg(feature = "tls12")]
pub mod record_1_2;
#[cfg(feature = "tls12")]
pub mod stream_1_2;

pub use error::{Result, TlsError};
pub use stream::TlsStream;

#[cfg(feature = "tls12")]
pub use stream_1_2::TlsStream12;

// Re-export the wrapper for version-aware TLS
// Note: TlsStreamWrapper is defined below after TlsConnector

/// TLS connector for establishing secure connections
pub struct TlsConnector {
    config: TlsConfig,
}

/// TLS version preference
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsVersion {
    /// TLS 1.3 only
    Tls13,
    /// TLS 1.2 only
    #[cfg(feature = "tls12")]
    Tls12,
    /// Try TLS 1.3 first.
    ///
    /// Note: This does NOT automatically fall back to TLS 1.2 on failure because
    /// streams cannot be cloned/reused after a failed handshake. If TLS 1.3 fails,
    /// you must create a new connection and try with `Tls12` explicitly.
    ///
    /// For automatic fallback, see the http.rs example which creates a new stream
    /// and retries with TLS 1.2 when TLS 1.3 fails.
    #[cfg(feature = "tls12")]
    Prefer13,
}

impl Default for TlsVersion {
    fn default() -> Self {
        TlsVersion::Tls13
    }
}

/// TLS configuration
#[derive(Clone)]
pub struct TlsConfig {
    /// Skip certificate verification (INSECURE - for testing only)
    pub skip_verification: bool,
    /// Application-Layer Protocol Negotiation protocols
    pub alpn_protocols: Vec<String>,
    /// TLS version preference
    pub version: TlsVersion,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            skip_verification: false,
            alpn_protocols: vec!["http/1.1".to_string()],
            version: TlsVersion::default(),
        }
    }
}

/// Wrapper enum for TLS streams (1.2 or 1.3)
#[cfg(feature = "tls12")]
pub enum TlsStreamWrapper<S> {
    Tls13(TlsStream<S>),
    Tls12(TlsStream12<S>),
}

#[cfg(feature = "tls12")]
impl<S> TlsStreamWrapper<S>
where
    S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
{
    /// Read application data
    pub async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            TlsStreamWrapper::Tls13(s) => s.read(buf).await,
            TlsStreamWrapper::Tls12(s) => s.read(buf).await,
        }
    }

    /// Write application data
    pub async fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            TlsStreamWrapper::Tls13(s) => s.write(buf).await,
            TlsStreamWrapper::Tls12(s) => s.write(buf).await,
        }
    }

    /// Flush the stream
    pub async fn flush(&mut self) -> std::io::Result<()> {
        match self {
            TlsStreamWrapper::Tls13(s) => s.flush().await,
            TlsStreamWrapper::Tls12(s) => s.flush().await,
        }
    }

    /// Get the peer certificate (DER-encoded)
    pub fn peer_certificate(&self) -> Option<&[u8]> {
        match self {
            TlsStreamWrapper::Tls13(s) => s.peer_certificate(),
            TlsStreamWrapper::Tls12(s) => s.peer_certificate(),
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
    /// Uses TLS 1.3 only (for backward compatibility)
    pub async fn connect<S>(&self, stream: S, server_name: &str) -> Result<TlsStream<S>>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        TlsStream::connect(stream, server_name, self.config.clone()).await
    }

    /// Connect to a server with version-aware TLS
    /// Returns a wrapper that handles both TLS 1.2 and 1.3 streams
    #[cfg(feature = "tls12")]
    pub async fn connect_versioned<S>(
        &self,
        stream: S,
        server_name: &str,
    ) -> Result<TlsStreamWrapper<S>>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        match self.config.version {
            TlsVersion::Tls13 => {
                let s = TlsStream::connect(stream, server_name, self.config.clone()).await?;
                Ok(TlsStreamWrapper::Tls13(s))
            }
            TlsVersion::Tls12 => {
                let s = TlsStream12::connect(stream, server_name, self.config.clone()).await?;
                Ok(TlsStreamWrapper::Tls12(s))
            }
            TlsVersion::Prefer13 => {
                // For now, just try TLS 1.3 - true fallback would require stream cloning
                // which is complex. Instead, caller can retry with Tls12 on failure.
                match TlsStream::connect(stream, server_name, self.config.clone()).await {
                    Ok(s) => Ok(TlsStreamWrapper::Tls13(s)),
                    Err(e) => {
                        tracing::warn!(
                            "TLS 1.3 failed: {}, use Tls12 version explicitly for fallback",
                            e
                        );
                        Err(e)
                    }
                }
            }
        }
    }

    /// Connect using TLS 1.2 explicitly
    #[cfg(feature = "tls12")]
    pub async fn connect_tls12<S>(&self, stream: S, server_name: &str) -> Result<TlsStream12<S>>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        TlsStream12::connect(stream, server_name, self.config.clone()).await
    }
}

impl Default for TlsConnector {
    fn default() -> Self {
        Self::new()
    }
}
