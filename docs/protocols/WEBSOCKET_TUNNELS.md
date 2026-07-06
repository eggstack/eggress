# WebSocket Tunnels

## Overview

WebSocket tunnel transport for proxying TCP streams. The WebSocket protocol
provides an HTTP Upgrade handshake that establishes a bidirectional byte pipe.
After the handshake, binary frames carry the tunneled data.

Source: `crates/eggress-protocol-websocket/src/`

## Wire Format

### Connection Establishment

```
Client                          Server
  │                                │
  │  HTTP Upgrade Request          │
  │  GET /proxy HTTP/1.1           │
  │  Host: proxy.example.com       │
  │  Upgrade: websocket            │
  │  Connection: Upgrade           │
  │  Sec-WebSocket-Key: <key>      │
  │  Sec-WebSocket-Version: 13     │
  │───────────────────────────────>│
  │                                │
  │  HTTP 101 Switching Protocols  │
  │  Upgrade: websocket            │
  │  Connection: Upgrade           │
  │  Sec-WebSocket-Accept: <hash>  │
  │<───────────────────────────────│
  │                                │
  │  WebSocket Binary Frames       │
  │  ┌─────────────────────────┐   │
  │  │ Opcode=0x2 (binary)     │   │
  │  │ Payload: stream data    │   │
  │  └─────────────────────────┘   │
  │<──────────────────────────────>│
```

### Frame Format (RFC 6455)

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-------+-+-------------+-------------------------------+
|F|R|R|R| opcode|M| Payload len |    Extended payload length    |
|I|S|S|S|  (4)  |A|     (7)     |             (16/64)           |
|N|V|V|V|       |S|             |   (if payload len==126/127)   |
| |1|2|3|       |K|             |                               |
+-+-+-+-+-------+-+-------------+ - - - - - - - - - - - - - - - +
|     Extended payload length continued, if payload len == 127  |
+ - - - - - - - - - - - - - - - +-------------------------------+
|                               |Masking-key, if MASK set to 1  |
+-------------------------------+-------------------------------+
| Masking-key (continued)       |          Payload Data         |
+-------------------------------- - - - - - - - - - - - - - - - +
:                     Payload Data continued ...                :
+ - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - +
|                     Payload Data (continued)                  |
+---------------------------------------------------------------+
```

For tunnel use, only **binary frames** (opcode `0x2`) are used. Text frames
(opcode `0x1`) are rejected. Control frames (close, ping, pong) are handled
per RFC 6455 but do not carry tunnel data.

## Server Behavior

### Accept Phase

1. Listen for TCP connections on the configured bind address.
2. Read the HTTP Upgrade request.
3. Validate:
   - Method is `GET`.
   - `Upgrade: websocket` header is present (case-insensitive).
   - `Connection: Upgrade` header is present (case-insensitive).
   - `Sec-WebSocket-Version: 13` is present.
   - `Sec-WebSocket-Key` is present and non-empty.
4. Validate path and host against configuration:
   - Path must match the configured proxy path (default: `/proxy`).
   - Host must match the configured host or be unrestricted.
5. Compute `Sec-WebSocket-Accept` from the key and the GUID `258EAFA5-E914-47DA-95CA-C5AB0DC85B11`.
6. Send the 101 Switching Protocols response.
7. After the response, the TCP stream is a raw bidirectional byte pipe.

### Binary Frame Relay

After the Upgrade, the server operates in binary frame mode:

- **Inbound to upstream**: Read client TCP bytes, wrap in WebSocket binary
  frames (opcode `0x2`), send to client. Client unwraps.
- **Upstream to client**: Receive WebSocket binary frames from client,
  extract payload bytes, write to upstream TCP connection.

In practice, after the WebSocket handshake completes, the connection IS a raw
TCP stream. The WebSocket framing is handled by the WebSocket layer in the
upgrade library. The tunnel operates on the unwrapped byte stream.

### Close Handling

- **TCP close**: When one side closes, the WebSocket close handshake is
  initiated (close frame sent, close response expected).
- **Close frame**: Server sends a close frame with status code 1000 (normal
  closure) when the tunnel ends.
- **Timeout**: If the close handshake does not complete within the configured
  timeout, the connection is forcibly closed.

## Client Behavior

### Connect Phase

1. Open TCP connection to the proxy server.
2. Send HTTP Upgrade request:
   ```
   GET /proxy HTTP/1.1
   Host: <proxy-host>
   Upgrade: websocket
   Connection: Upgrade
   Sec-WebSocket-Key: <random-16-bytes-base64>
   Sec-WebSocket-Version: 13
   ```
3. Receive and validate the 101 response.
4. After the response, the connection is a raw bidirectional byte pipe.

### Binary Frame Relay

Same as server — the client sends binary frames containing the tunneled data
and receives binary frames from the server.

## Close/Ping/Pong Handling

| Frame Type | Opcode | Behavior |
|-----------|--------|----------|
| Binary | `0x2` | Data frame; payload is tunneled bytes |
| Text | `0x1` | Rejected; tunnel uses binary only |
| Close | `0x8` | Graceful shutdown; sends close frame with status 1000 |
| Ping | `0x9` | Automatic pong reply; does not interrupt tunnel |
| Pong | `0xA` | Acknowledges ping; updates keepalive state |

Ping/pong frames are handled by the WebSocket layer transparently. They do
not interfere with the binary data stream.

## Frame Size Limits

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_frame_size` | 16,384 bytes | Maximum payload per WebSocket frame |
| `max_message_size` | 16,384 bytes | Maximum assembled message size |
| `max_header_size` | 4,096 bytes | Maximum HTTP header size during Upgrade |

