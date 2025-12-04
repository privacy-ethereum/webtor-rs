//! Snowflake broker client for getting proxy assignments
//!
//! The broker acts as a signaling channel that matches clients with volunteer
//! proxy WebRTC peers. It uses HTTP POST with JSON-encoded messages.
//!
//! Protocol:
//! 1. Client creates WebRTC SDP offer
//! 2. Client POSTs offer to broker at /client endpoint
//! 3. Broker matches with available proxy
//! 4. Proxy responds with SDP answer via broker
//! 5. Client receives answer and completes WebRTC connection

use crate::error::{Result, TorError};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Snowflake broker URL (direct - has CORS support)
pub const BROKER_URL: &str = "https://snowflake-broker.torproject.net/";

/// Front domains for domain fronting (CDN77) - kept for reference but not used with CORS proxy
pub const BROKER_FRONT_DOMAINS: &[&str] = &["www.cdn77.com", "www.phpmyadmin.net"];

/// Direct broker URL (doesn't work from browsers due to CORS)
pub const BROKER_URL_DIRECT: &str = "https://snowflake-broker.torproject.net/";

/// Client protocol version
const CLIENT_VERSION: &str = "1.0";

/// Default bridge fingerprint (Tor Project's primary Snowflake bridge)
pub const DEFAULT_BRIDGE_FINGERPRINT: &str = "2B280B23E1107BB62ABFC40DDCC8824814F80A72";

/// NAT type for the client
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NatType {
    #[default]
    Unknown,
    Restricted,
    Unrestricted,
}

impl std::fmt::Display for NatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NatType::Unknown => write!(f, "unknown"),
            NatType::Restricted => write!(f, "restricted"),
            NatType::Unrestricted => write!(f, "unrestricted"),
        }
    }
}

/// Client poll request sent to broker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientPollRequest {
    /// WebRTC SDP offer
    pub offer: String,
    /// Client's detected NAT type
    #[serde(default)]
    pub nat: String,
    /// Target bridge fingerprint
    #[serde(default)]
    pub fingerprint: String,
}

impl ClientPollRequest {
    pub fn new(offer: String) -> Self {
        Self {
            offer,
            nat: NatType::Unknown.to_string(),
            fingerprint: DEFAULT_BRIDGE_FINGERPRINT.to_string(),
        }
    }

    pub fn with_nat(mut self, nat: NatType) -> Self {
        self.nat = nat.to_string();
        self
    }

    pub fn with_fingerprint(mut self, fingerprint: String) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    /// Encode request in broker protocol format: "version\n{json}"
    pub fn encode(&self) -> Result<Vec<u8>> {
        let json = serde_json::to_string(self)
            .map_err(|e| TorError::Protocol(format!("Failed to serialize request: {}", e)))?;
        let encoded = format!("{}\n{}", CLIENT_VERSION, json);
        Ok(encoded.into_bytes())
    }
}

/// Client poll response from broker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientPollResponse {
    /// WebRTC SDP answer (empty if error)
    #[serde(default)]
    pub answer: String,
    /// Error message (empty if success)
    #[serde(default)]
    pub error: String,
}

impl ClientPollResponse {
    /// Decode response from broker
    pub fn decode(data: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(data)
            .map_err(|e| TorError::Protocol(format!("Invalid UTF-8 in response: {}", e)))?;
        
        serde_json::from_str(text)
            .map_err(|e| TorError::Protocol(format!("Failed to parse response: {}", e)))
    }

    pub fn is_success(&self) -> bool {
        !self.answer.is_empty() && self.error.is_empty()
    }
}

/// Snowflake broker client
pub struct BrokerClient {
    broker_url: String,
    fingerprint: String,
    nat_type: NatType,
}

impl BrokerClient {
    pub fn new(broker_url: &str) -> Self {
        Self {
            broker_url: broker_url.to_string(),
            fingerprint: DEFAULT_BRIDGE_FINGERPRINT.to_string(),
            nat_type: NatType::Unknown,
        }
    }

    pub fn with_fingerprint(mut self, fingerprint: String) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    pub fn with_nat_type(mut self, nat_type: NatType) -> Self {
        self.nat_type = nat_type;
        self
    }

    /// Exchange SDP offer for SDP answer via broker
    /// Returns the SDP answer from a volunteer proxy
    pub async fn negotiate(&self, sdp_offer: &str) -> Result<String> {
        info!("Contacting Snowflake broker via domain fronting");
        
        let request = ClientPollRequest::new(sdp_offer.to_string())
            .with_nat(self.nat_type)
            .with_fingerprint(self.fingerprint.clone());
        
        let body = request.encode()?;
        
        // Use CORS proxy - simple fetch without domain fronting
        let proxy_url = format!("{}/client", self.broker_url.trim_end_matches('/'));
        
        info!("Using CORS proxy: {}", proxy_url);
        debug!("Sending offer to broker ({} bytes)", body.len());
        
        #[cfg(target_arch = "wasm32")]
        let response_bytes = self.fetch_wasm(&proxy_url, &body).await?;
        
        #[cfg(not(target_arch = "wasm32"))]
        let response_bytes = self.fetch_native(&format!("{}client", self.broker_url.trim_end_matches('/')), &body).await?;
        
        let response = ClientPollResponse::decode(&response_bytes)?;
        
        if !response.error.is_empty() {
            warn!("Broker returned error: {}", response.error);
            return Err(TorError::Network(format!("Broker error: {}", response.error)));
        }
        
        if response.answer.is_empty() {
            return Err(TorError::Network("Broker returned empty answer".to_string()));
        }
        
        info!("Got SDP answer from broker ({} bytes)", response.answer.len());
        Ok(response.answer)
    }

