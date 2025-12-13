//! Demo webpage for webtor-rs

use std::sync::{Arc, Mutex, PoisonError};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use web_sys::console;

// Import the webtor WASM bindings
use webtor_wasm::{TorClient, TorClientOptions};

// Re-export logging and version functions
pub use webtor_wasm::{get_version_info, init as webtor_init, set_debug_enabled, set_log_callback};

/// Helper to handle mutex poisoning gracefully
fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Main demo application - simplified API for JavaScript
#[wasm_bindgen]
pub struct DemoApp {
    tor_client: Arc<Mutex<Option<TorClient>>>,
    status_callback: Arc<Mutex<Option<js_sys::Function>>>,
    status_interval: Arc<Mutex<Option<i32>>>,
}

#[wasm_bindgen]
impl DemoApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<DemoApp, JsValue> {
        console::log_1(&"DemoApp created".into());
        Ok(DemoApp {
            tor_client: Arc::new(Mutex::new(None)),
            status_callback: Arc::new(Mutex::new(None)),
            status_interval: Arc::new(Mutex::new(None)),
        })
    }

    /// Set a callback function for status updates
    #[wasm_bindgen(js_name = setStatusCallback)]
    pub fn set_status_callback(&self, callback: js_sys::Function) {
        *lock_or_recover(&self.status_callback) = Some(callback);
    }

    /// Open the TorClient using WebSocket (simpler, less censorship resistant)
    #[wasm_bindgen]
    pub fn open(&self) -> js_sys::Promise {
        let app = self.clone();

        future_to_promise(async move {
            app.update_status("Connecting to Snowflake (WebSocket)...")?;

            // Create TorClient with WebSocket-based Snowflake
            let options = TorClientOptions::new("wss://snowflake.torproject.net/".to_string())
                .with_connection_timeout(15000)
                .with_circuit_timeout(120000)
                .with_create_circuit_early(true)
                .with_circuit_update_interval(Some(120000))
                .with_circuit_update_advance(30000);

            app.update_status("Creating TorClient...")?;

            let client = TorClient::create(options)
                .await
                .map_err(|e| JsValue::from_str(&format!("Failed to create TorClient: {}", e)))?;

            // Store client before waiting
            *lock_or_recover(&app.tor_client) = Some(client.clone());

            app.update_status("Waiting for circuit...")?;

            // Wait for circuit
            client
                .wait_for_circuit_rust()
                .await
                .map_err(|e| JsValue::from_str(&format!("Circuit failed: {}", e)))?;

            // Start status polling
            app.start_status_polling()?;

            app.update_status("Ready")?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Open the TorClient using WebRTC (more censorship resistant via volunteer proxies)
    #[wasm_bindgen(js_name = openWebRtc)]
    pub fn open_webrtc(&self) -> js_sys::Promise {
        let app = self.clone();

        future_to_promise(async move {
            app.update_status("Connecting to Snowflake (WebRTC)...")?;
            app.update_status("Using volunteer proxies for censorship resistance")?;

            // Create TorClient with WebRTC-based Snowflake
            let options = TorClientOptions::snowflake_webrtc()
                .with_connection_timeout(30000) // WebRTC needs more time for signaling
                .with_circuit_timeout(120000)
                .with_create_circuit_early(true)
                .with_circuit_update_interval(Some(120000))
                .with_circuit_update_advance(30000);

            app.update_status("Creating TorClient (WebRTC)...")?;

            let client = TorClient::create(options)
                .await
                .map_err(|e| JsValue::from_str(&format!("Failed to create TorClient: {}", e)))?;

            // Store client before waiting
            *lock_or_recover(&app.tor_client) = Some(client.clone());

            app.update_status("Waiting for circuit...")?;

            // Wait for circuit
            client
                .wait_for_circuit_rust()
                .await
                .map_err(|e| JsValue::from_str(&format!("Circuit failed: {}", e)))?;

            // Start status polling
            app.start_status_polling()?;

            app.update_status("Ready (WebRTC)")?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Close the TorClient
    #[wasm_bindgen]
    pub fn close(&self) -> js_sys::Promise {
        let app = self.clone();

        future_to_promise(async move {
            app.stop_status_polling()?;

            if let Some(mut client) = lock_or_recover(&app.tor_client).take() {
                client.close_rust().await;
            }

            app.update_status("Closed")?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Make a GET request using the persistent circuit
    #[wasm_bindgen]
    pub fn get(&self, url: String) -> js_sys::Promise {
        let app = self.clone();

        future_to_promise(async move {
            let client = lock_or_recover(&app.tor_client)
                .clone()
                .ok_or_else(|| JsValue::from_str("TorClient not open"))?;

            let response = client
                .fetch_rust(&url)
                .await
                .map_err(|e| JsValue::from_str(&format!("{}", e)))?;

            let text = response
                .text()
                .map_err(|e| JsValue::from_str(&format!("Failed to get text: {:?}", e)))?;

            Ok(JsValue::from_str(&text))
        })
    }

    /// Make an isolated GET request (new circuit each time)
    #[wasm_bindgen(js_name = getIsolated)]
    pub fn get_isolated(&self, url: String) -> js_sys::Promise {
        future_to_promise(async move {
            let snowflake_url = "wss://snowflake.torproject.net/";

            let response = TorClient::fetch_one_time_rust(snowflake_url, &url, None, None)
                .await
                .map_err(|e| JsValue::from_str(&format!("{}", e)))?;

            let text = response
                .text()
                .map_err(|e| JsValue::from_str(&format!("Failed to get text: {:?}", e)))?;

            Ok(JsValue::from_str(&text))
        })
    }

    /// Trigger a circuit update
    #[wasm_bindgen(js_name = triggerCircuitUpdate)]
    pub fn trigger_circuit_update(&self) -> js_sys::Promise {
        let app = self.clone();

        future_to_promise(async move {
            let client = lock_or_recover(&app.tor_client)
                .clone()
                .ok_or_else(|| JsValue::from_str("TorClient not open"))?;

            app.update_status("Refreshing circuit...")?;

            client
                .update_circuit_rust(30000)
                .await
                .map_err(|e| JsValue::from_str(&format!("{}", e)))?;

            app.update_status("Ready")?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Get circuit relay information
    #[wasm_bindgen(js_name = getCircuitRelays)]
    pub fn get_circuit_relays(&self) -> js_sys::Promise {
        let app = self.clone();

        future_to_promise(async move {
            let client = lock_or_recover(&app.tor_client)
                .clone()
                .ok_or_else(|| JsValue::from_str("TorClient not open"))?;

            match client.get_circuit_relays_rust().await {
                Some(relays) => {
                    let js_array = js_sys::Array::new();
                    for relay in relays {
                        let obj = js_sys::Object::new();
                        js_sys::Reflect::set(
                            &obj,
                            &"role".into(),
                            &JsValue::from_str(&relay.role),
                        )?;
                        js_sys::Reflect::set(
                            &obj,
                            &"nickname".into(),
                            &JsValue::from_str(&relay.nickname),
                        )?;
                        js_sys::Reflect::set(
                            &obj,
                            &"address".into(),
                            &JsValue::from_str(&relay.address),
                        )?;
                        js_sys::Reflect::set(
                            &obj,
                            &"fingerprint".into(),
                            &JsValue::from_str(&relay.fingerprint),
                        )?;
                        js_array.push(&obj);
                    }
                    Ok(js_array.into())
                }
                None => Ok(JsValue::NULL),
            }
        })
    }
}

// Internal helpers
impl DemoApp {
    fn update_status(&self, status: &str) -> Result<(), JsValue> {
        if let Some(callback) = lock_or_recover(&self.status_callback).as_ref() {
            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(status));
        }
        Ok(())
    }

    fn start_status_polling(&self) -> Result<(), JsValue> {
        let window = web_sys::window().ok_or("No window")?;
        let app = self.clone();

        let closure = Closure::wrap(Box::new(move || {
            let _ = app.poll_status();
        }) as Box<dyn FnMut()>);

        let interval_id = window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            2000, // Poll every 2 seconds (reduced from 1s for performance)
        )?;

        *lock_or_recover(&self.status_interval) = Some(interval_id);
        closure.forget();

        Ok(())
    }

    fn stop_status_polling(&self) -> Result<(), JsValue> {
        if let Some(interval_id) = lock_or_recover(&self.status_interval).take() {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(interval_id);
            }
        }
        Ok(())
    }

    fn poll_status(&self) -> Result<(), JsValue> {
        if let Some(client) = lock_or_recover(&self.tor_client).clone() {
            let app = self.clone();

            let _ = future_to_promise(async move {
                match client.get_circuit_status_string_rust().await {
                    Ok(status) => {
                        let _ = app.update_status(&status);
                    }
                    Err(e) => {
                        console::warn_1(&format!("Status poll error: {}", e).into());
                    }
                }
                Ok(JsValue::UNDEFINED)
            });
        }
        Ok(())
    }
}

impl Clone for DemoApp {
    fn clone(&self) -> Self {
        Self {
            tor_client: self.tor_client.clone(),
            status_callback: self.status_callback.clone(),
            status_interval: self.status_interval.clone(),
        }
    }
}

/// Initialize logging when module loads
#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    webtor_init();
    console::log_1(&"Webtor Demo module initialized".into());
    Ok(())
}

// ============================================================================
// Performance Benchmarks
// ============================================================================

/// Benchmark result structure returned to JavaScript
#[wasm_bindgen]
pub struct BenchmarkResult {
    circuit_creation_ms: f64,
    fetch_latency_ms: f64,
}

#[wasm_bindgen]
impl BenchmarkResult {
    #[wasm_bindgen(getter)]
    pub fn circuit_creation_ms(&self) -> f64 {
        self.circuit_creation_ms
    }

    #[wasm_bindgen(getter)]
    pub fn fetch_latency_ms(&self) -> f64 {
        self.fetch_latency_ms
    }
}

/// Get the Performance API
fn get_performance() -> web_sys::Performance {
    web_sys::window()
        .expect("no window")
        .performance()
        .expect("performance unavailable")
}

/// Run a Tor benchmark measuring circuit creation and fetch latency
///
/// This function measures:
/// 1. Circuit creation time: from TorClient creation to ready circuit
/// 2. Fetch latency: time for a single HTTP GET request through Tor
///
/// @param test_url - URL to fetch for the latency test (e.g., "https://httpbin.org/ip")
/// @returns BenchmarkResult with timing measurements in milliseconds
#[wasm_bindgen(js_name = runTorBenchmark)]
pub async fn run_tor_benchmark(test_url: String) -> Result<BenchmarkResult, JsValue> {
    let perf = get_performance();

    console::log_1(&"Starting Tor benchmark...".into());

    // Measure circuit creation time
    let t0 = perf.now();

    // Create TorClient with WebRTC Snowflake (production config)
    let options = TorClientOptions::snowflake_webrtc()
        .with_connection_timeout(60000)
        .with_circuit_timeout(120000)
        .with_create_circuit_early(true);

    console::log_1(&"Creating TorClient...".into());

    let client = TorClient::create(options)
        .await
        .map_err(|e| JsValue::from_str(&format!("Failed to create TorClient: {}", e)))?;

    console::log_1(&"Waiting for circuit...".into());

    // Wait for circuit to be ready
    client
        .wait_for_circuit_rust()
        .await
        .map_err(|e| JsValue::from_str(&format!("Circuit failed: {}", e)))?;

    let t1 = perf.now();
    let circuit_creation_ms = t1 - t0;

    console::log_1(&format!("Circuit ready in {:.0}ms", circuit_creation_ms).into());

    // Measure fetch latency
    let t2 = perf.now();

    console::log_1(&format!("Fetching {}...", test_url).into());

    let _response = client
        .fetch_rust(&test_url)
        .await
        .map_err(|e| JsValue::from_str(&format!("Fetch failed: {}", e)))?;

    let t3 = perf.now();
    let fetch_latency_ms = t3 - t2;

    console::log_1(&format!("Fetch completed in {:.0}ms", fetch_latency_ms).into());

    // Cleanup
    let mut client_mut = client;
    client_mut.close_rust().await;

    console::log_1(
        &format!(
            "Benchmark complete: circuit={:.0}ms, fetch={:.0}ms",
            circuit_creation_ms, fetch_latency_ms
        )
        .into(),
    );

    Ok(BenchmarkResult {
        circuit_creation_ms,
        fetch_latency_ms,
    })
}

/// Run a quick benchmark using WebSocket Snowflake (faster but less censorship resistant)
#[wasm_bindgen(js_name = runQuickBenchmark)]
pub async fn run_quick_benchmark(test_url: String) -> Result<BenchmarkResult, JsValue> {
    let perf = get_performance();

    console::log_1(&"Starting quick benchmark (WebSocket)...".into());

    let t0 = perf.now();

    // Use WebSocket Snowflake for faster connection
    let options = TorClientOptions::new("wss://snowflake.torproject.net/".to_string())
        .with_connection_timeout(30000)
        .with_circuit_timeout(90000)
        .with_create_circuit_early(true);

    let client = TorClient::create(options)
        .await
        .map_err(|e| JsValue::from_str(&format!("Failed to create TorClient: {}", e)))?;

    client
        .wait_for_circuit_rust()
        .await
        .map_err(|e| JsValue::from_str(&format!("Circuit failed: {}", e)))?;

    let t1 = perf.now();
    let circuit_creation_ms = t1 - t0;

    let t2 = perf.now();
    let _response = client
        .fetch_rust(&test_url)
        .await
        .map_err(|e| JsValue::from_str(&format!("Fetch failed: {}", e)))?;
    let t3 = perf.now();
    let fetch_latency_ms = t3 - t2;

    let mut client_mut = client;
    client_mut.close_rust().await;

    console::log_1(
        &format!(
            "Quick benchmark: circuit={:.0}ms, fetch={:.0}ms",
            circuit_creation_ms, fetch_latency_ms
        )
        .into(),
    );

    Ok(BenchmarkResult {
        circuit_creation_ms,
        fetch_latency_ms,
    })
}
