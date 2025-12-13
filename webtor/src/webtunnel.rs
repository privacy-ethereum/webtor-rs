//! WebTunnel pluggable transport for Tor connections
//!
//! WebTunnel is a pluggable transport that mimics HTTPS/WebSocket traffic
//! for censorship evasion. Despite using WebSocket-like headers, it does NOT
//! use actual WebSocket framing - after the HTTP Upgrade, raw bytes are sent.
//!
//! Protocol flow:
//! 1. Connect via TLS to the bridge (outer TLS for HTTPS)
//! 2. Send HTTP Upgrade request with WebSocket headers (mimics WebSocket)
//! 3. Receive 101 Switching Protocols
//! 4. Establish Tor link TLS over the tunnel (inner TLS to Tor relay)
//! 5. Send/receive Tor channel handshake over the inner TLS
//!
//! The two TLS layers are:
//! - Outer TLS: Client ↔ WebTunnel bridge (WebPKI validated, HTTPS style)
//! - Inner TLS: Client ↔ Tor relay (tunneled, self-signed certs, validated via CERTS cells)
//!
//! Reference: https://gitlab.torproject.org/tpo/anti-censorship/pluggable-transports/webtunnel

use crate::error::{Result, TorError};
use futures::{AsyncRead, AsyncWrite};
use futures_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use futures_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use futures_rustls::rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};
use tracing::{debug, info};
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

    pub fn with_server_name(mut self, name: String) -> Self {
        self.server_name = Some(name);
        self
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

    /// Connect to the WebTunnel bridge
    ///
    /// Performs:
    /// 1. TCP connection
    /// 2. TLS handshake
    /// 3. HTTP Upgrade with WebSocket-like headers
    /// 4. Returns raw TLS stream for Tor protocol
    pub async fn connect(&self) -> Result<WebTunnelStream> {
        use rustls_pki_types::ServerName;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let url = Url::parse(&self.config.url)
            .map_err(|e| TorError::Configuration(format!("Invalid URL: {}", e)))?;

        let host = url
            .host_str()
            .ok_or_else(|| TorError::Configuration("URL missing host".to_string()))?;
        let port = url.port().unwrap_or(443);
        let path = url.path();

        info!(
            "Connecting to WebTunnel bridge at {}:{}{}",
            host, port, path
        );

        // 1. Connect TCP
        let tcp_stream = TcpStream::connect(format!("{}:{}", host, port))
            .await
            .map_err(|e| TorError::Network(format!("TCP connection failed: {}", e)))?;

        debug!("TCP connected to {}:{}", host, port);

        // 2. Setup TLS
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));

        // Use custom SNI if provided, otherwise use host from URL
        let sni_host = self.config.server_name.as_deref().unwrap_or(host);
        let server_name = ServerName::try_from(sni_host.to_string())
            .map_err(|e| TorError::Configuration(format!("Invalid server name: {}", e)))?;

        let mut tls_stream = connector
            .connect(server_name, tcp_stream)
            .await
            .map_err(|e| TorError::Network(format!("TLS handshake failed: {}", e)))?;

        debug!("TLS connected");

        // 3. Send HTTP Upgrade request with WebSocket-like headers
        // Note: We send Sec-WebSocket-Key for compatibility, but the server
        // may not validate or return Sec-WebSocket-Accept (WebTunnel doesn't require it)
        let ws_key = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &rand::random::<[u8; 16]>(),
        );

        let request = format!(
            "GET {} HTTP/1.1\r\n\
            Host: {}\r\n\
            Connection: Upgrade\r\n\
            Upgrade: websocket\r\n\
            Sec-WebSocket-Key: {}\r\n\
            Sec-WebSocket-Version: 13\r\n\
            \r\n",
            path, host, ws_key
        );

        debug!("Sending HTTP Upgrade request");

        tls_stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| TorError::Network(format!("Failed to send upgrade request: {}", e)))?;
        tls_stream
            .flush()
            .await
            .map_err(|e| TorError::Network(format!("Failed to flush upgrade request: {}", e)))?;

        // 4. Read response headers
        let mut response = Vec::new();
        let mut byte = [0u8; 1];

        loop {
            tls_stream
                .read_exact(&mut byte)
                .await
                .map_err(|e| TorError::Network(format!("Failed to read response: {}", e)))?;
            response.push(byte[0]);

            // Look for end of headers
            if response.len() >= 4 && &response[response.len() - 4..] == b"\r\n\r\n" {
                break;
            }

            if response.len() > 8192 {
                return Err(TorError::Network("Response headers too long".to_string()));
            }
        }

        let response_str = String::from_utf8_lossy(&response);
        debug!(
            "HTTP response: {}",
            response_str.lines().next().unwrap_or("(empty)")
        );

        // Check for 101 Switching Protocols
        if !response_str.contains("101") {
            return Err(TorError::Network(format!(
                "Expected 101 Switching Protocols, got: {}",
                response_str.lines().next().unwrap_or("(empty)")
            )));
        }

        // Verify Upgrade and Connection headers
        let response_lower = response_str.to_lowercase();
        if !response_lower.contains("upgrade") || !response_lower.contains("connection") {
            return Err(TorError::Network(format!(
                "Missing Upgrade/Connection headers in response: {}",
                response_str
            )));
        }

        info!("WebTunnel HTTP Upgrade complete, establishing Tor link TLS");

        // 5. Establish Tor link TLS over the tunneled connection
        // This is the INNER TLS layer - from client directly to Tor relay.
        // The WebTunnel bridge forwards these bytes to the Tor ORPort.
        //
        // Tor's link TLS uses self-signed certificates that are validated
        // via CERTS cells during the channel handshake, not via WebPKI.
        // So we use a custom verifier that accepts any certificate.
        let tor_tls_config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TorCertVerifier))
            .with_no_client_auth();

        let tor_connector = futures_rustls::TlsConnector::from(Arc::new(tor_tls_config));

        // Wrap the outer TLS stream for futures compatibility
        let compat_stream = tls_stream.compat();

        // Use a random hostname for SNI (Tor relays don't care about SNI)
        // Some bridges may log SNI so we use something innocuous
        let sni = ServerName::try_from("www.example.com".to_string())
            .map_err(|e| TorError::Configuration(format!("Invalid SNI: {}", e)))?;

        debug!("Starting Tor link TLS handshake");
        let tor_tls_stream = tor_connector
            .connect(sni, compat_stream)
            .await
            .map_err(|e| TorError::Network(format!("Tor link TLS handshake failed: {}", e)))?;

        info!("Tor link TLS established, ready for channel handshake");

        Ok(WebTunnelStream {
            inner: tor_tls_stream,
        })
    }
}

