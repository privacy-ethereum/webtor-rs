# Webtor-rs Project Summary

## 🎯 Project Overview

Webtor-rs is a complete Rust implementation of a Tor client designed to be compiled to WebAssembly and embedded in web pages. It provides anonymous HTTP/HTTPS requests through the Tor network using pluggable transports (Snowflake and WebTunnel bridges).

**Key differentiator**: Unlike other browser Tor clients, webtor-rs uses the **official Arti crates** (Rust Tor implementation by the Tor Project) for protocol handling, ensuring security and correctness.

## 📁 Project Structure

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
│       ├── consensus.rs         # Consensus fetching and caching
│       ├── error.rs             # Error types and handling
│       ├── http.rs              # HTTP client through Tor
│       ├── relay.rs             # Relay selection and management
│       ├── tls.rs               # TLS/HTTPS support
│       │
│       │   # Snowflake Transport (WebRTC-based)
│       ├── snowflake.rs         # Snowflake bridge integration
│       ├── snowflake_broker.rs  # Broker API client for proxy assignment
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
├── webtor-wasm/                  # WebAssembly bindings
│   ├── Cargo.toml               # WASM-specific dependencies
│   └── src/lib.rs               # JavaScript API bindings
│
├── webtor-demo/                  # Demo webpage
│   └── static/index.html        # Demo webpage
│
└── vendor/                       # Vendored dependencies
    └── arti/                    # Arti (official Rust Tor) with patches
```

## 🏗️ Architecture

### Protocol Stacks

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Application Layer                             │
│                    (TorClient, HTTP requests)                        │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         Tor Protocol                                 │
│           (tor-proto: Channel, Circuit, Stream)                      │
└────────────────────────────┬────────────────────────────────────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
              ▼                             ▼
┌─────────────────────────┐   ┌─────────────────────────┐
│     Snowflake           │   │      WebTunnel          │
│   (WASM only)           │   │  (WASM + Native)        │
├─────────────────────────┤   ├─────────────────────────┤
│ WebRTC DataChannel      │   │ HTTPS + HTTP Upgrade    │
│         ↓               │   │         ↓               │
│ Turbo (framing)         │   │ TLS (rustls)            │
│         ↓               │   │         ↓               │
│ KCP (reliability)       │   │ TCP/WebSocket           │
│         ↓               │   └─────────────────────────┘
│ SMUX (multiplexing)     │
└─────────────────────────┘
```

### Core Components

1. **TorClient** (`client.rs`) - Main entry point
   - Manages circuit lifecycle and HTTP requests
   - Supports both Snowflake (WASM) and WebTunnel (WASM+Native)
   - Handles consensus refresh and relay selection

2. **Circuit Management** (`circuit.rs`)
   - Creates 3-hop circuits through Tor network
   - Uses `tor-proto` for ntor handshakes and encryption
   - Handles circuit updates with graceful transitions

3. **Consensus Manager** (`consensus.rs`)
   - Fetches network consensus from directory authorities
   - Parses with `tor-netdoc` for relay information
   - Caches with TTL (1 hour fresh, 3 hours valid)

4. **Snowflake Transport** (`snowflake.rs`, `snowflake_broker.rs`, `webrtc_stream.rs`)
   - **Correct WebRTC architecture**: Client → Broker → Volunteer Proxy → Bridge
   - Broker API for SDP offer/answer exchange
   - WebRTC DataChannel for reliable transport
   - Turbo → KCP → SMUX protocol stack

5. **WebTunnel Transport** (`webtunnel.rs`)
   - HTTPS connection with HTTP Upgrade
   - Works through corporate proxies
   - Proper TLS certificate validation

## ✅ Completed Features

### Phase 1 - Foundation ✅
- [x] Project structure with Cargo workspace
- [x] WASM bindings with wasm-bindgen
- [x] Error handling with custom types
- [x] Configuration system with builder pattern
- [x] WebSocket implementation (WASM + Native)
- [x] Demo webpage

### Phase 2 - Tor Protocol ✅
- [x] Arti integration (tor-proto, tor-netdoc, tor-llcrypto)
- [x] Channel establishment with Tor handshake
- [x] Circuit creation (CREATE2 with ntor-v3)
- [x] Circuit extension (EXTEND2 for 3-hop circuits)
- [x] Stream creation (RELAY_BEGIN, DataStream)
- [x] Consensus fetching and parsing
- [x] Relay selection (guard, middle, exit)

