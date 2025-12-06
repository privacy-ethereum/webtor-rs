# webtor-rs

A Rust Tor client for WebAssembly. Provides anonymous HTTP/HTTPS through Tor using Snowflake (WebRTC) and WebTunnel bridges.

```
                             ░██           ░██                        
                             ░██           ░██                        
░██    ░██    ░██  ░███████  ░████████  ░████████  ░███████  ░██░████ 
░██    ░██    ░██ ░██    ░██ ░██    ░██    ░██    ░██    ░██ ░███     
 ░██  ░████  ░██  ░█████████ ░██    ░██    ░██    ░██    ░██ ░██      
  ░██░██ ░██░██   ░██        ░███   ░██    ░██    ░██    ░██ ░██      
   ░███   ░███     ░███████  ░██░█████      ░████  ░███████  ░██      
```

## Features

- **Arti-based** - Uses official Tor Project crates with full TLS validation
- **Two Transports** - Snowflake (WebRTC) and WebTunnel (HTTPS)
- **Full Tor Protocol** - 3-hop circuits with ntor-v3 handshakes
- **TLS 1.3 Support** - Pure-Rust TLS via SubtleCrypto (WASM)
- **Stream Isolation** - Separate circuits per domain (Tor Browser-style)
- **Circuit Reuse** - Persistent circuits for performance
- **Consensus Fetching** - Automatic relay discovery with caching

## Quick Start

```bash
./build.sh
cd webtor-demo/static && python3 -m http.server 8000
# Open http://localhost:8000
```

## Usage

```rust
use webtor::{TorClient, TorClientOptions};

// Snowflake (WASM only - uses WebRTC)
let client = TorClient::new(TorClientOptions::snowflake()).await?;

// WebTunnel (WASM + Native)  
let client = TorClient::new(
    TorClientOptions::webtunnel(url, fingerprint)
).await?;

// Configure stream isolation (default: PerDomain)
let client = TorClient::new(
    TorClientOptions::snowflake()
        .with_stream_isolation(StreamIsolationPolicy::PerDomain)
).await?;

// Bootstrap and make requests
client.bootstrap().await?;
let response = client.get("https://example.com/").await?;
println!("{}", response.text()?);

client.close().await;
```

## Architecture

```mermaid
flowchart TB
    subgraph App["Application Layer"]
        TC[TorClient]
        HTTP[HTTP Requests]
    end
    
    subgraph TorProto["Tor Protocol (tor-proto)"]
        Channel[Channel]
        Circuit[Circuit]
        Stream[Stream]
    end
    
    subgraph Transports["Transport Layer"]
        subgraph Snowflake["Snowflake (WASM only)"]
            WebRTC[WebRTC DataChannel]
            Turbo[Turbo - framing]
            KCP[KCP - reliability]
            SMUX[SMUX - multiplexing]
        end
        
        subgraph WebTunnel["WebTunnel (WASM + Native)"]
            HTTPS[HTTPS + HTTP Upgrade]
            TLS[TLS - SubtleCrypto]
            TCP[TCP/WebSocket]
        end
    end
    
    subgraph Network["Network"]
        Broker[Snowflake Broker]
        Proxies[Volunteer Proxies]
        Bridge[Tor Bridge]
        Relays[Tor Relays]
        Exit[Exit Relay]
    end
    
    TC --> HTTP
    HTTP --> Channel
    Channel --> Circuit
    Circuit --> Stream
    
    Stream --> WebRTC
    Stream --> HTTPS
    
    WebRTC --> Turbo
    Turbo --> KCP
    KCP --> SMUX
    SMUX --> Broker
    Broker --> Proxies
    Proxies --> Bridge
    
    HTTPS --> TLS
    TLS --> TCP
    TCP --> Bridge
    
    Bridge --> Relays
    Relays --> Exit
```

## Transports

| Transport | WASM | Native | Notes |
|-----------|------|--------|-------|
| Snowflake (WebSocket) | Yes | No | Direct connection to bridge (simpler) |
| Snowflake (WebRTC) | Yes | No | Via volunteer proxies (more censorship resistant) |
| WebTunnel | Yes | Yes | HTTPS, works through corporate proxies |

## Status

| Component | Status | Notes |
|-----------|--------|-------|
| Tor Protocol | Complete | ntor-v3, CREATE2, 3-hop circuits |
| Snowflake (WS) | Complete | Direct WebSocket + Turbo/KCP/SMUX |
| Snowflake (WebRTC) | Complete | Broker API + volunteer proxies |
| WebTunnel | Complete | HTTPS Upgrade with TLS validation |
| Consensus | Complete | Fetching + parsing + 1hr caching |
| HTTP Client | Complete | GET/POST through exit relays |
| TLS (WASM) | Complete | TLS 1.3 via SubtleCrypto, TLS 1.2 experimental |
| Demo App | Working | Interactive browser UI |

## Limitations

- **TLS 1.2 Experimental** - TLS 1.2 support is new and may have issues
- **Onion Services** - `.onion` addresses not yet implemented  
- **Mobile** - Not optimized for mobile browsers

## Roadmap

- [x] Tor protocol (Arti integration)
- [x] HTTP/HTTPS through Tor
- [x] Snowflake WebSocket (direct bridge connection)
- [x] Snowflake WebRTC (volunteer proxies via broker)
- [x] WebTunnel (HTTPS Upgrade)
- [x] TLS 1.3 support (SubtleCrypto)
- [x] Consensus fetching and caching
- [x] TLS 1.2 support (experimental)
- [x] Stream isolation per domain
- [ ] Onion service support
- [ ] Performance optimizations
- [ ] Security audit

## Documentation

- [PROJECT_SUMMARY.md](PROJECT_SUMMARY.md) - Detailed roadmap and architecture
- [COMPARISON.md](COMPARISON.md) - Comparison with echalote

## Testing

```bash
# Unit tests
cargo test -p webtor

# Property-based tests (proptest)
cargo test -p webtor proptest

# E2E tests (requires network)
npm run test:tls

# Benchmarks (requires network)
cargo bench -p webtor --bench tor_benchmark

# Criterion microbenchmarks (deterministic)
cargo bench -p webtor --bench circuit_params

# Fuzz tests (manual trigger via GitHub Actions)
cd subtle-tls/fuzz && cargo +nightly fuzz run fuzz_server_hello
```

## CI/CD

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| Tests | Push/PR | Unit tests, E2E, WASM build, Clippy, Rustfmt |
| Property Tests | Push/PR | Proptest for turbo, smux, relay modules |
| Fuzz Testing | Manual | Fuzz testing for TLS parsing |
| WASM Build | Push/PR | Build and size check |
| Daily Consensus | Scheduled | Update cached consensus data |
| Deploy Example | Push to main | Deploy demo to GitHub Pages |

## License

MIT
