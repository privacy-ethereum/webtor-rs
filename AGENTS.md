# Agents Guide

## Build Commands

```bash
# Check native build
cargo check --package webtor

# Check WASM build
cargo check --package webtor --target wasm32-unknown-unknown
cargo check --package webtor-wasm --target wasm32-unknown-unknown

# Build WASM demo (includes embedded consensus)
wasm-pack build webtor-demo --target web --out-dir pkg --release

# Run example locally (alternative, requires npm)
cd example && npm install && npm run dev
```

## Project Structure

- `webtor/` - Core Tor client library
- `webtor-wasm/` - WASM bindings for webtor
- `webtor-demo/` - Demo application library
- `example/` - Web demo (Vite + TypeScript)
- `subtle-tls/` - SubtleCrypto-based TLS for WASM
- `scripts/` - Build and consensus fetch scripts

## Key Files

- `webtor/src/client.rs` - Main TorClient implementation
- `webtor/src/circuit.rs` - Circuit management
- `webtor/src/directory.rs` - Consensus fetching and parsing
- `webtor/src/relay.rs` - Relay selection
- `webtor/src/cached/` - Embedded consensus data (updated daily)

## Architecture Notes

- For WASM builds, consensus is embedded at compile time from `webtor/src/cached/`
- The daily GitHub Action (`daily-consensus-update.yml`) refreshes cached consensus
- Snowflake bridge is used for WASM; WebTunnel is available for native builds
- `ring` crate doesn't compile to WASM, so `subtle-tls` provides TLS via SubtleCrypto
