# WebTunnel Connection Issue - Debug Report

**Date**: 2025-11-30
**Status**: ✅ FIXED

## Executive Summary

The WebTunnel implementation was incorrectly using WebSocket framing after the HTTP Upgrade. The official Tor WebTunnel protocol uses a "WebSocket-like" HTTP Upgrade handshake but **does NOT use actual WebSocket framing** for data transfer - it uses raw TCP bytes after the upgrade.

## Root Cause

**Our Implementation (WRONG)**:
```
Client → Server: HTTP Upgrade (Upgrade: websocket)
Server → Client: 101 Switching Protocols
Client → Server: WebSocket Binary Frame [masked, with opcode + length header]  ← WRONG!
Server: Receives garbage, closes connection
```

**Official WebTunnel Protocol (CORRECT)**:
```
Client → Server: HTTP Upgrade (Upgrade: websocket)
Server → Client: 101 Switching Protocols
Client → Server: Raw Tor TLS ClientHello bytes  ← CORRECT!
Server: Forwards to Tor, works
```

## Evidence

### 1. Official WebTunnel Source Code Analysis

From [`transport/httpupgrade/httpupgrade.go`](https://gitlab.torproject.org/tpo/anti-censorship/pluggable-transports/webtunnel/-/blob/main/transport/httpupgrade/httpupgrade.go):

```go
func (t Transport) Client(conn net.Conn) (net.Conn, error) {
    req, err := http.NewRequest("GET", "/"+t.path, nil)
    // ...
    req.Header.Set("Connection", "upgrade")
    req.Header.Set("Upgrade", "websocket")
    // ...
    if resp.Status == "101 Switching Protocols" {
        return conn, nil  // ← Returns raw connection, NO WebSocket wrapping!
    }
}
```

The official implementation:
- Sends `Upgrade: websocket` headers to **mimic** WebSocket (for censorship evasion)
- After receiving 101, uses the **raw TCP connection** directly
- No actual WebSocket framing is applied

### 2. Wireshark Capture Analysis

**Capture File**: `/tmp/webtunnel_capture.pcap`
**Total Packets**: 25 packets captured

#### Timeline Summary

| Time (s) | Direction | Description |
|----------|-----------|-------------|
| 0.000 | C→S | TCP SYN |
| 0.235 | S→C | TCP SYN-ACK |
| 0.236 | C→S | TLS ClientHello (to bridge) |
| 0.471 | S→C | TLS ServerHello + Certificate |
| 0.472 | C→S | TLS Finished |
| 0.748 | C→S | HTTP GET with Upgrade: websocket (encrypted in TLS) |
| 0.984 | S→C | HTTP 101 Switching Protocols (encrypted in TLS) |
| 0.985 | C→S | **First data after upgrade** (WebSocket framed - WRONG) |
| 1.255 | S→C | 24 bytes + FIN (server closes connection) |

#### Key Observations

1. **No TCP RST**: Server sends graceful FIN, not RST - it read our data but rejected it
2. **Quick closure**: ~270ms between our first data and server FIN
3. **Server sent 24 bytes before closing**: Likely a TLS alert or error message

### 3. Error Message Analysis

```
WebSocket protocol error: Connection reset without closing handshake
```

This error from tungstenite means:
- The underlying TCP connection was closed
- No WebSocket Close frame was received
- tungstenite expected WebSocket protocol, but server just closed TCP

## What We Were Doing Wrong

In `webtunnel.rs`:
```rust
// WRONG: Using WebSocket framing after upgrade
let maybe_tls = MaybeTlsStream::Rustls(tls_stream);
let ws = WebSocketStream::from_raw_socket(maybe_tls, Role::Client, None).await;
```

`from_raw_socket` wraps all I/O in WebSocket frames:
- Adds opcode byte (0x82 for binary)
- Adds length bytes
- Applies XOR masking (for client frames)

But the server expects raw Tor protocol bytes directly on the stream.

## Solution

After HTTP Upgrade succeeds, use the raw TLS stream directly without WebSocket framing:

```rust
// After getting 101 Switching Protocols:
// Just return the raw TLS stream - no WebSocket wrapping!
Ok(WebTunnelStream { 
    inner: WebTunnelInner::Raw(tls_stream),
    read_buffer: Vec::new(),
})
```

## Test Command

```bash
WEBTUNNEL_URL='https://dev-test.catcat.pictures/Fj7VnJi1mjigUKBb2rJ1TeWJ' \
WEBTUNNEL_FINGERPRINT='6F6C8E0069880AC253704507119464CE4BD1BA65' \
RUST_LOG=webtor=debug \
cargo test -p webtor --test integration_test test_webtunnel_fetch_ipify -- --nocapture
```

## References

1. [WebTunnel Tor Project Blog](https://blog.torproject.org/introducing-webtunnel-evading-censorship-by-hiding-in-plain-sight/)
2. [WebTunnel Source Code](https://gitlab.torproject.org/tpo/anti-censorship/pluggable-transports/webtunnel)
3. [HTTPT Paper](https://censorbib.nymity.ch/) - The protocol WebTunnel is based on

## Conclusion

WebTunnel's "websocket" in the headers is a **censorship evasion technique**, not an actual WebSocket implementation. The protocol mimics a WebSocket upgrade to bypass protocol allowlists, but uses raw TCP for data transfer.

---

# UPDATED: Final Solution (2025-11-30)

## The Complete Fix

The initial analysis was correct about not using WebSocket framing, but incomplete. The full solution requires **two TLS layers**:

### Architecture

```
┌────────────────────────────────────────────────────────────┐
│                    CLIENT                                  │
├────────────────────────────────────────────────────────────┤
│  Tor Channel Handshake (VERSIONS, CERTS, AUTH, NETINFO)   │
├────────────────────────────────────────────────────────────┤
│  Inner TLS (Tor link protocol, self-signed certs)         │ ← NEW!
├────────────────────────────────────────────────────────────┤
│  Raw bytes (after HTTP Upgrade 101)                       │
├────────────────────────────────────────────────────────────┤
│  HTTP Upgrade (Upgrade: websocket, Connection: Upgrade)   │
├────────────────────────────────────────────────────────────┤
│  Outer TLS (HTTPS to WebTunnel bridge, WebPKI validated)  │
├────────────────────────────────────────────────────────────┤
│  TCP                                                       │
└────────────────────────────────────────────────────────────┘
```

### Key Changes Made

1. **Two TLS layers**: 
   - Outer TLS: Client ↔ WebTunnel bridge (WebPKI validated)
   - Inner TLS: Client ↔ Tor relay (tunneled through WebTunnel, self-signed)

2. **Custom TLS verifier for inner layer**:
   - Tor relays use self-signed certificates
   - Verification happens via CERTS cells in Tor channel handshake
   - Created `TorCertVerifier` that accepts any certificate

3. **Extract and pass peer certificate**:
   - Implement `CertifiedConn` trait for `WebTunnelStream`
   - Extract peer certificate from inner TLS
   - Pass to `check()` during Tor channel handshake

### Test Result

```
✅ Channel established
✅ Circuit created: Bridge → DakotaEdge2 → TORtelliniExit01  
✅ Response: {"ip":"45.13.225.69"}
✅ Tor exit IP: 45.13.225.69
✅ Total time: 29.4s
```

### Files Changed

- `webtor/src/webtunnel.rs`: Added inner TLS layer with `TorCertVerifier`
- `webtor/src/client.rs`: Extract and pass peer certificate to `check()`
