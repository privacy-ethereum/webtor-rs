//! Demo webpage for webtor-rs

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use web_sys::console;
use std::sync::{Arc, Mutex, PoisonError};

// Import the webtor WASM bindings
use webtor_wasm::{TorClient, TorClientOptions};

// Re-export logging functions
pub use webtor_wasm::{init as webtor_init, set_log_callback, set_debug_enabled};

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

            let client = TorClient::create(options).await
                .map_err(|e| JsValue::from_str(&format!("Failed to create TorClient: {}", e)))?;

            // Store client before waiting
            *lock_or_recover(&app.tor_client) = Some(client.clone());

            app.update_status("Waiting for circuit...")?;

            // Wait for circuit
            client.wait_for_circuit_rust().await
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
                .with_connection_timeout(30000)  // WebRTC needs more time for signaling
                .with_circuit_timeout(120000)
                .with_create_circuit_early(true)
                .with_circuit_update_interval(Some(120000))
                .with_circuit_update_advance(30000);

            app.update_status("Creating TorClient (WebRTC)...")?;

            let client = TorClient::create(options).await
                .map_err(|e| JsValue::from_str(&format!("Failed to create TorClient: {}", e)))?;

            // Store client before waiting
            *lock_or_recover(&app.tor_client) = Some(client.clone());

            app.update_status("Waiting for circuit...")?;

            // Wait for circuit
            client.wait_for_circuit_rust().await
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
            let client = lock_or_recover(&app.tor_client).clone()
                .ok_or_else(|| JsValue::from_str("TorClient not open"))?;

            let response = client.fetch_rust(&url).await
                .map_err(|e| JsValue::from_str(&format!("{}", e)))?;

            let text = response.text()
                .map_err(|e| JsValue::from_str(&format!("Failed to get text: {:?}", e)))?;

            Ok(JsValue::from_str(&text))
        })
    }

    /// Make an isolated GET request (new circuit each time)
    #[wasm_bindgen(js_name = getIsolated)]
    pub fn get_isolated(&self, url: String) -> js_sys::Promise {
        future_to_promise(async move {
            let snowflake_url = "wss://snowflake.torproject.net/";

            let response = TorClient::fetch_one_time_rust(snowflake_url, &url, None, None).await
                .map_err(|e| JsValue::from_str(&format!("{}", e)))?;

            let text = response.text()
                .map_err(|e| JsValue::from_str(&format!("Failed to get text: {:?}", e)))?;

            Ok(JsValue::from_str(&text))
        })
    }

    /// Trigger a circuit update
    #[wasm_bindgen(js_name = triggerCircuitUpdate)]
    pub fn trigger_circuit_update(&self) -> js_sys::Promise {
        let app = self.clone();

        future_to_promise(async move {
            let client = lock_or_recover(&app.tor_client).clone()
                .ok_or_else(|| JsValue::from_str("TorClient not open"))?;

            app.update_status("Refreshing circuit...")?;

            client.update_circuit_rust(30000).await
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
            let client = lock_or_recover(&app.tor_client).clone()
                .ok_or_else(|| JsValue::from_str("TorClient not open"))?;

            match client.get_circuit_relays_rust().await {
                Some(relays) => {
                    let js_array = js_sys::Array::new();
                    for relay in relays {
                        let obj = js_sys::Object::new();
                        js_sys::Reflect::set(&obj, &"role".into(), &JsValue::from_str(&relay.role))?;
                        js_sys::Reflect::set(&obj, &"nickname".into(), &JsValue::from_str(&relay.nickname))?;
                        js_sys::Reflect::set(&obj, &"address".into(), &JsValue::from_str(&relay.address))?;
                        js_sys::Reflect::set(&obj, &"fingerprint".into(), &JsValue::from_str(&relay.fingerprint))?;
                        js_array.push(&obj);
                    }
                    Ok(js_array.into())
                }
                None => Ok(JsValue::NULL)
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
