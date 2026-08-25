# eggress-core — Foundation Types, Streams, Relay, Chain Execution

The root dependency of nearly every other crate. Defines the universal stream
boundary (`BoxStream`), destination/identity types, the bidirectional relay,
protocol sniffing/dispatch, the direct connector with DNS-rebinding protection,
and the multi-hop `ChainExecutor` used for all upstream chains.

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | `BoxStream`, `TargetAddr`/`TargetHost`, `ClientIdentity`, `SessionContext`, `ProtocolId`, `UpstreamId`, `RejectReason`; crate re-exports |
| `src/listener.rs` | Inbound TCP listener with semaphore-bounded concurrent connections |
| `src/connector.rs` | `DirectConnector`: TCP connect + DNS resolution; rejects resolutions landing in private/reserved ranges (DNS rebinding protection; explicit IP targets bypass) |
| `src/relay.rs` | `relay()`: bidirectional copy with TCP half-close awareness and byte accounting |
| `src/replay.rs` | `ReplayStream`: bounded sniff buffer that replays consumed bytes so detection is single-pass |
| `src/detect.rs` | Protocol detector trait surface used by sniffing |
| `src/dispatch.rs` | `ProtocolDispatcher`: ordered detector list → first match wins |
| `src/chain.rs` | `ChainExecutor` + `HopHandler`: walks a chain of hops, each handler consumes the prior hop's stream and returns an upgraded stream |
| `src/capability.rs` | `classify_upstream_chain()`: static classification of what a chain supports (used by admin display and UDP gating) |

## Key invariants

- Everything crossing a protocol/transport boundary is boxed: no generic stream
  types propagate upward through the stack.
- Domains stay unresolved (`TargetHost::Domain`) until something must dial.
- Credentials are never `Debug`/`Display` printed; identities redact.
- `unsafe_code = "deny"` workspace-wide applies here first.

## Interactions

- Consumed by every protocol crate (they accept/return `BoxStream`).
- `eggress-server` drives `ProtocolDispatcher`, `DirectConnector`,
  `ChainExecutor`, and `relay()`.
- `eggress-routing` uses `TargetAddr`, `ProtocolId`, `ClientIdentity`.

## Review entry points

- Read order: `lib.rs` types → `chain.rs` (composition model) → `connector.rs`
  (rebinding policy) → `dispatch.rs`.
- Verify: `cargo test -p eggress-core`
