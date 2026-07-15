# Advanced Transports

Phase 26 of the pproxy parity roadmap. This document describes advanced
transport wrappers available in eggress: WebSocket tunnels, H2 CONNECT
tunnels, and raw fixed-target tunnels. H3/QUIC is deferred pending
pproxy behavioral stability (see `docs/adr/ADR_quic_h3_pproxy_parity.md`).

## 1. Summary Table

| Transport | pproxy scheme | Eggress internal type | Status | TLS | ALPN |
|-----------|---------------|----------------------|--------|-----|------|
| WebSocket | `ws://` | `WebSocketTunnel` | Phase 26 | optional (wss) | `http/1.1` |
| WebSocket TLS | `wss://` | `WebSocketTunnel` | Phase 26 | required | `http/1.1` |
| HTTP/2 CONNECT | `h2://` | `H2ConnectTunnel` | Phase 26 | required | `h2` |
| Raw tunnel | `raw://` | `RawTunnel` | Phase 26 | optional | none |
| Tunnel (alias) | `tunnel://` | `RawTunnel` | Phase 26 | optional | none |
| H3/QUIC | `h3://` | deferred | deferred | required | `h3` |

## 2. Architecture Overview

### Transport Wrapper vs Application Protocol

Advanced transports are **transport wrappers**, not application protocols. The
distinction is:

- **Application protocol** (HTTP CONNECT, SOCKS5, Shadowsocks, Trojan): Negotiates
  a target address from the client, then relays bidirectional data. The protocol
  defines the handshake, address encoding, and authentication.
- **Transport wrapper** (WebSocket, H2 CONNECT, Raw): Provides a byte pipe between
  the client and the proxy. The target address is either implicit (Raw) or encoded
  in the wrapper's framing (WebSocket path, H2 CONNECT authority). No additional
  application protocol is negotiated on top of the wrapper.

```
┌─────────────────────────────────────────────────┐
│                  Application Layer               │
│  HTTP CONNECT │ SOCKS5 │ Shadowsocks │ Trojan    │
├─────────────────────────────────────────────────┤
│                Transport Wrapper                 │
│  WebSocket │ H2 CONNECT │ Raw │ (H3/QUIC)       │
├─────────────────────────────────────────────────┤
│                   TLS (optional)                 │
├─────────────────────────────────────────────────┤
│                    TCP                          │
└─────────────────────────────────────────────────┘
```

In practice, advanced transports operate **independently** of the application
protocol layer. A WebSocket tunnel does not carry HTTP CONNECT or SOCKS5
handshake bytes — it IS the tunnel. The target is determined by configuration
(Raw) or by the WebSocket handshake (path/host header).

### Chain Compatibility

Transport wrappers compose with application protocols in the chain executor.
The chain executor wraps each hop's stream in the appropriate transport before
applying the application protocol handler:

```
DirectConnector → BoxStream
  → [H2 CONNECT wrapper] → HTTP/2 stream
    → [Application protocol] → relay
```

Raw tunnels skip the application protocol layer entirely — the TCP stream is
forwarded directly to the configured target after the transport layer.

## 3. URI Scheme Mapping

pproxy schemes map to eggress internal types as follows:

| pproxy scheme | pproxy behavior | Eggress internal type | Notes |
|---------------|-----------------|----------------------|-------|
| `ws://` | WebSocket tunnel | `WebSocketTunnel` | Plain WebSocket; no TLS |
| `wss://` | WebSocket over TLS | `WebSocketTunnel` | TLS required; ALPN `http/1.1` |
| `h2://` | HTTP/2 CONNECT | `H2ConnectTunnel` | TLS required; ALPN `h2` |
| `raw://` | Raw TCP tunnel | `RawTunnel` | No protocol negotiation; fixed target |
| `tunnel://` | Raw TCP tunnel (alias) | `RawTunnel` | Alias for `raw://` |
| `h3://` | HTTP/3 over QUIC | deferred | See ADR |

## 4. TLS/ALPN Requirements

| Transport | TLS required | ALPN values | Notes |
|-----------|-------------|-------------|-------|
| `ws://` | No | `http/1.1` (if TLS enabled) | Plain WebSocket |
| `wss://` | Yes | `http/1.1` | TLS mandatory; server validates Upgrade request |
| `h2://` | Yes | `h2` | TLS mandatory; HTTP/2 requires TLS |
| `raw://` | No | none | No ALPN needed; raw TCP |
| `tunnel://` | No | none | Alias for `raw://` |
| `h3://` | Yes | `h3` | Deferred; QUIC transport |

TLS configuration uses the shared `eggress-transport-tls` layer:

- Client: `TlsClientConfigBuilder` with system roots, optional custom CA PEM,
  ALPN list, and server name override.
- Server: `TlsServerConfigBuilder` with cert chain and key PEM.

## 5. Chain Compatibility Matrix

| Listener → Upstream | HTTP | SOCKS4 | SOCKS5 | H2 | WS | Raw |
|---------------------|------|--------|--------|----|----|-----|
| **HTTP** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **SOCKS4** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **SOCKS5** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **H2** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **WS** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Raw** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

All transport wrappers produce a `BoxStream` that is compatible with all
application protocol handlers. The chain executor folds over hops applying
transport wrappers and protocol handlers in sequence.

## 6. Implementation Location

| Transport | Crate | Module |
|-----------|-------|--------|
| WebSocket tunnel | `eggress-protocol-websocket` | `lib.rs` |
| H2 CONNECT tunnel | `eggress-protocol-http` | `h2_connect.rs` |
| Raw tunnel | `eggress-protocol-raw` | `tunnel.rs` |

## 7. Test Coverage

- WebSocket tunnel: server accept, client connect, binary frame relay, close
  handling, path validation, WSS upgrade
- H2 CONNECT: server accept with `:authority`, client CONNECT request,
  per-stream relay, ALPN `h2` negotiation
- Raw tunnel: listener accept, fixed target connect, bidirectional relay
- Chain composition: transport wrapper + application protocol in multi-hop chains
- pproxy compatibility: `ws://`, `wss://`, `h2://`, `raw://`, `tunnel://` URI parsing

## 8. Runtime Integration (Phase B3/B4)

As of Phases B3 and B4, WebSocket (`ws://`/`wss://`), Raw/Tunnel
(`raw://`/`tunnel://`), and H2 (`h2://`) transports are
**runtime-integrated upstream protocols**. They are wired through the
config compiler, CLI, chain executor, and service supervisor.

Key constraints:
- **Upstream-only**: These transports are supported as upstream (terminal)
  chain positions. There is no listener-side support — eggress does not
  accept inbound connections via these transports.
- **Chain position**: They occupy the last hop in a chain. An upstream URI
  like `ws://target:80` or `h2://target:443` connects to the target using
  the corresponding transport.
- **Config**: `[[upstreams]]` entries with `ws://`, `wss://`, `raw://`,
  `tunnel://`, or `h2://` URIs are compiled into the runtime.

See `docs/adr/ADR_b3_ws_raw_runtime_promotion.md` and
`docs/adr/ADR_b4_h2_runtime_promotion.md` for the design rationale.

## 9. Limitations

- No HTTP/3 or QUIC transport (deferred — see ADR)
- No WebSocket subprotocol negotiation beyond binary framing
- No WebSocket compression (permessage-deflate)
- Raw tunnels have no authentication or encryption — target must be trusted
- H2 CONNECT requires TLS (per HTTP/2 specification)
- Transport wrappers do not carry application-layer authentication — use
  application protocols for auth when needed
