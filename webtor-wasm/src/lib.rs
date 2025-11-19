//! WebAssembly bindings for webtor

mod websocket;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use webtor::{TorClient as NativeTorClient, TorClientOptions as NativeTorClientOptions};

// Re-export for JavaScript
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    
    #[wasm_bindgen(js_namespace = console)]
    fn error(s: &str);
    
    #[wasm_bindgen(js_namespace = console)]
    fn warn(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

macro_rules! console_error {
    ($($t:tt)*) => (error(&format_args!($($t)*).to_string()))
}

macro_rules! console_warn {
    ($($t:tt)*) => (warn(&format_args!($($t)*).to_string()))
}

/// JavaScript-friendly options for TorClient
#[wasm_bindgen]
pub struct TorClientOptions {
    inner: NativeTorClientOptions,
}

#[wasm_bindgen]
impl TorClientOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(snowflake_url: String) -> Self {
        console_log!("Creating TorClientOptions with snowflake URL: {}", snowflake_url);
        
        Self {
            inner: NativeTorClientOptions::new(snowflake_url),
        }
    }
    
    #[wasm_bindgen(js_name = withConnectionTimeout)]
    pub fn with_connection_timeout(mut self, timeout: u32) -> Self {
        self.inner = self.inner.with_connection_timeout(timeout as u64);
        self
    }
    
    #[wasm_bindgen(js_name = withCircuitTimeout)]
    pub fn with_circuit_timeout(mut self, timeout: u32) -> Self {
        self.inner = self.inner.with_circuit_timeout(timeout as u64);
        self
    }
    
    #[wasm_bindgen(js_name = withCreateCircuitEarly)]
    pub fn with_create_circuit_early(mut self, create_early: bool) -> Self {
        self.inner = self.inner.with_create_circuit_early(create_early);
        self
    }
    
    #[wasm_bindgen(js_name = withCircuitUpdateInterval)]
    pub fn with_circuit_update_interval(mut self, interval: Option<u32>) -> Self {
        let interval_ms = interval.map(|i| i as u64);
        self.inner = self.inner.with_circuit_update_interval(interval_ms);
        self
    }
    
    #[wasm_bindgen(js_name = withCircuitUpdateAdvance)]
    pub fn with_circuit_update_advance(mut self, advance: u32) -> Self {
        self.inner = self.inner.with_circuit_update_advance(advance as u64);
        self
    }
}

/// JavaScript-friendly TorClient
#[wasm_bindgen]
pub struct TorClient {
    inner: Option<Arc<NativeTorClient>>,
}

#[wasm_bindgen]
impl TorClient {
    #[wasm_bindgen(constructor)]
    pub fn new(options: TorClientOptions) -> js_sys::Promise {
        console_log!("Creating new TorClient");
        
        let options = options.inner;
        
        future_to_promise(async move {
            match NativeTorClient::new(options).await {
                Ok(client) => {
                    console_log!("TorClient created successfully");
                    Ok(JsValue::from(TorClient {
                        inner: Some(Arc::new(client)),
                    }))
                }
                Err(e) => {
                    console_error!("Failed to create TorClient: {}", e);
                    Err(JsValue::from_str(&format!("Failed to create TorClient: {}", e)))
                }
            }
        })
    }
    
