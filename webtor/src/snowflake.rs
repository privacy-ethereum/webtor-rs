//! Snowflake bridge implementation for Tor connections
//!
//! Snowflake is a pluggable transport that routes traffic through volunteer
//! proxies using WebRTC. The protocol stack is:
//!
//!   WebRTC DataChannel (to volunteer proxy)
//!       ↓
//!   Turbo (framing + obfuscation)
//!       ↓
//!   KCP (reliability + ordering)
//!       ↓
//!   SMUX (stream multiplexing)
//!       ↓
//!   Tor protocol
//!
//! Note: Direct WebSocket to wss://snowflake.torproject.net/ is for volunteer
//! proxies, not clients. Clients must use WebRTC via the broker.

use crate::error::Result;
#[cfg(target_arch = "wasm32")]
use crate::kcp_stream::{KcpConfig, KcpStream};
#[cfg(target_arch = "wasm32")]
use crate::smux::SmuxStream;
use crate::snowflake_broker::{BROKER_URL, DEFAULT_BRIDGE_FINGERPRINT};
#[cfg(target_arch = "wasm32")]
use crate::turbo::TurboStream;
use futures::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tracing::{info, warn};

#[cfg(target_arch = "wasm32")]
use crate::webrtc_stream::WebRtcStream;

#[cfg(target_arch = "wasm32")]
use subtle_tls::{TlsConfig, TlsConnector, TlsStream};

/// Snowflake bridge configuration
#[derive(Debug, Clone)]
pub struct SnowflakeConfig {
    /// Broker URL for WebRTC signaling
    pub broker_url: String,
    /// Bridge fingerprint for verification
    pub fingerprint: String,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// KCP conversation ID (0 for Snowflake)
    pub kcp_conv: Option<u32>,
    /// SMUX stream ID (default: 3)
    pub smux_stream_id: Option<u32>,
}

impl SnowflakeConfig {
    /// Create a new Snowflake config with default Tor Project broker
    pub fn new() -> Self {
        Self {
            broker_url: BROKER_URL.to_string(),
            fingerprint: DEFAULT_BRIDGE_FINGERPRINT.to_string(),
            connection_timeout: Duration::from_secs(60),
            kcp_conv: None,
            smux_stream_id: None,
        }
    }

    /// Create config with custom broker URL
    pub fn with_broker(broker_url: String) -> Self {
        Self {
            broker_url,
            ..Self::new()
        }
    }

    /// Set bridge fingerprint
    pub fn with_fingerprint(mut self, fingerprint: String) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    /// Set connection timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Set SMUX stream ID
    pub fn with_stream_id(mut self, stream_id: u32) -> Self {
        self.smux_stream_id = Some(stream_id);
        self
    }
}

impl Default for SnowflakeConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Snowflake bridge connection manager
pub struct SnowflakeBridge {
    #[allow(dead_code)] // Used in wasm32 target
    config: SnowflakeConfig,
}

