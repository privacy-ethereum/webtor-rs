//! TLS support for Tor streams and direct connections
//!
//! This module provides TLS encryption for:
//! - Tor DataStreams (HTTPS over Tor)
//! - Direct connections (for bridge transport like WebTunnel)
//!
//! Note: In WASM, TLS is handled by the browser's native WebSocket (wss://)
//! and fetch API. The TLS functions here are only for native builds.

use crate::error::{Result, TorError};
use futures::io::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(not(target_arch = "wasm32"))]
use futures_rustls::rustls::{ClientConfig, RootCertStore};
#[cfg(not(target_arch = "wasm32"))]
use futures_rustls::TlsConnector;
#[cfg(not(target_arch = "wasm32"))]
use rustls_pki_types::ServerName;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use tracing::{debug, info};

/// Create a TLS connector with the default root certificates
#[cfg(not(target_arch = "wasm32"))]
pub fn create_tls_connector() -> Result<TlsConnector> {
    let mut root_store = RootCertStore::empty();
    
    // Add webpki root certificates
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    
    debug!("Loaded {} root certificates", root_store.len());
    
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    
    Ok(TlsConnector::from(Arc::new(config)))
}

/// Wrap an async stream with TLS encryption
#[cfg(not(target_arch = "wasm32"))]
pub async fn wrap_with_tls<S>(
    stream: S,
    domain: &str,
) -> Result<futures_rustls::client::TlsStream<S>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    info!("Initiating TLS handshake with {}", domain);
    
    let connector = create_tls_connector()?;
    
    let server_name = ServerName::try_from(domain.to_string())
        .map_err(|e| TorError::tls(format!("Invalid server name '{}': {}", domain, e)))?;
    
    let tls_stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| TorError::tls(format!("TLS handshake failed: {}", e)))?;
    
    info!("TLS handshake completed with {}", domain);
    
    Ok(tls_stream)
}

/// TLS stream for direct connections (e.g., WebTunnel bridge)
/// This wraps a native TCP+TLS connection, not a Tor stream.
#[cfg(not(target_arch = "wasm32"))]
pub struct TlsStream {
    inner: futures_rustls::client::TlsStream<TokioCompatStream>,
}

/// Wrapper to make tokio TcpStream compatible with futures AsyncRead/AsyncWrite
#[cfg(not(target_arch = "wasm32"))]
pub struct TokioCompatStream {
    inner: tokio::net::TcpStream,
}

// TcpStream is Unpin, so our wrapper is too
#[cfg(not(target_arch = "wasm32"))]
impl Unpin for TokioCompatStream {}

#[cfg(not(target_arch = "wasm32"))]
impl AsyncRead for TokioCompatStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        use tokio::io::AsyncRead as TokioAsyncRead;
        let mut read_buf = tokio::io::ReadBuf::new(buf);
        match Pin::new(&mut self.inner).poll_read(cx, &mut read_buf) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AsyncWrite for TokioCompatStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        use tokio::io::AsyncWrite as TokioAsyncWrite;
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        use tokio::io::AsyncWrite as TokioAsyncWrite;
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        use tokio::io::AsyncWrite as TokioAsyncWrite;
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl TlsStream {
    /// Connect to a host via TLS
    pub async fn connect(host: &str, port: u16, server_name: &str) -> Result<Self> {
        use tokio::net::TcpStream;
        
        info!("Connecting to {}:{} (SNI: {})", host, port, server_name);
        
        // 1. Establish TCP connection
        let addr = format!("{}:{}", host, port);
        let tcp_stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| TorError::Network(format!("TCP connection failed to {}: {}", addr, e)))?;
        
        debug!("TCP connection established to {}", addr);
        
        // 2. Wrap with TLS
        let connector = create_tls_connector()?;
        let sni = ServerName::try_from(server_name.to_string())
            .map_err(|e| TorError::tls(format!("Invalid server name '{}': {}", server_name, e)))?;
        
        // Wrap tokio TcpStream in our compat wrapper
        let compat_stream = TokioCompatStream { inner: tcp_stream };
        
        let tls_stream = connector
            .connect(sni, compat_stream)
            .await
            .map_err(|e| TorError::tls(format!("TLS handshake failed with {}: {}", server_name, e)))?;
        
        info!("TLS connection established to {}:{}", host, port);
        
        Ok(Self { inner: tls_stream })
    }
    
    /// Close the stream
    pub async fn close(&mut self) -> io::Result<()> {
        use futures::io::AsyncWriteExt;
        self.inner.close().await
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AsyncRead for TlsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AsyncWrite for TlsStream {
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

// WASM implementation - uses web-sys WebSocket with TLS built-in
#[cfg(target_arch = "wasm32")]
pub struct TlsStream {
    // WASM uses browser's native TLS via fetch/WebSocket
    // This is a placeholder - real WASM implementation would use web-sys
    _marker: std::marker::PhantomData<()>,
}

#[cfg(target_arch = "wasm32")]
impl TlsStream {
    pub async fn connect(_host: &str, _port: u16, _server_name: &str) -> Result<Self> {
        Err(TorError::Internal("Direct TLS connections not supported in WASM - use WebSocket".to_string()))
    }
    
    pub async fn close(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncRead for TlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Not supported in WASM")))
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncWrite for TlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Not supported in WASM")))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Not supported in WASM")))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Not supported in WASM")))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_tls_connector() {
        let connector = create_tls_connector();
        assert!(connector.is_ok());
    }
}
