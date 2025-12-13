//! HTTP client for making requests through Tor circuits

use crate::circuit::CircuitManager;
use crate::config::{CIRCUIT_PREBUILD_AGE_THRESHOLD_MS, MAX_CIRCUITS};
use crate::error::{Result, TorError};
use crate::isolation::{IsolationKey, StreamIsolationPolicy};
#[cfg(not(target_arch = "wasm32"))]
use crate::tls::wrap_with_tls;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use http::Method;
#[cfg(not(target_arch = "wasm32"))]
use rustls;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
#[cfg(not(target_arch = "wasm32"))]
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info, warn};
use url::Url;
#[cfg(not(target_arch = "wasm32"))]
use webpki_roots;

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

    /// Build the HTTP request as raw bytes
    fn build_request(&self, host: &str) -> Vec<u8> {
        let path = if self.url.path().is_empty() {
            "/"
        } else {
            self.url.path()
        };

        let query = self
            .url
            .query()
            .map(|q| format!("?{}", q))
            .unwrap_or_default();

        let mut request = format!(
            "{} {}{} HTTP/1.1\r\nHost: {}\r\n",
            self.method.as_str(),
            path,
            query,
            host
        );

        // Add default headers if not present
        if !self.headers.contains_key("User-Agent") && !self.headers.contains_key("user-agent") {
            request.push_str("User-Agent: webtor-rs/0.1.0\r\n");
        }
        if !self.headers.contains_key("Accept") && !self.headers.contains_key("accept") {
            request.push_str("Accept: */*\r\n");
        }
        if !self.headers.contains_key("Connection") && !self.headers.contains_key("connection") {
            request.push_str("Connection: close\r\n");
        }

        // Add custom headers
        for (key, value) in &self.headers {
            request.push_str(&format!("{}: {}\r\n", key, value));
        }

        // Add content-length for POST/PUT requests with body
        if let Some(ref body) = self.body {
            request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }

        // End headers
        request.push_str("\r\n");

        let mut bytes = request.into_bytes();

        // Add body if present
        if let Some(ref body) = self.body {
            bytes.extend_from_slice(body);
        }

        bytes
    }
}

trait AnyStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AnyStream for T {}

/// HTTP client that routes requests through Tor circuits
pub struct TorHttpClient {
    circuit_manager: Arc<RwLock<CircuitManager>>,
    isolation_policy: StreamIsolationPolicy,
    #[cfg(not(target_arch = "wasm32"))]
    tls_connector: Arc<tokio_rustls::TlsConnector>,
}

