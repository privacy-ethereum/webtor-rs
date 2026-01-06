# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.7] - 2026-01-06

### Added
- CI: WASM releases now auto-upload to Cloudflare R2 CDN (https://webtor-wasm.53627.org)
- Docs: Added R2 CDN documentation to README and AGENTS.md

### Changed
- Docs: Added CDN usage examples to README

## [0.5.6] - 2026-01-06

### Added
- TLS: Implement RFC 8446 keying material export for Tor channel binding
- TLS: Add `exporter_master_secret` derivation during handshake
- TLS: Add synchronous HKDF using hmac/sha2 crates (CertifiedConn requires sync)

### Removed
- Dead code: Remove unused `webtor-wasm/src/websocket.rs` (313 lines)
- Dead code: Remove `AnyStream` trait, `connected` field, `parse_response` method
- Dead code: Remove unused `tls_connector` field from `TorHttpClient`

### Changed
- Build: Add `#[cfg(target_arch = "wasm32")]` to WASM-only items in directory.rs

## [0.5.5] - 2026-01-06

### Changed
- TLS: Remove `Rc<RefCell<>>` from `TlsStream` - use direct ownership for async safety
- TLS: Replace `write_buf.clone()` with `std::mem::take` for zero-allocation writes
- TLS: Add `WriteZero` error handling to prevent infinite loops
- WASM: Replace `.unwrap()` with proper error handling in websocket and HTTP code
- Build: Pin nightly toolchain to `2026-01-05` for reproducible builds

### Fixed
- Clippy: Use `io::Error::other()`, `.is_multiple_of()`, `.div_ceil()`
- Clippy: Remove redundant closures in error mapping

## [0.5.4] - 2026-01-06

### Fixed
- HTTP: Decode `Transfer-Encoding: chunked` responses properly (fixes #64)
- HTTP: `TorResponse.text()` no longer includes raw chunk framing bytes
- HTTP: Per HTTP/1.1 semantics, `Transfer-Encoding` takes precedence over `Content-Length`

## [0.5.3] - 2025-12-21

### Added
- Error handling: Added `TorErrorKind` enum for error classification (Network, Timeout, Bootstrap, Circuit, Configuration, Environment, Protocol, Internal, Cancelled)
- Error handling: Added `kind()`, `is_retryable()`, and `code()` methods to `TorError`
- Error handling: Added `TorError::Cancelled` variant for user-initiated aborts
- WASM: Added `JsTorError` struct for structured error reporting (code, kind, message, retryable)
- Retry: Created `retry.rs` with `RetryPolicy` and `retry_with_backoff()` for transient failure handling
- Retry: Added `with_timeout()` helper for consistent timeout handling
- Retry: Added `CancellationToken` for cross-platform cooperative task cancellation
- Retry: Added `with_cancellation()` and `with_timeout_and_cancellation()` helpers
- Client: Added `abort()` method to cancel all in-flight operations
- Client: Added `is_aborted()` and `shutdown_token()` methods
- WASM: Exposed `abort()` and `isAborted()` in JavaScript bindings

### Changed
- Client: `establish_channel()`, `update_circuit()`, and `wait_for_circuit()` now support cancellation
- Client: `close()` now triggers cancellation before cleanup

## [0.5.2] - 2025-12-21

### Added
- WASM: Fail-fast CSPRNG check on init - validates crypto.getRandomValues availability before any crypto operations
- WASM: init() now returns Result and throws clear error if secure randomness unavailable

### Security
- Prevents use of weak/predictable randomness for Tor circuit keys in unsupported environments

## [0.5.1] - 2025-12-13

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
- WASM: Fix coarsetime::Instant panic by using web_time::Instant on WASM (coarse_time.rs)

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
