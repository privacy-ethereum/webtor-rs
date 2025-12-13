//! Async KCP stream wrapper
//!
//! This module provides an async wrapper around the `kcp` crate that works
//! with any AsyncRead + AsyncWrite transport (not just UDP sockets).
//!
//! KCP provides reliable, ordered delivery over an unreliable transport.
//!
//! Data flow:
//! - send() -> snd_queue -> update() -> flush() -> output -> transport
//! - transport -> input() -> rcv_buf -> recv() -> application

use crate::error::{Result, TorError};
use crate::time::Instant;
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt};
use kcp::Kcp;
use std::io::{self, Write};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tracing::{debug, trace};

/// Output buffer that collects data from KCP for sending
#[derive(Clone)]
struct OutputBuffer {
    data: Arc<Mutex<Vec<u8>>>,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn take(&self) -> Vec<u8> {
        let mut data = self.data.lock().unwrap();
        std::mem::take(&mut *data)
    }

    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.data.lock().unwrap().is_empty()
    }
}

impl Write for OutputBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.data.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// KCP configuration
#[derive(Debug, Clone)]
pub struct KcpConfig {
    /// Conversation ID (must match on both ends)
    pub conv: u32,
    /// Enable nodelay mode for faster retransmission
    pub nodelay: bool,
    /// Update interval in milliseconds
    pub interval: i32,
    /// Fast resend trigger (0 = off, 2 = on duplicate ACK)
    pub resend: i32,
    /// Disable congestion control
    pub nc: bool,
    /// Send window size
    pub snd_wnd: u16,
    /// Receive window size
    pub rcv_wnd: u16,
}

impl Default for KcpConfig {
    fn default() -> Self {
        Self {
            conv: 0,
            // Match Snowflake Go client settings:
            // conn.SetNoDelay(0, 0, 0, 1) means:
            // nodelay=0 (default), interval=0 (default 100ms), resend=0 (off), nc=1 (congestion off)
            nodelay: false,
            interval: 100, // Default KCP interval
            resend: 0,     // No fast resend
            nc: true,      // Disable congestion control (nc=1 in Go)
            snd_wnd: 128,
            rcv_wnd: 128,
        }
    }
}

/// Async KCP stream
pub struct KcpStream<S> {
    kcp: Kcp<OutputBuffer>,
    output: OutputBuffer,
    transport: S,
    #[allow(dead_code)]
    read_buffer: Vec<u8>,
    #[allow(dead_code)]
    recv_buffer: Vec<u8>,
    start_time: Instant,
    #[allow(dead_code)]
    last_update: u32,
    waker: Option<Waker>,
    /// Pending output data that needs to be written to transport
    pending_write: Vec<u8>,
}

impl<S> KcpStream<S> {
    pub fn new(transport: S, config: KcpConfig) -> Self {
        let output = OutputBuffer::new();
        // Use stream mode like Snowflake Go client (SetStreamMode(true))
        let mut kcp = Kcp::new_stream(config.conv, output.clone());

        // Configure KCP to match Snowflake Go client settings
        kcp.set_nodelay(config.nodelay, config.interval, config.resend, config.nc);
        kcp.set_wndsize(config.snd_wnd, config.rcv_wnd);

        Self {
            kcp,
            output,
            transport,
            read_buffer: Vec::with_capacity(4096),
            recv_buffer: vec![0u8; 65536],
            start_time: Instant::now(),
            last_update: 0,
            waker: None,
            pending_write: Vec::new(),
        }
    }

