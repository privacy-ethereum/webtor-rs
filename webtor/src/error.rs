//! Error types for the webtor library

use thiserror::Error;

pub type Result<T> = std::result::Result<T, TorError>;

#[derive(Error, Debug)]
pub enum TorError {
    #[error("WebSocket connection failed: {0}")]
    WebSocketConnection(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Tor protocol error: {0}")]
    TorProtocol(String),

    #[error("Circuit creation failed: {0}")]
    CircuitCreation(String),

    #[error("Circuit extension failed: {0}")]
    CircuitExtension(String),

    #[error("Relay selection failed: {0}")]
    RelaySelection(String),

    #[error("Consensus fetch failed: {0}")]
    ConsensusFetch(String),

    #[error("HTTP request failed: {0}")]
    HttpRequest(String),

    #[error("TLS setup failed: {0}")]
    TlsSetup(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("WASM error: {0}")]
    Wasm(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("NetDoc error: {0}")]
    NetDoc(#[from] tor_netdoc::Error),
}

impl TorError {
    pub fn websocket_connection(msg: impl Into<String>) -> Self {
        TorError::WebSocketConnection(msg.into())
    }

    pub fn websocket(msg: impl Into<String>) -> Self {
        TorError::WebSocket(msg.into())
    }

    pub fn tor_protocol(msg: impl Into<String>) -> Self {
        TorError::TorProtocol(msg.into())
    }

    pub fn circuit_creation(msg: impl Into<String>) -> Self {
        TorError::CircuitCreation(msg.into())
    }

    pub fn circuit_extension(msg: impl Into<String>) -> Self {
        TorError::CircuitExtension(msg.into())
    }

    pub fn relay_selection(msg: impl Into<String>) -> Self {
        TorError::RelaySelection(msg.into())
    }

    pub fn consensus_fetch(msg: impl Into<String>) -> Self {
        TorError::ConsensusFetch(msg.into())
    }

    pub fn http_request(msg: impl Into<String>) -> Self {
        TorError::HttpRequest(msg.into())
    }

    pub fn tls_setup(msg: impl Into<String>) -> Self {
        TorError::TlsSetup(msg.into())
    }

    pub fn tls(msg: impl Into<String>) -> Self {
        TorError::TlsSetup(msg.into())
    }

    pub fn timeout(msg: impl Into<String>) -> Self {
        TorError::Timeout(msg.into())
    }

    pub fn configuration(msg: impl Into<String>) -> Self {
        TorError::Configuration(msg.into())
    }

    pub fn network(msg: impl Into<String>) -> Self {
        TorError::Network(msg.into())
    }

    pub fn wasm(msg: impl Into<String>) -> Self {
        TorError::Wasm(msg.into())
    }

    pub fn serialization(msg: impl Into<String>) -> Self {
        TorError::Serialization(msg.into())
    }
}
