# eggress-udp

`crates/eggress-udp/src/`

UDP association management, relay, upstream forwarding, and standalone relay modes.

## Module Map

| Module | Role |
|---|---|
| `lib.rs` | Crate root; re-exports `flow::*` and `udp_capability::*`; defines `UdpMode` enum |
| `assoc.rs` | `UdpAssociation` and `UdpAssociationMeta`: ownership, lifecycle, client pinning |
| `registry.rs` | `UdpAssociationRegistry`: bounded global/per-listener association tracking |
| `relay.rs` | `udp_relay_loop`: main SOCKS5-UDP-ASSOCIATE relay path; `RelayCleanupGuard` |
| `standalone.rs` | `standalone_udp_relay`: pproxy-compatible standalone UDP relay |
| `standalone_shadowsocks.rs` | `shadowsocks_standalone_udp_relay`: standalone AEAD-encrypted UDP relay (feature-gated) |
| `flow.rs` | `UdpFlowKey`, `UdpFlowKind`, `TargetFlowEntry`, `ClientFlowState`; per-target flow management |
| `direct.rs` | `UdpTargetFlow`: connected UDP socket per target for direct forwarding |
| `hop.rs` | `UdpHopStack`, `UdpHop`: transport-neutral encode/decode for nested datagrams |
| `composed.rs` | `open_composed_udp_upstream`: multi-hop SOCKS5+Shadowsocks composition |
| `upstream_socks5.rs` | `open_socks5_udp_upstream`: TCP control + SOCKS5 handshake + UDP ASSOCIATE client |
| `udp_capability.rs` | `udp_capability`, `UdpRelayCapability`: chain classification for UDP support |
| `codec.rs` | `decode_packet`: size-gated wrapper around `socks5::udp_codec` |
| `security.rs` | `validate_target`, `validate_standalone_target`, `validate_datagram_size` |
| `limits.rs` | `UdpLimits`: all configurable bounds |
| `metrics.rs` | `UdpMetrics`: atomic counters for relay, standalone, and upstream paths |
| `error.rs` | `UdpError`: structured error enum |
| `testkit.rs` | `start_udp_echo_server`, `Socks5UdpTestServer`: integration test helpers |

## UdpMode Enum (`lib.rs:23-31`)

| Variant | Description |
|---|---|
| `Socks5UdpAssociate` | SOCKS5 UDP ASSOCIATE from inbound SOCKS5 clients (default) |
| `StandalonePproxyUdp` | Standalone pproxy-compatible UDP relay (SOCKS5-framed, no TCP control) |
| `ShadowsocksUdp` | Standalone AEAD-encrypted Shadowsocks UDP relay |
| `Echo` | Explicit UDP echo listener |
| `FixedTarget` | Explicit fixed-target UDP listener |

## Association Lifecycle

### Create (`registry.rs:26-56`)

`UdpAssociationRegistry::create_association()` acquires write lock, checks `max_associations_global` then `max_associations_per_listener`, generates monotonic ID via `AtomicU64`, inserts `Arc<UdpAssociation>`, returns clone.

`UdpAssociation` (`assoc.rs:25-31`): `id`, `meta` (listener, TCP peer, identity, generation, creation time, `Mutex<Instant>` last_activity, `Mutex<Option<SocketAddr>>` pinned client), `state: AtomicBool`, `cancel: CancellationToken`, `closed_notify: Notify`.

### Relay (`relay.rs:569-705`)

`udp_relay_loop` runs as a tracked task in `tokio::select!`:

- `idle_tick`: checks `last_activity().elapsed() >= idle_timeout`; breaks on expiry, records `association_timeouts`.
- `target_cleanup_tick`: `reap_idle_flows()` (`relay.rs:47-70`) evicts entries exceeding `target_idle_timeout`, aborts recv tasks, cancels SOCKS5 control connections.
- `relay_socket.recv_from()`: `pin_client_addr()` -> `touch()` -> `handle_client_datagram()`.
- `response_rx.recv()`: `encode_socks5_udp_datagram()` -> `relay_socket.send_to(client_addr)`.
- `cancel.cancelled()`: break.

`handle_client_datagram` (`relay.rs:72-567`) per-packet:
1. `decode_packet()` (SOCKS5 UDP codec + size check).
2. `validate_standalone_target()` (security gate).
3. `routing.route(&RouteRequest { transport: TransportKind::Udp, ... })`.
4. `SelectedRoute::Direct`: create/reuse `UdpTargetFlow`, send.
5. `SelectedRoute::Upstream`: classify via `udp_capability(&chain)`:
   - `SupportedSocks5`: create/reuse `Socks5UdpTargetFlow`.
   - `SupportedShadowsocks` (feature-gated): create/reuse `ShadowsocksUdpTargetFlow`.
   - `SupportedComposed`: create/reuse `ComposedUdpTargetFlow`.
   - `UnsupportedProtocol` / `UnsupportedMultiHop`: drop + `record_dropped()`.
