# Webtor-rs Project Summary

##  Project Overview

Webtor-rs is a complete Rust implementation of a Tor client designed to be compiled to WebAssembly and embedded in web pages. It provides anonymous HTTP/HTTPS requests through the Tor network using pluggable transports (Snowflake and WebTunnel bridges).

**Key differentiator**: Unlike other browser Tor clients, webtor-rs uses the **official Arti crates** (Rust Tor implementation by the Tor Project) for protocol handling, ensuring security and correctness.

## Project Structure

```
webtor-rs/
├── Cargo.toml                    # Workspace configuration
├── build.sh                      # Build script for WASM compilation
├── README.md                     # User documentation
├── PROJECT_SUMMARY.md            # This file (development roadmap)
├── COMPARISON.md                 # Comparison with echalote
│
├── webtor/                       # Core Tor client library
│   ├── Cargo.toml               # Library dependencies
│   └── src/
│       ├── lib.rs               # Main library exports
│       ├── client.rs            # Main TorClient implementation
│       ├── circuit.rs           # Circuit management
│       ├── config.rs            # Configuration options
│       ├── directory.rs         # Consensus fetching and relay discovery
│       ├── error.rs             # Error types and handling
│       ├── http.rs              # HTTP client through Tor
│       ├── relay.rs             # Relay selection and management
│       ├── time.rs              # WASM-compatible time handling
│       │
│       │   # TLS Support
│       ├── tls.rs               # TLS wrapper for HTTPS
│       │
│       │   # Snowflake Transport (WebRTC-based)
│       ├── snowflake.rs         # Snowflake bridge integration
│       ├── snowflake_broker.rs  # Broker API client for proxy assignment
│       ├── snowflake_ws.rs      # WebSocket fallback (legacy)
│       ├── webrtc_stream.rs     # WebRTC DataChannel stream (WASM)
│       ├── turbo.rs             # Turbo framing protocol
│       ├── kcp_stream.rs        # KCP reliable transport
│       ├── smux.rs              # SMUX multiplexing protocol
│       │
│       │   # WebTunnel Transport (HTTPS-based)
│       ├── webtunnel.rs         # WebTunnel bridge integration
│       │
│       │   # Shared
│       ├── websocket.rs         # WebSocket communication
│       └── wasm_runtime.rs      # WASM async runtime
│
├── subtle-tls/                   # Pure-Rust TLS 1.3 for WASM
│   ├── Cargo.toml               # TLS library dependencies
│   └── src/
│       ├── lib.rs               # TLS exports
│       ├── handshake.rs         # TLS 1.3 handshake
│       ├── record.rs            # TLS record layer
│       ├── crypto.rs            # SubtleCrypto bindings
│       ├── cert.rs              # Certificate verification
│       ├── stream.rs            # TLS stream wrapper
│       └── trust_store.rs       # Root CA certificates
│
├── webtor-wasm/                  # WebAssembly bindings
│   ├── Cargo.toml               # WASM-specific dependencies
│   └── src/lib.rs               # JavaScript API bindings
│
├── webtor-demo/                  # Demo webpage
│   └── static/
│       ├── index.html           # Demo webpage
│       └── pkg/                 # Built WASM package
│
└── vendor/                       # Vendored dependencies (gitignored)
    └── arti/                    # Arti with WASM patches
```

## Architecture

### Protocol Stacks

```
+---------------------------------------------------------------------+
|                        Application Layer                             |
|                    (TorClient, HTTP requests)                        |
+----------------------------+----------------------------------------+
                             |
                             v
+---------------------------------------------------------------------+
|                         Tor Protocol                                 |
|           (tor-proto: Channel, Circuit, Stream)                      |
+----------------------------+----------------------------------------+
                             |
              +--------------+--------------+
              |                             |
              v                             v
+-------------------------+   +-------------------------+
|     Snowflake           |   |      WebTunnel          |
|   (WASM only)           |   |  (WASM + Native)        |
+-------------------------+   +-------------------------+
| WebRTC DataChannel      |   | HTTPS + HTTP Upgrade    |
|         |               |   |         |               |
| Turbo (framing)         |   | TLS (rustls/SubtleCrypto)|
|         |               |   |         |               |
| KCP (reliability)       |   | TCP/WebSocket           |
|         |               |   +-------------------------+
| SMUX (multiplexing)     |
+-------------------------+
```

### Core Components

1. **TorClient** (`client.rs`) - Main entry point
   - Manages circuit lifecycle and HTTP requests
   - Supports both Snowflake (WASM) and WebTunnel (WASM+Native)
   - Handles consensus refresh and relay selection

2. **Circuit Management** (`circuit.rs`)
   - Creates 3-hop circuits through Tor network
   - Uses `tor-proto` for ntor-v3 handshakes and encryption
   - Handles circuit updates with graceful transitions

3. **Directory Manager** (`directory.rs`)
   - Fetches network consensus from bridge (1-hop circuit)
   - Parses with `tor-netdoc` for relay information
   - Fetches microdescriptors for relay details
   - Caches with TTL (1 hour fresh, 3 hours valid)

