//! HTTP client for making requests through Tor circuits

use crate::circuit::CircuitManager;
use crate::error::{Result, TorError};
use http::Method;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info};
use url::Url;

/// HTTP request configuration
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub url: Url,
    pub method: Method,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
}

impl Default for HttpRequest {
    fn default() -> Self {
        Self {
            url: Url::parse("http://example.com/").unwrap(),
            method: Method::GET,
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
        }
    }
}

impl HttpRequest {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            ..Default::default()
        }
    }
    
    pub fn with_method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }
    
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
    
    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// HTTP client that routes requests through Tor circuits
pub struct TorHttpClient {
    circuit_manager: CircuitManager,
}

impl TorHttpClient {
    pub fn new(circuit_manager: CircuitManager) -> Self {
        Self { circuit_manager }
    }
    
    /// Make an HTTP request through Tor
    pub async fn request(&self, request: HttpRequest) -> Result<HttpResponse> {
        info!("Making {} request to {} through Tor", request.method, request.url);
        
        // Parse URL to get host and port
        let host = request.url.host_str()
            .ok_or_else(|| TorError::http_request("Invalid URL: no host"))?;
        
        let port = request.url.port_or_known_default()
            .ok_or_else(|| TorError::http_request("Invalid URL: no port"))?;
        
        let is_https = request.url.scheme() == "https";
        
        debug!("Target: {}:{} (HTTPS: {})", host, port, is_https);
        
        // Get a ready circuit
        let circuit = self.circuit_manager.get_ready_circuit().await?;
        
        // Update circuit last used time
        {
            let mut circuit_write = circuit.write().await;
            circuit_write.update_last_used();
        }
        
        // For now, we'll return a placeholder response
        Ok(HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body: b"Placeholder response - full implementation pending".to_vec(),
            url: request.url.clone(),
        })
    }
    
    /// Convenience method for GET requests
    pub async fn get(&self, url: &str) -> Result<HttpResponse> {
        let url = Url::parse(url)?;
        let request = HttpRequest::new(url);
        self.request(request).await
    }
    
    /// Convenience method for POST requests
    pub async fn post(&self, url: &str, body: Vec<u8>) -> Result<HttpResponse> {
        let url = Url::parse(url)?;
        let request = HttpRequest::new(url)
            .with_method(Method::POST)
            .with_body(body);
        self.request(request).await
    }
}

/// HTTP response from Tor
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub url: Url,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
    
    pub fn text(&self) -> Result<String> {
        String::from_utf8(self.body.clone())
            .map_err(|e| TorError::serialization(format!("Invalid UTF-8 in response: {}", e)))
    }
    
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        let text = self.text()?;
        serde_json::from_str(&text)
            .map_err(|e| TorError::serialization(format!("Invalid JSON in response: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::{Relay, RelayManager, flags};
    
    fn create_test_relay(fingerprint: &str, flags: Vec<&str>) -> Relay {
        Relay {
            fingerprint: fingerprint.to_string(),
            nickname: format!("test_{}", fingerprint),
            address: "127.0.0.1".to_string(),
            or_port: 9001,
            dir_port: Some(9030),
            flags: flags.into_iter().map(String::from).collect(),
            bandwidth: 1000000,
            consensus_weight: 100,
            version: "0.4.5.6".to_string(),
            microdescriptor_hash: "test_hash".to_string(),
        }
    }
    
    #[tokio::test]
    async fn test_http_request_creation() {
        let url = Url::parse("https://httpbin.org/ip").unwrap();
        let request = HttpRequest::new(url.clone())
            .with_method(Method::GET)
            .with_header("User-Agent", "Webtor/0.1.0")
            .with_timeout(Duration::from_secs(30));
        
        assert_eq!(request.url, url);
        assert_eq!(request.method, Method::GET);
        assert_eq!(request.headers.get("User-Agent"), Some(&"Webtor/0.1.0".to_string()));
        assert_eq!(request.timeout, Duration::from_secs(30));
    }
    
    #[tokio::test]
    async fn test_http_response() {
        let response = HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body: b"{\"ip\": \"127.0.0.1\"}".to_vec(),
            url: Url::parse("https://httpbin.org/ip").unwrap(),
        };
        
        assert!(response.is_success());
        
        let text = response.text().unwrap();
        assert_eq!(text, "{\"ip\": \"127.0.0.1\"}");
        
        #[derive(serde::Deserialize)]
        struct IpResponse {
            ip: String,
        }
        
        let json: IpResponse = response.json().unwrap();
        assert_eq!(json.ip, "127.0.0.1");
    }
    
    #[tokio::test]
    async fn test_tor_http_client() {
        let relays = vec![
            create_test_relay("guard1", vec![flags::FAST, flags::STABLE, flags::GUARD]),
            create_test_relay("middle1", vec![flags::FAST, flags::STABLE, flags::V2DIR]),
            create_test_relay("exit1", vec![flags::FAST, flags::STABLE, flags::EXIT]),
        ];
        
        let relay_manager = RelayManager::new(relays);
        let circuit_manager = CircuitManager::new(relay_manager);
        let http_client = TorHttpClient::new(circuit_manager);
        
        // This will fail because we don't have WASM WebSocket implementation
        let result = http_client.get("https://httpbin.org/ip").await;
        assert!(result.is_err());
    }
}
