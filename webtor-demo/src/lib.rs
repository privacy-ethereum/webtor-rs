//! Demo webpage for webtor-rs

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use web_sys::{console, Document, Element, HtmlButtonElement, HtmlElement, HtmlInputElement, HtmlTextAreaElement, Window};
use std::sync::{Arc, Mutex};

// Import the webtor WASM bindings
use webtor_wasm::{TorClient, TorClientOptions, JsHttpResponse, JsCircuitStatus};

/// Main demo application
#[wasm_bindgen]
pub struct DemoApp {
    document: Document,
    window: Window,
    tor_client: Arc<Mutex<Option<TorClient>>>,
    status_update_interval: Arc<Mutex<Option<i32>>>,
}

#[wasm_bindgen]
impl DemoApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<DemoApp, JsValue> {
        console::log_1(&"Initializing Webtor Demo App".into());
        
        let window = web_sys::window().ok_or("No window found")?;
        let document = window.document().ok_or("No document found")?;
        
        Ok(DemoApp {
            document,
            window,
            tor_client: Arc::new(Mutex::new(None)),
            status_update_interval: Arc::new(Mutex::new(None)),
        })
    }
    
    /// Initialize the demo UI
    #[wasm_bindgen(js_name = initUI)]
    pub fn init_ui(&self) -> Result<(), JsValue> {
        console::log_1(&"Initializing UI".into());
        
        // Set up event listeners
        self.setup_event_listeners()?;
        
        // Initialize status display
        self.update_status_display("TorClient not initialized")?;
        
        // Log initialization complete
        self.log_output("🌐 Webtor Demo initialized", "info")?;
        self.log_output("📦 TorClient WASM module loaded", "info")?;
        self.log_output("👆 Click 'Open TorClient' to begin!", "info")?;
        
        Ok(())
    }
    
    /// Open TorClient
    #[wasm_bindgen(js_name = openTorClient)]
    pub fn open_tor_client(&self) -> js_sys::Promise {
        console::log_1(&"Opening TorClient".into());
        
        let app = self.clone();
        
        future_to_promise(async move {
            // Get Snowflake URL from input
            let snowflake_url = app.get_snowflake_url()?;
            console::log_1(&format!("Using Snowflake URL: {}", snowflake_url).into());
            
            // Create TorClient options
            let options = TorClientOptions::new(snowflake_url)
                .with_connection_timeout(15000)
                .with_circuit_timeout(90000)
                .with_create_circuit_early(true)
                .with_circuit_update_interval(Some(120000)) // 2 minutes
                .with_circuit_update_advance(30000); // 30 seconds
            
            // Create TorClient
            match TorClient::create(options).await {
                Ok(client) => {
                    console::log_1(&"TorClient created successfully".into());
                    
                    // Store the client
                    *app.tor_client.lock().unwrap() = Some(client);
                    
                    // Update UI
                    app.set_button_state("openBtn", true, "⏳ Opening...")?;
                    
                    // Wait for circuit to be ready
                    let client_ref = app.tor_client.lock().unwrap().as_ref().unwrap().clone();
                    match client_ref.wait_for_circuit_rust().await {
                        Ok(()) => {
                            console::log_1(&"Circuit is ready".into());
                            
                            // Update UI
                            app.set_button_state("openBtn", false, "🔓 Close TorClient")?;
                            app.set_button_state("closeBtn", false, "🛑 Close TorClient")?;
                            
                            // Start status updates
                            app.start_status_updates()?;
                            
                            app.log_output("🎉 TorClient is ready!", "success")?;
                            app.log_output("💡 Use the URL textboxes to make requests", "info")?;
                            
                            Ok(JsValue::UNDEFINED)
                        }
                        Err(e) => {
                            console::error_1(&format!("Failed to wait for circuit: {:?}", e).into());
                            app.log_output(&format!("❌ Failed to wait for circuit: {:?}", e), "error")?;
                            Err(JsValue::from_str(&format!("Failed to wait for circuit: {:?}", e)))
                        }
                    }
                }
                Err(e) => {
                    console::error_1(&format!("Failed to create TorClient: {:?}", e).into());
                    app.log_output(&format!("❌ Failed to create TorClient: {:?}", e), "error")?;
                    Err(JsValue::from_str(&format!("Failed to create TorClient: {:?}", e)))
                }
            }
        })
    }
    
    /// Close TorClient
    #[wasm_bindgen(js_name = closeTorClient)]
    pub fn close_tor_client(&self) -> js_sys::Promise {
        console::log_1(&"Closing TorClient".into());
        
        let app = self.clone();
        
        future_to_promise(async move {
            // Stop status updates
            app.stop_status_updates()?;
            
            // Close TorClient
            if let Some(mut client) = app.tor_client.lock().unwrap().take() {
                client.close_rust().await;
            }
            
            // Update UI
            app.set_button_state("openBtn", false, "🔓 Open TorClient")?;
            app.set_button_state("closeBtn", true, "🛑 Close TorClient")?;
            app.update_status_display("TorClient closed")?;
            
            app.log_output("🛑 TorClient closed", "info")?;
            
            Ok(JsValue::UNDEFINED)
        })
    }
    
    /// Make a request
    #[wasm_bindgen(js_name = makeRequest)]
    pub fn make_request(&self, index: u32) -> js_sys::Promise {
        console::log_1(&format!("Making request {}", index).into());
        
        let app = self.clone();
        
        future_to_promise(async move {
            // Get URL from input
            let url = app.get_url_input(index)?;
            if url.is_empty() {
                app.set_request_output(index, "❌ Please enter a URL", "error")?;
                return Err(JsValue::from_str("Please enter a URL"));
            }
            
            // Check if TorClient is available
            let client_opt = app.tor_client.lock().unwrap().clone();
            let client = if client_opt.is_none() {
                app.set_request_output(index, "🔧 Creating TorClient automatically...", "loading")?;
                app.log_output("🔧 TorClient not open. Creating automatically...", "info")?;
                
                // Try to create TorClient automatically
                match wasm_bindgen_futures::JsFuture::from(app.open_tor_client()).await {
                    Ok(_) => {
                        // Try again with the new client
                        let new_client = app.tor_client.lock().unwrap().clone();
                        if new_client.is_none() {
                            app.set_request_output(index, "❌ Failed to create TorClient automatically", "error")?;
                            return Err(JsValue::from_str("Failed to create TorClient automatically"));
                        }
                        new_client.unwrap()
                    }
                    Err(e) => {
                        app.set_request_output(index, "❌ Failed to create TorClient automatically", "error")?;
                        return Err(e);
                    }
                }
            } else {
                client_opt.unwrap()
            };
            
            // Update UI
            app.set_request_output(index, "🔄 Loading...", "loading")?;
            app.set_button_state(&format!("btn{}", index), true, "⏳ Loading...")?;
            
            // Make the request
            console::log_1(&format!("Making request to: {}", url).into());
            app.log_output(&format!("🌐 Making request {} to {}", index, url), "info")?;
            
            let start = js_sys::Date::now();
            match client.fetch_rust(&url).await {
                Ok(response) => {
                    let duration = js_sys::Date::now() - start;
                    console::log_1(&format!("Request completed in {}ms", duration).into());
                    
                    // Try to parse response
                    let text = response.text().map_err(|e| JsValue::from_str(&e.as_string().unwrap_or_else(|| "Unknown error".to_string())))?;
                    
                    // Format output based on URL and response
                    let output = if url.contains("/ip") && text.contains("origin") {
                        // Try to extract IP from JSON
                        if let Ok(json) = response.json() {
                            let obj = js_sys::Object::from(json);
                            if let Some(ip) = js_sys::Reflect::get(&obj, &"origin".into()).ok() {
                                if let Some(ip_str) = ip.as_string() {
                                    format!("✅ Success ({:.0}ms)\n📍 IP: {}", duration, ip_str)
                                } else {
                                    format!("✅ Success ({:.0}ms)\n📄 Response: {}", duration, &text[..text.len().min(200)])
                                }
                            } else {
                                format!("✅ Success ({:.0}ms)\n📄 Response: {}", duration, &text[..text.len().min(200)])
                            }
                        } else {
                            format!("✅ Success ({:.0}ms)\n📄 Response: {}", duration, &text[..text.len().min(200)])
                        }
                    } else if url.contains("/user-agent") {
                        format!("✅ Success ({:.0}ms)\n🔍 User-Agent extracted", duration)
                    } else if url.contains("/headers") {
                        format!("✅ Success ({:.0}ms)\n📋 Headers extracted", duration)
                    } else {
                        format!("✅ Success ({:.0}ms)\n📄 Response: {}", duration, &text[..text.len().min(200)])
                    };
                    
                    app.set_request_output(index, &output, "success")?;
                    app.log_output(&format!("✅ Request {} completed in {:.0}ms", index, duration), "success")?;
                }
                Err(e) => {
                    let error_msg = format!("❌ Request failed: {:?}", e);
                    app.set_request_output(index, &error_msg, "error")?;
                    app.log_output(&format!("❌ Request {} failed: {:?}", index, e), "error")?;
                }
            }
            
            // Re-enable button
            app.set_button_state(&format!("btn{}", index), false, &format!("🌐 Make Request {}", index))?;
            
            Ok(JsValue::UNDEFINED)
        })
    }
    
    /// Make an isolated request
    #[wasm_bindgen(js_name = makeIsolatedRequest)]
    pub fn make_isolated_request(&self) -> js_sys::Promise {
        console::log_1(&"Making isolated request".into());
        
        let app = self.clone();
        
        future_to_promise(async move {
            // Get URL from input
            let url = app.get_isolated_url()?;
            if url.is_empty() {
                app.set_isolated_output("❌ Please enter a URL for isolated request", "error")?;
                return Err(JsValue::from_str("Please enter a URL for isolated request"));
            }
            
            // Get Snowflake URL
            let snowflake_url = app.get_snowflake_url()?;
            
            // Update UI
            app.set_isolated_output("🔒 Creating temporary circuit and making request...", "loading")?;
            app.set_button_state("btnIsolated", true, "⏳ Loading...")?;
            
            app.log_output("🔒 Making isolated request with temporary circuit...", "info")?;
            app.log_output(&format!("🔒 Using Snowflake URL: {}", snowflake_url), "info")?;
            
            // Make isolated request
            let start = js_sys::Date::now();
            match TorClient::fetch_one_time_rust(&snowflake_url, &url, None, None).await {
                Ok(response) => {
                    let duration = js_sys::Date::now() - start;
                    console::log_1(&format!("Isolated request completed in {}ms", duration).into());
                    
                    // Try to parse response
                    let text = response.text().map_err(|e| JsValue::from_str(&e.as_string().unwrap_or_else(|| "Unknown error".to_string())))?;
                    
                    // Format output
                    let output = if url.contains("/uuid") {
                        format!("✅ Success ({:.0}ms)\n🔒 UUID from isolated circuit: {}", duration, &text[..text.len().min(50)])
                    } else if url.contains("/ip") {
                        format!("✅ Success ({:.0}ms)\n🔒 IP from isolated circuit: {}", duration, &text[..text.len().min(50)])
                    } else {
                        format!("✅ Success ({:.0}ms)\n🔒 Response: {}", duration, &text[..text.len().min(200)])
                    };
                    
                    app.set_isolated_output(&output, "success")?;
                    app.log_output(&format!("🔒 Isolated request completed in {:.0}ms", duration), "success")?;
                }
                Err(e) => {
                    let error_msg = format!("❌ Isolated request failed: {:?}", e);
                    app.set_isolated_output(&error_msg, "error")?;
                    app.log_output(&format!("❌ Isolated request failed: {:?}", e), "error")?;
                }
            }
            
            // Re-enable button
            app.set_button_state("btnIsolated", false, "🔒 Make Isolated Request")?;
            
            Ok(JsValue::UNDEFINED)
        })
    }
    
    /// Trigger circuit update
    #[wasm_bindgen(js_name = triggerCircuitUpdate)]
    pub fn trigger_circuit_update(&self) -> js_sys::Promise {
        console::log_1(&"Triggering circuit update".into());
        
        let app = self.clone();
        
        future_to_promise(async move {
            let client = app.tor_client.lock().unwrap().clone();
            if client.is_none() {
                app.log_output("❌ No persistent client available for circuit update", "error")?;
                return Err(JsValue::from_str("No persistent client available for circuit update"));
            }
            
            app.log_output("🔄 Manually triggering circuit update with 10s deadline...", "info")?;
            
            match client.unwrap().update_circuit_rust(10000).await {
                Ok(()) => {
                    app.log_output("🔄 Circuit update completed successfully", "success")?;
                }
                Err(e) => {
                    app.log_output(&format!("❌ Circuit update failed: {:?}", e), "error")?;
                    return Err(JsValue::from_str(&format!("Circuit update failed: {:?}", e)));
                }
            }
            
            Ok(JsValue::UNDEFINED)
        })
    }
    
    /// Clear output
    #[wasm_bindgen(js_name = clearOutput)]
    pub fn clear_output(&self) -> Result<(), JsValue> {
        let output = self.document.get_element_by_id("output")
            .ok_or("Output element not found")?
            .dyn_into::<HtmlTextAreaElement>()?;
        
        output.set_value("");
        Ok(())
    }
}

