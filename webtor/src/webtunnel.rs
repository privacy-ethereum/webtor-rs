//! WebTunnel pluggable transport for Tor connections
//!
//! WebTunnel is a pluggable transport that wraps Tor traffic in HTTPS,
//! using WebSocket protocol for the actual data transport.
//!
//! Protocol flow:
//! 1. Connect to bridge via WebSocket (wss://)
//! 2. WebSocket handles TLS + HTTP Upgrade automatically
//! 3. Send/receive Tor cells as WebSocket binary frames

use crate::error::{Result, TorError};
use crate::websocket::WebSocketStream;
use futures::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tracing::info;
use url::Url;

/// WebTunnel bridge configuration
#[derive(Debug, Clone)]
pub struct WebTunnelConfig {
    /// Full URL to the WebTunnel endpoint (e.g., https://example.com/secret-path)
    pub url: String,
    /// Bridge fingerprint (RSA identity, 40 hex chars)
    pub fingerprint: String,
    /// Optional: Override server name for TLS SNI
    pub server_name: Option<String>,
    /// Connection timeout
    pub connection_timeout: Duration,
}

impl WebTunnelConfig {
    pub fn new(url: String, fingerprint: String) -> Self {
        Self {
            url,
            fingerprint,
            server_name: None,
            connection_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    pub fn with_server_name(self, _name: String) -> Self {
        // Server name is extracted from URL for WebSocket
        self
    }

    /// Convert HTTPS URL to WSS URL for WebSocket connection
    fn to_wss_url(&self) -> Result<String> {
        let url = Url::parse(&self.url)
            .map_err(|e| TorError::Configuration(format!("Invalid WebTunnel URL: {}", e)))?;

        // Convert https:// to wss://
        let scheme = match url.scheme() {
            "https" => "wss",
            "http" => "ws",
            "wss" | "ws" => url.scheme(),
            _ => return Err(TorError::Configuration(format!(
                "Invalid WebTunnel URL scheme: {}. Expected https, http, wss, or ws.",
                url.scheme()
            ))),
        };

        let host = url.host_str()
            .ok_or_else(|| TorError::Configuration("WebTunnel URL missing host".to_string()))?;

        let port = url.port();
        let path = url.path();

        let wss_url = if let Some(p) = port {
            format!("{}://{}:{}{}", scheme, host, p, path)
        } else {
            format!("{}://{}{}", scheme, host, path)
        };

        Ok(wss_url)
    }
}

/// WebTunnel bridge connection manager
pub struct WebTunnelBridge {
    config: WebTunnelConfig,
}

impl WebTunnelBridge {
    pub fn new(config: WebTunnelConfig) -> Self {
        Self { config }
    }

    /// Connect to the WebTunnel bridge via WebSocket
    pub async fn connect(&self) -> Result<WebTunnelStream> {
        let wss_url = self.config.to_wss_url()?;
        
        info!("Connecting to WebTunnel bridge at {}", wss_url);

        // Connect via WebSocket - this handles TLS, HTTP Upgrade, and WebSocket framing
        let ws_stream = WebSocketStream::connect(&wss_url).await?;
        
        info!("WebTunnel WebSocket connection established");

        Ok(WebTunnelStream { inner: ws_stream })
    }
}

/// WebTunnel stream for Tor communication
/// Wraps a WebSocket connection that carries Tor cells as binary frames
pub struct WebTunnelStream {
    inner: WebSocketStream,
}

// Safety: Same reasoning as SnowflakeStream - WASM is single-threaded
unsafe impl Send for WebTunnelStream {}

impl tor_rtcompat::StreamOps for WebTunnelStream {
    // Use default implementation
}

impl WebTunnelStream {
    /// Close the WebTunnel stream
    pub async fn close(&mut self) -> io::Result<()> {
        info!("Closing WebTunnel stream");
        use futures::AsyncWriteExt;
        self.inner.close().await
    }
}

impl AsyncRead for WebTunnelStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for WebTunnelStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

/// Create a WebTunnel stream (convenience function)
pub async fn create_webtunnel_stream(config: WebTunnelConfig) -> Result<WebTunnelStream> {
    let bridge = WebTunnelBridge::new(config);
    bridge.connect().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_to_wss_url() {
        let config = WebTunnelConfig::new(
            "https://example.com/secret-path".to_string(),
            "AAAA".repeat(10),
        );
        let wss = config.to_wss_url().unwrap();
        assert_eq!(wss, "wss://example.com/secret-path");
    }

    #[test]
    fn test_config_to_wss_url_with_port() {
        let config = WebTunnelConfig::new(
            "https://example.com:8443/path".to_string(),
            "AAAA".repeat(10),
        );
        let wss = config.to_wss_url().unwrap();
        assert_eq!(wss, "wss://example.com:8443/path");
    }

    #[test]
    fn test_config_already_wss() {
        let config = WebTunnelConfig::new(
            "wss://example.com/path".to_string(),
            "AAAA".repeat(10),
        );
        let wss = config.to_wss_url().unwrap();
        assert_eq!(wss, "wss://example.com/path");
    }
}
