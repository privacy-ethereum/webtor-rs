//! HTTP client for making requests through Tor circuits

use crate::circuit::CircuitManager;
use crate::error::{Result, TorError};
use http::Method;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;
use tracing::{debug, info};
use tokio::sync::RwLock;
use url::Url;
use futures::{AsyncReadExt as FuturesAsyncReadExt, AsyncWriteExt as FuturesAsyncWriteExt};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use rustls;
use webpki_roots;
use std::sync::Arc;

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

trait AnyStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AnyStream for T {}


/// HTTP client that routes requests through Tor circuits
pub struct TorHttpClient {
    circuit_manager: Arc<RwLock<CircuitManager>>,
    tls_connector: Arc<TlsConnector>,
}

impl TorHttpClient {
    pub fn new(circuit_manager: Arc<RwLock<CircuitManager>>) -> Self {
        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store.extend(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
        );

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();

        let tls_connector = TlsConnector::from(Arc::new(config));

        Self {
            circuit_manager,
            tls_connector: Arc::new(tls_connector),
        }
    }
    
    /// Make an HTTP request through Tor
    pub async fn request(&self, request: HttpRequest) -> Result<HttpResponse> {
        info!("Making {} request to {} through Tor", request.method, request.url);
        
        // Parse URL to get host and port
        let url = request.url.clone();
        let host = url.host_str()
            .ok_or_else(|| TorError::http_request("Invalid URL: no host"))?.to_string();
        
        let port = url.port_or_known_default()
            .ok_or_else(|| TorError::http_request("Invalid URL: no port"))?;
        
        let is_https = url.scheme() == "https";
        
        debug!("Target: {}:{} (HTTPS: {})", host, port, is_https);
        
        // Get a ready circuit
        let circuit_manager = self.circuit_manager.read().await;
        let circuit = circuit_manager.get_ready_circuit().await?;
        
        // Get the internal tunnel
        let tunnel = {
            let mut circuit_write = circuit.write().await;
            circuit_write.update_last_used();
            circuit_write.internal_circuit.clone()
                .ok_or_else(|| TorError::Internal("Circuit has no internal tunnel".to_string()))? 
        };
        
        // Begin stream
        let stream = tunnel.begin_stream(&host, port, None)
            .await
            .map_err(|e| TorError::Network(format!("Failed to begin stream: {}", e)))?;
            
        let mut boxed_stream: Box<dyn AnyStream> = if is_https {
            let server_name = rustls::pki_types::ServerName::try_from(host.as_str())
                .map_err(|_| TorError::http_request("Invalid DNS name"))? 
                .to_owned();

            let tls_stream = self.tls_connector.connect(server_name, stream.compat()).await
                .map_err(|e| TorError::Network(format!("TLS connect failed: {}", e)))?;
            Box::new(tls_stream)
        } else {
            Box::new(stream.compat())
        };

        // Construct HTTP request
        let path = request.url.path();
        let query = request.url.query().map(|q| format!("?{}", q)).unwrap_or_default();
        let target = format!("{}{}", path, query);
        
        let mut headers = request.headers.clone();
        headers.entry("Host".to_string()).or_insert_with(|| host.clone());
        headers.entry("Connection".to_string()).or_insert_with(|| "close".to_string());
        headers.entry("User-Agent".to_string()).or_insert_with(|| "webtor-rs/0.1.0".to_string());
        
        let mut req_buf = Vec::new();
        req_buf.extend_from_slice(format!("{} {} HTTP/1.1\r\n", request.method, target).as_bytes());
        
        for (key, value) in &headers {
            req_buf.extend_from_slice(format!("{}: {}\r\n", key, value).as_bytes());
        }
        
        if let Some(body) = &request.body {
            req_buf.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
            req_buf.extend_from_slice(b"\r\n");
            req_buf.extend_from_slice(body);
        } else {
            req_buf.extend_from_slice(b"\r\n");
        }
        
        // Write request
        boxed_stream.write_all(&req_buf).await
            .map_err(|e| TorError::Network(format!("Failed to write request: {}", e)))?;
        boxed_stream.flush().await
            .map_err(|e| TorError::Network(format!("Failed to flush request: {}", e)))?;
            
        // Read response
        let mut response_buf = Vec::new();
        boxed_stream.read_to_end(&mut response_buf).await
            .map_err(|e| TorError::Network(format!("Failed to read response: {}", e)))?;
            
        Self::parse_response(response_buf, url)
    }
    
    fn parse_response(data: Vec<u8>, url: Url) -> Result<HttpResponse> {
        // Simple HTTP parser
        // Split into headers and body
        let mut headers = [httparse::Header { name: "", value: &[] }; 64];
        let mut req = httparse::Response::new(&mut headers);
        
        let status = match req.parse(&data) {
            Ok(httparse::Status::Complete(n)) => {
                let code = req.code.unwrap_or(0);
                let mut headers_map = HashMap::new();
                
                for header in req.headers {
                    if let Ok(value) = std::str::from_utf8(header.value) {
                        headers_map.insert(header.name.to_string(), value.to_string());
                    }
                }
                
                let body = data[n..].to_vec();
                
                HttpResponse {
                    status: code,
                    headers: headers_map,
                    body,
                    url,
                }
            },
            Ok(httparse::Status::Partial) => return Err(TorError::serialization("Partial response received")),
            Err(e) => return Err(TorError::serialization(format!("Failed to parse response: {}", e))),
        };
        
        Ok(status)
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
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    fn create_test_relay(fingerprint: &str, flags: Vec<&str>) -> Relay {
        Relay::new(
            fingerprint.to_string(),
            format!("test_{{}}", fingerprint),
            "127.0.0.1".to_string(),
            9001,
            flags.into_iter().map(String::from).collect(),
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        )
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
        let circuit_manager = Arc::new(RwLock::new(CircuitManager::new(Arc::new(RwLock::new(relay_manager)), Arc::new(RwLock::new(None)))));
        let http_client = TorHttpClient::new(circuit_manager);
        
        // This will fail because we don't have WASM WebSocket implementation
        let result = http_client.get("https://httpbin.org/ip").await;
        assert!(result.is_err());
    }
}