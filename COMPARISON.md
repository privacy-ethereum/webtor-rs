# webtor-rs vs echalote: A Technical Comparison

This document compares **webtor-rs** (Rust) and **echalote** (TypeScript) - two browser-based Tor client implementations.

## Executive Summary

| Aspect | webtor-rs | echalote |
|--------|-----------|----------|
| **Language** | Rust → WASM | TypeScript + WASM modules |
| **Tor Protocol** | Uses battle-tested `tor-proto` crate | Custom implementation |
| **Security** | Production-grade TLS validation | ❌ No TLS certificate validation |
| **Transport** | WebTunnel + Snowflake (WebRTC) | Snowflake (WebSocket) + Meek |
| **Maturity** | Built on Arti (official Rust Tor) | Experimental, early-stage |
| **Performance** | Native WASM, zero-copy where possible | Zero-copy TypeScript |

## Architecture Comparison

### Protocol Stack

```
┌─────────────────────────────────────────────────────────────────────┐
│                           webtor-rs                                  │
├─────────────────────────────────────────────────────────────────────┤
│  TorClient                                                           │
│    ├── tor-proto (official Arti crate)                              │
│    │     ├── Channel (Tor handshake, cell processing)               │
│    │     ├── Circuit (CREATE2, EXTEND2, ntor-v3)                    │
│    │     └── Stream (RELAY cells, flow control)                     │
│    ├── futures-rustls (TLS 1.2/1.3 with cert validation)            │
│    └── Transport Layer                                               │
│          ├── WebTunnel (HTTPS + HTTP Upgrade)                       │
│          └── Snowflake (WebRTC → Turbo → KCP → SMUX)                │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                           echalote                                   │
├─────────────────────────────────────────────────────────────────────┤
│  TorClientDuplex                                                     │
│    ├── Custom Tor implementation                                     │
│    │     ├── Handshake (VERSION, CERTS, NETINFO)                    │
│    │     ├── Circuit (CREATE_FAST, EXTEND2, ntor)                   │
│    │     └── Stream (RELAY cells, SENDME flow control)              │
│    ├── @hazae41/cadenas (TLS - NO cert validation ⚠️)               │
│    └── Transport Layer                                               │
│          ├── Snowflake (WebSocket → Turbo → KCP → SMUX)             │
│          └── Meek (HTTP batched transport)                          │
└─────────────────────────────────────────────────────────────────────┘
```

## Detailed Comparison

### 1. Tor Protocol Implementation

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **Codebase** | Uses `tor-proto` from Arti project | Custom TypeScript implementation |
| **Protocol Version** | Full v3-v5 negotiation | v5 only |
| **Handshake** | ntor-v3 (latest) | ntor (older) |
| **Circuit Creation** | CREATE2 (modern) | CREATE_FAST (legacy, less secure) |
| **Cell Format** | Variable-length (modern) | Both fixed and variable |
| **Flow Control** | Built-in SENDME handling | Manual SENDME implementation |

**Why webtor-rs is better:**
- Uses the **official Rust Tor implementation** maintained by the Tor Project
- **ntor-v3** provides better forward secrecy than ntor
- **CREATE2** is the modern circuit creation method; CREATE_FAST is deprecated
- Battle-tested code with security audits

### 2. TLS Security

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **Certificate Validation** | ✅ Full validation with webpki-roots | ❌ None (unsafe) |
| **TLS Version** | TLS 1.2/1.3 | TLS 1.2 only |
| **Implementation** | rustls (memory-safe) | @hazae41/cadenas |
| **MITM Protection** | ✅ Yes | ❌ No |

**Why webtor-rs is better:**
```rust
// webtor-rs: Proper TLS validation
let mut root_store = rustls::RootCertStore::empty();
root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
let config = rustls::ClientConfig::builder()
    .with_root_certificates(root_store)  // ✅ Validates certs
    .with_no_client_auth();
```

```typescript
// echalote: NO validation (from their docs)
// "TLS connection to guard relay doesn't validate certs"
// "Vulnerable to MITM if guard relay is compromised"
```

### 3. Transport Layer

#### Snowflake Implementation

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **Connection Method** | WebRTC via broker | Direct WebSocket |
| **Broker Support** | ✅ Full broker API | ❌ Hardcoded endpoints |
| **Protocol Stack** | WebRTC → Turbo → KCP → SMUX | WebSocket → Turbo → KCP → SMUX |
| **Correct Architecture** | ✅ Yes | ❌ No (bypasses volunteer proxies) |

