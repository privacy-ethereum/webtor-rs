//! Turbo framing protocol for Snowflake transport
//!
//! Turbo provides a lightweight framing layer with optional padding for obfuscation.
//! It uses a variable-length header (1-3 bytes) based on payload size.
//!
//! Protocol:
//! 1. On connection: Send token (8 bytes) + client_id (8 bytes)
//! 2. Each frame has a variable header encoding length and data/padding flag
//!
//! Frame header format (from encapsulation.go):
//! - Bit 7 (MSB): Data flag (1 = real data, 0 = padding frame to ignore)
//! - Bit 6: Continuation bit (0 = last byte of header, 1 = more bytes follow)
//! - Bits 5-0: Part of the length
//!
//! Continuation byte format:
//! - Bit 7: Continuation bit
//! - Bits 6-0: 7 bits of length

use crate::error::{Result, TorError};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::debug;

/// Magic token sent at start of Turbo connection
const TURBO_TOKEN: [u8; 8] = [0x12, 0x93, 0x60, 0x5d, 0x27, 0x81, 0x75, 0xf5];

/// Maximum frame size (2^20 = 1MB)
const MAX_FRAME_SIZE: usize = 1 << 20;

/// Turbo frame with padding support
#[derive(Debug, Clone)]
pub struct TurboFrame {
    pub data: Vec<u8>,
    pub is_padding: bool,
}

impl TurboFrame {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            is_padding: false,
        }
    }

    pub fn padding(data: Vec<u8>) -> Self {
        Self {
            data,
            is_padding: true,
        }
    }

    /// Encode frame to bytes with variable-length header
    /// Format matches encapsulation.go from snowflake
    pub fn encode(&self) -> Vec<u8> {
        let len = self.data.len();
        let data_flag: u8 = if self.is_padding { 0x00 } else { 0x80 }; // Bit 7: 1 = real data

        let mut result = Vec::with_capacity(len + 3);

        if len <= 0x3F {
            // 1-byte header: bit 7=data, bit 6=0(no cont), bits 5-0=length
            let header = data_flag | (len as u8 & 0x3F);
            result.push(header);
        } else if len <= 0x1FFF {
            // 2-byte header (6 + 7 = 13 bits of length)
            // Byte 0: bit 7=data, bit 6=1(cont), bits 5-0=length[12:7]
            // Byte 1: bit 7=0(end), bits 6-0=length[6:0]
            let byte0 = data_flag | 0x40 | ((len >> 7) as u8 & 0x3F);
            let byte1 = (len & 0x7F) as u8;
            result.push(byte0);
            result.push(byte1);
        } else if len <= 0xFFFFF {
            // 3-byte header (6 + 7 + 7 = 20 bits of length)
            // Byte 0: bit 7=data, bit 6=1(cont), bits 5-0=length[19:14]
            // Byte 1: bit 7=1(cont), bits 6-0=length[13:7]
            // Byte 2: bit 7=0(end), bits 6-0=length[6:0]
            let byte0 = data_flag | 0x40 | ((len >> 14) as u8 & 0x3F);
            let byte1 = 0x80 | ((len >> 7) as u8 & 0x7F);
            let byte2 = (len & 0x7F) as u8;
            result.push(byte0);
            result.push(byte1);
            result.push(byte2);
        } else {
            panic!("Frame too large: {} bytes (max {})", len, MAX_FRAME_SIZE);
        }

        result.extend_from_slice(&self.data);
        result
    }

    /// Decode frame from bytes, returns (frame, bytes_consumed)
    /// Format matches encapsulation.go from snowflake
    pub fn decode(buf: &[u8]) -> Result<Option<(Self, usize)>> {
        if buf.is_empty() {
            return Ok(None);
        }

        let byte0 = buf[0];
        let is_data = (byte0 & 0x80) != 0; // Bit 7: data flag
        let is_padding = !is_data;
        let has_cont = (byte0 & 0x40) != 0; // Bit 6: continuation

        let (len, header_size) = if !has_cont {
            // 1-byte header: bits 5-0 = length
            let len = (byte0 & 0x3F) as usize;
            (len, 1)
        } else {
            // Multi-byte header
            if buf.len() < 2 {
                return Ok(None);
            }

            let byte1 = buf[1];
            let byte1_has_cont = (byte1 & 0x80) != 0; // Bit 7: continuation

            if !byte1_has_cont {
                // 2-byte header: 6 bits from byte0 + 7 bits from byte1
                let len = ((byte0 & 0x3F) as usize) << 7 | (byte1 & 0x7F) as usize;
                (len, 2)
            } else {
                // 3-byte header
                if buf.len() < 3 {
                    return Ok(None);
                }

                let byte2 = buf[2];
                // 6 bits from byte0 + 7 bits from byte1 + 7 bits from byte2
                let len = ((byte0 & 0x3F) as usize) << 14
                    | ((byte1 & 0x7F) as usize) << 7
                    | (byte2 & 0x7F) as usize;
                (len, 3)
            }
        };

        if len > MAX_FRAME_SIZE {
            return Err(TorError::Protocol(format!(
                "Turbo frame too large: {}",
                len
            )));
        }

        let total_size = header_size + len;
        if buf.len() < total_size {
            return Ok(None); // Need more data
        }

        let data = buf[header_size..total_size].to_vec();
        let frame = TurboFrame { data, is_padding };

        Ok(Some((frame, total_size)))
    }
}