Frames exceeding `max_frame_size` cause the connection to be closed with
status 1009 (message too big).

## WSS via TLS with ALPN

WebSocket over TLS (WSS) uses the shared TLS transport layer:

```
TCP connect → tls_connect(ALPN=["http/1.1"]) → TlsStream<TcpStream>
  → WebSocket Upgrade over TLS
  → binary frame tunnel
```

ALPN negotiation uses `http/1.1` as the sole protocol identifier. The TLS
server verifies the Upgrade request after the handshake.

WSS is required when:
- The upstream server expects TLS.
- The deployment requires encryption (e.g., traversing a CDN).
- The proxy is exposed on a public network.

## pproxy Compatibility

### Supported Schemes

| pproxy scheme | Eggress behavior | Notes |
|---------------|------------------|-------|
| `ws://` | WebSocket tunnel (plain) | No TLS; direct TCP |
| `wss://` | WebSocket tunnel (TLS) | TLS required; ALPN `http/1.1` |

### Configuration Translation

pproxy-style invocation:

```bash
pproxy -l ws://:8080 -r direct://
pproxy -l wss://:8443 -r direct:// --ssl cert.pem,key.pem
```

Eggress TOML equivalent:

```toml
[[listeners]]
name = "ws-in"
bind = "0.0.0.0:8080"

[listeners.transport]
type = "websocket"
path = "/proxy"

[[upstreams]]
id = "direct"
uri = "direct://"

[[rules]]
id = "default"
upstream_group = "default"
```

### pproxy Parity Tier

| Feature | Tier | Notes |
|---------|------|-------|
| `ws://` listener | Supported | Eggress implements WebSocket tunnel transport |
| `wss://` listener | Supported | TLS required; uses shared TLS layer |
| WebSocket path validation | Supported | Configurable proxy path |
| WebSocket close handling | Supported | Graceful close handshake |

## Test Coverage

- WebSocket Upgrade handshake (valid request, missing headers, wrong version)
- Binary frame relay (echo through tunnel)
- Close frame handling (graceful close, timeout)
- Ping/pong handling
- Path validation (correct path, wrong path)
- WSS (TLS) connect and relay
- Frame size limit enforcement
- pproxy URI parsing (`ws://`, `wss://`)

Test count: planned across `eggress-transport-websocket` and runtime
integration tests.

## Limitations

- No WebSocket subprotocol negotiation (binary only)
- No permessage-deflate compression
- No chunked transfer encoding support
- Raw tunnel only — no application protocol negotiation on top of WebSocket
- No server-side WebSocket multiplexing (one tunnel per connection)
