# Advanced Transport Development

## When to use
Use when implementing or modifying HTTP/2 CONNECT, WebSocket tunnels, raw fixed-target tunnels, or TLS/ALPN negotiation.

## QUIC/HTTP/3 status
QUIC and HTTP/3 are **deferred by ADR** (`docs/adr/ADR_quic_h3_pproxy_parity.md`). The URI schemes `quic://` and `h3://` are rejected with `UnsupportedProtocol` at parse time. No `quinn`/`h3`/`h3-quinn` dependencies exist in the workspace. Do not add QUIC/H3 code without re-evaluation of the ADR conditions.

## Architecture
- Three protocol crates: `eggress-protocol-http` (H2 CONNECT module), `eggress-protocol-websocket`, `eggress-protocol-raw`
- Advanced transports are stream adapters, not protocol-specific special cases
- Each transport produces/accepts `BoxStream` — the universal stream type
- TLS/ALPN configured via `[listeners.tls]` alpn field, wired through `eggress-transport-tls`

## Tier classification

WebSocket, Raw, and H2 CONNECT are native runtime upstream protocols. The
compatibility translator bridges bounded upstream forms through the native
URI/config path; this does not make their upstream-only boundaries or fixed
target restrictions disappear.

- The CLI `parse_listener_uri` rejects `h2://`, `ws://`, and `wss://` as
  listener URIs. Phase 5 admits bounded `raw://`/`tunnel://` listener forms
  only when a brace-delimited fixed target is present.
- The compatibility translator rejects H2/WS/WSS listener roles with
  structured diagnostics; native TOML configuration may support the
  corresponding upstream compositions. `echo` is an explicit compatibility
  listener utility, including a bounded UDP echo mode.
- Tests: `cargo test -p eggress-config` and the protocol/runtime tests cover
  the native paths and refusal boundaries.

## Chain composition behavior (stream-native, Track B hard closure)

WS, WSS, Raw, and H2 upstream handlers now **consume the prior-hop stream** supplied by the chain executor instead of opening independent connections. This means:

- Native intermediate-hop chains (socks5→ws, http→ws, socks5→raw, http→raw,
  socks5→h2, http→h2) are runtime-supported; they are not automatically
  pproxy-compatible translator paths.
- `RawHopHandler` passes through the stream directly (raw passthrough).
- `WebSocketHopHandler` performs the WebSocket handshake over the prior-hop stream via `connect_over_stream()`.
- `H2HopHandler` performs the H2 CONNECT handshake over the prior-hop stream; TLS ALPN is handled by the chain executor.
- These protocols cannot act as listeners (upstream-only), enforced by the `upstream_only_no_listener` constraint type.

## H2 CONNECT
- Server: `h2_connect::handle_h2_connect()` accepts H2 connections, dispatches CONNECT, bridges stream to TCP target
- Client: Use `h2` crate to connect to upstream H2 proxy, issue CONNECT request
- Key type: `H2StreamWrite` — AsyncWrite adapter for h2::SendStream with flow control
- `H2HopHandler` — Runtime HopHandler for H2 CONNECT upstream, performs H2 handshake over the prior-hop stream (stream-native); TLS ALPN handled by chain executor
- ALPN: `h2` for TLS negotiation

## WebSocket Tunnels
- Server: `WebSocketTunnelServer::accept_upgrade()` accepts TCP, completes WS handshake, returns BoxStream
- Client: `WebSocketTunnelClient::connect()` connects to WS/WSS upstream, returns BoxStream
- Key type: `WebSocketStreamAdapter` — wraps split WS stream as AsyncRead+AsyncWrite
- Binary frames = stream data, Close = shutdown, Ping/Pong handled automatically
- Max message size enforced (default 16MB)

## Raw Tunnels
- `RawTunnelListener::bind()` + `run()` accepts TCP, connects to fixed target, relays via copy_bidirectional
- No protocol negotiation — explicit listener mode only
- Fixed target validated at startup

## TLS/ALPN
- Config: `[listeners.tls]` with `alpn = ["h2", "http/1.1"]`
- Builder methods: `TlsClientConfigBuilder::with_h2_alpn()`, `TlsServerConfigBuilder::with_h2_alpn()`
- ALPN validated at config compile time

## Testing
- H2 protocol: `cargo test -p eggress-protocol-http h2`
- H2 upstream integration: `cargo test -p eggress-runtime --test upstream_protocols h2`
- WebSocket: `cargo test -p eggress-protocol-websocket`
- Raw: `cargo test -p eggress-protocol-raw`
- All: `cargo test --workspace`

## Common pitfalls
- H2 flow control: must use `reserve_capacity`/`poll_capacity` before sending DATA
- WebSocket binary frames only — text frames are logged and skipped
- Raw tunnels have no protocol detection — must be explicitly configured
- ALPN values must be valid ASCII strings
