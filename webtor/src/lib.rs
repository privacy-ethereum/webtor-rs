//! Webtor - A Tor client library for web browsers
//! 
//! This library provides a Rust implementation of a Tor client that can be
//! compiled to WebAssembly and embedded in web pages. It supports anonymous
//! HTTP/HTTPS requests through the Tor network using Snowflake bridges.

pub mod client;
pub mod circuit;
pub mod config;
pub mod directory;
pub mod error;
pub mod http;
pub mod relay;
pub mod snowflake;
pub mod websocket;
pub mod wasm_runtime;

pub use client::TorClient;
pub use config::TorClientOptions;
pub use error::{TorError, Result};

// Re-export commonly used types
pub use url::Url;
pub use http::HttpResponse;