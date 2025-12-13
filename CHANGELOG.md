# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- UI: Added "Our Crates" version tooltip showing webtor, webtor-wasm, webtor-demo, subtle-tls versions

### Changed
- websocket.rs: Store closures on struct fields instead of Closure::forget() to prevent memory leaks
- websocket.rs: Remove nested Arc<Mutex<>> wrapper for receiver (simpler design)
- websocket.rs: Mark flaky echo.websocket.org test as #[ignore]
- lib.rs: Build-time version extraction for dependencies via build.rs
- trust_store.rs: Refactor find_root_for_issuer to return owned Vec<u8> to support extended roots
- record.rs: Use debug! instead of info! for per-record logging (reduce noise)
- client.rs: Use .cloned() instead of .map(|s| s.clone())
- smux.rs: Clarify partial-write recovery guidance in poll_write comment
- build.rs: Derive Arti version from vendored tor-proto Cargo.toml

### Fixed
- subtle-tls: Avoid panic on truncated ServerHello by checking buffer length before parsing

## [0.4.0] - 2025-12-13

### Changed
- **Breaking**: Upgraded vendored Arti crates from 1.7.0 to 1.8.0 (tor-proto 0.37.0)
- UI: Switched to Dracula Pro color palette
- UI: Added version tooltip showing all vendored Arti crate versions

### Fixed
- WASM: Fixed socket2 dependency not compiling on wasm32 target
- WASM: Fixed tor-basic-utils ENOTDIR check for non-unix/windows platforms
- WASM: Fixed tor-memquota 8*GIB overflow on 32-bit platforms
- WASM: Fixed std::time::Instant panic by using web-time crate throughout tor-rtcompat/tor-proto

## [0.3.0] - 2025-12-13

### Added
- Arti 1.8.0 compatibility with updated tracing-subscriber (RUSTSEC-2025-0055 fix)
- Documented congestion control parameters in circuit.rs

### Changed
- Circuit padding: silently drop unexpected padding cells instead of erroring
  (receive-only tolerance for future Tor network compatibility)

### Fixed
- E2E tests: mark external service tests (HTTPBin, Llama RPC) as optional to avoid flaky CI failures

## [0.2.2] - 2025-12-11

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