**Why webtor-rs is better:**

webtor-rs implements the **correct Snowflake architecture**:
```
Client ←(WebRTC)→ Volunteer Proxy ←(WebSocket)→ Bridge
```

echalote connects **directly** to the bridge WebSocket, which:
- Bypasses the volunteer proxy network
- May not work reliably (server expects proxy-formatted data)
- Loses the censorship-resistance benefit of Snowflake

```rust
// webtor-rs: Correct WebRTC flow
pub async fn connect(broker_url: &str, fingerprint: &str) -> Result<Self> {
    // 1. Create RTCPeerConnection
    let pc = RtcPeerConnection::new_with_configuration(&config)?;
    let dc = pc.create_data_channel(DATA_CHANNEL_LABEL);
    
    // 2. Exchange SDP via broker
    let offer_sdp = create_and_gather_offer(&pc).await?;
    let broker = BrokerClient::new(broker_url);
    let answer_sdp = broker.negotiate(&offer_sdp).await?;  // ✅ Proper signaling
    
    // 3. Complete WebRTC handshake
    pc.set_remote_description(&answer_init).await?;
}
```

#### WebTunnel Support

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **WebTunnel Bridge** | ✅ Full support | ❌ Not implemented |
| **HTTPS Upgrade** | ✅ RFC 9298 compliant | N/A |
| **TLS SNI** | ✅ Configurable | N/A |

**webtor-rs WebTunnel** provides an alternative transport that:
- Works through corporate proxies
- Looks like normal HTTPS traffic
- Uses standard TLS with proper validation

### 4. Turbo/KCP/SMUX Protocols

Both implementations have similar Turbo/KCP/SMUX stacks, but with key differences:

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **Turbo Token** | `0x12936...` (correct) | `0x12936...` (correct) |
| **KCP Mode** | Stream mode (ordered) | Datagram mode |
| **KCP Settings** | Match Go client (conv=0, nc=true) | Custom settings |
| **SMUX Version** | v2 (little-endian) | v2 (little-endian) |
| **Async I/O** | futures `AsyncRead`/`AsyncWrite` | Custom duplex streams |

```rust
// webtor-rs: Matches official Snowflake Go client settings
let kcp_config = KcpConfig {
    conv: 0,           // Snowflake uses conv=0
    nodelay: false,    // Match Go: SetNoDelay(0, 0, 0, 1)
    interval: 100,     // Default KCP interval
    nc: true,          // Disable congestion control
    ..Default::default()
};
let kcp = Kcp::new_stream(config.conv, output);  // ✅ Stream mode
```

### 5. Cryptography

| Algorithm | webtor-rs | echalote |
|-----------|-----------|----------|
| **AES-128-CTR** | `ring` crate (audited) | @hazae41/aes.wasm |
| **SHA-1** | `sha1` crate | @hazae41/sha1 |
| **X25519** | `x25519-dalek` | @hazae41/x25519 |
| **Ed25519** | `ed25519-dalek` | @hazae41/ed25519 |
| **RSA** | `rsa` crate | @hazae41/rsa.wasm |
| **Random** | `getrandom` (secure) | Web Crypto API |

**Why webtor-rs is better:**
- Uses **audited cryptographic crates** from the Rust ecosystem
- `ring` and `dalek` are industry-standard implementations
- Memory-safe by default (no buffer overflows)

### 6. Error Handling & Safety

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **Type Safety** | Rust's strict type system | TypeScript (weaker) |
| **Memory Safety** | ✅ Guaranteed (no GC) | Depends on JS runtime |
| **Error Propagation** | `Result<T, E>` types | Exceptions/Promises |
| **Null Safety** | ✅ `Option<T>` | Nullable types |

```rust
// webtor-rs: Explicit error handling
pub async fn connect(&self) -> Result<SnowflakeStream> {
    let ws = WebSocketStream::connect(&url).await?;  // Propagates errors
    let mut turbo = TurboStream::new(ws);
    turbo.initialize().await?;  // Each step can fail safely
    // ...
}
```

