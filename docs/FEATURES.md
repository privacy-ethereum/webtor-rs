# webtor-rs Features

## Transport Layer

### Snowflake Bridge
Two connection modes for different use cases:

#### WebSocket Mode (Direct)
- Direct WebSocket connection to Snowflake bridge
- Simpler setup, faster connection
- Good for most use cases

#### WebRTC Mode (Volunteer Proxies)
- Connects via Snowflake broker to volunteer proxies
- More censorship resistant (traffic routed through volunteers)
- Uses JSON-encoded SDP offer/answer for signaling
- Automatic ICE candidate gathering

### Shared Protocol Stack
```
WebSocket / WebRTC DataChannel
    |
Turbo (framing + obfuscation)
    |
KCP (reliability + ordering)
    |
SMUX (stream multiplexing)
    |
TLS 1.3 (link encryption)
    |
Tor Protocol
```

## Tor Protocol

### 3-Hop Circuits
- **Guard**: First relay, protects your IP
- **Middle**: Intermediate relay, adds anonymity
- **Exit**: Final relay, connects to destination

### Circuit Management
- Automatic circuit creation and rotation
- Configurable refresh intervals
- Graceful circuit transitions
- Circuit reuse for persistent connections
- Isolated circuits for enhanced privacy

## Cryptography

### TLS 1.3
- WebCrypto API (SubtleCrypto) for browser compatibility
- ECDH key exchange (P-256, P-384, P-521)
- AES-GCM encryption
- SHA-256/SHA-384 hashing

### Tor Encryption
- ntor-v3 handshake (modern key exchange)
- CREATE2 cells (current Tor standard)
- Relay encryption with AES-CTR

## Network Features

### HTTP Client
- GET/POST requests through Tor
- HTTPS support (TLS 1.3 only)
- Proper certificate validation (webpki-roots)
- Automatic content decompression

### Consensus
- Automatic consensus fetching from directory authorities
- Embedded consensus for fast startup
- Consensus diff support for bandwidth efficiency
- Relay selection with bandwidth weights

## Browser Compatibility

### WASM Support
- Compiled to WebAssembly for browser execution
- No native dependencies
- Works in modern browsers (Chrome, Firefox, Safari, Edge)

### API
- Simple JavaScript API
- Promise-based async operations
- Event callbacks for status updates
- Logging with configurable verbosity

## Limitations

### TLS 1.3 Only
Sites requiring TLS 1.2 will not work. Most modern sites support TLS 1.3.

**Working**: example.com, google.com, cloudflare.com, github.com
**Not working**: Sites with TLS 1.2 only

### WebRTC Required
Snowflake transport requires WebRTC support in the browser.

## Performance

| Metric | Typical Value |
|--------|---------------|
| WASM Bundle | ~3-4 MB |
| Initial Load | 2-5 seconds |
| Circuit Creation | 20-60 seconds |
| Request Latency | 1-5 seconds |

## Security

- Memory-safe Rust implementation
- Official Arti crates for protocol handling
- Certificate validation for HTTPS
- No direct IP exposure to destinations