// Helper methods
impl DemoApp {
    fn setup_event_listeners(&self) -> Result<(), JsValue> {
        // Open button
        if let Some(btn) = self.document.get_element_by_id("openBtn") {
            let app = self.clone();
            let closure = Closure::wrap(Box::new(move || {
                let _ = app.open_tor_client();
            }) as Box<dyn FnMut()>);
            
            btn.dyn_into::<HtmlButtonElement>()?
                .set_onclick(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
        
        // Close button
        if let Some(btn) = self.document.get_element_by_id("closeBtn") {
            let app = self.clone();
            let closure = Closure::wrap(Box::new(move || {
                let _ = app.close_tor_client();
            }) as Box<dyn FnMut()>);
            
            btn.dyn_into::<HtmlButtonElement>()?
                .set_onclick(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
        
        // Clear output button
        if let Some(btn) = self.document.get_element_by_id("clearBtn") {
            let app = self.clone();
            let closure = Closure::wrap(Box::new(move || {
                let _ = app.clear_output();
            }) as Box<dyn FnMut()>);
            
            btn.dyn_into::<HtmlButtonElement>()?
                .set_onclick(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
        
        // Request buttons
        for i in 1..=3 {
            if let Some(btn) = self.document.get_element_by_id(&format!("btn{}", i)) {
                let app = self.clone();
                let index = i;
                let closure = Closure::wrap(Box::new(move || {
                    let _ = app.make_request(index);
                }) as Box<dyn FnMut()>);
                
                btn.dyn_into::<HtmlButtonElement>()?
                    .set_onclick(Some(closure.as_ref().unchecked_ref()));
                closure.forget();
            }
        }
        
        // Isolated request button
        if let Some(btn) = self.document.get_element_by_id("btnIsolated") {
            let app = self.clone();
            let closure = Closure::wrap(Box::new(move || {
                let _ = app.make_isolated_request();
            }) as Box<dyn FnMut()>);
            
            btn.dyn_into::<HtmlButtonElement>()?
                .set_onclick(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
        
        // Circuit update button
        if let Some(btn) = self.document.get_element_by_id("updateBtn") {
            let app = self.clone();
            let closure = Closure::wrap(Box::new(move || {
                let _ = app.trigger_circuit_update();
            }) as Box<dyn FnMut()>);
            
            btn.dyn_into::<HtmlButtonElement>()?
                .set_onclick(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
        
        Ok(())
    }
    
    fn get_snowflake_url(&self) -> Result<String, JsValue> {
        let input = self.document.get_element_by_id("snowflakeUrl")
            .ok_or("Snowflake URL input not found")?
            .dyn_into::<HtmlInputElement>()?;
        
        Ok(input.value())
    }
    
    fn get_url_input(&self, index: u32) -> Result<String, JsValue> {
        let input = self.document.get_element_by_id(&format!("url{}", index))
            .ok_or(&format!("URL input {} not found", index))?
            .dyn_into::<HtmlInputElement>()?;
        
        Ok(input.value())
    }
    
    fn get_isolated_url(&self) -> Result<String, JsValue> {
        let input = self.document.get_element_by_id("isolatedUrl")
            .ok_or("Isolated URL input not found")?
            .dyn_into::<HtmlInputElement>()?;
        
        Ok(input.value())
    }
    
    fn set_request_output(&self, index: u32, message: &str, class_name: &str) -> Result<(), JsValue> {
        let output = self.document.get_element_by_id(&format!("output{}", index))
            .ok_or(&format!("Output element {} not found", index))?;
        
        output.set_text_content(Some(message));
        
        // Set CSS class for styling
        let element = output.dyn_into::<HtmlElement>()?;
        element.set_class_name(class_name);
        
        Ok(())
    }
    
    fn set_isolated_output(&self, message: &str, class_name: &str) -> Result<(), JsValue> {
        self.set_request_output(0, message, class_name) // Use index 0 for isolated output
    }
    
    fn set_button_state(&self, button_id: &str, disabled: bool, text: &str) -> Result<(), JsValue> {
        let button = self.document.get_element_by_id(button_id)
            .ok_or(&format!("Button {} not found", button_id))?
            .dyn_into::<HtmlButtonElement>()?;
        
        button.set_disabled(disabled);
        button.set_text_content(Some(text));
        
        Ok(())
    }
    
    fn log_output(&self, message: &str, log_type: &str) -> Result<(), JsValue> {
        let output = self.document.get_element_by_id("output")
            .ok_or("Output element not found")?
            .dyn_into::<HtmlTextAreaElement>()?;
        
        let timestamp = js_sys::Date::new_0().to_locale_time_string("en-US");
        let prefix = match log_type {
            "error" => "❌",
            "success" => "✅",
            _ => "ℹ️",
        };
        
        let log_entry = format!("[{}] {} {}\n", timestamp, prefix, message);
        let current_value = output.value();
        output.set_value(&format!("{}{}", current_value, log_entry));
        
        // Auto-scroll to bottom
        output.set_scroll_top(output.scroll_height());
        
        // Also log to console
        console::log_1(&format!("{}: {}", prefix, message).into());
        
        Ok(())
    }
    
    fn update_status_display(&self, status: &str) -> Result<(), JsValue> {
        let status_element = self.document.get_element_by_id("status")
            .ok_or("Status element not found")?;
        
        status_element.set_inner_html(&format!("<div><strong>Circuit Status:</strong> {}</div>", status));
        
        Ok(())
    }
    
    fn start_status_updates(&self) -> Result<(), JsValue> {
        let app = self.clone();
        
        let closure = Closure::wrap(Box::new(move || {
            let _ = app.update_circuit_status();
        }) as Box<dyn FnMut()>);
        
        let interval_id = self.window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            500, // Update every 500ms
        )?;
        
        *self.status_update_interval.lock().unwrap() = Some(interval_id);
        closure.forget();
        
        Ok(())
    }
    
    fn stop_status_updates(&self) -> Result<(), JsValue> {
        if let Some(interval_id) = self.status_update_interval.lock().unwrap().take() {
            self.window.clear_interval_with_handle(interval_id);
        }
        Ok(())
    }
    
    fn update_circuit_status(&self) -> Result<(), JsValue> {
        if let Some(client) = self.tor_client.lock().unwrap().clone() {
            let app = self.clone();
            
            let _ = future_to_promise(async move {
                match client.get_circuit_status_string_rust().await {
                    Ok(status_string) => {
                        let _ = app.update_status_display(&status_string);
                    }
                    Err(e) => {
                        console::warn_1(&format!("Failed to get circuit status: {:?}", e).into());
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
            document: self.document.clone(),
            window: self.window.clone(),
            tor_client: self.tor_client.clone(),
            status_update_interval: self.status_update_interval.clone(),
        }
    }
}

/// Initialize the demo when the page loads
#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    console::log_1(&"Webtor Demo starting...".into());
    
    // Create the demo app
    let app = DemoApp::new()?;
    
    // Initialize the UI
    app.init_ui()?;
    
    console::log_1(&"Webtor Demo initialized successfully".into());
    
    Ok(())
}