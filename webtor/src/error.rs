//! Error types for the webtor library

use thiserror::Error;

pub type Result<T> = std::result::Result<T, TorError>;

/// Classification of error types for UX and retry decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TorErrorKind {
    /// Network connectivity issues (WebSocket, connection failures)
    Network,
    /// Operation timed out
    Timeout,
    /// Tor bootstrap/consensus issues
    Bootstrap,
    /// Circuit creation/extension failures
    Circuit,
    /// Configuration errors (bad fingerprint, invalid options)
    Configuration,
    /// Environment issues (missing CSPRNG, WASM limitations)
    Environment,
    /// Protocol errors (TLS, Tor protocol violations)
    Protocol,
    /// Internal bugs or unexpected states
    Internal,
    /// User-initiated cancellation
    Cancelled,
}

impl TorErrorKind {
    /// Returns a stable string code for JS consumption
    pub fn as_code(&self) -> &'static str {
        match self {
            TorErrorKind::Network => "NETWORK",
            TorErrorKind::Timeout => "TIMEOUT",
            TorErrorKind::Bootstrap => "BOOTSTRAP",
            TorErrorKind::Circuit => "CIRCUIT",
            TorErrorKind::Configuration => "CONFIGURATION",
            TorErrorKind::Environment => "ENVIRONMENT",
            TorErrorKind::Protocol => "PROTOCOL",
            TorErrorKind::Internal => "INTERNAL",
            TorErrorKind::Cancelled => "CANCELLED",
        }
    }
}

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

    #[error("Operation cancelled")]
    Cancelled,
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

    /// Returns the error kind for classification
    pub fn kind(&self) -> TorErrorKind {
        match self {
            TorError::WebSocketConnection(_) => TorErrorKind::Network,
            TorError::WebSocket(_) => TorErrorKind::Network,
            TorError::Network(_) => TorErrorKind::Network,
            TorError::Timeout(_) => TorErrorKind::Timeout,
            TorError::CircuitCreation(_) => TorErrorKind::Circuit,
            TorError::CircuitExtension(_) => TorErrorKind::Circuit,
            TorError::RelaySelection(_) => TorErrorKind::Circuit,
            TorError::ConsensusFetch(_) => TorErrorKind::Bootstrap,
            TorError::TorProtocol(_) => TorErrorKind::Protocol,
            TorError::TlsSetup(_) => TorErrorKind::Protocol,
            TorError::Protocol(_) => TorErrorKind::Protocol,
            TorError::HttpRequest(_) => TorErrorKind::Network,
            TorError::Configuration(_) => TorErrorKind::Configuration,
            TorError::Wasm(_) => TorErrorKind::Environment,
            TorError::Serialization(_) => TorErrorKind::Internal,
            TorError::Io(_) => TorErrorKind::Network,
            TorError::UrlParse(_) => TorErrorKind::Configuration,
            TorError::Json(_) => TorErrorKind::Internal,
            TorError::Internal(_) => TorErrorKind::Internal,
            TorError::NetDoc(_) => TorErrorKind::Bootstrap,
            TorError::Cancelled => TorErrorKind::Cancelled,
        }
    }

    /// Returns true if this error is likely transient and the operation could succeed on retry
    pub fn is_retryable(&self) -> bool {
        match self {
            // Network issues are typically transient
            TorError::WebSocketConnection(_) => true,
            TorError::WebSocket(_) => true,
            TorError::Network(_) => true,
            TorError::Io(_) => true,

            // Timeouts are retryable (might succeed with more time or less load)
            TorError::Timeout(_) => true,

            // Circuit failures can be retried with different relays
            TorError::CircuitCreation(_) => true,
            TorError::CircuitExtension(_) => true,

            // HTTP request failures through Tor are often transient
            TorError::HttpRequest(_) => true,

            // Bootstrap/consensus issues might resolve (network conditions change)
            TorError::ConsensusFetch(_) => true,

            // Relay selection might work with different criteria or updated consensus
            TorError::RelaySelection(_) => true,

            // Protocol errors are usually not retryable (indicates a bug or incompatibility)
            TorError::TorProtocol(_) => false,
            TorError::TlsSetup(_) => false,
            TorError::Protocol(_) => false,

            // Configuration errors require user action
            TorError::Configuration(_) => false,
            TorError::UrlParse(_) => false,

            // Environment issues require environment changes
            TorError::Wasm(_) => false,

            // Internal errors are bugs, not transient
            TorError::Serialization(_) => false,
            TorError::Json(_) => false,
            TorError::Internal(_) => false,
            TorError::NetDoc(_) => false,

            // Cancellation is user-initiated, not retryable
            TorError::Cancelled => false,
        }
    }

    /// Returns a stable error code string for JS consumption
    pub fn code(&self) -> &'static str {
        match self {
            TorError::WebSocketConnection(_) => "WEBSOCKET_CONNECTION",
            TorError::WebSocket(_) => "WEBSOCKET",
            TorError::TorProtocol(_) => "TOR_PROTOCOL",
            TorError::CircuitCreation(_) => "CIRCUIT_CREATION",
            TorError::CircuitExtension(_) => "CIRCUIT_EXTENSION",
            TorError::RelaySelection(_) => "RELAY_SELECTION",
            TorError::ConsensusFetch(_) => "CONSENSUS_FETCH",
            TorError::HttpRequest(_) => "HTTP_REQUEST",
            TorError::TlsSetup(_) => "TLS_SETUP",
            TorError::Timeout(_) => "TIMEOUT",
            TorError::Configuration(_) => "CONFIGURATION",
            TorError::Network(_) => "NETWORK",
            TorError::Protocol(_) => "PROTOCOL",
            TorError::Wasm(_) => "WASM_ENVIRONMENT",
            TorError::Serialization(_) => "SERIALIZATION",
            TorError::Io(_) => "IO",
            TorError::UrlParse(_) => "URL_PARSE",
            TorError::Json(_) => "JSON",
            TorError::Internal(_) => "INTERNAL",
            TorError::NetDoc(_) => "NETDOC",
            TorError::Cancelled => "CANCELLED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kinds_and_retryability_are_stable() {
        let cases: Vec<(TorError, TorErrorKind, &str, bool)> = vec![
            (
                TorError::websocket_connection("x"),
                TorErrorKind::Network,
                "WEBSOCKET_CONNECTION",
                true,
            ),
            (
                TorError::websocket("x"),
                TorErrorKind::Network,
                "WEBSOCKET",
                true,
            ),
            (
                TorError::network("x"),
                TorErrorKind::Network,
                "NETWORK",
                true,
            ),
            (
                TorError::timeout("x"),
                TorErrorKind::Timeout,
                "TIMEOUT",
                true,
            ),
            (
                TorError::circuit_creation("x"),
                TorErrorKind::Circuit,
                "CIRCUIT_CREATION",
                true,
            ),
            (
                TorError::circuit_extension("x"),
                TorErrorKind::Circuit,
                "CIRCUIT_EXTENSION",
                true,
            ),
            (
                TorError::relay_selection("x"),
                TorErrorKind::Circuit,
                "RELAY_SELECTION",
                true,
            ),
            (
                TorError::consensus_fetch("x"),
                TorErrorKind::Bootstrap,
                "CONSENSUS_FETCH",
                true,
            ),
            (
                TorError::http_request("x"),
                TorErrorKind::Network,
                "HTTP_REQUEST",
                true,
            ),
            (
                TorError::tor_protocol("x"),
                TorErrorKind::Protocol,
                "TOR_PROTOCOL",
                false,
            ),
            (
                TorError::tls_setup("x"),
                TorErrorKind::Protocol,
                "TLS_SETUP",
                false,
            ),
            (
                TorError::configuration("x"),
                TorErrorKind::Configuration,
                "CONFIGURATION",
                false,
            ),
            (
                TorError::wasm("x"),
                TorErrorKind::Environment,
                "WASM_ENVIRONMENT",
                false,
            ),
            (
                TorError::serialization("x"),
                TorErrorKind::Internal,
                "SERIALIZATION",
                false,
            ),
            (
                TorError::Internal("x".into()),
                TorErrorKind::Internal,
                "INTERNAL",
                false,
            ),
            (TorError::Cancelled, TorErrorKind::Cancelled, "CANCELLED", false),
        ];

        for (err, expected_kind, expected_code, expected_retryable) in cases {
            assert_eq!(
                err.kind(),
                expected_kind,
                "kind mismatch for {:?}",
                err
            );
            assert_eq!(err.code(), expected_code, "code mismatch for {:?}", err);
            assert_eq!(
                err.is_retryable(),
                expected_retryable,
                "retryable mismatch for {:?}",
                err
            );
        }
    }

    #[test]
    fn error_kind_codes_are_uppercase() {
        let kinds = [
            TorErrorKind::Network,
            TorErrorKind::Timeout,
            TorErrorKind::Bootstrap,
            TorErrorKind::Circuit,
            TorErrorKind::Configuration,
            TorErrorKind::Environment,
            TorErrorKind::Protocol,
            TorErrorKind::Internal,
            TorErrorKind::Cancelled,
        ];

        for kind in kinds {
            let code = kind.as_code();
            assert_eq!(
                code,
                code.to_uppercase(),
                "kind code should be uppercase: {}",
                code
            );
        }
    }
}
