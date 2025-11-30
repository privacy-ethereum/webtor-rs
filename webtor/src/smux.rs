//! SMUX (Simple MUltipleXing) protocol implementation
//!
//! SMUX provides stream multiplexing over a single connection.
//! This implements version 2 of the protocol.
//!
//! Segment format (8 bytes header + payload):
//! - Byte 0: version (must be 2)
//! - Byte 1: command (syn=0, fin=1, psh=2, nop=3, upd=4)
//! - Bytes 2-3: payload length (little-endian)
//! - Bytes 4-7: stream ID (little-endian)
//! - Bytes 8+: payload

use crate::error::{Result, TorError};
use futures::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{debug, trace, warn};

/// SMUX protocol version
const SMUX_VERSION: u8 = 2;

/// Default stream ID
const DEFAULT_STREAM_ID: u32 = 3;

/// Default window size (64KB)
const DEFAULT_WINDOW: u32 = 65535;

/// SMUX commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SmuxCommand {
    /// Stream open (SYN)
    Syn = 0,
    /// Stream close (FIN)
    Fin = 1,
    /// Push data (PSH)
    Psh = 2,
    /// No-op / ping (NOP)
    Nop = 3,
    /// Window update (UPD)
    Upd = 4,
}

impl TryFrom<u8> for SmuxCommand {
    type Error = TorError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(SmuxCommand::Syn),
            1 => Ok(SmuxCommand::Fin),
            2 => Ok(SmuxCommand::Psh),
            3 => Ok(SmuxCommand::Nop),
            4 => Ok(SmuxCommand::Upd),
            _ => Err(TorError::Protocol(format!("Invalid SMUX command: {}", value))),
        }
    }
}

/// SMUX segment
#[derive(Debug, Clone)]
pub struct SmuxSegment {
    pub version: u8,
    pub command: SmuxCommand,
    pub stream_id: u32,
    pub data: Vec<u8>,
}

impl SmuxSegment {
    pub fn new(command: SmuxCommand, stream_id: u32, data: Vec<u8>) -> Self {
        Self {
            version: SMUX_VERSION,
            command,
            stream_id,
            data,
        }
    }

    pub fn syn(stream_id: u32) -> Self {
        Self::new(SmuxCommand::Syn, stream_id, Vec::new())
    }

    pub fn fin(stream_id: u32) -> Self {
        Self::new(SmuxCommand::Fin, stream_id, Vec::new())
    }

    pub fn psh(stream_id: u32, data: Vec<u8>) -> Self {
        Self::new(SmuxCommand::Psh, stream_id, data)
    }

    pub fn nop(stream_id: u32) -> Self {
        Self::new(SmuxCommand::Nop, stream_id, Vec::new())
    }

    pub fn upd(stream_id: u32, consumed: u32, window: u32) -> Self {
        let mut data = Vec::with_capacity(8);
        // smux-go uses little-endian for UPD payload
        data.extend_from_slice(&consumed.to_le_bytes());
        data.extend_from_slice(&window.to_le_bytes());
        Self::new(SmuxCommand::Upd, stream_id, data)
    }

    /// Encode segment to bytes
    /// Note: smux-go uses little-endian for length and stream_id
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + self.data.len());

        buf.push(self.version);
        buf.push(self.command as u8);
        buf.extend_from_slice(&(self.data.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.stream_id.to_le_bytes());
        buf.extend_from_slice(&self.data);

        buf
    }

    /// Decode segment from bytes, returns (segment, bytes_consumed)
    pub fn decode(buf: &[u8]) -> Result<Option<(Self, usize)>> {
        if buf.len() < 8 {
            return Ok(None); // Need more data
        }

        let version = buf[0];
        if version != SMUX_VERSION {
            return Err(TorError::Protocol(format!(
                "Invalid SMUX version: {} (expected {})",
                version, SMUX_VERSION
            )));
        }

        let command = SmuxCommand::try_from(buf[1])?;
        // smux-go uses little-endian for length and stream_id
        let data_len = u16::from_le_bytes([buf[2], buf[3]]) as usize;
        let stream_id = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);

        let total_len = 8 + data_len;
        if buf.len() < total_len {
            return Ok(None); // Need more data
        }

        let data = buf[8..total_len].to_vec();

        let segment = SmuxSegment {
            version,
            command,
            stream_id,
            data,
        };

        Ok(Some((segment, total_len)))
    }
}

