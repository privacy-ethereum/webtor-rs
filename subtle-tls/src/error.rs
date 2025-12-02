//! Error types for subtle-tls

use thiserror::Error;

pub type Result<T> = std::result::Result<T, TlsError>;

#[derive(Error, Debug)]
pub enum TlsError {
    #[error("Handshake failed: {0}")]
    Handshake(String),

    #[error("Certificate error: {0}")]
    Certificate(String),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Record layer error: {0}")]
    Record(String),

    #[error("Alert received: {0}")]
    Alert(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unexpected message: expected {expected}, got {got}")]
    UnexpectedMessage { expected: String, got: String },

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("SubtleCrypto error: {0}")]
    SubtleCrypto(String),
}

impl TlsError {
    pub fn handshake(msg: impl Into<String>) -> Self {
        TlsError::Handshake(msg.into())
    }

    pub fn certificate(msg: impl Into<String>) -> Self {
        TlsError::Certificate(msg.into())
    }

    pub fn crypto(msg: impl Into<String>) -> Self {
        TlsError::Crypto(msg.into())
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        TlsError::Protocol(msg.into())
    }

    pub fn record(msg: impl Into<String>) -> Self {
        TlsError::Record(msg.into())
    }

    pub fn alert(msg: impl Into<String>) -> Self {
        TlsError::Alert(msg.into())
    }

    pub fn subtle_crypto(msg: impl Into<String>) -> Self {
        TlsError::SubtleCrypto(msg.into())
    }
}
