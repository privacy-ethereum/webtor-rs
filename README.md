# 🧅 Webtor-rs

A Rust implementation of a Tor client that compiles to WebAssembly and can be embedded in web pages. This project provides anonymous HTTP/HTTPS requests through the Tor network using Snowflake bridges.

## 🚀 Features

- **🔄 Persistent Circuits**: Reuses Tor circuits for better performance
- **🔃 Automatic Updates**: Configurable circuit refresh with graceful transitions  
- **🌨️ Snowflake Support**: Uses Snowflake bridge to enable access over WebSockets
- **🔒 Isolated Requests**: One-time circuits for maximum privacy
- **⚡ Lazy Scheduling**: Smart updates only when circuits are actively used
- **📊 Status Monitoring**: Real-time circuit status information
- **🦀 Rust + WASM**: Memory-safe implementation compiled to WebAssembly
- **🌐 Browser Native**: No external dependencies, runs entirely in the browser

## 📦 Architecture

The project is organized as a Rust workspace with three main components:

```
webtor-rs/
├── webtor/           # Core Tor client library
├── webtor-wasm/      # WebAssembly bindings
├── webtor-demo/      # Demo webpage
└── build.sh          # Build script
```

### Core Library (`webtor/`)

The main Tor client implementation including:

- **Circuit Management**: Handles Tor circuit creation, extension, and lifecycle
- **Relay Selection**: Smart selection of guard, middle, and exit relays
- **HTTP Client**: HTTP/HTTPS requests through Tor circuits with TLS support
- **Snowflake Integration**: WebSocket-based bridge communication
- **Error Handling**: Comprehensive error types and recovery mechanisms

### WASM Bindings (`webtor-wasm/`)

WebAssembly bindings that expose the Rust functionality to JavaScript:

- **JavaScript API**: Promise-based async interface
- **TypeScript Definitions**: Full type safety for JavaScript consumers
- **WebSocket Implementation**: Browser-native WebSocket handling
- **Memory Management**: Safe handling of WASM memory

### Demo Application (`webtor-demo/`)

A complete demonstration webpage showing all features:

- **Interactive UI**: Real-time circuit status and request testing
- **Multiple Request Types**: Persistent and isolated request examples
- **Comprehensive Logging**: Detailed operation tracking
- **Responsive Design**: Works on desktop and mobile browsers

## 🛠️ Building

### Prerequisites