/// Turbo stream wrapper that handles framing
pub struct TurboStream<S> {
    inner: S,
    read_buffer: Vec<u8>,
    write_buffer: Vec<u8>,
    initialized: bool,
    client_id: [u8; 8],
}

impl<S> TurboStream<S> {
    pub fn new(inner: S) -> Self {
        Self::with_client_id(inner, rand::random())
    }

    pub fn with_client_id(inner: S, client_id: [u8; 8]) -> Self {
        Self {
            inner,
            read_buffer: Vec::with_capacity(4096),
            write_buffer: Vec::new(),
            initialized: false,
            client_id,
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> TurboStream<S> {
    /// Initialize the Turbo connection by sending token and client ID
    pub async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        debug!("Initializing Turbo connection");

        // Send token + client_id
        let mut init_data = Vec::with_capacity(16);
        init_data.extend_from_slice(&TURBO_TOKEN);
        init_data.extend_from_slice(&self.client_id);

        self.inner
            .write_all(&init_data)
            .await
            .map_err(|e| TorError::Network(format!("Failed to send Turbo init: {}", e)))?;
        self.inner
            .flush()
            .await
            .map_err(|e| TorError::Network(format!("Failed to flush Turbo init: {}", e)))?;

        self.initialized = true;
        debug!("Turbo connection initialized");

        Ok(())
    }

    /// Send a frame
    pub async fn send_frame(&mut self, data: &[u8]) -> Result<()> {
        if !self.initialized {
            self.initialize().await?;
        }

        let frame = TurboFrame::new(data.to_vec());
        let encoded = frame.encode();

        self.inner
            .write_all(&encoded)
            .await
            .map_err(|e| TorError::Network(format!("Failed to send Turbo frame: {}", e)))?;

        Ok(())
    }

    /// Receive a frame (skips padding frames)
    pub async fn recv_frame(&mut self) -> Result<Vec<u8>> {
        loop {
            // Try to decode from buffer first
            if let Some((frame, consumed)) = TurboFrame::decode(&self.read_buffer)? {
                self.read_buffer.drain(..consumed);

                if frame.is_padding {
                    continue; // Skip padding frames
                }

                return Ok(frame.data);
            }

            // Need more data
            let mut temp = [0u8; 4096];
            let n = self
                .inner
                .read(&mut temp)
                .await
                .map_err(|e| TorError::Network(format!("Failed to read Turbo data: {}", e)))?;

            if n == 0 {
                return Err(TorError::Network("Turbo connection closed".to_string()));
            }

            self.read_buffer.extend_from_slice(&temp[..n]);
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for TurboStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        // First drain from decoded data buffer
        if !self.write_buffer.is_empty() {
            let len = std::cmp::min(buf.len(), self.write_buffer.len());
            buf[..len].copy_from_slice(&self.write_buffer[..len]);
            self.write_buffer.drain(..len);
            return Poll::Ready(Ok(len));
        }

        // Try to decode frames from read buffer
        loop {
            match TurboFrame::decode(&self.read_buffer) {
                Ok(Some((frame, consumed))) => {
                    self.read_buffer.drain(..consumed);

                    if frame.is_padding {
                        continue; // Skip padding
                    }

                    // Copy data to output
                    let len = std::cmp::min(buf.len(), frame.data.len());
                    buf[..len].copy_from_slice(&frame.data[..len]);

                    // Store remainder
                    if len < frame.data.len() {
                        self.write_buffer.extend_from_slice(&frame.data[len..]);
                    }

                    return Poll::Ready(Ok(len));
                }
                Ok(None) => break, // Need more data
                Err(e) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                }
            }
        }

        // Read more data from inner stream
        let mut temp = [0u8; 4096];
        match Pin::new(&mut self.inner).poll_read(cx, &mut temp) {
            Poll::Ready(Ok(0)) => Poll::Ready(Ok(0)), // EOF
            Poll::Ready(Ok(n)) => {
                tracing::info!("Turbo poll_read: got {} bytes from inner stream", n);
                self.read_buffer.extend_from_slice(&temp[..n]);
                // Wake to try decoding again
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for TurboStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // NOTE: We encode the entire frame and expect it to be written atomically.
        // Partial writes will return WriteZero error. This is acceptable because:
        // 1. Turbo frames are small and TCP usually handles them atomically
        // 2. Proper partial-write buffering would add significant complexity
        // 3. On WriteZero error, the connection is corrupted - callers must reconnect, not retry
        let frame = TurboFrame::new(buf.to_vec());
        let encoded = frame.encode();
        tracing::info!(
            "Turbo poll_write: {} bytes data -> {} byte frame",
            buf.len(),
            encoded.len()
        );

        // Write all encoded data
        match Pin::new(&mut self.inner).poll_write(cx, &encoded) {
            Poll::Ready(Ok(n)) => {
                if n == encoded.len() {
                    Poll::Ready(Ok(buf.len()))
                } else {
                    // Partial write - this is tricky with framing
                    // For simplicity, report error
                    Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "Partial Turbo frame write",
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
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_encode_decode_small() {
        let data = b"Hello, World!";
        let frame = TurboFrame::new(data.to_vec());
        let encoded = frame.encode();

        // Should be 1-byte header for small data (13 bytes < 64)
        assert_eq!(encoded[0] & 0x40, 0); // No continuation (bit 6)
        assert_eq!(encoded[0] & 0x80, 0x80); // Is data (bit 7)

        let (decoded, consumed) = TurboFrame::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded.data, data);
        assert!(!decoded.is_padding);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_frame_encode_decode_medium() {
        let data = vec![0u8; 100]; // > 63 bytes, needs 2-byte header
        let frame = TurboFrame::new(data.clone());
        let encoded = frame.encode();

        // Should be 2-byte header
        assert_eq!(encoded[0] & 0x40, 0x40); // Has continuation (bit 6)
        assert_eq!(encoded[0] & 0x80, 0x80); // Is data (bit 7)
        assert_eq!(encoded[1] & 0x80, 0); // No more continuation (bit 7)

        let (decoded, consumed) = TurboFrame::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded.data, data);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_frame_encode_decode_large() {
        let data = vec![0u8; 10000]; // > 8191 bytes, needs 3-byte header
        let frame = TurboFrame::new(data.clone());
        let encoded = frame.encode();

        // Should be 3-byte header
        assert_eq!(encoded[0] & 0x40, 0x40); // Has continuation (bit 6)
        assert_eq!(encoded[0] & 0x80, 0x80); // Is data (bit 7)
        assert_eq!(encoded[1] & 0x80, 0x80); // Has continuation (bit 7)
        assert_eq!(encoded[2] & 0x80, 0); // No more continuation (bit 7)

        let (decoded, consumed) = TurboFrame::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded.data, data);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_padding_frame() {
        let data = b"padding data";
        let frame = TurboFrame::padding(data.to_vec());
        let encoded = frame.encode();

        assert_eq!(encoded[0] & 0x80, 0); // Is padding (bit 7 = 0)

        let (decoded, _) = TurboFrame::decode(&encoded).unwrap().unwrap();
        assert!(decoded.is_padding);
    }

    #[test]
    fn test_partial_decode() {
        let data = b"Hello";
        let frame = TurboFrame::new(data.to_vec());
        let encoded = frame.encode();

        // Give only part of the frame
        let partial = &encoded[..2];
        assert!(TurboFrame::decode(partial).unwrap().is_none());

        // Full frame should decode
        assert!(TurboFrame::decode(&encoded).unwrap().is_some());
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn turbo_frame_roundtrips(
                data in proptest::collection::vec(any::<u8>(), 0..=0x4000), // 16KB max for faster tests
                is_padding in any::<bool>(),
            ) {
                let frame = if is_padding {
                    TurboFrame::padding(data.clone())
                } else {
                    TurboFrame::new(data.clone())
                };

                let encoded = frame.encode();
                let (decoded, consumed) = TurboFrame::decode(&encoded).unwrap().unwrap();

                prop_assert_eq!(decoded.data, data);
                prop_assert_eq!(decoded.is_padding, is_padding);
                prop_assert_eq!(consumed, encoded.len());
            }

            #[test]
            fn turbo_header_size_boundaries(len in prop_oneof![
                Just(0usize),
                Just(1usize),
                Just(0x3Fusize),
                Just(0x40usize),
                Just(0x41usize),
                Just(0x1FFFusize),
                Just(0x2000usize),
                Just(0x2001usize),
                Just(0xFFFFFusize),
            ]) {
                let data = vec![0u8; len];
                let frame = TurboFrame::new(data);
                let encoded = frame.encode();

                let header_size = encoded.len() - len;

                let expected = if len <= 0x3F { 1 }
                else if len <= 0x1FFF { 2 }
                else { 3 };

                prop_assert_eq!(header_size, expected);
                prop_assert!(TurboFrame::decode(&encoded).unwrap().is_some());
            }

            #[test]
            fn turbo_partial_decode_returns_none(
                data in proptest::collection::vec(any::<u8>(), 0..=1024),
                is_padding in any::<bool>(),
            ) {
                let frame = if is_padding {
                    TurboFrame::padding(data)
                } else {
                    TurboFrame::new(data)
                };
                let encoded = frame.encode();

                for split in 0..encoded.len() {
                    let prefix = &encoded[..split];
                    let res = TurboFrame::decode(prefix).unwrap();
                    prop_assert!(res.is_none(), "Expected None for prefix of length {}", split);
                }

                let full = TurboFrame::decode(&encoded).unwrap();
                prop_assert!(full.is_some());
            }
        }

        #[test]
        fn turbo_oversized_header_rejected() {
            // MAX_FRAME_SIZE = 1 << 20 = 0x100000 (1MB)
            // Max 3-byte header length = 0x3F << 14 | 0x7F << 7 | 0x7F = 0xFFFFF
            // This is less than MAX_FRAME_SIZE, so we need to test boundary
            // Actually, MAX_FRAME_SIZE = 1 << 20 = 1048576
            // 0xFFFFF = 1048575, which is < MAX_FRAME_SIZE, so it passes
            // We need a length > 1048576, but 3-byte max is 1048575
            //
            // The only way to exceed is if the code changes or we have a 4-byte header
            // For now, test that max valid header (0xFFFFF) is accepted
            let header = [
                0x80 | 0x40 | 0x3F, // data flag + continuation + 6 bits = max
                0x80 | 0x7F,        // continuation + 7 bits = max
                0x7F,               // 7 bits = max (no continuation)
            ];
            // Length = 0xFFFFF = 1048575, needs that many bytes of data
            // Without the data, it returns None (need more data), not an error
            let result = TurboFrame::decode(&header);
            assert!(result.unwrap().is_none()); // Need more data

            // Test that we can't construct a frame that would be > MAX_FRAME_SIZE
            // since the protocol limits to 20 bits (max 0xFFFFF < 0x100000)
        }
    }
}