impl TorHttpClient {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        circuit_manager: Arc<RwLock<CircuitManager>>,
        isolation_policy: StreamIsolationPolicy,
    ) -> Self {
        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();

        let tls_connector = tokio_rustls::TlsConnector::from(Arc::new(config));

        Self {
            circuit_manager,
            isolation_policy,
            tls_connector: Arc::new(tls_connector),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(
        circuit_manager: Arc<RwLock<CircuitManager>>,
        isolation_policy: StreamIsolationPolicy,
    ) -> Self {
        Self {
            circuit_manager,
            isolation_policy,
        }
    }

    /// Make an HTTP request through Tor
    pub async fn request(&self, request: HttpRequest) -> Result<HttpResponse> {
        info!(
            "Making {} request to {} through Tor",
            request.method, request.url
        );

        // Parse URL to get host and port
        let url = request.url.clone();
        let host = url
            .host_str()
            .ok_or_else(|| TorError::http_request("Invalid URL: no host"))?
            .to_string();

        let port = url
            .port_or_known_default()
            .ok_or_else(|| TorError::http_request("Invalid URL: no port"))?;

        let is_https = url.scheme() == "https";

        debug!("Target: {}:{} (HTTPS: {})", host, port, is_https);

        // Compute isolation key based on policy
        let isolation_key = IsolationKey::from_url(&url, self.isolation_policy);
        if let Some(ref key) = isolation_key {
            debug!(
                "Using isolation key: {} (policy: {:?})",
                key, self.isolation_policy
            );
        }

        // Get a circuit for this isolation key
        let circuit_manager = self.circuit_manager.read().await;
        let circuit = circuit_manager
            .get_circuit_for_isolation_key(isolation_key)
            .await?;

        // Begin stream on the circuit
        let stream = {
            let circuit_read = circuit.read().await;
            circuit_read.begin_stream(&host, port).await?
        };

        // Build the HTTP request
        let request_bytes = request.build_request(&host);
        debug!("Sending {} bytes of HTTP request", request_bytes.len());

        // Execute request with or without TLS
        let response_bytes = if is_https {
            #[cfg(not(target_arch = "wasm32"))]
            {
                // Wrap stream with TLS using rustls
                let tls_stream = wrap_with_tls(stream, &host).await?;
                execute_http_request(tls_stream, &request_bytes).await?
            }
            #[cfg(target_arch = "wasm32")]
            {
                // Use subtle-tls for WASM (SubtleCrypto-based TLS)
                use subtle_tls::{TlsConfig, TlsConnector, TlsVersion};

                let config = TlsConfig {
                    skip_verification: false,
                    alpn_protocols: vec!["http/1.1".to_string()],
                    version: TlsVersion::Tls13,
                };
                let connector = TlsConnector::with_config(config);

                // Try TLS 1.3 first
                match connector.connect(stream, &host).await {
                    Ok(mut tls_stream) => {
                        info!(
                            "TLS 1.3 connection established with {} (WASM/SubtleCrypto)",
                            host
                        );
                        execute_http_request_wasm(&mut tls_stream, &request_bytes).await?
                    }
                    Err(tls13_err) => {
                        warn!(
                            "TLS 1.3 handshake failed with {}: {}, trying TLS 1.2...",
                            host, tls13_err
                        );

                        // Get a new stream for TLS 1.2 retry
                        let stream_tls12 = {
                            let mut circuit_write = circuit.write().await;
                            circuit_write.begin_stream(&host, port).await?
                        };

                        // Try TLS 1.2
                        let config_tls12 = TlsConfig {
                            skip_verification: false,
                            alpn_protocols: vec!["http/1.1".to_string()],
                            version: TlsVersion::Tls12,
                        };
                        let connector_tls12 = TlsConnector::with_config(config_tls12);

                        match connector_tls12.connect_tls12(stream_tls12, &host).await {
                            Ok(mut tls_stream) => {
                                info!(
                                    "TLS 1.2 connection established with {} (WASM/SubtleCrypto)",
                                    host
                                );
                                execute_http_request_wasm_tls12(&mut tls_stream, &request_bytes)
                                    .await?
                            }
                            Err(tls12_err) => {
                                warn!("TLS 1.2 handshake also failed with {}: {}", host, tls12_err);
                                return Err(TorError::tls(format!(
                                    "TLS handshake failed - TLS 1.3: {}, TLS 1.2: {}",
                                    tls13_err, tls12_err
                                )));
                            }
                        }
                    }
                }
            }
        } else {
            execute_http_request(stream, &request_bytes).await?
        };

        info!("Received {} bytes of HTTP response", response_bytes.len());

        // Trigger preemptive circuit building after successful request
        let age_threshold = Duration::from_millis(CIRCUIT_PREBUILD_AGE_THRESHOLD_MS);
        circuit_manager
            .maybe_prebuild_circuit(MAX_CIRCUITS, age_threshold)
            .await;

        // Parse the HTTP response
        parse_http_response(&response_bytes, request.url)
    }

    fn parse_response(data: Vec<u8>, url: Url) -> Result<HttpResponse> {
        // Simple HTTP parser
        // Split into headers and body
        let mut headers = [httparse::Header {
            name: "",
            value: &[],
        }; 64];
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
            }
            Ok(httparse::Status::Partial) => {
                return Err(TorError::serialization("Partial response received"))
            }
            Err(e) => {
                return Err(TorError::serialization(format!(
                    "Failed to parse response: {}",
                    e
                )))
            }
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

/// Trait for TLS streams that support async read/write operations
#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
trait WasmTlsStream {
    async fn tls_write(&mut self, buf: &[u8]) -> std::io::Result<usize>;
    async fn tls_flush(&mut self) -> std::io::Result<()>;
    async fn tls_read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
}

#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
impl<S> WasmTlsStream for subtle_tls::TlsStream<S>
where
    S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
{
    async fn tls_write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write(buf).await
    }
    async fn tls_flush(&mut self) -> std::io::Result<()> {
        self.flush().await
    }
    async fn tls_read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read(buf).await
    }
}

#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
impl<S> WasmTlsStream for subtle_tls::TlsStream12<S>
where
    S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
{
    async fn tls_write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write(buf).await
    }
    async fn tls_flush(&mut self) -> std::io::Result<()> {
        self.flush().await
    }
    async fn tls_read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read(buf).await
    }
}

/// Execute an HTTP request over a TLS stream in WASM using async methods
#[cfg(target_arch = "wasm32")]
async fn execute_http_request_wasm<T: WasmTlsStream>(
    tls_stream: &mut T,
    request_bytes: &[u8],
) -> Result<Vec<u8>> {
    tls_stream
        .tls_write(request_bytes)
        .await
        .map_err(|e| TorError::http_request(format!("Failed to write request: {}", e)))?;
    tls_stream
        .tls_flush()
        .await
        .map_err(|e| TorError::http_request(format!("Failed to flush request: {}", e)))?;

    let mut response_bytes = Vec::new();
    let mut buf = [0u8; 8192];

    loop {
        match tls_stream.tls_read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                response_bytes.extend_from_slice(&buf[..n]);
                debug!("Read {} bytes (total: {})", n, response_bytes.len());

                if response_bytes.len() > 1024 * 1024 {
                    warn!("Response exceeds 1MB limit, truncating");
                    break;
                }
            }
            Err(e) => {
                if response_bytes.is_empty() {
                    return Err(TorError::http_request(format!(
                        "Failed to read response: {}",
                        e
                    )));
                }
                debug!("Read ended with error (may be normal close): {}", e);
                break;
            }
        }
    }

    Ok(response_bytes)
}

