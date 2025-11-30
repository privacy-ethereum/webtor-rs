# webtor-rs

A Rust Tor client for WebAssembly. Provides anonymous HTTP/HTTPS through Tor using Snowflake (WebRTC) and WebTunnel bridges.

                             ░██           ░██                        
                             ░██           ░██                        
░██    ░██    ░██  ░███████  ░████████  ░████████  ░███████  ░██░████ 
░██    ░██    ░██ ░██    ░██ ░██    ░██    ░██    ░██    ░██ ░███     
 ░██  ░████  ░██  ░█████████ ░██    ░██    ░██    ░██    ░██ ░██      
  ░██░██ ░██░██   ░██        ░███   ░██    ░██    ░██    ░██ ░██      
   ░███   ░███     ░███████  ░██░█████      ░████  ░███████  ░██      
                                                                    

## Features

-  **Arti-based** - Uses official Arti crates with TLS validation
-  **Two Transports** - Snowflake (WebRTC) and WebTunnel (HTTPS)
-  **Circuit Reuse** - Persistent circuits for performance
-  **Rust + WASM** - Memory-safe, runs in browser

## Quick Start

```bash
./build.sh
cd webtor-demo/static && python3 -m http.server 8000
# Open http://localhost:8000
```

## Usage

```rust
use webtor::{TorClient, TorClientOptions};

// Snowflake (WASM only)
let client = TorClient::new(TorClientOptions::snowflake()).await?;

// WebTunnel (WASM + Native)
let client = TorClient::new(
    TorClientOptions::webtunnel(url, fingerprint)
).await?;

let response = client.get("https://check.torproject.org/").await?;
client.close().await;
```

## Transports

| Transport | WASM | Native | Notes |
|-----------|------|--------|-------|
| Snowflake | ✅ | ❌ | WebRTC via volunteer proxies |
| WebTunnel | ✅ | ✅ | HTTPS, works through corporate proxies |

## Roadmap

- [x] Tor protocol (Arti integration)
- [x] HTTP/HTTPS through Tor  
- [x] Snowflake (WebRTC + Broker API)
- [x] WebTunnel (HTTPS Upgrade)
- [ ] Performance optimizations
- [ ] Security audit

See [PROJECT_SUMMARY.md](PROJECT_SUMMARY.md) for detailed roadmap and [COMPARISON.md](COMPARISON.md) for comparison with echalote.

## License

MIT