6. `RouteError::Rejected`: drop + `record_dropped()`.

Activity touch points: valid client datagrams (`relay.rs:634`), occupied flow entry reuse (`relay.rs:152`). Rejected packets do NOT touch.

### Close / Every Removal Path

1. **Normal exit** (idle timeout or cancel): loop breaks -> abort recv tasks -> cancel SOCKS5 controls -> `association.close()` -> `cleanup_guard.disarm()` -> `registry.remove(id).await`.
2. **Abort/panic**: `RelayCleanupGuard` (`relay.rs:714-748`) `Drop` fires: `record_association_closed()`, `association.close()`, spawns async `registry.remove()` or falls back to `try_remove_now()`.
3. **TCP control close**: stream EOF -> cancel token -> breaks relay loop.
4. **Runtime shutdown**: `registry.close_all()` drains all, `udp_tasks.close()` prevents spawns, grace timeout waits.

Every path removes from registry, ensuring `active_count()` returns to zero.

## Datagram Path Walkthrough

### Direct

```
Client -> recv_from() -> pin/touch -> decode_packet() -> validate_standalone_target()
  -> routing.route() -> SelectedRoute::Direct
  -> UdpTargetFlow::new(target, "127.0.0.1:0") [bind+connect+DNS]
  -> flow.send(payload) [socket.send()]
  -> recv task: socket.recv() -> try_send(ResponseMsg)
  -> response_rx -> encode_socks5_udp_datagram() -> send_to(client_addr)
```

### Upstream SOCKS5 Single-Hop

```
-> udp_capability(&chain) == SupportedSocks5
-> open_socks5_udp_upstream() [upstream_socks5.rs:94-172]:
     TCP connect -> SOCKS5 method+auth -> UDP ASSOCIATE cmd
     -> bind UDP "127.0.0.1:0" -> spawn control keepalive (300s read timeout)
-> Socks5UdpTargetFlow::send():
     encode_socks5_udp_datagram(target, payload) -> udp_socket.send_to(relay_addr)
-> recv task: decode_socks5_udp_datagram() -> verify target -> try_send()
```

### Composed Multi-Hop

```
open_composed_udp_upstream() [composed.rs:33-124]:
  UdpHopStack::from_chain() -> validate all hops SOCKS5/Shadowsocks
  for each hop (last to first):
    SOCKS5: open_socks5_udp_upstream() -> relay_addr, control_task
    Shadowsocks: resolve endpoint -> bind UDP (outermost only)
  return ComposedUdpTargetFlow { socket, stack, relay_targets, outer_relay_addr }

Encoding (destination inward, hop.rs:135-154):
  hop[N-1].encode(target, payload) -> hop[N-2].encode(relay, frame) -> ... -> hop[0].encode(relay, frame)

Decoding (outer first, hop.rs:158-171):
  hop[0].decode(packet) -> hop[1].decode(inner) -> ... -> (target, payload)
```

## Routing Integration

Every datagram routed via `RouteService::route()` (full selection, not just `decide()`):
- `SelectedRoute::Direct { reason: Normal/DirectFallback }` -- forward direct.
- `SelectedRoute::Upstream { upstream, chain, pending_lease }` -- classify and forward or reject.
- `RouteError::Rejected` -- drop.
- Route rules match `transport = "udp"` (`MatchExpr::Transport(TransportKind::Udp)`).
- Reload: existing flows keep selected upstream until idle; new flows use latest snapshot; rule changes take effect next datagram.

## Security Model

### Client Pinning (`assoc.rs:85-99`)

First valid packet pins `SocketAddr`; mismatches return `ClientAddressMismatch`. Rejected packets do NOT touch (do not extend lifetime). Controlled by `client_pin` (default: `true`).

### Target Validation (`security.rs`)

`validate_standalone_target(target, allow_private_egress)` rejects: multicast, broadcast, unspecified, port zero, loopback (when !allow_private_egress), RFC 1918/link-local/unique-local IPv6 (when !allow_private_egress). IPv4-mapped IPv6 checked via `to_ipv4_mapped()`.

`validate_datagram_size(size, max_size)` is a standalone helper; `decode_packet()` enforces size limit at codec level.

### Bounds

| Limit | Default |
|---|---|
| `max_associations_global` | 1024 |
| `max_associations_per_listener` | 256 |
| `max_targets_per_association` | 64 |
| `max_datagram_size` | 65535 |
| `idle_timeout` | 60s |
| `target_idle_timeout` | 30s |
| `max_standalone_flows` | 0 (falls back to `max_associations_global`) |

## Unsupported Chains Policy

`udp_capability()` (`udp_capability.rs:40-93`):

