//! WebSocket-based Snowflake transport
//!
//! This module provides Snowflake connectivity using WebSocket instead of WebRTC.
//! This is simpler and more reliable in browsers since WebSocket has native support
//! without the complexity of WebRTC signaling.
//!
//! Protocol stack (bottom to top):
//!   WebSocket (wss://snowflake.torproject.net/)
//!       ↓
//!   Turbo (framing + obfuscation)
//!       ↓
//!   KCP (reliability + ordering)
//!       ↓
//!   SMUX (stream multiplexing)
//!       ↓
//!   TLS (link encryption)
//!       ↓
//!   Tor protocol

use crate::error::{Result, TorError};
use crate::websocket::WebSocketStream;
use futures::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::info;

#[cfg(target_arch = "wasm32")]
use crate::kcp_stream::{KcpConfig, KcpStream};
#[cfg(target_arch = "wasm32")]
use crate::smux::SmuxStream;
#[cfg(target_arch = "wasm32")]
use crate::turbo::TurboStream;
#[cfg(target_arch = "wasm32")]
use subtle_tls::{TlsConfig, TlsConnector, TlsStream};

/// WebSocket Snowflake endpoints
pub const SNOWFLAKE_WS_URL: &str = "wss://snowflake.torproject.net/";
pub const SNOWFLAKE_WS_URL_ALT: &str = "wss://snowflake.bamsoftware.com/";

/// Snowflake bridge fingerprint (Tor Project's primary Snowflake bridge)
pub const SNOWFLAKE_FINGERPRINT: &str = "2B280B23E1107BB62ABFC40DDCC8824814F80A72";

/// WebSocket Snowflake configuration
#[derive(Debug, Clone)]
pub struct SnowflakeWsConfig {
    /// WebSocket URL for Snowflake endpoint
    pub ws_url: String,
    /// Bridge fingerprint
    pub fingerprint: String,
    /// KCP conversation ID (0 for default)
    pub kcp_conv: u32,
    /// SMUX stream ID (default: 3)
    pub smux_stream_id: u32,
}

impl Default for SnowflakeWsConfig {
    fn default() -> Self {
        Self {
            ws_url: SNOWFLAKE_WS_URL.to_string(),
            fingerprint: SNOWFLAKE_FINGERPRINT.to_string(),
            kcp_conv: 0,
            smux_stream_id: 3,
        }
    }
}

impl SnowflakeWsConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.ws_url = url.to_string();
        self
    }

    pub fn with_fingerprint(mut self, fingerprint: &str) -> Self {
        self.fingerprint = fingerprint.to_string();
        self
    }
}

/// Inner stream type for WASM
#[cfg(target_arch = "wasm32")]
type SnowflakeWsStack = SmuxStream<KcpStream<TurboStream<WebSocketStream>>>;

#[cfg(target_arch = "wasm32")]
enum SnowflakeWsInner {
    Connected(TlsStream<SnowflakeWsStack>),
}

#[cfg(not(target_arch = "wasm32"))]
enum SnowflakeWsInner {
    #[allow(dead_code)]
    Placeholder,
}

/// WebSocket-based Snowflake stream
pub struct SnowflakeWsStream {
    inner: SnowflakeWsInner,
}

// Safety: WASM is single-threaded
unsafe impl Send for SnowflakeWsStream {}

impl SnowflakeWsStream {
    /// Connect to Snowflake via WebSocket
    #[cfg(target_arch = "wasm32")]
    pub async fn connect(config: SnowflakeWsConfig) -> Result<Self> {
        info!("Connecting to Snowflake via WebSocket");
        info!("URL: {}", config.ws_url);
        info!("Fingerprint: {}", config.fingerprint);

        // 1. Establish WebSocket connection
        info!("Opening WebSocket connection...");
        let ws = WebSocketStream::connect(&config.ws_url).await?;
        info!("WebSocket connected");

        // 2. Wrap with Turbo framing
        info!("Initializing Turbo layer...");
        let mut turbo = TurboStream::new(ws);
        turbo.initialize().await?;
        info!("Turbo layer initialized");

        // 3. Wrap with KCP for reliability
        info!("Initializing KCP layer...");
        let kcp_config = KcpConfig {
            conv: config.kcp_conv,
            ..Default::default()
        };
        let kcp = KcpStream::new(turbo, kcp_config);
        info!("KCP layer initialized");

        // 4. Wrap with SMUX for multiplexing
        info!("Initializing SMUX layer...");
        let mut smux = SmuxStream::with_stream_id(kcp, config.smux_stream_id);
        smux.initialize().await?;
        info!("SMUX layer initialized");

        // 5. Wrap with TLS
        info!("Establishing TLS...");
        let tls_config = TlsConfig {
            skip_verification: true, // Tor uses self-signed certs
            alpn_protocols: vec![],
            ..Default::default()
        };
        let connector = TlsConnector::with_config(tls_config);
        info!("Snowflake: calling TLS connect...");
        let tls_result = connector.connect(smux, "www.example.com").await;
        info!("Snowflake: TLS connect returned");
        let tls_stream =
            tls_result.map_err(|e| TorError::tls(format!("TLS handshake failed: {}", e)))?;
        info!("TLS layer established");

        info!("Snowflake WS connection established: WebSocket → Turbo → KCP → SMUX → TLS");

        Ok(Self {
            inner: SnowflakeWsInner::Connected(tls_stream),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn connect(_config: SnowflakeWsConfig) -> Result<Self> {
        Err(TorError::Internal(
            "WebSocket Snowflake is only available in WASM".to_string(),
        ))
    }
}

impl tor_rtcompat::StreamOps for SnowflakeWsStream {}

impl tor_rtcompat::CertifiedConn for SnowflakeWsStream {
    fn peer_certificate(&self) -> io::Result<Option<Vec<u8>>> {
        match &self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeWsInner::Connected(tls) => {
                Ok(tls.peer_certificate().map(|cert| cert.to_vec()))
            }
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeWsInner::Placeholder => unreachable!(),
        }
    }

    fn export_keying_material(
        &self,
        len: usize,
        _label: &[u8],
        _context: Option<&[u8]>,
    ) -> io::Result<Vec<u8>> {
        match &self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeWsInner::Connected(_tls) => {
                tracing::warn!("export_keying_material not fully implemented");
                Ok(vec![0u8; len])
            }
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeWsInner::Placeholder => unreachable!(),
        }
    }
}

impl AsyncRead for SnowflakeWsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeWsInner::Connected(tls) => Pin::new(tls).poll_read(cx, buf),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeWsInner::Placeholder => unreachable!(),
        }
    }
}

impl AsyncWrite for SnowflakeWsStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeWsInner::Connected(tls) => Pin::new(tls).poll_write(cx, buf),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeWsInner::Placeholder => unreachable!(),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeWsInner::Connected(tls) => Pin::new(tls).poll_flush(cx),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeWsInner::Placeholder => unreachable!(),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeWsInner::Connected(tls) => Pin::new(tls).poll_close(cx),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeWsInner::Placeholder => unreachable!(),
        }
    }
}

/// Convenience function to create a WebSocket Snowflake stream
pub async fn create_snowflake_ws_stream() -> Result<SnowflakeWsStream> {
    SnowflakeWsStream::connect(SnowflakeWsConfig::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = SnowflakeWsConfig::default();
        assert_eq!(config.ws_url, SNOWFLAKE_WS_URL);
        assert_eq!(config.fingerprint, SNOWFLAKE_FINGERPRINT);
        assert_eq!(config.kcp_conv, 0);
        assert_eq!(config.smux_stream_id, 3);
    }
}
