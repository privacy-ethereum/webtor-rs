//! WebAssembly bindings for webtor

mod websocket;

use gloo_console::{error as console_error, log as console_log, warn as console_warn};
use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use webtor::{TorClient as NativeTorClient, TorClientOptions as NativeTorClientOptions, TorError};

/// Structured error for JavaScript consumption
/// Provides machine-readable error classification for UX and retry decisions
#[derive(serde::Serialize)]
pub struct JsTorError {
    /// Stable error code (e.g., "CIRCUIT_CREATION", "TIMEOUT", "NETWORK")
    pub code: String,
    /// Error kind/category (e.g., "network", "timeout", "circuit", "configuration")
    pub kind: String,
    /// Human-readable error message
    pub message: String,
    /// Whether this error is likely transient and the operation could succeed on retry
    pub retryable: bool,
}

impl From<TorError> for JsTorError {
    fn from(e: TorError) -> Self {
        JsTorError {
            code: e.code().to_string(),
            kind: e.kind().as_code().to_lowercase(),
            message: e.to_string(),
            retryable: e.is_retryable(),
        }
    }
}

impl From<&TorError> for JsTorError {
    fn from(e: &TorError) -> Self {
        JsTorError {
            code: e.code().to_string(),
            kind: e.kind().as_code().to_lowercase(),
            message: e.to_string(),
            retryable: e.is_retryable(),
        }
    }
}

impl JsTorError {
    /// Convert to a JsValue for returning from WASM functions
    pub fn into_js_value(self) -> JsValue {
        serde_wasm_bindgen::to_value(&self).unwrap_or_else(|_| JsValue::from_str(&self.message))
    }

    /// Create a JsTorError for non-TorError cases (e.g., "not initialized")
    pub fn from_str(code: &str, kind: &str, message: &str, retryable: bool) -> Self {
        JsTorError {
            code: code.to_string(),
            kind: kind.to_string(),
            message: message.to_string(),
            retryable,
        }
    }

    /// Create a "not initialized" error
    pub fn not_initialized() -> Self {
        JsTorError::from_str(
            "NOT_INITIALIZED",
            "internal",
            "TorClient is not initialized",
            false,
        )
    }
}

/// Helper to convert TorError to JsValue for Promise rejection
fn tor_error_to_js(e: TorError) -> JsValue {
    JsTorError::from(e).into_js_value()
}

// Thread-local log callback for forwarding logs to JavaScript (WASM is single-threaded)
thread_local! {
    static LOG_CALLBACK: RefCell<Option<js_sys::Function>> = RefCell::new(None);
    static DEBUG_ENABLED: RefCell<bool> = RefCell::new(false);
}

/// Set the log callback function for receiving tracing logs in JavaScript
#[wasm_bindgen(js_name = setLogCallback)]
pub fn set_log_callback(callback: js_sys::Function) {
    LOG_CALLBACK.with(|cb| {
        *cb.borrow_mut() = Some(callback);
    });
}

/// Enable or disable debug-level logging
#[wasm_bindgen(js_name = setDebugEnabled)]
pub fn set_debug_enabled(enabled: bool) {
    DEBUG_ENABLED.with(|debug| {
        *debug.borrow_mut() = enabled;
    });
    console_log!(format!(
        "Debug logging {}",
        if enabled { "enabled" } else { "disabled" }
    ));
}

/// Internal function to forward a log message to JS
fn forward_log(level: &str, target: &str, message: &str) {
    // Check if we should log based on level
    let is_debug = level == "DEBUG" || level == "TRACE";
    if is_debug {
        let debug_enabled = DEBUG_ENABLED.with(|debug| *debug.borrow());
        if !debug_enabled {
            return;
        }
    }

    LOG_CALLBACK.with(|cb| {
        if let Some(ref callback) = *cb.borrow() {
            let this = JsValue::NULL;
            let level_js = JsValue::from_str(level);
            let target_js = JsValue::from_str(target);
            let message_js = JsValue::from_str(message);
            let _ = callback.call3(&this, &level_js, &target_js, &message_js);
        }
    });
}

