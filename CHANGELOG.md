# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- WASM bindings: `post(url, body)` method for POST requests with raw body bytes
- WASM bindings: `postJson(url, jsonBody)` convenience method for JSON-RPC (auto-sets Content-Type header)
- WASM bindings: `request(method, url, headers, body, timeout)` for full HTTP control
- Native: `TorClient::request()` method for generic HTTP requests with custom method, headers, body, and timeout

## [0.1.0] - 2025-12-10

### Added
- Initial release
- Full Tor protocol support using `tor-proto` crate (ntor-v3, CREATE2, 3-hop circuits)
- Snowflake transport (WebSocket and WebRTC) for WASM
- WebTunnel transport for WASM and native builds
- TLS 1.3/1.2 via SubtleCrypto for WASM (subtle-tls)
- Embedded consensus with daily auto-updates
- Stream isolation (per-domain, per-request, global)
- HTTP client with GET/POST through exit relays
- Security hardening: cargo-deny, cargo-audit, fuzz tests
- Performance benchmarks with Criterion
- Property-based tests with proptest