    /// Fetch via CORS proxy
    #[cfg(target_arch = "wasm32")]
    async fn fetch_wasm(&self, url: &str, body: &[u8]) -> Result<Vec<u8>> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::{Request, RequestInit, RequestMode, Response};

        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);
        
        // Convert body to Uint8Array
        let body_array = js_sys::Uint8Array::from(body);
        opts.set_body(&body_array.into());
        
        let request = Request::new_with_str_and_init(url, &opts)
            .map_err(|e| TorError::Network(format!("Failed to create request: {:?}", e)))?;
        
        // Set Content-Type header
        request.headers()
            .set("Content-Type", "application/x-www-form-urlencoded")
            .map_err(|e| TorError::Network(format!("Failed to set Content-Type header: {:?}", e)))?;
        
        let window = web_sys::window()
            .ok_or_else(|| TorError::Internal("No window object".to_string()))?;
        
        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| TorError::Network(format!("Fetch failed: {:?}", e)))?;
        
        let resp: Response = resp_value
            .dyn_into()
            .map_err(|_| TorError::Internal("Response cast failed".to_string()))?;
        
        if !resp.ok() {
            return Err(TorError::Network(format!(
                "Broker returned HTTP {}", 
                resp.status()
            )));
        }
        
        let array_buffer = JsFuture::from(
            resp.array_buffer()
                .map_err(|e| TorError::Network(format!("Failed to get body: {:?}", e)))?
        )
        .await
        .map_err(|e| TorError::Network(format!("Failed to read body: {:?}", e)))?;
        
        let uint8_array = js_sys::Uint8Array::new(&array_buffer);
        Ok(uint8_array.to_vec())
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn fetch_native(&self, url: &str, body: &[u8]) -> Result<Vec<u8>> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;
        use tokio_rustls::TlsConnector;
        use rustls_pki_types::ServerName;

        let parsed = url::Url::parse(url)
            .map_err(|e| TorError::Configuration(format!("Invalid URL: {}", e)))?;
        
        let host = parsed.host_str()
            .ok_or_else(|| TorError::Configuration("URL has no host".to_string()))?;
        let port = parsed.port().unwrap_or(443);
        
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr).await
            .map_err(|e| TorError::Network(format!("Failed to connect to broker: {}", e)))?;
        
        // Setup TLS
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        
        let connector = TlsConnector::from(std::sync::Arc::new(config));
        let server_name = ServerName::try_from(host.to_string())
            .map_err(|_| TorError::Configuration("Invalid server name".to_string()))?;
        
        let mut tls_stream = connector.connect(server_name, stream).await
            .map_err(|e| TorError::Network(format!("TLS handshake failed: {}", e)))?;
        
        // Build HTTP request
        let path = parsed.path();
        let request = format!(
            "POST {} HTTP/1.1\r\n\
             Host: {}\r\n\
             Content-Type: application/x-www-form-urlencoded\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n",
            path, host, body.len()
        );
        
        tls_stream.write_all(request.as_bytes()).await
            .map_err(|e| TorError::Network(format!("Failed to send request: {}", e)))?;
        tls_stream.write_all(body).await
            .map_err(|e| TorError::Network(format!("Failed to send body: {}", e)))?;
        tls_stream.flush().await
            .map_err(|e| TorError::Network(format!("Failed to flush: {}", e)))?;
        
        // Read response
        let mut response = Vec::new();
        tls_stream.read_to_end(&mut response).await
            .map_err(|e| TorError::Network(format!("Failed to read response: {}", e)))?;
        
        // Parse HTTP response (simple parser)
        let response_str = String::from_utf8_lossy(&response);
        
        // Find body after \r\n\r\n
        if let Some(idx) = response_str.find("\r\n\r\n") {
            let body_start = idx + 4;
            if body_start < response.len() {
                return Ok(response[body_start..].to_vec());
            }
        }
        
        Err(TorError::Protocol("Invalid HTTP response from broker".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_encode() {
        let request = ClientPollRequest::new("test-offer".to_string())
            .with_nat(NatType::Unknown);
        
        let encoded = request.encode().unwrap();
        let text = String::from_utf8(encoded).unwrap();
        
        assert!(text.starts_with("1.0\n"));
        assert!(text.contains("\"offer\":\"test-offer\""));
        assert!(text.contains("\"nat\":\"unknown\""));
    }

    #[test]
    fn test_response_decode() {
        let json = r#"{"answer":"test-answer","error":""}"#;
        let response = ClientPollResponse::decode(json.as_bytes()).unwrap();
        
        assert_eq!(response.answer, "test-answer");
        assert!(response.error.is_empty());
        assert!(response.is_success());
    }

    #[test]
    fn test_response_error() {
        let json = r#"{"answer":"","error":"no proxies available"}"#;
        let response = ClientPollResponse::decode(json.as_bytes()).unwrap();
        
        assert!(response.answer.is_empty());
        assert_eq!(response.error, "no proxies available");
        assert!(!response.is_success());
    }
}