/// JavaScript-friendly options for TorClient
#[wasm_bindgen]
#[derive(Clone)]
pub struct TorClientOptions {
    inner: NativeTorClientOptions,
}

#[wasm_bindgen]
impl TorClientOptions {
    /// Create options for Snowflake bridge (default)
    #[wasm_bindgen(constructor)]
    pub fn new(snowflake_url: String) -> Self {
        console_log!(format!(
            "Creating TorClientOptions with snowflake URL: {}",
            snowflake_url
        ));

        Self {
            inner: NativeTorClientOptions::new(snowflake_url),
        }
    }

    /// Create options for WebTunnel bridge
    #[wasm_bindgen(js_name = webtunnel)]
    pub fn webtunnel(url: String, fingerprint: String) -> Self {
        console_log!(format!(
            "Creating TorClientOptions with WebTunnel URL: {}",
            url
        ));

        Self {
            inner: NativeTorClientOptions::webtunnel(url, fingerprint),
        }
    }

    /// Create options for Snowflake bridge via WebRTC (more censorship resistant)
    #[wasm_bindgen(js_name = snowflakeWebRtc)]
    pub fn snowflake_webrtc() -> Self {
        console_log!("Creating TorClientOptions with Snowflake WebRTC");

        Self {
            inner: NativeTorClientOptions::snowflake_webrtc(),
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

    #[wasm_bindgen(js_name = withBridgeFingerprint)]
    pub fn with_bridge_fingerprint(mut self, fingerprint: String) -> Self {
        self.inner = self.inner.with_bridge_fingerprint(fingerprint);
        self
    }
}

/// JavaScript-friendly TorClient
#[wasm_bindgen]
#[derive(Clone)]
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
                    console_error!(format!("Failed to create TorClient: {}", e));
                    Err(tor_error_to_js(e))
                }
            }
        })
    }

    /// Make a fetch (GET) request through Tor
    #[wasm_bindgen(js_name = fetch)]
    pub fn fetch(&self, url: String) -> js_sys::Promise {
        console_log!(format!("Starting fetch request to: {}", url));

        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsTorError::not_initialized().into_js_value())
                });
            }
        };

        future_to_promise(async move {
            match client.fetch(&url).await {
                Ok(response) => {
                    console_log!("Fetch request completed successfully");

                    let js_response = JsHttpResponse {
                        status: response.status,
                        headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                        body: response.body,
                        url: response.url.to_string(),
                    };

                    Ok(JsValue::from(js_response))
                }
                Err(e) => {
                    console_error!(format!("Fetch request failed: {}", e));
                    Err(tor_error_to_js(e))
                }
            }
        })
    }

    /// Make a POST request through Tor
    #[wasm_bindgen(js_name = post)]
    pub fn post(&self, url: String, body: Vec<u8>) -> js_sys::Promise {
        console_log!(format!("Starting POST request to: {}", url));

        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsTorError::not_initialized().into_js_value())
                });
            }
        };

        future_to_promise(async move {
            match client.post(&url, body).await {
                Ok(response) => {
                    console_log!("POST request completed successfully");

                    let js_response = JsHttpResponse {
                        status: response.status,
                        headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                        body: response.body,
                        url: response.url.to_string(),
                    };

                    Ok(JsValue::from(js_response))
                }
                Err(e) => {
                    console_error!(format!("POST request failed: {}", e));
                    Err(tor_error_to_js(e))
                }
            }
        })
    }

    /// Make a POST request with JSON body and Content-Type header (convenience for JSON-RPC)
    #[wasm_bindgen(js_name = postJson)]
    pub fn post_json(&self, url: String, json_body: String) -> js_sys::Promise {
        console_log!(format!("Starting POST JSON request to: {}", url));

        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsTorError::not_initialized().into_js_value())
                });
            }
        };

        future_to_promise(async move {
            let mut headers = std::collections::HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json".to_string());

            match client
                .request(
                    http::Method::POST,
                    &url,
                    headers,
                    Some(json_body.into_bytes()),
                    None,
                )
                .await
            {
                Ok(response) => {
                    console_log!("POST JSON request completed successfully");

                    let js_response = JsHttpResponse {
                        status: response.status,
                        headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                        body: response.body,
                        url: response.url.to_string(),
                    };

                    Ok(JsValue::from(js_response))
                }
                Err(e) => {
                    console_error!(format!("POST JSON request failed: {}", e));
                    Err(tor_error_to_js(e))
                }
            }
        })
    }

    /// Make a generic HTTP request with full control over method, headers, body, and timeout
    #[wasm_bindgen(js_name = request)]
    pub fn request(
        &self,
        method: String,
        url: String,
        headers: JsValue,
        body: Option<Vec<u8>>,
        timeout_ms: Option<u32>,
    ) -> js_sys::Promise {
        console_log!(format!("Starting {} request to: {}", method, url));

        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsTorError::not_initialized().into_js_value())
                });
            }
        };

        future_to_promise(async move {
            let method_parsed: http::Method = method
                .parse()
                .map_err(|e| JsValue::from_str(&format!("Invalid HTTP method: {}", e)))?;

            let headers_map: std::collections::HashMap<String, String> =
                if headers.is_undefined() || headers.is_null() {
                    std::collections::HashMap::new()
                } else {
                    serde_wasm_bindgen::from_value(headers)
                        .map_err(|e| JsValue::from_str(&format!("Invalid headers object: {}", e)))?
                };

            let timeout = timeout_ms.map(|ms| std::time::Duration::from_millis(ms as u64));

            match client
                .request(method_parsed, &url, headers_map, body, timeout)
                .await
            {
                Ok(response) => {
                    console_log!("Request completed successfully");

                    let js_response = JsHttpResponse {
                        status: response.status,
                        headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                        body: response.body,
                        url: response.url.to_string(),
                    };

                    Ok(JsValue::from(js_response))
                }
                Err(e) => {
                    console_error!(format!("Request failed: {}", e));
                    Err(tor_error_to_js(e))
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
        console_log!(format!(
            "Making one-time fetch request to: {} via {}",
            url, snowflake_url
        ));

        future_to_promise(async move {
            match NativeTorClient::fetch_one_time(
                &snowflake_url,
                &url,
                connection_timeout.map(|t| t as u64),
                circuit_timeout.map(|t| t as u64),
            )
            .await
            {
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
                    console_error!(format!("One-time fetch request failed: {}", e));
                    Err(tor_error_to_js(e))
                }
            }
        })
    }

    /// Update the circuit
    #[wasm_bindgen(js_name = updateCircuit)]
    pub fn update_circuit(&self, deadline_ms: u32) -> js_sys::Promise {
        console_log!(format!("Updating circuit with {}ms deadline", deadline_ms));

        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move {
                    Err(JsTorError::not_initialized().into_js_value())
                });
            }
        };

        future_to_promise(async move {
            match client
                .update_circuit(Duration::from_millis(deadline_ms as u64))
                .await
            {
                Ok(()) => {
                    console_log!("Circuit update completed");
                    Ok(JsValue::UNDEFINED)
                }
                Err(e) => {
                    console_error!(format!("Circuit update failed: {}", e));
                    Err(tor_error_to_js(e))
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
                    Err(JsTorError::not_initialized().into_js_value())
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
                    console_error!(format!("Failed to wait for circuit: {}", e));
                    Err(tor_error_to_js(e))
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
                    Err(JsTorError::not_initialized().into_js_value())
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
                return future_to_promise(async move { Ok(JsValue::from_str("Not initialized")) });
            }
        };

        future_to_promise(async move {
            let status_string = client.get_circuit_status_string().await;
            Ok(JsValue::from_str(&status_string))
        })
    }

    /// Get circuit relay information
    #[wasm_bindgen(js_name = getCircuitRelays)]
    pub fn get_circuit_relays(&self) -> js_sys::Promise {
        let client = match &self.inner {
            Some(client) => client.clone(),
            None => {
                return future_to_promise(async move { Ok(JsValue::NULL) });
            }
        };

        future_to_promise(async move {
            match client.get_circuit_relays().await {
                Some(relays) => {
                    let js_relays: Vec<JsCircuitRelay> = relays
                        .into_iter()
                        .map(|r| JsCircuitRelay {
                            role: r.role,
                            nickname: r.nickname,
                            address: r.address,
                            fingerprint: r.fingerprint,
                        })
                        .collect();
                    Ok(serde_wasm_bindgen::to_value(&js_relays).unwrap_or(JsValue::NULL))
                }
                None => Ok(JsValue::NULL),
            }
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

    /// Abort all in-flight operations.
    ///
    /// This cancels long-running operations like circuit creation and HTTP requests.
    /// Operations will reject with a "CANCELLED" error code.
    /// Unlike `close()`, this does not clean up resources - the client can still be used.
    #[wasm_bindgen(js_name = abort)]
    pub fn abort(&self) {
        console_log!("Aborting TorClient operations");

        if let Some(client) = &self.inner {
            client.abort();
            console_log!("TorClient abort signal sent");
        } else {
            console_warn!("TorClient is not initialized");
        }
    }

    /// Check if the client has been aborted.
    #[wasm_bindgen(js_name = isAborted)]
    pub fn is_aborted(&self) -> bool {
        if let Some(client) = &self.inner {
            client.is_aborted()
        } else {
            true
        }
    }

    // --- Rust-friendly methods (not exposed to JS, but usable by other Rust crates) ---

    pub async fn create(options: TorClientOptions) -> Result<Self, String> {
        match NativeTorClient::new(options.inner).await {
            Ok(client) => Ok(TorClient {
                inner: Some(Arc::new(client)),
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn fetch_rust(&self, url: &str) -> Result<JsHttpResponse, String> {
        let client = self.inner.as_ref().ok_or("TorClient is not initialized")?;
        match client.fetch(url).await {
            Ok(response) => Ok(JsHttpResponse {
                status: response.status,
                headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                body: response.body,
                url: response.url.to_string(),
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn post_rust(&self, url: &str, body: Vec<u8>) -> Result<JsHttpResponse, String> {
        let client = self.inner.as_ref().ok_or("TorClient is not initialized")?;
        match client.post(url, body).await {
            Ok(response) => Ok(JsHttpResponse {
                status: response.status,
                headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                body: response.body,
                url: response.url.to_string(),
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn wait_for_circuit_rust(&self) -> Result<(), String> {
        let client = self.inner.as_ref().ok_or("TorClient is not initialized")?;
        client.wait_for_circuit().await.map_err(|e| e.to_string())
    }

    pub async fn close_rust(&mut self) {
        if let Some(client) = self.inner.take() {
            // We need to clone the Arc because close takes self/&self but we are taking ownership of the Arc from Option
            // Actually client.close() takes &self.
            // But we removed it from Option, so we own the Arc.
            // NativeTorClient::close is async.
            client.close().await;
        }
    }

    pub async fn get_circuit_status_string_rust(&self) -> Result<String, String> {
        let client = self.inner.as_ref().ok_or("TorClient is not initialized")?;
        Ok(client.get_circuit_status_string().await)
    }

    pub async fn fetch_one_time_rust(
        snowflake_url: &str,
        url: &str,
        connection_timeout: Option<u64>,
        circuit_timeout: Option<u64>,
    ) -> Result<JsHttpResponse, String> {
        match NativeTorClient::fetch_one_time(
            snowflake_url,
            url,
            connection_timeout,
            circuit_timeout,
        )
        .await
        {
            Ok(response) => Ok(JsHttpResponse {
                status: response.status,
                headers: serde_wasm_bindgen::to_value(&response.headers).unwrap(),
                body: response.body,
                url: response.url.to_string(),
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn update_circuit_rust(&self, deadline_ms: u64) -> Result<(), String> {
        let client = self.inner.as_ref().ok_or("TorClient is not initialized")?;
        client
            .update_circuit(Duration::from_millis(deadline_ms))
            .await
            .map_err(|e| e.to_string())
    }
}

/// Simple circuit relay info for internal use (not WASM-bound)
pub struct CircuitRelayInfoSimple {
    pub role: String,
    pub nickname: String,
    pub address: String,
    pub fingerprint: String,
}

// Non-wasm_bindgen impl block for methods that return non-WASM types
impl TorClient {
    pub async fn get_circuit_relays_rust(&self) -> Option<Vec<CircuitRelayInfoSimple>> {
        let client = self.inner.as_ref()?;
        client.get_circuit_relays().await.map(|relays| {
            relays
                .into_iter()
                .map(|r| CircuitRelayInfoSimple {
                    role: r.role,
                    nickname: r.nickname,
                    address: r.address,
                    fingerprint: r.fingerprint,
                })
                .collect()
        })
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

/// JavaScript-friendly circuit relay info
#[derive(serde::Serialize, serde::Deserialize)]
pub struct JsCircuitRelay {
    pub role: String,
    pub nickname: String,
    pub address: String,
    pub fingerprint: String,
}

/// Custom tracing layer that forwards logs to JavaScript
struct JsLogLayer;

impl<S> tracing_subscriber::Layer<S> for JsLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let level = metadata.level().as_str();
        let target = metadata.target();

        // Extract the message from the event
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        let message = visitor.0;

        forward_log(level, target, &message);
    }
}

struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{:?}", value);
        } else if !self.0.is_empty() {
            self.0.push_str(&format!(" {}={:?}", field.name(), value));
        } else {
            self.0 = format!("{}={:?}", field.name(), value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        } else if !self.0.is_empty() {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        } else {
            self.0 = format!("{}={}", field.name(), value);
        }
    }
}

/// Check if secure randomness (crypto.getRandomValues) is available
fn check_secure_randomness() -> Result<(), String> {
    let mut test_buf = [0u8; 32];
    getrandom::getrandom(&mut test_buf).map_err(|e| {
        format!(
            "Secure randomness (crypto.getRandomValues) is not available: {}. \
             Webtor requires a secure CSPRNG and cannot run in this environment.",
            e
        )
    })?;
    
    if test_buf == [0u8; 32] {
        return Err(
            "Secure randomness check failed: CSPRNG returned all zeros. \
             This indicates a broken or missing crypto.getRandomValues implementation."
                .to_string(),
        );
    }
    Ok(())
}

/// Initialize the WASM module
#[wasm_bindgen]
pub fn init() -> Result<(), JsValue> {
    // Set up panic handler
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    // Fail-fast: verify secure randomness is available before anything else
    check_secure_randomness().map_err(|e| {
        console_error!(format!("[FAIL] {}", e));
        JsTorError::from_str("WASM_ENVIRONMENT", "environment", &e, false).into_js_value()
    })?;

    // Set up tracing with our custom layer that forwards to JS
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let wasm_layer = tracing_wasm::WASMLayer::new(tracing_wasm::WASMLayerConfig::default());
    let js_layer = JsLogLayer;

    tracing_subscriber::registry()
        .with(wasm_layer)
        .with(js_layer)
        .init();

    console_log!("Webtor WASM module initialized (CSPRNG verified)");
    Ok(())
}

/// Test function for WASM
#[wasm_bindgen]
pub fn test_wasm() -> String {
    "Webtor WASM is working!".to_string()
}

/// Get version information for display in UI
#[wasm_bindgen(js_name = getVersionInfo)]
pub fn get_version_info() -> JsValue {
    let info = serde_json::json!({
        "webtor": env!("CARGO_PKG_VERSION"),
        "webtor_wasm": env!("CARGO_PKG_VERSION"),
        "subtle_tls": env!("SUBTLE_TLS_VERSION"),
        "arti_version": env!("ARTI_VERSION"),
        "tor_proto": env!("TOR_PROTO_VERSION"),
    });
    serde_wasm_bindgen::to_value(&info).unwrap_or(JsValue::NULL)
}
