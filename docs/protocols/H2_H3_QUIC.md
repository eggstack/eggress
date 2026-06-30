# HTTP/2 CONNECT and HTTP/3/QUIC

## Overview

HTTP/2 CONNECT provides a multiplexed tunnel transport using the HTTP/2
CONNECT method. Each tunneled stream is an independent HTTP/2 stream within
a single TLS connection, enabling connection multiplexing.

HTTP/3 over QUIC is deferred pending pproxy behavioral stability and
dependency evaluation. See `docs/adr/ADR_quic_h3_pproxy_parity.md` for the
decision rationale.

Source: `crates/eggress-protocol-h2/src/`

## HTTP/2 CONNECT

### Server Behavior

The H2 CONNECT server accepts HTTP/2 connections with the `CONNECT` method
and the target in the `:authority` pseudo-header.

```
Client                          Server
  │                                │
  │  TLS Handshake (ALPN h2)       │
  │<──────────────────────────────>│
  │                                │
  │  HTTP/2 CONNECT                │
  │  :method CONNECT               │
  │  :authority target:port        │
  │  :scheme https                 │
  │  :path /                       │
  │───────────────────────────────>│
  │                                │
  │  HTTP/2 200 OK                 │
  │  (end_stream=false)            │
  │<───────────────────────────────│
  │                                │
  │  HTTP/2 DATA frames            │
  │  ┌─────────────────────────┐   │
  │  │ Stream: <id>            │   │
  │  │ Payload: tunneled data  │   │
  │  └─────────────────────────┘   │
  │<──────────────────────────────>│
```

1. Accept TCP connection.
2. Perform TLS handshake with ALPN `h2`.
3. Read HTTP/2 `CONNECT` request.
4. Validate `:authority` pseudo-header (target address).
5. Connect to the target address.
6. Send HTTP/2 `200 OK` response (headers only, no body).
7. Relay DATA frames bidirectionally between the HTTP/2 stream and the
   target TCP connection.
8. When the HTTP/2 stream closes (RST_STREAM or END_STREAM), close the
   target TCP connection.

### Server Limits

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_concurrent_streams` | 100 | Maximum concurrent CONNECT streams per connection |
| `max_header_list_size` | 8,192 bytes | Maximum total header size |
| `initial_window_size` | 65,535 bytes | HTTP/2 flow control window |
| `handshake_timeout` | 30s | TLS + HTTP/2 handshake timeout |

### Client Behavior

The H2 CONNECT client establishes a single TLS connection with ALPN `h2` and
multiplexes tunnel streams over it.

```
Client                          Server
  │                                │
  │  TLS Handshake (ALPN h2)       │
  │<──────────────────────────────>│
  │                                │
  │  HTTP/2 CONNECT                │
  │  :method CONNECT               │
  │  :authority target:port        │
  │  :scheme https                 │
  │  :path /                       │
  │───────────────────────────────>│
  │                                │
  │  HTTP/2 200 OK                 │
  │<───────────────────────────────│
  │                                │
  │  HTTP/2 DATA frames            │
  │  ┌─────────────────────────┐   │
  │  │ Stream: <id>            │   │
  │  │ Payload: tunneled data  │   │
  │  └─────────────────────────┘   │
  │<──────────────────────────────>│
```

1. Open TCP connection to the H2 proxy server.
2. Perform TLS handshake with ALPN `h2`.
3. Send HTTP/2 `CONNECT` request with `:authority` set to `target:port`.
4. Wait for HTTP/2 `200 OK` response.
5. After the response, the HTTP/2 stream is a bidirectional byte pipe.
6. Send and receive DATA frames containing the tunneled bytes.

### Connection Reuse

The H2 client maintains a connection pool. Multiple tunnel streams share the
same TLS connection. This provides:

- **Reduced latency**: No TLS handshake per tunnel.
- **Multiplexing**: Multiple concurrent tunnels over one TCP connection.
- **Flow control**: Per-stream flow control prevents head-of-line blocking.

### Error Handling

| HTTP/2 Error | Behavior |
|--------------|----------|
| `NO_ERROR` (0x0) | Graceful close |
| `PROTOCOL_ERROR` (0x1) | Connection closed; streams reset |
| `INTERNAL_ERROR` (0x2) | Connection closed; streams reset |
| `FLOW_CONTROL_ERROR` (0x3) | Window adjustment; connection may survive |
| `STREAM_CLOSED` (0x5) | Individual stream closed |
| `CONNECT_ERROR` (0xb) | CONNECT failed; returns error to caller |

### ALPN Negotiation

H2 requires TLS with ALPN `h2`. The TLS handshake negotiates the protocol:

```
Client ALPN list: ["h2"]
Server ALPN list: ["h2"]
Negotiated: h2
```

If ALPN negotiation fails or the server does not support `h2`, the TLS
handshake fails with `NoApplicationProtocol`.

## H3/QUIC (Deferred)

### Investigation Findings

pproxy mentions H3/QUIC as a transport option but:

- pproxy's H3/QUIC behavior is **experimental and unstable** in version 2.7.9.
- No documented interop evidence between pproxy's H3/QUIC and standard QUIC
  implementations.
- The `aioquic` Python library (pproxy's QUIC backend) has limited deployment
  and is not the reference QUIC implementation.

### Deferral Decision

H3/QUIC implementation is deferred. See `docs/adr/ADR_quic_h3_pproxy_parity.md`
for the full rationale.

Key factors:
- **Dependency weight**: `quinn` crate adds a significant dependency tree
  (rustls,ring, etc.) for a transport with uncertain pproxy interop.
- **No clear use case**: H3/QUIC is primarily beneficial for lossy networks
  (mobile, satellite). Proxy deployments typically have stable TCP connections.
- **pproxy instability**: The H3 behavior in pproxy is not stable enough to
  serve as a differential testing oracle.

### Dependencies (if implemented)

| Crate | Purpose | Size |
|-------|---------|------|
| `quinn` | QUIC transport | Large (rustls, ring) |
| `h3` | HTTP/3 protocol | Small |
| `h3-quinn` | h3 + quinn integration | Small |

ALPN values: `h3` (HTTP/3 over QUIC).

## Test Coverage

- H2 CONNECT server: accept, connect to target, relay DATA frames, close
- H2 CONNECT client: connect, send CONNECT request, relay DATA frames
- ALPN negotiation: h2 selected, fallback on failure
- Connection reuse: multiple streams on one connection
- Error handling: RST_STREAM, GOAWAY, connection errors
- Frame size limits: oversized headers rejected
- Handshake timeout: slow TLS handshake aborted

Test count: planned across `eggress-protocol-h2` and runtime integration tests.

## Limitations

### H2

- No server push (not applicable for tunnel use)
- No stream prioritization (tunnel streams are equal priority)
- No connection preface optimization (standard HTTP/2 preface)
- Single-hop only — H2 CONNECT does not chain with other protocols at the
  transport layer

### H3/QUIC

- Deferred — not implemented in Phase 26
- Requires `quinn` + `h3` + `h3-quinn` dependency stack
- pproxy H3 behavior is experimental and unstable
- No interop evidence available