1. **Rust**: Install from [rustup.rs](https://rustup.rs/)
2. **wasm-pack**: Install with `curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh`
3. **Node.js** (optional): For development server

### Build Instructions

```bash
# Clone the repository
git clone https://github.com/voltrevo/webtor-rs.git
cd webtor-rs

# Build the project
./build.sh

# Or build manually:
cd webtor-wasm && wasm-pack build --target web && cd ..
cd webtor-demo && wasm-pack build --target web && cd ..
```

## 🚀 Usage

### Basic Usage (JavaScript)

```javascript
import { TorClient, TorClientOptions } from './pkg/webtor_wasm.js';

// Create a Tor client
const options = new TorClientOptions('wss://snowflake.torproject.net/');
const tor = await TorClient.new(options);

// Make a request through Tor
const response = await tor.fetch('https://httpbin.org/ip');
const text = await response.text();
console.log('Your Tor IP:', text);

// Close when done
tor.close();
```

### One-time Requests (Maximum Privacy)

```javascript
import { TorClient } from './pkg/webtor_wasm.js';

// Make a single isolated request
const response = await TorClient.fetchOneTime(
    'wss://snowflake.torproject.net/',
    'https://httpbin.org/ip'
);

const data = await response.json();
console.log('Anonymous IP:', data.origin);
```

### Advanced Configuration

```javascript
const options = new TorClientOptions('wss://snowflake.torproject.net/')
    .withConnectionTimeout(15000)      // 15 second connection timeout
    .withCircuitTimeout(90000)         // 90 second circuit creation timeout
    .withCreateCircuitEarly(true)      // Create circuit immediately
    .withCircuitUpdateInterval(600000) // Auto-update every 10 minutes
    .withCircuitUpdateAdvance(60000);  // 1 minute advance notice

const tor = await TorClient.new(options);
```

## 🌐 Running the Demo

After building the project:

```bash
cd webtor-demo/static
python3 -m http.server 8000
# Or: npx serve -s .
```

Then open http://localhost:8000 in your browser.

The demo includes:

- **Configuration Panel**: Snowflake bridge URL settings
- **Circuit Status**: Real-time monitoring of Tor circuits
- **Persistent Requests**: Multiple concurrent requests using the same circuit
- **Isolated Requests**: One-time circuits for maximum privacy
- **Activity Log**: Detailed operation tracking
- **Performance Metrics**: Connection times and throughput

## 🔧 API Reference

### TorClientOptions

Configuration options for the Tor client:

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `snowflakeUrl` | string | required | Snowflake bridge WebSocket URL |
| `connectionTimeout` | number | 15000 | WebSocket connection timeout (ms) |
| `circuitTimeout` | number | 90000 | Circuit creation timeout (ms) |
| `createCircuitEarly` | boolean | true | Create circuit immediately |
| `circuitUpdateInterval` | number? | 600000 | Auto-update interval (ms), null to disable |
| `circuitUpdateAdvance` | number | 60000 | Update advance time (ms) |

### TorClient Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `fetch(url)` | Promise&lt;Response&gt; | Make HTTP request through Tor |
| `updateCircuit(deadline?)` | Promise&lt;void&gt; | Update circuit with graceful transition |
| `waitForCircuit()` | Promise&lt;void&gt; | Wait for circuit to be ready |
| `getCircuitStatus()` | Promise&lt;CircuitStatus&gt; | Get current circuit status |
| `getCircuitStatusString()` | Promise&lt;string&gt; | Get human-readable status |
| `close()` | Promise&lt;void&gt; | Close client and cleanup |

### Static Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `fetchOneTime(snowflakeUrl, url, options?)` | Promise&lt;Response&gt; | Make one-time isolated request |

## 🧪 Testing

Run the test suite:

```bash
# Run all tests
cargo test --workspace

# Run specific component tests
cargo test -p webtor
cargo test -p webtor-wasm

# Run with output
cargo test --workspace -- --nocapture
```

Note: Some tests may fail in native environments since they require WASM/browser APIs.

## 🔍 Debugging

Enable detailed logging in the browser:

```javascript
// Enable console logging
console.log = function(...args) {
    // Your custom logging logic
};

// The demo includes comprehensive logging by default
```

## 📊 Performance

Expected performance characteristics:

- **Initial Connection**: 20-60 seconds (first time, includes WASM loading)
- **Subsequent Requests**: 1-5 seconds (circuit reuse)
- **Circuit Updates**: 10-30 seconds (background operation)
- **Memory Usage**: ~50-100MB (WASM + circuit state)

## 🔒 Security Considerations

- **Experimental Software**: This implementation is experimental and should not be used for critical anonymity needs
- **Browser Fingerprinting**: WebAssembly and JavaScript execution may be fingerprintable
- **Network Timing**: Request timing patterns may be observable
- **DNS Leaks**: All DNS requests go through Tor circuits
- **TLS Validation**: Proper certificate validation is performed for HTTPS requests

## 🚧 Current Limitations

- **Tor Protocol**: Integration with `arti` (official Rust Tor implementation) is in progress. Channel establishment works, but full circuit construction is still being implemented.
- **Relay Discovery**: Consensus fetching and relay selection needs improvement.
- **Performance**: Initial connection times are longer than native Tor.
- **Browser Support**: Requires modern browsers with WebAssembly support.

## 🗺️ Roadmap

### Phase 1 (Current)
- [x] Basic project structure and WASM bindings
- [x] WebSocket communication framework
- [x] Circuit management architecture
- [x] Demo webpage with basic functionality
- [ ] Complete Tor protocol implementation
- [ ] Relay consensus fetching
- [ ] HTTP client through Tor circuits

### Phase 2
- [ ] TLS support for HTTPS requests
- [ ] Advanced relay selection algorithms
- [ ] Performance optimizations
- [ ] Comprehensive error handling
- [ ] Browser compatibility improvements

### Phase 3
- [ ] Stream isolation per domain
- [ ] Advanced circuit path selection
- [ ] Bandwidth management
- [ ] Mobile browser optimizations
- [ ] Security audits

## 🤝 Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

### Development Setup

```bash
# Install development dependencies
cargo install wasm-pack
cargo install cargo-watch

# Run development server with auto-rebuild
cargo watch -s "./build.sh"

# Run tests in watch mode
cargo watch -x "test --workspace"
```

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- **Tor Project**: For the Tor network and protocol specifications
- **Rust WASM Working Group**: For excellent WASM tooling
- **Original TypeScript Implementation**: Based on concepts from tor-hazae41
- **Snowflake Project**: For WebSocket-based bridge technology

## 📚 References

- [Tor Protocol Specifications](https://gitweb.torproject.org/torspec.git/)
- [Rust WASM Book](https://rustwasm.github.io/docs.html)
- [WebAssembly](https://webassembly.org/)
- [Snowflake Bridge](https://snowflake.torproject.org/)

---

**⚠️ Disclaimer**: This software is experimental and should not be relied upon for strong anonymity. Use at your own risk.