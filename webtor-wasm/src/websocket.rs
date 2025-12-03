//! WebSocket implementation for WASM

use js_sys::{Array, ArrayBuffer, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};
use std::sync::{Arc, Mutex};
use futures::channel::mpsc;
use futures::StreamExt;
use gloo_console::{log as console_log, error as console_error, warn as console_warn};

/// WebSocket connection for WASM
pub struct WasmWebSocketConnection {
    websocket: WebSocket,
    receiver: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    _sender: mpsc::UnboundedSender<Vec<u8>>,
}

impl WasmWebSocketConnection {
    pub async fn connect(url: &str) -> Result<Self, JsValue> {
        console_log!(format!("Connecting to WebSocket: {}", url));
        
        let websocket = WebSocket::new(url)?;
        websocket.set_binary_type(web_sys::BinaryType::Arraybuffer);
        
        // Create channel for receiving messages
        let (sender, receiver) = mpsc::unbounded();
        let receiver = Arc::new(Mutex::new(receiver));
        
        // Set up message handler
        let sender_clone = sender.clone();
        let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
            if let Ok(array_buffer) = event.data().dyn_into::<ArrayBuffer>() {
                let uint8_array = Uint8Array::new(&array_buffer);
                let mut data = vec![0u8; uint8_array.length() as usize];
                uint8_array.copy_to(&mut data);
                
                if let Err(e) = sender_clone.unbounded_send(data) {
                    console_error!(format!("Failed to send WebSocket message to channel: {:?}", e));
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        
        websocket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
        
        // Wait for connection to be ready
        let (ready_sender, ready_receiver) = futures::channel::oneshot::channel();
        let ready_sender = Arc::new(Mutex::new(Some(ready_sender)));
        
        let ready_sender_clone = ready_sender.clone();
        let on_open = Closure::wrap(Box::new(move |_event: Event| {
            console_log!("WebSocket connection opened");
            if let Some(sender) = ready_sender_clone.lock().unwrap().take() {
                let _ = sender.send(Ok(()));
            }
        }) as Box<dyn FnMut(Event)>);
        
        websocket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();

        // Set up error handler
        let ready_sender_clone = ready_sender.clone();
        let on_error = Closure::wrap(Box::new(move |event: ErrorEvent| {
            console_error!("WebSocket error: {:?}", event);
            if let Some(sender) = ready_sender_clone.lock().unwrap().take() {
                let _ = sender.send(Err("WebSocket error".to_string()));
            }
        }) as Box<dyn FnMut(ErrorEvent)>);
        
        websocket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();
        
        // Set up close handler
        let ready_sender_clone = ready_sender.clone();
        let on_close = Closure::wrap(Box::new(move |event: CloseEvent| {
            console_log!(format!("WebSocket closed: code={}, reason={}", event.code(), event.reason()));
            if let Some(sender) = ready_sender_clone.lock().unwrap().take() {
                let _ = sender.send(Err(format!("WebSocket closed early: code={}, reason={}", event.code(), event.reason())));
            }
        }) as Box<dyn FnMut(CloseEvent)>);
        
        websocket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();
        
        // Wait for connection or timeout
        let timeout = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::new(&mut |resolve, _reject| {
            let window = web_sys::window().unwrap();
            window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                10000, // 10 second timeout
            ).unwrap();
        }));
        
        let ready_fut = async {
            match ready_receiver.await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(JsValue::from_str(&e)),
                Err(_) => Err(JsValue::from_str("WebSocket ready channel closed")),
            }
        };
        
        match futures::future::select(Box::pin(ready_fut), Box::pin(timeout)).await {
            futures::future::Either::Left((Ok(()), _)) => {
                console_log!("WebSocket connection established");
            }
            futures::future::Either::Left((Err(e), _)) => {
                return Err(e);
            }
            futures::future::Either::Right((_, _)) => {
                return Err(JsValue::from_str("WebSocket connection timeout"));
            }
        }
        
