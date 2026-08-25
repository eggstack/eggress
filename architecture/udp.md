# eggress-udp — Associations, Flows, Upstream Relay, Standalone Modes

UDP subsystem behind SOCKS5 UDP ASSOCIATE plus standalone datagram relays.
TCP-only protocols are explicitly rejected here — never silently downgraded.

## Modes (`UdpMode`)

Socks5UdpAssociate · StandalonePproxyUdp · ShadowsocksUdp · Echo · FixedTarget

## Module map

| File | Role |
|---|---|
| `src/assoc.rs`, `registry.rs` | `UdpAssociation` owned by the TCP control connection; bounded registry (global/per-listener limits); every close path removes exactly once |
| `src/relay.rs` | Main relay loop: decode → ownership/pin check → full `RouteService::route()` per datagram (preserves group fallback semantics) → send/response mapping; idle reaping |
| `src/flow.rs` | Per-target connected-socket flows keyed by target; kinds: `Socks5UdpTargetFlow`, `ShadowsocksUdpTargetFlow`, `ComposedUdpTargetFlow`; touch/idle/reap logic |
| `src/hop.rs` | `UdpHopStack`: nested datagram framing encoded destination-inward, decoded outer-to-inner |
| `src/composed.rs` | Multi-hop composition for SOCKS5/Shadowsocks chains |
| `src/upstream_socks5.rs` | Upstream client: TCP control + handshake + UDP ASSOCIATE + bound socket |
| `src/direct.rs` | Direct forwarding path |
| `src/standalone.rs` | pproxy-style standalone UDP relay (fixed target modes) |
| `src/standalone_shadowsocks.rs` | Feature-gated Shadowsocks standalone UDP |
| `src/security.rs` | `validate_target` (reject multicast/broadcast/unspecified/port 0), `validate_standalone_target` (private/reserved-range rejection = DNS rebinding defense), datagram size bounds |
| `src/udp_capability.rs` | Classifies chains: which hop stacks support UDP at all |
| `src/metrics.rs` | `UdpMetrics` atomics, bridged into Prometheus by eggress-metrics |
| `src/testkit.rs` | UDP echo + SOCKS5 UDP test servers |

## Security defaults

- Client address pinning ON: first valid packet pins the association;
  foreign-source packets are dropped and do NOT refresh activity timers.
- All targets validated against reserved ranges (standalone mode especially).
- Bounds: association count, targets-per-association, max datagram size.

## Reload behavior

Route changes apply to the next datagram; existing flows keep their upstream
until idle expiry; bind/advertise changes need restart.

## Review entry points

- Verify: `cargo test -p eggress-udp`; runtime-level:
  `cargo test -p eggress-runtime udp`
