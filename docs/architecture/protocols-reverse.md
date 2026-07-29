# eggress-protocol-reverse

`crates/eggress-protocol-reverse/`

Reverse (backward) proxy for NAT traversal. A server behind NAT connects outward to an acceptor in a datacenter.

## Architecture

```
External client → ReverseServer (acceptor) → Control channel → ReverseClient (NAT server) → Local target
```

## Key Types

| Type | Location | Description |
|---|---|---|
| `ReverseServer` | `server.rs` | Acceptor side: binds control + external listeners |
| `ReverseClient` | `client.rs` | NAT server side: connects to acceptor, resolves targets locally |
| `ControlState` | `lib.rs` | Control connection state enum |
| `ProtocolError` | `lib.rs` | Protocol-level error types |
| `ReverseMetrics` | `metrics.rs` | Prometheus metrics for reverse proxy |

## Control Protocol

1. Client connects to control listener
2. Client sends `user:password\n`
3. Server validates, responds with `0x01` (accept) or `0x00` (reject)
4. Authenticated connection enters unbounded channel
5. External client arrives → server pairs with a control connection
6. Bidirectional relay between external stream and control stream

## Runtime Integration

`eggress-runtime/src/reverse.rs` bridges routing to reverse clients:
- `RouteEngineTargetResolver` implements `TargetResolver`
- Routing decisions gate target resolution
- `Direct` or `UpstreamGroup` → `TargetResolution::Connect`
- `Reject` → `TargetResolution::Reject`

## Security

- Plaintext by default (wrap in external TLS for production)
- Non-loopback external binds require auth + allowlist
- Bounded control connections, streams per listener
- Credential redaction in logs

## Dependencies

None — standalone protocol crate.

See [overview.md](overview.md) for context.