impl SnowflakeBridge {
    /// Create with default configuration
    pub fn new() -> Self {
        Self {
            config: SnowflakeConfig::new(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: SnowflakeConfig) -> Self {
        Self { config }
    }

    /// Connect to the Snowflake bridge via WebRTC (WASM) or fallback (native)
    #[cfg(target_arch = "wasm32")]
    pub async fn connect(&self) -> Result<SnowflakeStream> {
        use crate::error::TorError;

        const MAX_WEBRTC_RETRIES: u32 = 3;

        info!("Connecting to Snowflake via WebRTC");
        info!("Broker: {}", self.config.broker_url);
        info!("Fingerprint: {}", self.config.fingerprint);

        // 1. Establish WebRTC connection via broker (with retry for unreliable proxies)
        let mut webrtc = None;
        let mut last_error = None;

        for attempt in 1..=MAX_WEBRTC_RETRIES {
            info!(
                "Connecting to volunteer proxy via WebRTC (attempt {}/{})...",
                attempt, MAX_WEBRTC_RETRIES
            );

            match WebRtcStream::connect(&self.config.broker_url, &self.config.fingerprint).await {
                Ok(stream) => {
                    info!("WebRTC DataChannel established on attempt {}", attempt);
                    webrtc = Some(stream);
                    break;
                }
                Err(e) => {
                    let err_str = e.to_string();
                    warn!("WebRTC connection attempt {} failed: {}", attempt, err_str);
                    last_error = Some(e);

                    // Only retry on timeout errors (proxy didn't respond)
                    if !err_str.contains("timeout") {
                        return Err(last_error.unwrap());
                    }

                    if attempt < MAX_WEBRTC_RETRIES {
                        info!("Retrying with a different volunteer proxy...");
                    }
                }
            }
        }

        let webrtc = webrtc.ok_or_else(|| {
            last_error.unwrap_or_else(|| {
                TorError::Network("WebRTC connection failed after all retries".to_string())
            })
        })?;
        info!("WebRTC DataChannel established");

        // 2. Wrap with Turbo framing
        info!("Initializing Turbo layer...");
        let mut turbo = TurboStream::new(webrtc);
        turbo.initialize().await?;
        info!("Turbo layer initialized");

        // 3. Wrap with KCP for reliability
        info!("Initializing KCP layer...");
        let kcp_config = KcpConfig {
            conv: self.config.kcp_conv.unwrap_or(0),
            ..Default::default()
        };
        let kcp = KcpStream::new(turbo, kcp_config);
        info!("KCP layer initialized");

        // 4. Wrap with SMUX for multiplexing
        info!("Initializing SMUX layer...");
        let stream_id = self.config.smux_stream_id.unwrap_or(3);
        let mut smux = SmuxStream::with_stream_id(kcp, stream_id);
        smux.initialize().await?;
        info!("SMUX layer initialized");

        // 5. Wrap with TLS for Tor link encryption
        // Tor relays use self-signed certificates, so skip verification
        // (authentication happens via CERTS cells in the Tor protocol)
        info!("Establishing TLS over SMUX...");
        let tls_config = TlsConfig {
            skip_verification: true, // Tor uses self-signed certs, validated via CERTS cells
            alpn_protocols: vec![],
            ..Default::default()
        };
        let connector = TlsConnector::with_config(tls_config);
        // Use a placeholder server name since Tor doesn't use SNI
        let tls_stream = connector
            .connect(smux, "www.example.com")
            .await
            .map_err(|e| crate::error::TorError::tls(format!("TLS handshake failed: {}", e)))?;
        info!("TLS layer established over SMUX");

        info!("Snowflake connection established: WebRTC → Turbo → KCP → SMUX → TLS");

        Ok(SnowflakeStream {
            inner: SnowflakeInner::WebRtc(tls_stream),
        })
    }

    /// Native: WebRTC not implemented, return error
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn connect(&self) -> Result<SnowflakeStream> {
        use crate::error::TorError;

        // Native WebRTC requires the webrtc-rs crate which is complex to set up.
        // For native builds, recommend using WebTunnel instead.
        Err(TorError::Internal(
            "Snowflake requires WebRTC which is only available in WASM. \
             For native builds, use WebTunnel bridge instead."
                .to_string(),
        ))
    }
}

impl Default for SnowflakeBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Inner stream type (WebRTC on WASM, wrapped with TLS)
#[cfg(target_arch = "wasm32")]
type SnowflakeSmuxStack = SmuxStream<KcpStream<TurboStream<WebRtcStream>>>;

#[cfg(target_arch = "wasm32")]
enum SnowflakeInner {
    WebRtc(TlsStream<SnowflakeSmuxStack>),
}

/// Native stub - Snowflake not supported on native (use WebTunnel instead)
#[cfg(not(target_arch = "wasm32"))]
enum SnowflakeInner {
    // Placeholder variant to make the enum non-empty
    #[allow(dead_code)]
    Placeholder,
}

/// Snowflake stream for Tor communication
pub struct SnowflakeStream {
    inner: SnowflakeInner,
}

// Safety: WASM is single-threaded. Native streams handle their own thread safety.
unsafe impl Send for SnowflakeStream {}

impl tor_rtcompat::StreamOps for SnowflakeStream {
    // Use default implementation
}

impl tor_rtcompat::CertifiedConn for SnowflakeStream {
    fn peer_certificate(&self) -> io::Result<Option<Vec<u8>>> {
        match &self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeInner::WebRtc(tls) => {
                // TlsStream::peer_certificate returns Option<&[u8]>
                // CertifiedConn trait expects io::Result<Option<Vec<u8>>>
                Ok(tls.peer_certificate().map(|cert| cert.to_vec()))
            }
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeInner::Placeholder => unreachable!("Snowflake not available on native"),
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
            SnowflakeInner::WebRtc(_tls) => {
                // TLS 1.3 keying material export is complex
                // For now, return zeros as a placeholder
                // TODO: Implement proper RFC 5705 key export
                tracing::warn!("export_keying_material called but not fully implemented");
                Ok(vec![0u8; len])
            }
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeInner::Placeholder => unreachable!("Snowflake not available on native"),
        }
    }
}

impl SnowflakeStream {
    /// Close the Snowflake stream
    pub async fn close(&mut self) -> io::Result<()> {
        info!("Closing Snowflake stream");
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeInner::WebRtc(tls) => tls
                .close()
                .await
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string())),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeInner::Placeholder => unreachable!("Snowflake not available on native"),
        }
    }
}

