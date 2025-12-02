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
pub mod kcp_stream;
pub mod relay;
pub mod smux;
pub mod snowflake;
pub mod snowflake_ws;
pub mod time;
pub mod snowflake_broker;
pub mod tls;
pub mod turbo;
pub mod wasm_runtime;
pub mod websocket;

#[cfg(not(target_arch = "wasm32"))]
pub mod webtunnel;

#[cfg(target_arch = "wasm32")]
pub mod webrtc_stream;

pub use client::TorClient;
pub use config::TorClientOptions;
pub use error::{TorError, Result};

// Re-export commonly used types
pub use url::Url;
pub use http::HttpResponse;

// Re-export Tor stream types for advanced usage
pub use tor_proto::client::stream::DataStream;