4. **Snowflake Transport** (`snowflake.rs`, `snowflake_broker.rs`, `webrtc_stream.rs`)
   - **Two modes**: WebSocket (direct) and WebRTC (via volunteer proxies)
   - WebSocket: Direct connection to bridge (simpler, faster)
   - WebRTC: Client → Broker → Volunteer Proxy → Bridge (more censorship resistant)
   - Broker API for SDP offer/answer exchange (JSON-encoded SDPs)
   - Turbo → KCP → SMUX protocol stack

5. **WebTunnel Transport** (`webtunnel.rs`)
   - HTTPS connection with HTTP Upgrade
   - Works through corporate proxies
   - Proper TLS certificate validation

6. **SubtleCrypto TLS** (`subtle-tls/`)
   - Pure-Rust TLS 1.3 implementation for WASM
   - Uses browser's SubtleCrypto for cryptographic operations
   - Proper certificate chain validation

## Completed Features

### Phase 1 - Foundation (Complete)
- [x] Project structure with Cargo workspace
- [x] WASM bindings with wasm-bindgen
- [x] Error handling with custom types
- [x] Configuration system with builder pattern
- [x] WebSocket implementation (WASM + Native)
- [x] Demo webpage

### Phase 2 - Tor Protocol (Complete)
- [x] Arti integration (tor-proto, tor-netdoc, tor-llcrypto)
- [x] Channel establishment with Tor handshake
- [x] Circuit creation (CREATE2 with ntor-v3)
- [x] Circuit extension (EXTEND2 for 3-hop circuits)
- [x] Stream creation (RELAY_BEGIN, DataStream)
- [x] Consensus fetching and parsing
- [x] Microdescriptor fetching
- [x] Relay selection (guard, middle, exit)

### Phase 3 - HTTP/TLS (Complete)
- [x] HTTP request/response through Tor streams
- [x] TLS 1.3 support via SubtleCrypto (WASM)
- [x] TLS support via rustls (Native)
- [x] Proper certificate validation (P-256, P-384 curves)
- [x] Request routing through exit relays

### Phase 4 - Transports (Complete)
- [x] **WebTunnel bridge** - Full implementation
  - [x] HTTPS connection with HTTP Upgrade
  - [x] TLS with SNI support
  - [x] Works on WASM and Native
  
- [x] **Snowflake bridge** - Full implementation
  - [x] Turbo framing protocol (variable-length headers)
  - [x] KCP reliable transport (stream mode, conv=0)
  - [x] SMUX multiplexing (v2, little-endian)
  - [x] WebSocket mode (direct connection to bridge)
  - [x] WebRTC mode (via volunteer proxies, WASM only)
  - [x] Broker API client for proxy assignment
  - [x] Proper signaling flow (JSON-encoded SDP offer/answer)

## In Progress / Planned

### Phase 5 - Optimization (Complete)
- [x] WASM bundle size optimization (0.94 MB gzipped, was 1.30 MB)
- [x] Circuit creation performance improvements
  - [x] Parallel microdescriptor fetching (CHUNK_SIZE=256, MAX_PARALLEL_CHUNKS=3)
  - [x] Circuit reuse via `get_ready_circuit_and_mark_used()`
  - [x] Preemptive circuit creation with `maybe_prebuild_circuit()`
- [x] Connection pooling and reuse (circuits kept alive, MAX_CIRCUITS=2)
- [x] Parallel consensus fetching (microdescriptors fetched in parallel batches)
- [x] Criterion benchmarks for CPU-bound operations
- [x] WebRTC connection retry for unreliable volunteer proxies

