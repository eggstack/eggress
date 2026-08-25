# eggress-server — Connection Orchestration

The reusable data plane: accept a connection, detect/handshake the inbound
protocol, route, open the upstream (direct or chained), reply, relay, and
report. Both the CLI service and the embed API drive connections through this
crate.

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | `serve_connection()`, `ConnectionConfig` (routing service, timeouts, protocol allow-list, auth policy, UDP service, TLS client config), `SessionMetrics` trait, `UdpService` trait, `NoopMetrics` |
| `src/accept.rs` | Protocol detection with timeout: `AcceptedSession` (Tunnel / HttpForward / UdpAssociate / Echo), `TunnelProtocol` (HttpConnect, Http2, Http3, WebSocket, Socks4, Socks5, Shadowsocks, ShadowsocksR, Trojan, Raw), `ReplyContext`, `InboundAuthentication` (None / UsernamePassword / WithReuse + `AuthReuseCache`, IP-keyed, 4096 entries) |
| `src/execute.rs` | `execute()` → `open_route()` → relay; `build_chain_executor()` assembles all hop handlers; `SessionReport`, `SessionOutcome`, `FailureCategory` |
| `src/reply.rs` | Protocol-correct success/failure replies (SOCKS REP codes, HTTP status mapping). Success is DEFERRED until the upstream route opens |
| `src/error.rs` | `SessionOpenError`: normalized route failures mapped to per-protocol replies |
| `src/advanced.rs` | `serve_h2_connection()`, `serve_websocket_connection()` (listener-side H2/WS termination) |
| `src/listener/unix.rs` | Unix domain socket listener; refuses to unlink non-socket files/symlinks |
| `src/listener/transparent.rs` | Linux SO_ORIGINAL_DST retrieval — the one documented `unsafe` block in the workspace boundary |

## Lifecycle invariants

- `record_session_start()` and exactly one `record_session(&report)` per
  connection (enforced structurally and by tests).
- Handshake bounded by `timeouts.handshake`; route opening bounded by
  `timeouts.connect`; HTTP body upload intentionally NOT bounded by connect
  timeout.
- HTTP forward supports keep-alive loops, rejects `Expect: 100-continue`
  (417), filters hop-by-hop headers upstream of the protocol crate's framing.
- Chain hop handlers are stream-consuming: each takes the prior stream and
  returns an upgraded one (see [core.md](core.md)).

## Failure taxonomy

`SessionOutcome`: Completed, ClientProtocolError, AuthenticationFailed,
HandshakeTimedOut, RouteFailed, RelayFailed, Cancelled.
`FailureCategory` refines: Dns, ConnectionRefused, NetworkUnreachable,
HostUnreachable, RouteTimeout, RouteHop, UpstreamAuthentication, PolicyDenied,
Protocol, Relay, Internal...

## Review entry points

- Trace one CONNECT end-to-end: `serve_connection` → `accept::…` →
  `execute_tunnel` → `open_route` → `relay`.
- Verify: `cargo test -p eggress-server`