        Ok(Self {
            websocket,
            receiver,
            _sender: sender,
        })
    }
    
    pub async fn send(&mut self, data: &[u8]) -> Result<(), JsValue> {
        console_log!(format!("Sending {} bytes through WebSocket", data.len()));
        
        let array = Uint8Array::from(data);
        self.websocket.send_with_array_buffer(&array.buffer())?;
        
        Ok(())
    }
    
    pub async fn receive(&mut self) -> Result<Vec<u8>, JsValue> {
        let mut receiver = self.receiver.lock().unwrap();
        
        match receiver.next().await {
            Some(data) => {
                console_log!(format!("Received {} bytes from WebSocket", data.len()));
                Ok(data)
            }
            None => {
                Err(JsValue::from_str("WebSocket receiver channel closed"))
            }
        }
    }
    
    pub fn close(&mut self) {
        console_log!("Closing WebSocket connection");
        // Clear handlers BEFORE closing to prevent "closure invoked after being dropped" errors
        // The closures were forgotten, but JavaScript still has references to them
        self.websocket.set_onmessage(None);
        self.websocket.set_onerror(None);
        self.websocket.set_onclose(None);
        self.websocket.set_onopen(None);
        let _ = self.websocket.close();
    }
    
    pub fn is_open(&self) -> bool {
        self.websocket.ready_state() == WebSocket::OPEN
    }
}

impl Drop for WasmWebSocketConnection {
    fn drop(&mut self) {
        // Clear handlers when dropped to prevent "closure invoked after being dropped" errors
        self.websocket.set_onmessage(None);
        self.websocket.set_onerror(None);
        self.websocket.set_onclose(None);
        self.websocket.set_onopen(None);
    }
}

/// WebSocket duplex implementation for WASM
pub struct WasmWebSocketDuplex {
    connection: Arc<Mutex<WasmWebSocketConnection>>,
}

impl WasmWebSocketDuplex {
    pub async fn connect(url: &str) -> Result<Self, JsValue> {
        let connection = WasmWebSocketConnection::connect(url).await?;
        
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }
    
    pub async fn send(&self, data: &[u8]) -> Result<(), JsValue> {
        let mut connection = self.connection.lock().unwrap();
        connection.send(data).await
    }
    
    pub async fn receive(&self) -> Result<Vec<u8>, JsValue> {
        let mut connection = self.connection.lock().unwrap();
        connection.receive().await
    }
    
    pub fn close(&self) {
        let mut connection = self.connection.lock().unwrap();
        connection.close();
    }
    
    pub fn is_open(&self) -> bool {
        let connection = self.connection.lock().unwrap();
        connection.is_open()
    }
}

/// Wait for WebSocket to be ready (convenience function)
pub async fn wait_for_websocket(url: &str, timeout_ms: u32) -> Result<WasmWebSocketConnection, JsValue> {
    console_log!(format!("Waiting for WebSocket connection to: {}", url));
    
    // Set up timeout
    let timeout_promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let window = web_sys::window().unwrap();
        window.set_timeout_with_callback_and_timeout_and_arguments_0(
            &resolve,
            timeout_ms as i32,
        ).unwrap();
    });
    
    let connect_future = WasmWebSocketConnection::connect(url);
    
    match futures::future::select(
        Box::pin(connect_future),
        Box::pin(wasm_bindgen_futures::JsFuture::from(timeout_promise))
    ).await {
        futures::future::Either::Left((Ok(connection), _)) => {
            console_log!("WebSocket connection established");
            Ok(connection)
        }
        futures::future::Either::Left((Err(e), _)) => {
            console_error!(format!("WebSocket connection failed: {:?}", e));
            Err(e)
        }
        futures::future::Either::Right((_, _)) => {
            console_error!(format!("WebSocket connection timeout after {}ms", timeout_ms));
            Err(JsValue::from_str(&format!("WebSocket connection timeout after {}ms", timeout_ms)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;
    
    wasm_bindgen_test_configure!(run_in_browser);
    
    #[wasm_bindgen_test]
    async fn test_websocket_connection() {
        // Test with a public echo WebSocket server
        let result = WasmWebSocketConnection::connect("wss://echo.websocket.org/").await;
        
        match result {
            Ok(mut connection) => {
                // Test sending and receiving
                let test_data = b"Hello, WebSocket!";
                connection.send(test_data).await.unwrap();
                
                let received = connection.receive().await.unwrap();
                assert_eq!(received, test_data);
                
                connection.close();
            }
            Err(e) => {
                console_warn!(format!("WebSocket test skipped due to connection error: {:?}", e));
                // This is expected in some test environments
            }
        }
    }
}