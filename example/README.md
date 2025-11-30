# Webtor HTTP Test Example

A simple React app using Chakra UI to test HTTP requests through Tor via WebTunnel.

## Quick Start

```bash
# Build the WASM module and install dependencies
./build.sh

# Start the development server
npm run dev
```

## Features

- Connect to Tor network via WebTunnel pluggable transport
- Fetch your Tor exit IP from api64.ipify.org
- Real-time connection logs
- Dark theme UI with Chakra

## Configuration

Default WebTunnel bridge:
- URL: `https://dev-test.catcat.pictures/Fj7VnJi1mjigUKBb2rJ1TeWJ`
- Fingerprint: `6F6C8E0069880AC253704507119464CE4BD1BA65`

## Notes

- First connection takes 20-60 seconds (consensus fetch)
- Subsequent requests use cached consensus (~1-2s)
- WebTunnel works in native mode (not WASM WebRTC like Snowflake)