/// Window update structure
#[derive(Debug, Clone, Copy)]
pub struct SmuxUpdate {
    pub consumed: u32,
    pub window: u32,
}

impl SmuxUpdate {
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(TorError::Protocol("SmuxUpdate too short".to_string()));
        }

        // smux-go uses little-endian for UPD payload
        let consumed = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let window = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        Ok(Self { consumed, window })
    }
}

/// SMUX stream state
#[derive(Debug)]
struct SmuxState {
    stream_id: u32,
    /// Bytes we've read from peer
    self_read: u32,
    /// Bytes we've written to peer
    self_write: u32,
    /// Bytes read since last UPD sent
    self_increment: u32,
    /// Our receive window size
    self_window: u32,
    /// Bytes peer has consumed
    peer_consumed: u32,
    /// Peer's window size
    peer_window: u32,
    /// Whether we've sent SYN
    syn_sent: bool,
    /// Whether we've received SYN
    syn_received: bool,
}

impl SmuxState {
    fn new(stream_id: u32) -> Self {
        Self {
            stream_id,
            self_read: 0,
            self_write: 0,
            self_increment: 0,
            self_window: DEFAULT_WINDOW,
            peer_consumed: 0,
            peer_window: DEFAULT_WINDOW,
            syn_sent: false,
            syn_received: false,
        }
    }

    /// Check if we can send more data
    fn can_send(&self, len: usize) -> bool {
        let inflight = self.self_write.saturating_sub(self.peer_consumed);
        inflight + (len as u32) <= self.peer_window
    }

    /// Check if we should send a window update
    fn should_send_update(&self) -> bool {
        self.self_increment >= self.self_window / 2
    }
}

/// SMUX multiplexed stream
pub struct SmuxStream<S> {
    inner: S,
    state: SmuxState,
    read_buffer: Vec<u8>,
    data_buffer: Vec<u8>,
}

impl<S> SmuxStream<S> {
    pub fn new(inner: S) -> Self {
        Self::with_stream_id(inner, DEFAULT_STREAM_ID)
    }

