//! WebSocket implementation for WASM

use crate::error::{Result, TorError};
use futures::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use futures::channel::mpsc;
    use futures::StreamExt;
    use js_sys::{ArrayBuffer, Uint8Array};
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{BinaryType, ErrorEvent, MessageEvent, WebSocket};

    /// WebSocket stream wrapper implementing AsyncRead and AsyncWrite
    #[allow(dead_code)] // Fields are held to keep callbacks alive
    pub struct WebSocketStream {
        socket: WebSocket,
        rx: mpsc::UnboundedReceiver<io::Result<Vec<u8>>>,
        buffer: Vec<u8>,
        _on_message: Closure<dyn FnMut(MessageEvent)>,
        _on_error: Closure<dyn FnMut(ErrorEvent)>,
        _on_close: Closure<dyn FnMut(web_sys::CloseEvent)>,
    }

    impl WebSocketStream {
        /// Connect to a WebSocket URL
        pub async fn connect(url: &str) -> Result<Self> {
            let socket = WebSocket::new(url)
                .map_err(|e| TorError::Network(format!("Failed to create WebSocket: {:?}", e)))?;
            
            socket.set_binary_type(BinaryType::Arraybuffer);

            let (tx, rx) = mpsc::unbounded();
            
            // Prepare channel clones for callbacks
            let tx_msg = tx.clone();
            let tx_err = tx.clone();
            let tx_close = tx;

            // Shared state for connection status (Open/Error/Close during handshake)
            let (open_tx, open_rx) = futures::channel::oneshot::channel::<std::result::Result<(), String>>();
            let open_tx = Rc::new(RefCell::new(Some(open_tx)));

            // onmessage
            let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                if let Ok(abuf) = e.data().dyn_into::<ArrayBuffer>() {
                    let array = Uint8Array::new(&abuf);
                    let data = array.to_vec();
                    let _ = tx_msg.unbounded_send(Ok(data));
                } else if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                    let data = String::from(txt).into_bytes();
                    let _ = tx_msg.unbounded_send(Ok(data));
                }
            }) as Box<dyn FnMut(MessageEvent)>);
            socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            // onerror
            let open_tx_err = open_tx.clone();
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                let msg = e.message();
                // If we are still connecting, fail the connection future
                if let Some(tx) = open_tx_err.borrow_mut().take() {
                    let _ = tx.send(Err(format!("WebSocket error during connection: {}", msg)));
                }
                let _ = tx_err.unbounded_send(Err(io::Error::new(io::ErrorKind::Other, msg)));
            }) as Box<dyn FnMut(ErrorEvent)>);
            socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            // onclose
            let open_tx_close = open_tx.clone();
            let on_close = Closure::wrap(Box::new(move |e: web_sys::CloseEvent| {
                // If we are still connecting, fail the connection future
                if let Some(tx) = open_tx_close.borrow_mut().take() {
                    let _ = tx.send(Err(format!("WebSocket closed during connection: code={}, reason={}", e.code(), e.reason())));
                }

                if !e.was_clean() {
                     let _ = tx_close.unbounded_send(Err(io::Error::new(io::ErrorKind::ConnectionAborted, format!("Close code: {}", e.code()))));
                }
                // If clean close, just drop sender (handled by closure capture drop)
            }) as Box<dyn FnMut(web_sys::CloseEvent)>);
            socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            // onopen
            let open_tx_open = open_tx.clone();
            let on_open = Closure::once(move || {
                if let Some(tx) = open_tx_open.borrow_mut().take() {
                    let _ = tx.send(Ok(()));
                }
            });
            socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            // Wait for connection or error
            match open_rx.await {
                Ok(Ok(())) => {},
                Ok(Err(e)) => return Err(TorError::Network(e)),
                Err(_) => return Err(TorError::Network("Connection cancelled or sender dropped".to_string())),
            }

            Ok(Self {
                socket,
                rx,
                buffer: Vec::new(),
                _on_message: on_message,
                _on_error: on_error,
                _on_close: on_close,
            })
        }
    }

    impl AsyncRead for WebSocketStream {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            // First, drain internal buffer
            if !self.buffer.is_empty() {
                let len = std::cmp::min(buf.len(), self.buffer.len());
                buf[0..len].copy_from_slice(&self.buffer[0..len]);
                self.buffer.drain(0..len);
                return Poll::Ready(Ok(len));
            }

            // Poll the channel
            match self.rx.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(data))) => {
                    if data.is_empty() {
                        return Poll::Pending; // Should not happen usually
                    }
                    let len = std::cmp::min(buf.len(), data.len());
                    buf[0..len].copy_from_slice(&data[0..len]);
                    
                    // Store remaining
                    if len < data.len() {
                        self.buffer.extend_from_slice(&data[len..]);
                    }
                    
                    Poll::Ready(Ok(len))
                }
                Poll::Ready(Some(Err(e))) => Poll::Ready(Err(e)),
                Poll::Ready(None) => Poll::Ready(Ok(0)), // EOF
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl AsyncWrite for WebSocketStream {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            match self.socket.send_with_u8_array(buf) {
                Ok(_) => Poll::Ready(Ok(buf.len())),
                Err(e) => Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("WebSocket send error: {:?}", e),
                ))),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.socket.close() {
                Ok(_) => Poll::Ready(Ok(())),
                Err(e) => Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("WebSocket close error: {:?}", e),
                ))),
            }
        }
    }
    
    // Implement Sync and Send appropriately if needed, though in WASM it's single threaded.
    // Since we are using Rc in create, this struct is !Send and !Sync.
    // Tor-proto might require Send/Sync depending on the runtime.
    // In WASM with arti, we usually use a LocalRuntime or similar that allows !Send.
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;

    pub struct WebSocketStream;

    impl WebSocketStream {
        pub async fn connect(_url: &str) -> Result<Self> {
            Err(TorError::Internal("WebSocket not supported on native arch".to_string()))
        }
    }

    impl AsyncRead for WebSocketStream {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Not implemented")))
        }
    }

    impl AsyncWrite for WebSocketStream {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Not implemented")))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