    fn current_ms(&self) -> u32 {
        self.start_time.elapsed().as_millis() as u32
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> KcpStream<S> {
    /// Update KCP state and flush pending data
    async fn update_and_flush(&mut self) -> Result<()> {
        let current = self.current_ms();

        // Update KCP (handles retransmission, ACKs, etc.)
        self.kcp
            .update(current)
            .map_err(|e| TorError::Protocol(format!("KCP update error: {:?}", e)))?;

        self.last_update = current;

        // Flush output to transport
        self.flush_output().await?;

        Ok(())
    }

    /// Flush KCP output buffer to transport
    async fn flush_output(&mut self) -> Result<()> {
        let data = self.output.take();
        if !data.is_empty() {
            trace!("KCP sending {} bytes to transport", data.len());
            self.transport
                .write_all(&data)
                .await
                .map_err(|e| TorError::Network(format!("KCP transport write error: {}", e)))?;
            self.transport
                .flush()
                .await
                .map_err(|e| TorError::Network(format!("KCP transport flush error: {}", e)))?;
        }
        Ok(())
    }

    /// Process incoming data from transport
    async fn process_input(&mut self) -> Result<bool> {
        // Try to read from transport
        let mut temp = [0u8; 4096];

        // Non-blocking read attempt using poll
        let n = match futures::future::poll_fn(|cx| {
            Pin::new(&mut self.transport).poll_read(cx, &mut temp)
        })
        .await
        {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) => {
                return Err(TorError::Network(format!(
                    "KCP transport read error: {}",
                    e
                )))
            }
        };

        if n == 0 {
            return Ok(false); // EOF
        }

        trace!("KCP received {} bytes from transport", n);

        // Feed to KCP
        self.kcp
            .input(&temp[..n])
            .map_err(|e| TorError::Protocol(format!("KCP input error: {:?}", e)))?;

        Ok(true)
    }

    /// Send data through KCP
    pub async fn send(&mut self, data: &[u8]) -> Result<usize> {
        let sent = self
            .kcp
            .send(data)
            .map_err(|e| TorError::Protocol(format!("KCP send error: {:?}", e)))?;

        // Immediately try to flush
        self.update_and_flush().await?;

        Ok(sent)
    }

    /// Receive data from KCP
    pub async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            // Try to receive from KCP
            match self.kcp.recv(buf) {
                Ok(n) => return Ok(n),
                Err(kcp::Error::RecvQueueEmpty) => {
                    // Need more data - process input and update
                }
                Err(e) => return Err(TorError::Protocol(format!("KCP recv error: {:?}", e))),
            }

            // Process any incoming data
            self.process_input().await?;

            // Update KCP state
            self.update_and_flush().await?;

            // Check how long until next update
            let check = self.kcp.check(self.current_ms());
            if check > 0 {
                // Wait a bit before trying again
                #[cfg(not(target_arch = "wasm32"))]
                tokio::time::sleep(Duration::from_millis(check.min(10) as u64)).await;

                #[cfg(target_arch = "wasm32")]
                {
                    use crate::wasm_runtime::WasmRuntime;
                    use tor_rtcompat::SleepProvider;
                    let runtime = WasmRuntime::new();
                    runtime
                        .sleep(Duration::from_millis(check.min(10) as u64))
                        .await;
                }
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for KcpStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        // Save waker for later
        self.waker = Some(cx.waker().clone());

        // Try to receive from KCP first
        match self.kcp.recv(buf) {
            Ok(n) => {
                debug!("KCP read: received {} bytes from KCP queue", n);
                return Poll::Ready(Ok(n));
            }
            Err(kcp::Error::RecvQueueEmpty) => {}
            Err(e) => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("KCP recv error: {:?}", e),
                )))
            }
        }

        // Need more data from transport
        let mut temp = [0u8; 4096];
        match Pin::new(&mut self.transport).poll_read(cx, &mut temp) {
            Poll::Ready(Ok(0)) => {
                debug!("KCP read: transport EOF");
                Poll::Ready(Ok(0))
            }
            Poll::Ready(Ok(n)) => {
                debug!("KCP read: got {} bytes from transport, feeding to KCP", n);
                // Feed to KCP
                if let Err(e) = self.kcp.input(&temp[..n]) {
                    debug!("KCP read: input error: {:?}", e);
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("KCP input error: {:?}", e),
                    )));
                }

                // Update KCP
                let current = self.current_ms();
                if let Err(e) = self.kcp.update(current) {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("KCP update error: {:?}", e),
                    )));
                }

                // Write any output that update generated (ACKs, etc.)
                // We need to send ACKs immediately or the sender will retransmit
                let output_data = self.output.take();
                if !output_data.is_empty() {
                    debug!(
                        "KCP read: sending {} bytes of KCP output (ACKs)",
                        output_data.len()
                    );
                    // Try to write ACKs immediately
                    if let Poll::Ready(Err(e)) =
                        Pin::new(&mut self.transport).poll_write(cx, &output_data)
                    {
                        return Poll::Ready(Err(e));
                    }
                    // Also try to flush
                    let _ = Pin::new(&mut self.transport).poll_flush(cx);
                }

                // Try recv again
                match self.kcp.recv(buf) {
                    Ok(n) => {
                        debug!("KCP read: received {} bytes after input", n);
                        Poll::Ready(Ok(n))
                    }
                    Err(kcp::Error::RecvQueueEmpty) => {
                        debug!("KCP read: recv queue still empty, pending");
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    Err(e) => Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("KCP recv error: {:?}", e),
                    ))),
                }
            }
            Poll::Ready(Err(e)) => {
                debug!("KCP read: transport error: {}", e);
                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for KcpStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        debug!("KCP write: sending {} bytes", buf.len());

        // Queue data in KCP
        match self.kcp.send(buf) {
            Ok(n) => {
                debug!("KCP write: queued {} bytes in KCP", n);

                // Update KCP state first (required before flush)
                let current = self.current_ms();
                if let Err(e) = self.kcp.update(current) {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("KCP update error: {:?}", e),
                    )));
                }

                // Force flush to produce output immediately
                if let Err(e) = self.kcp.flush() {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("KCP flush error: {:?}", e),
                    )));
                }

                // Flush output
                let output_data = self.output.take();
                if !output_data.is_empty() {
                    debug!(
                        "KCP write: flushing {} bytes to transport",
                        output_data.len()
                    );
                    match Pin::new(&mut self.transport).poll_write(cx, &output_data) {
                        Poll::Ready(Ok(written)) => {
                            debug!("KCP write: wrote {} bytes to transport", written);
                            Poll::Ready(Ok(n))
                        }
                        Poll::Ready(Err(e)) => {
                            debug!("KCP write: transport error: {}", e);
                            Poll::Ready(Err(e))
                        }
                        Poll::Pending => {
                            debug!("KCP write: transport pending");
                            Poll::Pending
                        }
                    }
                } else {
                    debug!("KCP write: no output to flush");
                    Poll::Ready(Ok(n))
                }
            }
            Err(e) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Other,
                format!("KCP send error: {:?}", e),
            ))),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // First, write any pending data (ACKs from poll_read)
        if !self.pending_write.is_empty() {
            let data = std::mem::take(&mut self.pending_write);
            debug!("KCP flush: writing {} bytes of pending data", data.len());
            match Pin::new(&mut self.transport).poll_write(cx, &data) {
                Poll::Ready(Ok(n)) if n == data.len() => {}
                Poll::Ready(Ok(n)) => {
                    // Partial write - save remaining
                    self.pending_write = data[n..].to_vec();
                    return Poll::Pending;
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => {
                    self.pending_write = data;
                    return Poll::Pending;
                }
            }
        }

        // Flush KCP output
        let _current = self.current_ms();
        if let Err(e) = self.kcp.flush() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Other,
                format!("KCP flush error: {:?}", e),
            )));
        }

        // Flush output buffer from KCP flush
        let output_data = self.output.take();
        if !output_data.is_empty() {
            debug!("KCP flush: writing {} bytes from KCP", output_data.len());
            match Pin::new(&mut self.transport).poll_write(cx, &output_data) {
                Poll::Ready(Ok(_)) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        Pin::new(&mut self.transport).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Flush before closing
        match self.as_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }

        Pin::new(&mut self.transport).poll_close(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kcp_config_default() {
        let config = KcpConfig::default();
        // Match Snowflake Go settings: nodelay=false, interval=100, nc=true
        assert!(!config.nodelay);
        assert_eq!(config.interval, 100);
        assert!(config.nc);
    }

    #[test]
    fn test_output_buffer() {
        let mut buf = OutputBuffer::new();
        buf.write_all(b"hello").unwrap();
        buf.write_all(b" world").unwrap();

        let data = buf.take();
        assert_eq!(data, b"hello world");
        assert!(buf.is_empty());
    }
}