| Chain | Result |
|---|---|
| Empty hops | `UnsupportedProtocol { "direct" }` |
| Single SOCKS5 | `SupportedSocks5` |
| Single Shadowsocks (with creds) | `SupportedShadowsocks { method, password }` |
| Single non-UDP protocol | `UnsupportedProtocol { name }` |
| Multi-hop all SOCKS5/Shadowsocks | `SupportedComposed` |
| Multi-hop any non-UDP | `UnsupportedMultiHop` |

Unsupported chains are explicitly rejected with metrics. No silent fallback to direct -- fallback is routing-engine policy.

## Metrics Bridge

`UdpMetrics` (`metrics.rs:4-38`) atomic counters:

| Group | Key Counters |
|---|---|
| Association | `associations_active/total`, `association_timeouts`, `association_failures` |
| Datagram | `packets_up/down`, `bytes_up/down` |
| Drop | `dropped_packets`, `dropped_encode_errors`, `dropped_send_errors`, `dropped_response_channel_full` |
| Target flow | `target_flows_active/total`, `decode_errors` |
| Upstream | `upstream_associations_active/total`, `upstream_packets_up/down`, `upstream_failures`, `unsupported_upstream_total` |
| Standalone | `standalone_flows_active/total`, `standalone_packets_in/out`, `standalone_malformed/rejected_datagrams`, `standalone_flow_reaps` |

Bridged via `MetricsRegistry::set_udp_metrics()` (`eggress-metrics/src/lib.rs:893`); `/-/udp` admin endpoint exposes active/flow gauges.

## Test Coverage

### Unit Tests (203, `cargo test -p eggress-udp --lib`)

`assoc.rs`: create, close, idempotency, touch, pin. `registry.rs`: create/get/remove, limits, close_all, slot reuse. `relay.rs`: echo, pin reject, route reject, metrics, cancel, idle timeout, flow create/reuse/eviction, registry cleanup, double-close, composed/Shadowsocks upstream. `standalone.rs`+`standalone_shadowsocks.rs`: echo, reject, metrics, flow reuse, limits, timeout, malformed, decode error, wrong password. `flow.rs`: address equivalence (IPv4/v6/domain/mapped), endpoint resolution. `direct.rs`: echo, metrics, encode format. `hop.rs`: nested encode/decode, non-UDP rejection. `codec.rs`: size limits. `security.rs`: all validation paths. `udp_capability.rs`: all chain combinations. `upstream_socks5.rs`: wire format, credential limits, error labels. `metrics.rs`: all counters.

### Crate Integration Tests (`crates/eggress-udp/tests/`)

`udp_integration.rs`, `socks5_upstream.rs`, `standalone_udp.rs`.

### Runtime Tests (`crates/eggress-runtime/tests/`)

`udp.rs` (26 tests): full runtime SOCKS5 UDP ASSOCIATE lifecycle, standalone modes, registry cleanup, metrics, advertise IP. `udp_upstream.rs` (10 tests): upstream SOCKS5 flow, shutdown drain, composed chain, target idle timeout.

## Reviewer Gotchas

1. **`validate_target` vs `validate_standalone_target`**: `validate_target` is stricter (always rejects loopback). Relay/standalone paths use `validate_standalone_target` which respects `allow_private_egress`.
2. **Duplicate helpers**: `socks_to_target_addr`, `target_to_socks_addr`, `socks_addr_equivalent` are duplicated across `relay.rs`, `flow.rs`, `standalone.rs`.
3. **`RelayCleanupGuard`** prevents permanent slot exhaustion from aborted/panicked relay tasks.
4. **Response channel bounded at 256**: overflow drops silently with `record_dropped_response_channel_full()`.
5. **Standalone has no client pinning**: any client address can send to any target.
6. **Flow key includes upstream_id**: different upstreams for same target get separate flows.
7. **SOCKS5 control keepalive** reads 1 byte with 300s timeout; upstream close tears down flow.
8. **`max_standalone_flows = 0`** falls back to `max_associations_global` (`flow.rs:271-277`).
9. **Feature-gated Shadowsocks**: `standalone_shadowsocks.rs`, `UdpHop::Shadowsocks`, `ShadowsocksUdpTargetFlow` behind `#[cfg(feature = "shadowsocks")]`.
10. **`touch()` after pin check**: decode error or security reject after successful pin does NOT extend lifetime.

## See Also

- [overview.md](overview.md) -- system context
- [routing.md](routing.md) -- `RouteService`, `TransportKind::Udp`
- [runtime.md](runtime.md) -- supervisor lifecycle, task tracking
- [protocols-socks.md](protocols-socks.md) -- SOCKS5 UDP datagram codec
- [protocols-shadowsocks.md](protocols-shadowsocks.md) -- Shadowsocks AEAD UDP
- [metrics.md](metrics.md) -- Prometheus bridge
- [admin.md](admin.md) -- `/-/udp` endpoint
