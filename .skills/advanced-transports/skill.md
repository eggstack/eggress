# Advanced Transport Development

## When to use
Use when implementing or modifying HTTP/2 CONNECT, HTTP/3 CONNECT, WebSocket
tunnels, raw fixed-target tunnels, QUIC streams, or TLS/ALPN negotiation.

## QUIC/HTTP/3 status
QUIC and HTTP/3 are implemented behind the opt-in `quic` feature. The
`eggress-transport-quic` crate owns Quinn endpoint and stream lifecycle, and
`eggress-protocol-h3` owns HTTP/3 CONNECT. `h3://` is the multiplexed HTTP/3
form; `quic+http://` is raw QUIC carrying the HTTP protocol. Default and
`common` builds remain dependency-free. Native clients verify certificates;
the compatibility-only insecure mode must be explicit and warning-bearing.

QUIC listeners require certificate/key material and advertise ALPN `h3` for
HTTP/3. UDP association mode is rejected because UDP-over-QUIC stream mapping
is not part of the current supported composition matrix. Do not add
WebTransport or MASQUE as an implicit extension.

## Architecture
- Three protocol crates: `eggress-protocol-http` (H2 CONNECT module), `eggress-protocol-websocket`, `eggress-protocol-raw`
- Advanced transports are stream adapters, not protocol-specific special cases
- Each transport produces/accepts `BoxStream` — the universal stream type
- TLS/ALPN configured via `[listeners.tls]` alpn field, wired through `eggress-transport-tls`

## Tier classification

WebSocket, Raw, H2 CONNECT, and optional H3/QUIC transports are native runtime
transports. The
compatibility translator bridges upstream forms through the native URI/config
path and also exposes bounded H2 listener and fixed-target WS/WSS listener
roles. WS/WSS listeners require a fixed target; H2 listeners multiplex one
CONNECT request per stream.

- Compatibility `h2://` listeners are multiplexed H2 CONNECT servers.
- Compatibility `ws{target}://listener` and
  `wss{target}://listener` forms are fixed-target WebSocket servers;
  `--ssl` supplies the WSS certificate and `http/1.1` ALPN.
- `echo` is an explicit compatibility listener utility, including a bounded
  UDP echo mode.
- `h3://` listeners require TLS certificate/key material and multiplex
  independent CONNECT streams; `quic+http://` listeners multiplex raw QUIC
  streams into the selected application protocol.
- Tests: `cargo test -p eggress-config` and the protocol/runtime tests cover
  the native paths and refusal boundaries.
- Focused QUIC tests: `cargo test -p eggress-transport-quic` and
  `cargo test -p eggress-protocol-h3`.

## Chain composition behavior (stream-native, Track B hard closure)

WS, WSS, Raw, and H2 upstream handlers now **consume the prior-hop stream** supplied by the chain executor instead of opening independent connections. This means:

- Native intermediate-hop chains (socks5→ws, http→ws, socks5→raw, http→raw,
  socks5→h2, http→h2) are runtime-supported; they are not automatically
  pproxy-compatible translator paths.
- `RawHopHandler` passes through the stream directly (raw passthrough).
- `WebSocketHopHandler` performs the WebSocket handshake over the prior-hop stream via `connect_over_stream()`.
- `H2HopHandler` performs the H2 CONNECT handshake over the prior-hop stream; TLS ALPN is handled by the chain executor.
- Raw remains an explicit fixed-target listener; H2 and WebSocket listener
  roles use dedicated runtime handlers rather than protocol sniffing.

## H2 CONNECT
- Server: `h2_connect::handle_h2_connect()` accepts H2 connections, dispatches CONNECT, bridges stream to TCP target
- Compatibility server: `eggress_server::advanced::serve_h2_connection()` accepts
  independent CONNECT streams, validates proxy auth, and routes each stream.
- Client: Use `h2` crate to connect to upstream H2 proxy, issue CONNECT request
- Key type: `H2StreamWrite` — AsyncWrite adapter for h2::SendStream with flow control
- `H2HopHandler` — Runtime HopHandler for H2 CONNECT upstream, performs H2 handshake over the prior-hop stream (stream-native); TLS ALPN handled by chain executor
- ALPN: `h2` for TLS negotiation

## WebSocket Tunnels
- Server: `WebSocketTunnelServer::accept_upgrade()` accepts TCP, completes WS handshake, returns BoxStream
- Compatibility server: `serve_websocket_connection()` validates the upgrade
  auth header and relays to the configured fixed target.
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
  and release received capacity as DATA is consumed.
- WebSocket binary frames only — text frames are logged and skipped
- Raw tunnels have no protocol detection — must be explicitly configured
- Raw/tunnel fixed-target listeners are TCP stream forwarding; the bounded
  pproxy-compatible UDP fixed-target mode belongs to `eggress-udp` and must be
  configured explicitly as a UDP listener
- ALPN values must be valid ASCII strings