### Phase 3 - HTTP/TLS ✅
- [x] HTTP request/response through Tor streams
- [x] TLS/HTTPS support (rustls + futures-rustls)
- [x] Proper certificate validation
- [x] Request routing through exit relays

### Phase 4 - Transports ✅
- [x] **WebTunnel bridge** - Full implementation
  - [x] HTTPS connection with HTTP Upgrade
  - [x] TLS with SNI support
  - [x] Works on WASM and Native
  
- [x] **Snowflake bridge** - Full implementation
  - [x] Turbo framing protocol (variable-length headers)
  - [x] KCP reliable transport (stream mode, conv=0)
  - [x] SMUX multiplexing (v2, little-endian)
  - [x] WebRTC DataChannel (WASM only)
  - [x] Broker API client for proxy assignment
  - [x] Proper signaling flow (SDP offer/answer)

## 🚧 In Progress / Planned

### Phase 5 - Optimization
- [ ] WASM bundle size optimization
- [ ] Circuit creation performance improvements
- [ ] Connection pooling and reuse
- [ ] Parallel consensus fetching

### Phase 6 - Advanced Features
- [ ] Stream isolation per domain
- [ ] Advanced relay selection (bandwidth weights)
- [ ] Circuit preemptive rotation
- [ ] Onion service (.onion) support

### Phase 7 - Production Readiness
- [ ] Security audit
- [ ] Comprehensive test suite
- [ ] Performance benchmarks
- [ ] Documentation improvements
- [ ] Mobile browser optimizations

## 📊 Current Status

| Component | Status | Notes |
|-----------|--------|-------|
| Core Library | ✅ Complete | Full Tor protocol support |
| WebTunnel | ✅ Complete | Works on WASM + Native |
| Snowflake | ✅ Complete | WASM only (WebRTC) |
| TLS/HTTPS | ✅ Complete | rustls with cert validation |
| Consensus | ✅ Complete | 1-hour caching |
| Circuit Creation | ✅ Complete | 3-hop circuits |
| HTTP Client | ✅ Complete | GET/POST support |
| WASM Build | ✅ Working | ~2-3 MB bundle |
| Demo App | ✅ Working | Interactive UI |

## 🔒 Security Features

- ✅ **TLS Certificate Validation** - Using webpki-roots
- ✅ **ntor-v3 Handshake** - Modern key exchange
- ✅ **CREATE2 Circuits** - Current Tor standard
- ✅ **Memory Safety** - Rust guarantees
- ✅ **Audited Crypto** - ring, dalek crates
- ✅ **Correct Snowflake** - Proper WebRTC architecture

## 📈 Performance Characteristics

| Metric | Value | Notes |
|--------|-------|-------|
| WASM Bundle | ~2-3 MB | Compressed |
| Initial Load | 2-5 sec | WASM compilation |
| Consensus Fetch | 5-15 sec | First time only |
| Circuit Creation | 20-60 sec | 3-hop with handshakes |
| Request Latency | 1-5 sec | Circuit reuse |
| Memory Usage | 50-100 MB | Runtime |

## 🆚 Comparison with Alternatives

See [COMPARISON.md](COMPARISON.md) for detailed comparison with echalote.

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| Language | Rust → WASM | TypeScript |
| Tor Protocol | Official Arti | Custom |
| TLS Validation | ✅ Yes | ❌ No |
| Snowflake | ✅ WebRTC | ❌ Direct WS |
| WebTunnel | ✅ Yes | ❌ No |
| Security | Production-grade | Experimental |

## 🚀 Quick Start

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

// Make request
let response = client.get("https://check.torproject.org/").await?;
println!("Response: {}", response.text()?);

client.close().await;
```

## 🧪 Testing

```bash
# Unit tests
cargo test -p webtor

# E2E tests (requires network, slow)
cargo test -p webtor --test e2e -- --ignored --nocapture

# Specific test
cargo test -p webtor --test e2e test_webtunnel_https_request -- --ignored --nocapture
```

## 📝 Development Notes

### Bridge Sources
- WebTunnel bridges: https://github.com/scriptzteam/Tor-Bridges-Collector/blob/main/bridges-webtunnel
- Snowflake broker: https://snowflake-broker.torproject.net/

### Key Dependencies
- `tor-proto` v0.36.0 - Tor protocol implementation
- `tor-netdoc` v0.36.0 - Consensus parsing
- `rustls` v0.23 - TLS implementation
- `kcp` v0.6 - KCP protocol
- `web-sys` - WebRTC bindings

---

**Project Status**: Active Development  
**License**: MIT  
**Repository**: https://github.com/igor53627/webtor-rs