    /// Make a fetch request through Tor
    #[wasm_bindgen(js_name = fetch)]
    pub fn fetch(&self, url: String) -> js_sys::Promise {
        console_log!("Starting fetch request to: {}", url);
        
        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsValue::from_str("TorClient is not initialized"))
                });
            }
        };
        
        future_to_promise(async move {
            match client.fetch(&url).await {
                Ok(response) => {
                    console_log!("Fetch request completed successfully");
                    
                    // Convert to JavaScript-friendly response
                    let js_response = JsHttpResponse {
                        status: response.status,
                        headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                        body: response.body,
                        url: response.url.to_string(),
                    };
                    
                    Ok(JsValue::from(js_response))
                }
                Err(e) => {
                    console_error!("Fetch request failed: {}", e);
                    Err(JsValue::from_str(&format!("Fetch request failed: {}", e)))
                }
            }
        })
    }
    
    /// Make a one-time fetch request (static method)
    #[wasm_bindgen(js_name = fetchOneTime)]
    pub fn fetch_one_time(
        snowflake_url: String,
        url: String,
        connection_timeout: Option<u32>,
        circuit_timeout: Option<u32>,
    ) -> js_sys::Promise {
        console_log!("Making one-time fetch request to: {} via {}", url, snowflake_url);
        
        future_to_promise(async move {
            match NativeTorClient::fetch_one_time(
                &snowflake_url,
                &url,
                connection_timeout.map(|t| t as u64),
                circuit_timeout.map(|t| t as u64),
            ).await {
                Ok(response) => {
                    console_log!("One-time fetch request completed successfully");
                    
                    let js_response = JsHttpResponse {
                        status: response.status,
                        headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                        body: response.body,
                        url: response.url.to_string(),
                    };
                    
                    Ok(JsValue::from(js_response))
                }
                Err(e) => {
                    console_error!("One-time fetch request failed: {}", e);
                    Err(JsValue::from_str(&format!("One-time fetch request failed: {}", e)))
                }
            }
        })
    }
    
    /// Update the circuit
    #[wasm_bindgen(js_name = updateCircuit)]
    pub fn update_circuit(&self, deadline_ms: u32) -> js_sys::Promise {
        console_log!("Updating circuit with {}ms deadline", deadline_ms);
        
        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsValue::from_str("TorClient is not initialized"))
                });
            }
        };
        
        future_to_promise(async move {
            match client.update_circuit(Duration::from_millis(deadline_ms as u64)).await {
                Ok(()) => {
                    console_log!("Circuit update completed");
                    Ok(JsValue::UNDEFINED)
                }
                Err(e) => {
                    console_error!("Circuit update failed: {}", e);
                    Err(JsValue::from_str(&format!("Circuit update failed: {}", e)))
                }
            }
        })
    }
    
    /// Wait for circuit to be ready
    #[wasm_bindgen(js_name = waitForCircuit)]
    pub fn wait_for_circuit(&self) -> js_sys::Promise {
        console_log!("Waiting for circuit to be ready");
        
        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsValue::from_str("TorClient is not initialized"))
                });
            }
        };
        
        future_to_promise(async move {
            match client.wait_for_circuit().await {
                Ok(()) => {
                    console_log!("Circuit is ready");
                    Ok(JsValue::UNDEFINED)
                }
                Err(e) => {
                    console_error!("Failed to wait for circuit: {}", e);
                    Err(JsValue::from_str(&format!("Failed to wait for circuit: {}", e)))
                }
            }
        })
    }
    
    /// Get circuit status
    #[wasm_bindgen(js_name = getCircuitStatus)]
    pub fn get_circuit_status(&self) -> js_sys::Promise {
        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsValue::from_str("TorClient is not initialized"))
                });
            }
        };
        
        future_to_promise(async move {
            let status = client.get_circuit_status().await;
            
            let js_status = JsCircuitStatus {
                total_circuits: status.total_circuits as u32,
                ready_circuits: status.ready_circuits as u32,
                creating_circuits: status.creating_circuits as u32,
                failed_circuits: status.failed_circuits as u32,
                has_ready_circuits: status.has_ready_circuits(),
                is_healthy: status.is_healthy(),
            };
            
            Ok(JsValue::from(js_status))
        })
    }
    
    /// Get circuit status string
    #[wasm_bindgen(js_name = getCircuitStatusString)]
    pub fn get_circuit_status_string(&self) -> js_sys::Promise {
        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Ok(JsValue::from_str("Not initialized"))
                });
            }
        };
        
        future_to_promise(async move {
            let status_string = client.get_circuit_status_string().await;
            Ok(JsValue::from_str(&status_string))
        })
    }
    
    /// Close the Tor client
    #[wasm_bindgen(js_name = close)]
    pub fn close(&mut self) -> js_sys::Promise {
        console_log!("Closing TorClient");
        
        if let Some(client) = self.inner.take() {
            future_to_promise(async move {
                // Create a copy of the client for the async block
                let client_copy = (*client).clone();
                client_copy.close().await;
                console_log!("TorClient closed successfully");
                Ok(JsValue::UNDEFINED)
            })
        } else {
            future_to_promise(async move {
                console_warn!("TorClient was already closed");
                Ok(JsValue::UNDEFINED)
            })
        }
    }
}

/// JavaScript-friendly HTTP response
#[wasm_bindgen]
pub struct JsHttpResponse {
    status: u16,
    headers: JsValue,
    body: Vec<u8>,
    url: String,
}

#[wasm_bindgen]
impl JsHttpResponse {
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> u16 {
        self.status
    }
    
    #[wasm_bindgen(getter)]
    pub fn headers(&self) -> JsValue {
        self.headers.clone()
    }
    
    #[wasm_bindgen(getter)]
    pub fn body(&self) -> Vec<u8> {
        self.body.clone()
    }
    
    #[wasm_bindgen(getter)]
    pub fn url(&self) -> String {
        self.url.clone()
    }
    
    #[wasm_bindgen(js_name = text)]
    pub fn text(&self) -> Result<String, JsValue> {
        String::from_utf8(self.body.clone())
            .map_err(|e| JsValue::from_str(&format!("Invalid UTF-8: {}", e)))
    }
    
    #[wasm_bindgen(js_name = json)]
    pub fn json(&self) -> Result<JsValue, JsValue> {
        let text = self.text()?;
        serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| JsValue::from_str(&format!("Invalid JSON: {}", e)))
            .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
    }
}

/// JavaScript-friendly circuit status
#[wasm_bindgen]
pub struct JsCircuitStatus {
    total_circuits: u32,
    ready_circuits: u32,
    creating_circuits: u32,
    failed_circuits: u32,
    has_ready_circuits: bool,
    is_healthy: bool,
}

#[wasm_bindgen]
impl JsCircuitStatus {
    #[wasm_bindgen(getter)]
    pub fn total_circuits(&self) -> u32 {
        self.total_circuits
    }
    
    #[wasm_bindgen(getter)]
    pub fn ready_circuits(&self) -> u32 {
        self.ready_circuits
    }
    
    #[wasm_bindgen(getter)]
    pub fn creating_circuits(&self) -> u32 {
        self.creating_circuits
    }
    
    #[wasm_bindgen(getter)]
    pub fn failed_circuits(&self) -> u32 {
        self.failed_circuits
    }
    
    #[wasm_bindgen(getter)]
    pub fn has_ready_circuits(&self) -> bool {
        self.has_ready_circuits
    }
    
    #[wasm_bindgen(getter)]
    pub fn is_healthy(&self) -> bool {
        self.is_healthy
    }
}

/// Initialize the WASM module
#[wasm_bindgen(start)]
pub fn main() {
    // Set up panic handler
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    
    // Set up tracing
    tracing_wasm::set_as_global_default();
    
    console_log!("Webtor WASM module initialized");
}

/// Test function for WASM
#[wasm_bindgen]
pub fn test_wasm() -> String {
    "Webtor WASM is working!".to_string()
}