/// Wrapper for TLS 1.2 streams to use with execute_http_request_wasm
#[cfg(target_arch = "wasm32")]
async fn execute_http_request_wasm_tls12<S>(
    tls_stream: &mut subtle_tls::TlsStream12<S>,
    request_bytes: &[u8],
) -> Result<Vec<u8>>
where
    S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
{
    execute_http_request_wasm(tls_stream, request_bytes).await
}

/// Execute an HTTP request over a stream and return the response bytes
async fn execute_http_request<S>(mut stream: S, request_bytes: &[u8]) -> Result<Vec<u8>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // Write the request
    stream
        .write_all(request_bytes)
        .await
        .map_err(|e| TorError::http_request(format!("Failed to write request: {}", e)))?;
    stream
        .flush()
        .await
        .map_err(|e| TorError::http_request(format!("Failed to flush request: {}", e)))?;

    // Read the response
    let mut response_bytes = Vec::new();
    let mut buf = [0u8; 8192];

    loop {
        match stream.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                response_bytes.extend_from_slice(&buf[..n]);
                debug!("Read {} bytes (total: {})", n, response_bytes.len());

                // Limit response size to 1MB for safety
                if response_bytes.len() > 1024 * 1024 {
                    warn!("Response exceeds 1MB limit, truncating");
                    break;
                }
            }
            Err(e) => {
                if response_bytes.is_empty() {
                    return Err(TorError::http_request(format!(
                        "Failed to read response: {}",
                        e
                    )));
                }
                // We have some data, maybe connection was closed
                debug!("Read ended with error (may be normal close): {}", e);
                break;
            }
        }
    }

    Ok(response_bytes)
}

/// Parse raw HTTP response bytes into HttpResponse
fn parse_http_response(data: &[u8], url: Url) -> Result<HttpResponse> {
    // Find the header/body separator
    let header_end = find_subsequence(data, b"\r\n\r\n")
        .ok_or_else(|| TorError::http_request("Invalid HTTP response: no header separator"))?;

    let header_bytes = &data[..header_end];
    let body = data[header_end + 4..].to_vec();

    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|e| TorError::http_request(format!("Invalid HTTP headers: {}", e)))?;

    let mut lines = header_str.lines();

    // Parse status line: "HTTP/1.1 200 OK"
    let status_line = lines
        .next()
        .ok_or_else(|| TorError::http_request("Invalid HTTP response: no status line"))?;

    let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(TorError::http_request("Invalid HTTP status line"));
    }

    let status: u16 = parts[1]
        .parse()
        .map_err(|e| TorError::http_request(format!("Invalid status code: {}", e)))?;

    // Parse headers
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    debug!(
        "Parsed response: status={}, headers={}, body_len={}",
        status,
        headers.len(),
        body.len()
    );

    Ok(HttpResponse {
        status,
        headers,
        body,
        url,
    })
}

/// Find the position of a subsequence in a byte slice
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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
    use crate::relay::{flags, Relay, RelayManager};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn create_test_relay(fingerprint: &str, flags: Vec<&str>) -> Relay {
        Relay::new(
            fingerprint.to_string(),
            format!("test_{}", fingerprint),
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
        assert_eq!(
            request.headers.get("User-Agent"),
            Some(&"Webtor/0.1.0".to_string())
        );
        assert_eq!(request.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_build_request() {
        let url = Url::parse("http://example.com/path?query=1").unwrap();
        let request = HttpRequest::new(url).with_header("X-Custom", "value");

        let bytes = request.build_request("example.com");
        let request_str = String::from_utf8(bytes).unwrap();

        assert!(request_str.starts_with("GET /path?query=1 HTTP/1.1\r\n"));
        assert!(request_str.contains("Host: example.com\r\n"));
        assert!(request_str.contains("X-Custom: value\r\n"));
        assert!(request_str.ends_with("\r\n\r\n"));
    }

    #[test]
    fn test_parse_http_response() {
        let response_bytes = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nHello, World!";
        let url = Url::parse("http://example.com/").unwrap();

        let response = parse_http_response(response_bytes, url).unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(
            response.headers.get("content-type"),
            Some(&"text/plain".to_string())
        );
        assert_eq!(response.text().unwrap(), "Hello, World!");
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
        let circuit_manager = Arc::new(RwLock::new(CircuitManager::new(
            Arc::new(RwLock::new(relay_manager)),
            Arc::new(RwLock::new(None)),
        )));
        let http_client = TorHttpClient::new(circuit_manager, StreamIsolationPolicy::PerDomain);

        // This will fail because we don't have a real circuit
        let result = http_client.get("http://httpbin.org/ip").await;
        assert!(result.is_err());
    }
}
