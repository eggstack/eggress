# eggress-core

`crates/eggress-core/`

The foundational crate defining shared types, traits, and infrastructure used by all other crates. It has no workspace dependencies.

## Key Types

| Type | Description |
|---|---|
| `BoxStream` | `Pin<Box<dyn AsyncRead + AsyncWrite + Send + Unpin>>` — the universal async byte stream |
| `TargetAddr` | Typed destination address: `SocketAddr`, `IpAddr`, or `DomainName` + port |
| `TargetHost` | Resolved or unresolved host: `IpAddr` or `DomainName` |
| `ProtocolId` | Enum of supported protocols: `Http`, `Socks4`, `Socks5`, `Shadowsocks`, `Trojan`, `WebSocket`, `H2` |
| `ClientIdentity` | Anonymous or authenticated client (username/password) |
| `SessionContext` | Per-connection metadata: target, client identity, listener ID, transport kind |
| `RouteAction` | What to do: `Direct`, `UpstreamGroup(id)`, or `Reject(reason)` |
| `UpstreamId` | Typed string ID for an upstream |
| `ListenerId` | Typed string ID for a listener |

## Key Traits

| Trait | Description |
|---|---|
| `AsyncStream` | Marker trait alias for `AsyncRead + AsyncWrite + Send + Unpin + 'static` |
| `TransportCapability` | Describes what a transport layer supports (TCP, UDP, etc.) |

## Modules

### `relay`

Bidirectional half-close-aware data relay between two streams. Handles:
- `AsyncRead` → `AsyncWrite` in both directions
- Half-close: when one side EOFs, the other side's write is shut down
- Byte counting via `SessionMetrics` trait

### `chain`

Multi-hop proxy chain execution via `ChainExecutor`. Each hop applies:
1. Optional TLS wrapping (`transport-tls`)
2. Protocol handshake via `HopHandler` trait
3. Returns the upgraded `BoxStream`

### `detect`

Protocol detection using `ReplayStream` (bounded sniff buffer) and `ProtocolDispatcher` (ordered detection).

### `error`

Structured error enums:
- `ConnectError` — connection failures (DNS, refused, unreachable)
- `ProtocolError` — protocol-level errors (parse, handshake)
- `AuthError` — authentication failures
- `RelayError` — relay-phase errors

## Architecture Position

`eggress-core` is the leaf dependency. Every other crate depends on it. It defines no runtime logic — only types, traits, and pure functions.

See [overview.md](overview.md) for context.