/// Custom certificate verifier for Tor link TLS
///
/// Tor relays use self-signed certificates. The actual authentication happens
/// via CERTS cells during the Tor channel handshake, not via the TLS layer.
/// This verifier accepts any certificate, leaving validation to the Tor protocol.
#[derive(Debug)]
struct TorCertVerifier;

impl ServerCertVerifier for TorCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, futures_rustls::rustls::Error> {
        // Accept any certificate - Tor validates via CERTS cells
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, futures_rustls::rustls::Error> {
        // Accept signature - Tor validates via CERTS cells
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, futures_rustls::rustls::Error> {
        // Accept signature - Tor validates via CERTS cells
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // Support common signature schemes
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// Type aliases for the nested TLS stream
///
/// The stream architecture is:
/// - TCP socket
/// - Outer TLS (tokio_rustls, HTTPS to WebTunnel bridge)
/// - Compat wrapper (tokio::io -> futures::io)
/// - Inner TLS (futures_rustls, Tor link protocol to relay)
type OuterTlsStream = tokio_rustls::client::TlsStream<tokio::net::TcpStream>;
type CompatOuterTlsStream = Compat<OuterTlsStream>;
type TorLinkTlsStream = futures_rustls::client::TlsStream<CompatOuterTlsStream>;

/// WebTunnel stream for Tor communication
///
/// This wraps a Tor link TLS stream that is tunneled through WebTunnel.
/// The architecture is:
/// - Outer TLS: Client ↔ WebTunnel bridge (validates bridge cert via WebPKI)
/// - HTTP Upgrade: Mimics WebSocket to bypass protocol filters
/// - Inner TLS: Client ↔ Tor relay (tunneled, uses TorCertVerifier)
///
/// The inner TLS stream implements futures::io::AsyncRead/Write which
/// tor_proto expects for the channel handshake.
pub struct WebTunnelStream {
    inner: TorLinkTlsStream,
}

impl WebTunnelStream {
    /// Get the peer certificate from the Tor link TLS connection
    ///
    /// This returns the DER-encoded certificate of the Tor relay.
    /// Used by tor_proto during the channel handshake to verify
    /// that the CERTS cells properly authenticate this certificate.
    fn get_peer_certificate(&self) -> io::Result<Option<Vec<u8>>> {
        let (_, session) = self.inner.get_ref();
        Ok(session
            .peer_certificates()
            .and_then(|certs| certs.first().map(|c| Vec::from(c.as_ref()))))
    }

    /// Close the WebTunnel stream
    pub async fn close(&mut self) -> io::Result<()> {
        use futures::AsyncWriteExt;
        info!("Closing WebTunnel stream");
        self.inner.close().await
    }
}

// SAFETY: WebTunnelStream wraps TorLinkTlsStream which is:
// - futures_rustls::client::TlsStream<Compat<tokio_rustls::client::TlsStream<TcpStream>>>
// - All inner types (TcpStream, tokio_rustls::TlsStream, Compat, futures_rustls::TlsStream)
//   implement Send + Sync when their generic parameters do
// - tokio::net::TcpStream is Send + Sync
// - This is required because tor_proto::Channel requires Send + Sync bounds on its transport
unsafe impl Send for WebTunnelStream {}
unsafe impl Sync for WebTunnelStream {}

impl tor_rtcompat::StreamOps for WebTunnelStream {
    // Default implementation
}

impl tor_rtcompat::CertifiedConn for WebTunnelStream {
    fn peer_certificate(&self) -> io::Result<Option<Vec<u8>>> {
        self.get_peer_certificate()
    }

    fn export_keying_material(
        &self,
        len: usize,
        label: &[u8],
        context: Option<&[u8]>,
    ) -> io::Result<Vec<u8>> {
        let (_, session) = self.inner.get_ref();
        session
            .export_keying_material(Vec::with_capacity(len), label, context)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

impl AsyncRead for WebTunnelStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(n)) = &result {
            tracing::trace!("WebTunnelStream poll_read: {} bytes", n);
        }
        result
    }
}

impl AsyncWrite for WebTunnelStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        tracing::trace!("WebTunnelStream poll_write: {} bytes", buf.len());
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
    fn test_config_creation() {
        let config = WebTunnelConfig::new(
            "https://example.com/secret-path".to_string(),
            "AAAA".repeat(10),
        );
        assert_eq!(config.url, "https://example.com/secret-path");
        assert_eq!(config.fingerprint, "AAAA".repeat(10));
        assert!(config.server_name.is_none());
    }

    #[test]
    fn test_config_with_timeout() {
        let config =
            WebTunnelConfig::new("https://example.com/path".to_string(), "AAAA".repeat(10))
                .with_timeout(Duration::from_secs(60));

        assert_eq!(config.connection_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_config_with_server_name() {
        let config =
            WebTunnelConfig::new("https://example.com/path".to_string(), "AAAA".repeat(10))
                .with_server_name("custom.example.com".to_string());

        assert_eq!(config.server_name, Some("custom.example.com".to_string()));
    }
}
