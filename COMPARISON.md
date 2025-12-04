# webtor-rs vs echalote: A Technical Comparison

This document compares **webtor-rs** (Rust) and **echalote** (TypeScript) - two browser-based Tor client implementations.

## Executive Summary

| Aspect | webtor-rs | echalote |
|--------|-----------|----------|
| **Language** | Rust → WASM | TypeScript + WASM modules |
| **Tor Protocol** | Uses battle-tested `tor-proto` crate | Custom implementation |
| **Security** | Production-grade TLS validation | No No TLS certificate validation |
| **Transport** | WebTunnel + Snowflake (WebSocket/WebRTC) | Snowflake (WebSocket) + Meek |
| **Maturity** | Built on Arti (official Rust Tor) | Experimental, early-stage |
| **TLS** | TLS 1.3 via SubtleCrypto | TLS 1.2 via @hazae41/cadenas |

## Architecture Comparison

### Protocol Stack

**webtor-rs**
```
+------------------------------------------------------------------+
|  TorClient                                                       |
|    +-- tor-proto (official Arti crate)                           |
|    |     +-- Channel (Tor handshake, cell processing)            |
|    |     +-- Circuit (CREATE2, EXTEND2, ntor-v3)                 |
|    |     +-- Stream (RELAY cells, flow control)                  |
|    +-- subtle-tls (TLS 1.3 with SubtleCrypto + cert validation)  |
|    +-- Transport Layer                                           |
|          +-- WebTunnel (HTTPS + HTTP Upgrade)                    |
|          +-- Snowflake (WebSocket -> Turbo -> KCP -> SMUX)       |
+------------------------------------------------------------------+
```

**echalote**
```
+------------------------------------------------------------------+
|  TorClientDuplex                                                 |
|    +-- Custom Tor implementation                                 |
|    |     +-- Handshake (VERSION, CERTS, NETINFO)                 |
|    |     +-- Circuit (CREATE_FAST, EXTEND2, ntor)                |
|    |     +-- Stream (RELAY cells, SENDME flow control)           |
|    +-- @hazae41/cadenas (TLS - NO cert validation)               |
|    +-- Transport Layer                                           |
|          +-- Snowflake (WebSocket -> Turbo -> KCP -> SMUX)       |
|          +-- Meek (HTTP batched transport)                       |
+------------------------------------------------------------------+
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
| **Certificate Validation** | Yes Full validation with webpki-roots | No None (unsafe) |
| **TLS Version** | TLS 1.3 | TLS 1.2 only |
| **Implementation** | subtle-tls (SubtleCrypto) | @hazae41/cadenas |
| **MITM Protection** | Yes Yes | No No |
| **Curve Support** | P-256, P-384, P-521 | Unknown |

**Why webtor-rs is better:**
```rust
// webtor-rs: Proper TLS validation via SubtleCrypto
async fn verify_certificate_chain(&self, certs: &[Certificate]) -> Result<()> {
    // Extracts public key, verifies signature chain
    // Uses browser's trusted root store
    let result = subtle_crypto.verify(algorithm, public_key, signature, data).await?;
}
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
| **Connection Methods** | WebSocket OR WebRTC | Direct WebSocket only |
| **WebRTC Support** | Yes (via broker) | No |
| **Protocol Stack** | WebRTC/WebSocket -> Turbo -> KCP -> SMUX | WebSocket -> Turbo -> KCP -> SMUX |

**webtor-rs supports both connection methods:**
- **WebSocket**: Direct connection to bridge (simpler, faster setup)
- **WebRTC**: Via broker + volunteer proxies (more censorship resistant)

The WebRTC option routes traffic through volunteer browser extensions, making it harder to block.

#### WebTunnel Support

| Feature | webtor-rs | echalote |
|---------|-----------|----------|
| **WebTunnel Bridge** | Yes Full support | No Not implemented |
| **HTTPS Upgrade** | Yes RFC 9298 compliant | N/A |
| **TLS SNI** | Yes Configurable | N/A |

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
let kcp = Kcp::new_stream(config.conv, output);  // Yes Stream mode
```

### 5. Cryptography

| Algorithm | webtor-rs | echalote |
|-----------|-----------|----------|
| **AES-128-CTR** | SubtleCrypto (WASM) / ring (native) | @hazae41/aes.wasm |
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
| **Memory Safety** | Yes Guaranteed (no GC) | Depends on JS runtime |
| **Error Propagation** | `Result<T, E>` types | Exceptions/Promises |
| **Null Safety** | Yes `Option<T>` | Nullable types |

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
| **Microdescriptors** | Yes Full support | Unknown |
| **Relay Flags** | Full flag support | Basic flags |
| **Exit Policy** | Yes Parsed and enforced | Limited |
| **Bandwidth Weights** | Yes Supported | Not mentioned |
| **Caching** | Yes With expiration (1hr) | Unknown |

```rust
// webtor-rs: Proper consensus handling via DirectoryManager
pub async fn fetch_and_process_consensus(&self, channel: Arc<Channel>) -> Result<()> {
    let consensus_body = self.fetch_consensus_body(channel.clone()).await?;
    let consensus = MdConsensus::parse(&consensus_body)?;
    
    // Fetch microdescriptors for relay details
    let microdescs = self.fetch_microdescriptors_body(channel, &digests).await?;
    
    // Update relay manager
    self.relay_manager.write().await.update_relays(relays);
}
```

### 8. API Design

#### webtor-rs: Simple, High-Level API

```rust
// One-line setup
let client = TorClient::new(TorClientOptions::snowflake()).await?;

// Bootstrap (fetch consensus)
client.bootstrap().await?;

// Make requests
let response = client.get("https://example.com/").await?;
println!("Response: {}", response.text()?);

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
| **Unit Tests** | Yes Comprehensive | Limited |
| **E2E Tests** | Yes Real network tests | Example app |
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

### webtor-rs Yes
- Yes TLS 1.3 certificate validation via SubtleCrypto
- Yes Uses official Tor protocol crate
- Yes Modern ntor-v3 handshake
- Yes CREATE2 circuit creation
- Yes Proper Snowflake WebSocket architecture
- Yes Memory-safe Rust code
- Yes Audited crypto libraries

### echalote Warning
- No **No TLS certificate validation** (MITM vulnerable)
- No Custom Tor implementation (not audited)
- No Legacy CREATE_FAST (less secure)
- No Direct WebSocket to bridge (incorrect architecture)
- Warning Experimental, early-stage
- Warning TypeScript security depends on runtime

## Conclusion

**webtor-rs** is the better choice for production use because:

1. **Security**: Proper TLS validation and audited crypto
2. **Correctness**: Uses official Tor protocol implementation
3. **Architecture**: WebTunnel + Snowflake transports
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

// Bootstrap and make request
client.bootstrap().await?;
let response = client.get("https://example.com").await?;
```

### Bridge Options

| Bridge | WASM | Native | Censorship Resistance |
|--------|------|--------|----------------------|
| Snowflake (WebSocket) | Yes | No | Medium (direct) |
| Snowflake (WebRTC) | Yes | No | High (volunteer proxies) |
| WebTunnel | Yes | Yes | Medium (HTTPS) |