### Phase 6 - Advanced Features (Complete)
- [x] TLS 1.2 support with automatic fallback (PR #13)
- [x] Comprehensive E2E test suite (regression tests for all preset URLs)
- [x] Performance benchmarks (Criterion + E2E via Playwright)
- [x] Fuzz testing for TLS parsing (4 targets: certificate, server_hello, handshake, record)

### Phase 7 - Future Enhancements
Open issues for future work:
- [ ] Stream isolation per domain (#21)
- [ ] WASM bundle further optimization (#22)
- [ ] Onion service (.onion) support (#23)
- [ ] Security audit with Rocq formal verification (#25)

## Known Limitations

### TLS Version Support
The WASM TLS implementation (`subtle-tls`) supports both TLS 1.3 and TLS 1.2:
- **TLS 1.3**: Preferred, used by default
- **TLS 1.2**: Automatic fallback when server doesn't support TLS 1.3

Most modern sites work. Some legacy servers may have compatibility issues.

## Current Status

| Component | Status | Notes |
|-----------|--------|-------|
| Core Library | Complete | Full Tor protocol support |
| WebTunnel | Complete | Works on WASM + Native |
| Snowflake (WS) | Complete | Direct WebSocket to bridge |
| Snowflake (WebRTC) | Complete | Via volunteer proxies (WASM) |
| TLS/HTTPS | Complete | TLS 1.3 + 1.2 fallback |
| Consensus | Complete | Fetching + parsing + caching |
| Circuit Creation | Complete | 3-hop circuits with reuse |
| HTTP Client | Complete | GET/POST support |
| WASM Build | Working | 0.94 MB gzipped |
| Demo App | Working | Interactive UI |
| E2E Tests | Complete | Regression + benchmarks |
| Fuzz Testing | Complete | 4 TLS parsing targets |

## Security Features

- **TLS Certificate Validation** - Using webpki-roots + SubtleCrypto
- **TLS 1.3 + 1.2 Support** - Automatic version negotiation
- **ntor-v3 Handshake** - Modern key exchange
- **CREATE2 Circuits** - Current Tor standard
- **Memory Safety** - Rust guarantees
- **Audited Crypto** - ring, dalek crates (native), SubtleCrypto (WASM)
- **Correct Snowflake** - Proper WebRTC architecture via broker
- **Fuzz Testing** - Continuous fuzzing of TLS parsing

## Performance Characteristics

| Metric | Value | Notes |
|--------|-------|-------|
| WASM Bundle | ~2.7 MB uncompressed, ~0.94 MB gzipped | Optimized with wasm-opt |
| Initial Load | 2-5 sec | WASM compilation |
| Consensus Fetch | 5-15 sec | Parallel microdesc fetching |
| Circuit Creation | 20-60 sec | 3-hop with handshakes |
| Request Latency | <3 sec | Circuit reuse enabled |
| Memory Usage | 50-100 MB | Runtime |

### Benchmark Results (Criterion)

| Operation | Time | Notes |
|-----------|------|-------|
| make_circ_params | 35.8 ns | Circuit parameter construction |
| select_guard_relay | 34.2 us | From 1000 relays |
| select_middle_relay | 22.8 us | Fewer constraints |
| select_exit_relay | 35.0 us | Similar to guard |
| X25519 key gen | 1.46 us | Per key |
| ChaCha20-Poly1305 | 181.7 MB/s | 1KB payload |
| SHA-256 | 223 ns | 64 bytes |

**Key insight**: CPU operations are us/ns scale; network latency (20-60s) dominates.

## Comparison with Alternatives

See [COMPARISON.md](COMPARISON.md) for detailed comparison with echalote.

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| Language | Rust -> WASM | TypeScript |
| Tor Protocol | Official Arti | Custom |
| TLS Validation | Yes | No |
| Snowflake | WebRTC (correct) | Direct WS (wrong) |
| WebTunnel | Yes | No |
| Security | Production-grade | Experimental |

## Quick Start

```bash
# Build
./build.sh

# Run demo
cd webtor-demo/static && python3 -m http.server 8000

# Open http://localhost:8000
```

### Rust Usage

```rust
use webtor::{TorClient, TorClientOptions};

// Snowflake (WASM only)
let client = TorClient::new(TorClientOptions::snowflake()).await?;

// WebTunnel (WASM + Native)
let client = TorClient::new(
    TorClientOptions::webtunnel(url, fingerprint)
).await?;

// Bootstrap (fetch consensus)
client.bootstrap().await?;

// Make request
let response = client.get("https://example.com/").await?;
println!("Response: {}", response.text()?);

client.close().await;
```

## Testing

```bash
# Unit tests
cargo test -p webtor

# E2E tests (requires network, slow)
cargo test -p webtor --test e2e -- --ignored --nocapture

# Specific test
cargo test -p webtor --test e2e test_webtunnel_https_request -- --ignored --nocapture
```

## Known Issues & Fixes

### TLS 1.3 Handshake Message Boundary Bug (FIXED)

**Problem**: When TLS handshake messages (Certificate, CertificateVerify) spanned multiple encrypted records, message boundaries got corrupted.

**Root Cause**: Padding removal logic incorrectly stripped legitimate zero bytes from content data.

**Fix**: Simplified decryption to take last byte as content type without padding removal.

### WASM Time Support (FIXED)

**Problem**: Tor channel handshake panicked with "time not implemented on this platform".

**Fix**: Created `PortableInstant` type and `wasm_time` module for WASM-compatible time handling across tor-proto, tor-rtcompat, and related crates.

### TLS ECDSA P-384 Curve Support (FIXED)

**Problem**: HTTPS failed with "imported EC key specifies different curve" for P-384 certificates.

**Fix**: Added curve detection from certificate's SubjectPublicKeyInfo and proper coordinate size handling for P-256/P-384/P-521.

### TLS ALPN Extension (FIXED)

**Problem**: TLS handshake failed with `close_notify` immediately.

**Fix**: Added ALPN extension advertising "http/1.1" in ClientHello.

## Development Notes

### Bridge Sources
- WebTunnel bridges: https://github.com/scriptzteam/Tor-Bridges-Collector/blob/main/bridges-webtunnel
- Snowflake broker: https://snowflake-broker.torproject.net/

### Key Dependencies
- `tor-proto` v0.36.0 - Tor protocol implementation
- `tor-netdoc` v0.36.0 - Consensus parsing
- `rustls` v0.22 - Native TLS implementation
- `kcp` v0.6 - KCP protocol
- `web-sys` - WebRTC/SubtleCrypto bindings

---

**Project Status**: Active Development  
**License**: MIT  
**Repository**: https://github.com/igor53627/webtor-rs