impl AsyncRead for SnowflakeStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeInner::WebRtc(tls) => Pin::new(tls).poll_read(_cx, _buf),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeInner::Placeholder => unreachable!("Snowflake not available on native"),
        }
    }
}

impl AsyncWrite for SnowflakeStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeInner::WebRtc(tls) => Pin::new(tls).poll_write(_cx, _buf),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeInner::Placeholder => unreachable!("Snowflake not available on native"),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeInner::WebRtc(tls) => Pin::new(tls).poll_flush(_cx),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeInner::Placeholder => unreachable!("Snowflake not available on native"),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            #[cfg(target_arch = "wasm32")]
            SnowflakeInner::WebRtc(tls) => Pin::new(tls).poll_close(_cx),
            #[cfg(not(target_arch = "wasm32"))]
            SnowflakeInner::Placeholder => unreachable!("Snowflake not available on native"),
        }
    }
}

/// Create a new Snowflake stream using WebRTC (convenience function)
pub async fn create_snowflake_stream(
    _broker_url: &str, // Kept for API compatibility but ignored
    _connection_timeout: Duration,
) -> Result<SnowflakeStream> {
    // Use default config with WebRTC
    let bridge = SnowflakeBridge::new();
    bridge.connect().await
}

/// Create a Snowflake stream with full configuration
pub async fn create_snowflake_stream_with_config(
    config: SnowflakeConfig,
) -> Result<SnowflakeStream> {
    let bridge = SnowflakeBridge::with_config(config);
    bridge.connect().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snowflake_config_default() {
        let config = SnowflakeConfig::new();
        assert_eq!(config.broker_url, BROKER_URL);
        assert_eq!(config.fingerprint, DEFAULT_BRIDGE_FINGERPRINT);
        assert_eq!(config.connection_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_snowflake_config_with_timeout() {
        let config = SnowflakeConfig::new().with_timeout(Duration::from_secs(120));
        assert_eq!(config.connection_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_snowflake_config_with_fingerprint() {
        let config = SnowflakeConfig::new().with_fingerprint("ABCD1234".to_string());
        assert_eq!(config.fingerprint, "ABCD1234");
    }
}
