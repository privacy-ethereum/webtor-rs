//! Snowflake bridge implementation for Tor connections

use crate::error::Result;
use crate::websocket::WebSocketStream;
use futures::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tracing::{debug, info};

/// Snowflake bridge connection manager
pub struct SnowflakeBridge {
    websocket_url: String,
    _connection_timeout: Duration,
}

impl SnowflakeBridge {
    pub fn new(websocket_url: String, connection_timeout: Duration) -> Self {
        Self {
            websocket_url,
            _connection_timeout: connection_timeout,
        }
    }
    
    /// Connect to the Snowflake bridge
    pub async fn connect(&self) -> Result<SnowflakeStream> {
        info!("Connecting to Snowflake bridge at {}", self.websocket_url);
        
        use crate::wasm_runtime::WasmRuntime;
        use tor_rtcompat::SleepProvider;
        use futures::FutureExt;

        let runtime = WasmRuntime::new();
        let timeout = runtime.sleep(self._connection_timeout);
        let connect_fut = WebSocketStream::connect(&self.websocket_url);
        
        futures::select! {
            res = connect_fut.fuse() => {
                let stream = res?;
                Ok(SnowflakeStream {
                    inner: stream,
                })
            }
            _ = timeout.fuse() => {
                Err(crate::error::TorError::Network(format!("Snowflake connection timed out after {:?}", self._connection_timeout)))
            }
        }
    }
}

use tor_rtcompat::StreamOps;

/// Snowflake stream for Tor communication
pub struct SnowflakeStream {
    inner: WebSocketStream,
}

// Safety: We are in a WASM environment which is effectively single-threaded.
// The types inside WebSocketStream (Rc, RefCell, etc.) are !Send.
// However, since we are not actually sharing this across threads (because WASM doesn't support them fully yet
// in this context, and we use spawn_local), we assert Send to satisfy tor-proto's bounds.
unsafe impl Send for SnowflakeStream {}

impl StreamOps for SnowflakeStream {
    // Use default implementation
}

impl SnowflakeStream {
    /// Close the Snowflake stream
    pub async fn close(&mut self) -> io::Result<()> {
        info!("Closing Snowflake stream");
        use futures::AsyncWriteExt;
        self.inner.close().await
    }
}

impl AsyncRead for SnowflakeStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for SnowflakeStream {
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

/// Create a new Snowflake stream (convenience function)
pub async fn create_snowflake_stream(
    websocket_url: &str,
    connection_timeout: Duration,
) -> Result<SnowflakeStream> {
    let bridge = SnowflakeBridge::new(
        websocket_url.to_string(),
        connection_timeout,
    );
    bridge.connect().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TorError;
    
    #[tokio::test]
    async fn test_snowflake_bridge_creation() {
        let bridge = SnowflakeBridge::new(
            "wss://snowflake.torproject.net/".to_string(),
            Duration::from_secs(15),
        );
        
        // This will fail in native Rust, but should work in WASM
        let result = bridge.connect().await;
        
        // On native, it returns Err because WebSocketStream isn't implemented
        assert!(result.is_err());
         match result {
            Err(TorError::Internal(msg)) => {
                assert!(msg.contains("not supported") || msg.contains("Not implemented"));
            }
            _ => {} // Other errors possible
        }
    }
}