### 7. Consensus & Relay Selection

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **Consensus Parsing** | `tor-netdoc` crate | Custom parser |
| **Relay Flags** | Full flag support | Basic flags |
| **Exit Policy** | ✅ Parsed and enforced | Limited |
| **Bandwidth Weights** | ✅ Supported | Not mentioned |
| **Caching** | ✅ With expiration | Unknown |

```rust
// webtor-rs: Proper consensus handling
pub async fn get_relays(&self) -> Result<Vec<RelayInfo>> {
    let consensus_text = self.fetch_consensus().await?;
    let consensus = MicrodescConsensus::parse(&consensus_text)?;
    
    for relay in consensus.relays() {
        if relay.flags().contains(RelayFlags::GUARD) { /* ... */ }
        if relay.flags().contains(RelayFlags::EXIT) { /* ... */ }
    }
}
```

### 8. API Design

#### webtor-rs: Simple, High-Level API

```rust
// One-line setup
let client = TorClient::new(TorClientOptions::snowflake()).await?;

// Make requests
let response = client.get("https://check.torproject.org/").await?;
println!("IP: {}", response.text()?);

// Cleanup
client.close().await;
```

#### echalote: Lower-Level API

```typescript
// Multiple setup steps
const tcp = await createWebSocketSnowflakeStream(url);
const tor = new TorClientDuplex(tcp, { fallbacks, ed25519, x25519, sha1 });
await tor.waitOrThrow();

const circuit = await tor.createOrThrow();
await circuit.extendOrThrow(middleRelay);
await circuit.extendOrThrow(exitRelay);

const stream = await circuit.openOrThrow("example.com", 443);
const tls = new TlsClientDuplex({ host_name: "example.com" });
// Manual stream piping...
```

### 9. Testing & Quality

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **Unit Tests** | ✅ Comprehensive | Limited |
| **E2E Tests** | ✅ Real network tests | Example app |
| **CI/CD** | GitHub Actions | Unknown |
| **Documentation** | Doc comments + README | README + examples |

## Performance Comparison

| Metric | webtor-rs | echalote |
|--------|-----------|----------|
| **WASM Size** | ~2-3 MB (optimized) | Smaller (TS + small WASM) |
| **Memory Usage** | Lower (no GC pressure) | Higher (JS heap) |
| **Startup Time** | Fast (compiled) | Fast (interpreted) |
| **Throughput** | Higher (native code) | Good (JS optimized) |

## Security Summary

### webtor-rs ✅
- ✅ TLS certificate validation
- ✅ Uses official Tor protocol crate
- ✅ Modern ntor-v3 handshake
- ✅ CREATE2 circuit creation
- ✅ Proper Snowflake WebRTC architecture
- ✅ Memory-safe Rust code
- ✅ Audited crypto libraries

### echalote ⚠️
- ❌ **No TLS certificate validation** (MITM vulnerable)
- ❌ Custom Tor implementation (not audited)
- ❌ Legacy CREATE_FAST (less secure)
- ❌ Direct WebSocket to bridge (incorrect architecture)
- ⚠️ Experimental, early-stage
- ⚠️ TypeScript security depends on runtime

## Conclusion

**webtor-rs** is the better choice for production use because:

1. **Security**: Proper TLS validation and audited crypto
2. **Correctness**: Uses official Tor protocol implementation
3. **Architecture**: Correct Snowflake WebRTC flow
4. **Flexibility**: Supports both WebTunnel and Snowflake
5. **Maintainability**: Built on maintained Arti crates
6. **Safety**: Rust's memory safety guarantees

**echalote** is suitable for:
- Learning/experimentation
- Non-security-critical applications
- Quick prototyping

---

## Quick Reference

### Install webtor-rs

```rust
// Cargo.toml
[dependencies]
webtor = "0.1"
```

### Basic Usage

```rust
use webtor::{TorClient, TorClientOptions};

// Snowflake (WASM only)
let client = TorClient::new(TorClientOptions::snowflake()).await?;

// WebTunnel (WASM + Native)
let client = TorClient::new(
    TorClientOptions::webtunnel(url, fingerprint)
).await?;

// Make request
let response = client.get("https://example.com").await?;
```

### Bridge Options

| Bridge | WASM | Native | Censorship Resistance |
|--------|------|--------|----------------------|
| Snowflake | ✅ | ❌ | High (WebRTC P2P) |
| WebTunnel | ✅ | ✅ | Medium (HTTPS) |