    pub fn with_stream_id(inner: S, stream_id: u32) -> Self {
        Self {
            inner,
            state: SmuxState::new(stream_id),
            read_buffer: Vec::with_capacity(4096),
            data_buffer: Vec::new(),
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> SmuxStream<S> {
    /// Initialize SMUX connection (send SYN + UPD)
    pub async fn initialize(&mut self) -> Result<()> {
        if self.state.syn_sent {
            return Ok(());
        }

        debug!("Initializing SMUX stream {}", self.state.stream_id);

        // Send SYN
        let syn = SmuxSegment::syn(self.state.stream_id);
        let syn_bytes = syn.encode();
        debug!("SMUX SYN bytes: {:02x?}", syn_bytes);
        self.inner.write_all(&syn_bytes).await
            .map_err(|e| TorError::Network(format!("Failed to send SMUX SYN: {}", e)))?;

        // Send initial window update
        let upd = SmuxSegment::upd(self.state.stream_id, 0, self.state.self_window);
        let upd_bytes = upd.encode();
        debug!("SMUX UPD bytes: {:02x?}", upd_bytes);
        self.inner.write_all(&upd_bytes).await
            .map_err(|e| TorError::Network(format!("Failed to send SMUX UPD: {}", e)))?;

        self.inner.flush().await
            .map_err(|e| TorError::Network(format!("Failed to flush SMUX init: {}", e)))?;

        self.state.syn_sent = true;
        debug!("SMUX stream initialized");

        Ok(())
    }

    /// Send data
    pub async fn send(&mut self, data: &[u8]) -> Result<usize> {
        if !self.state.syn_sent {
            self.initialize().await?;
        }

        // Check flow control
        if !self.state.can_send(data.len()) {
            // Wait for window update
            self.process_incoming().await?;
            if !self.state.can_send(data.len()) {
                return Err(TorError::Network("SMUX window full".to_string()));
            }
        }

        let segment = SmuxSegment::psh(self.state.stream_id, data.to_vec());
        self.inner.write_all(&segment.encode()).await
            .map_err(|e| TorError::Network(format!("Failed to send SMUX data: {}", e)))?;

        self.state.self_write = self.state.self_write.wrapping_add(data.len() as u32);

        Ok(data.len())
    }

    /// Receive data
    pub async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        // First drain data buffer
        if !self.data_buffer.is_empty() {
            let len = std::cmp::min(buf.len(), self.data_buffer.len());
            buf[..len].copy_from_slice(&self.data_buffer[..len]);
            self.data_buffer.drain(..len);
            return Ok(len);
        }

        // Process incoming segments until we get data
        loop {
            if let Some(data) = self.process_next_segment().await? {
                if data.is_empty() {
                    continue;
                }

                let len = std::cmp::min(buf.len(), data.len());
                buf[..len].copy_from_slice(&data[..len]);

                if len < data.len() {
                    self.data_buffer.extend_from_slice(&data[len..]);
                }

                return Ok(len);
            }

            // Read more data
            let mut temp = [0u8; 4096];
            let n = self.inner.read(&mut temp).await
                .map_err(|e| TorError::Network(format!("SMUX read error: {}", e)))?;

            if n == 0 {
                return Ok(0); // EOF
            }

            self.read_buffer.extend_from_slice(&temp[..n]);
        }
    }

    /// Process incoming data
    async fn process_incoming(&mut self) -> Result<()> {
        let mut temp = [0u8; 4096];

        // Try non-blocking read
        match futures::future::poll_fn(|cx| {
            Pin::new(&mut self.inner).poll_read(cx, &mut temp)
        }).await {
            Ok(n) if n > 0 => {
                self.read_buffer.extend_from_slice(&temp[..n]);
            }
            _ => {}
        }

        // Process all complete segments
        while let Some(_) = self.process_next_segment().await? {}

        Ok(())
    }

    /// Process next segment from buffer, returns data if PSH
    async fn process_next_segment(&mut self) -> Result<Option<Vec<u8>>> {
        let (segment, consumed) = match SmuxSegment::decode(&self.read_buffer)? {
            Some(s) => s,
            None => return Ok(None),
        };

        self.read_buffer.drain(..consumed);

        // Validate stream ID (allow stream 0 for control messages)
        if segment.stream_id != self.state.stream_id && segment.stream_id != 0 {
            trace!("Ignoring SMUX segment for stream {} (expected {})", 
                   segment.stream_id, self.state.stream_id);
            return Ok(Some(Vec::new())); // Empty = no data but continue
        }

        match segment.command {
            SmuxCommand::Syn => {
                debug!("Received SMUX SYN for stream {}", segment.stream_id);
                self.state.syn_received = true;
                Ok(Some(Vec::new()))
            }

            SmuxCommand::Fin => {
                debug!("Received SMUX FIN for stream {}", segment.stream_id);
                Ok(None) // Signal EOF
            }

            SmuxCommand::Psh => {
                trace!("Received SMUX PSH: {} bytes", segment.data.len());

                // Update read counters
                self.state.self_read = self.state.self_read.wrapping_add(segment.data.len() as u32);
                self.state.self_increment = self.state.self_increment.wrapping_add(segment.data.len() as u32);

                // Send window update if needed
                if self.state.should_send_update() {
                    let upd = SmuxSegment::upd(
                        self.state.stream_id,
                        self.state.self_read,
                        self.state.self_window,
                    );
                    self.inner.write_all(&upd.encode()).await
                        .map_err(|e| TorError::Network(format!("Failed to send UPD: {}", e)))?;
                    self.state.self_increment = 0;
                }

                Ok(Some(segment.data))
            }

            SmuxCommand::Nop => {
                trace!("Received SMUX NOP, sending NOP response");
                // Respond with NOP (ping-pong)
                let nop = SmuxSegment::nop(segment.stream_id);
                self.inner.write_all(&nop.encode()).await
                    .map_err(|e| TorError::Network(format!("Failed to send NOP: {}", e)))?;
                Ok(Some(Vec::new()))
            }

            SmuxCommand::Upd => {
                let update = SmuxUpdate::decode(&segment.data)?;
                trace!("Received SMUX UPD: consumed={}, window={}", update.consumed, update.window);
                self.state.peer_consumed = update.consumed;
                self.state.peer_window = update.window;
                Ok(Some(Vec::new()))
            }
        }
    }

    /// Close the stream
    pub async fn close(&mut self) -> Result<()> {
        let fin = SmuxSegment::fin(self.state.stream_id);
        self.inner.write_all(&fin.encode()).await
            .map_err(|e| TorError::Network(format!("Failed to send FIN: {}", e)))?;
        self.inner.flush().await
            .map_err(|e| TorError::Network(format!("Failed to flush FIN: {}", e)))?;
        Ok(())
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for SmuxStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        trace!("SMUX poll_read: buf size {}, data_buffer size {}, read_buffer size {}", 
               buf.len(), self.data_buffer.len(), self.read_buffer.len());
        
        // Drain data buffer first
        if !self.data_buffer.is_empty() {
            let len = std::cmp::min(buf.len(), self.data_buffer.len());
            buf[..len].copy_from_slice(&self.data_buffer[..len]);
            self.data_buffer.drain(..len);
            trace!("SMUX poll_read: returning {} bytes from data buffer", len);
            return Poll::Ready(Ok(len));
        }

        // Try to decode from read buffer
        loop {
            match SmuxSegment::decode(&self.read_buffer) {
                Ok(Some((segment, consumed))) => {
                    self.read_buffer.drain(..consumed);

                    if segment.stream_id != self.state.stream_id && segment.stream_id != 0 {
                        continue; // Wrong stream
                    }

                    match segment.command {
                        SmuxCommand::Syn => {
                            debug!("SMUX poll_read: received SYN for stream {}", segment.stream_id);
                            self.state.syn_received = true;
                            continue;
                        }
                        SmuxCommand::Psh => {
                            debug!("SMUX poll_read: received PSH {} bytes for stream {}", segment.data.len(), segment.stream_id);
                            self.state.self_read = self.state.self_read.wrapping_add(segment.data.len() as u32);
                            self.state.self_increment = self.state.self_increment.wrapping_add(segment.data.len() as u32);

                            let len = std::cmp::min(buf.len(), segment.data.len());
                            buf[..len].copy_from_slice(&segment.data[..len]);

                            if len < segment.data.len() {
                                self.data_buffer.extend_from_slice(&segment.data[len..]);
                            }

                            return Poll::Ready(Ok(len));
                        }
                        SmuxCommand::Fin => {
                            debug!("SMUX poll_read: received FIN for stream {}", segment.stream_id);
                            return Poll::Ready(Ok(0)); // EOF
                        }
                        SmuxCommand::Upd => {
                            if let Ok(update) = SmuxUpdate::decode(&segment.data) {
                                debug!("SMUX poll_read: received UPD consumed={}, window={}", update.consumed, update.window);
                                self.state.peer_consumed = update.consumed;
                                self.state.peer_window = update.window;
                            }
                            continue;
                        }
                        SmuxCommand::Nop => {
                            debug!("SMUX poll_read: received NOP");
                            continue;
                        }
                    }
                }
                Ok(None) => break, // Need more data
                Err(e) => return Poll::Ready(Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()))),
            }
        }

        // Read more data
        trace!("SMUX poll_read: need more data, reading from inner");
        let mut temp = [0u8; 4096];
        match Pin::new(&mut self.inner).poll_read(cx, &mut temp) {
            Poll::Ready(Ok(0)) => {
                trace!("SMUX poll_read: inner EOF");
                Poll::Ready(Ok(0))
            }
            Poll::Ready(Ok(n)) => {
                debug!("SMUX poll_read: got {} bytes from inner: {:02x?}", n, &temp[..n]);
                self.read_buffer.extend_from_slice(&temp[..n]);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Err(e)) => {
                warn!("SMUX poll_read: inner error: {}", e);
                Poll::Ready(Err(e))
            }
            Poll::Pending => {
                trace!("SMUX poll_read: inner pending");
                Poll::Pending
            }
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for SmuxStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Check flow control
        if !self.state.can_send(buf.len()) {
            // For now, just try anyway - proper impl would wait
        }

        let segment = SmuxSegment::psh(self.state.stream_id, buf.to_vec());
        let encoded = segment.encode();
        debug!("SMUX poll_write: sending {} bytes data as {} byte frame: {:02x?}", buf.len(), encoded.len(), &encoded[..std::cmp::min(32, encoded.len())]);

        match Pin::new(&mut self.inner).poll_write(cx, &encoded) {
            Poll::Ready(Ok(n)) => {
                if n == encoded.len() {
                    self.state.self_write = self.state.self_write.wrapping_add(buf.len() as u32);
                    Poll::Ready(Ok(buf.len()))
                } else {
                    Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "Partial SMUX write",
                    )))
                }
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Send FIN before closing
        let fin = SmuxSegment::fin(self.state.stream_id);
        let encoded = fin.encode();

        match Pin::new(&mut self.inner).poll_write(cx, &encoded) {
            Poll::Ready(Ok(_)) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }

        Pin::new(&mut self.inner).poll_close(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_encode_decode() {
        let segment = SmuxSegment::psh(3, b"Hello".to_vec());
        let encoded = segment.encode();

        assert_eq!(encoded[0], SMUX_VERSION);
        assert_eq!(encoded[1], SmuxCommand::Psh as u8);

        let (decoded, consumed) = SmuxSegment::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded.version, SMUX_VERSION);
        assert_eq!(decoded.command, SmuxCommand::Psh);
        assert_eq!(decoded.stream_id, 3);
        assert_eq!(decoded.data, b"Hello");
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_segment_syn() {
        let segment = SmuxSegment::syn(5);
        assert_eq!(segment.command, SmuxCommand::Syn);
        assert_eq!(segment.stream_id, 5);
        assert!(segment.data.is_empty());
    }

    #[test]
    fn test_segment_upd() {
        let segment = SmuxSegment::upd(3, 1000, 65535);
        assert_eq!(segment.command, SmuxCommand::Upd);
        assert_eq!(segment.data.len(), 8);

        let update = SmuxUpdate::decode(&segment.data).unwrap();
        assert_eq!(update.consumed, 1000);
        assert_eq!(update.window, 65535);
    }

    #[test]
    fn test_partial_decode() {
        let segment = SmuxSegment::psh(3, b"Hello".to_vec());
        let encoded = segment.encode();

        // Partial header
        assert!(SmuxSegment::decode(&encoded[..4]).unwrap().is_none());

        // Full segment
        assert!(SmuxSegment::decode(&encoded).unwrap().is_some());
    }

    #[test]
    fn test_invalid_version() {
        let mut buf = SmuxSegment::psh(3, b"test".to_vec()).encode();
        buf[0] = 1; // Wrong version

        assert!(SmuxSegment::decode(&buf).is_err());
    }
}
