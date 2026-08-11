# eggress-server

`crates/eggress-server/`

Connection lifecycle management — the main entry point for processing inbound connections through detection, routing, and relay.

## Key Functions

| Function | Description |
|---|---|
| `serve_connection()` | Top-level: detect → accept → route → reply → relay → report |

## Key Types

| Type | Description |
|---|---|
| `AcceptedSession` | Parsed inbound session: `Tunnel` (CONNECT) or `HttpForward` |
| `PendingTunnel` | Parsed CONNECT request before route opening |
| `PendingHttpForward` | Parsed HTTP forward request before route opening |
| `ConnectionConfig` | Per-connection config: routing, upstreams, TLS, timeouts |
| `ConnectionContext` | Per-connection runtime state |
| `SessionReport` | Structured connection outcome with protocol, target, bytes, outcome |
| `SessionOutcome` | Normalized outcomes: Completed, ClientProtocolError, AuthenticationFailed, HandshakeTimedOut, RouteFailed, RelayFailed, Cancelled |
| `FailureCategory` | Detailed diagnostics: Protocol, Authentication, Dns, ConnectionRefused, NetworkUnreachable, HostUnreachable, RouteTimeout, UpstreamAuthentication, Relay, Internal, etc. |
| `SessionOpenError` | Normalized route failure types with protocol-specific reply mapping |

## Session Lifecycle

```
serve_connection()
  │
  ├─ record_session_start()
  │
  ├─ accept() → AcceptedSession
  │   ├─ ReplayStream sniffs initial bytes
  │   ├─ ProtocolDispatcher tries detectors in order
  │   ├─ HTTP CONNECT → PendingTunnel
  │   ├─ HTTP forward → PendingHttpForward
  │   └─ SOCKS4/5, Shadowsocks, Trojan → PendingTunnel
  │
  ├─ (auth failure / protocol error / timeout → SessionReport)
  │
  ├─ RouteRequest built from session metadata
  ├─ Router.decide() → RouteDecision
  ├─ Router.select() → SelectedRoute (with ActiveLease)
  │
  ├─ open_route()
  │   ├─ Direct: DirectConnector → BoxStream
  │   └─ Chain: ChainExecutor → BoxStream
  │
  ├─ send success/failure reply to client
  │
  ├─ relay() / HTTP forward exchange
  │
  └─ record_session(&report)   ← exactly one terminal call
```

## Traits

| Trait | Description |
|---|---|
| `SessionMetrics` | Record session outcomes (latency, bytes, failure category) |
| `UdpService` | Handle UDP ASSOCIATE requests from SOCKS5 |

## Deferred Success Replies

Success replies are sent only after the outbound route is established. If route opening fails, an appropriate error reply is sent to the client instead.

## Protocol Enforcement

Listener configuration restricts which protocols are accepted. A listener configured for `["socks5"]` will reject HTTP connections.

## Dependencies

- `eggress-core` — `BoxStream`, `TargetAddr`, `ProtocolId`, `SessionContext`
- `eggress-routing` — `Router`, `SelectedRoute`, `ActiveLease`
- `eggress-udp` — UDP association handling

See [overview.md](overview.md) for context.
