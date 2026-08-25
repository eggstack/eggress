# eggress-metrics -- Prometheus Registry and Bridges

Single `MetricsRegistry` owning every metric family; implements
`eggress_server::SessionMetrics` so the server records sessions without knowing
about Prometheus. Bridges live atomic counters from subsystems (UDP relay,
Shadowsocks, H2, transparent proxy) into the Prometheus render using
delta-promotion to avoid double-counting.

## Module map

Single file `src/lib.rs` (approx 2800 lines including tests).

| Component | Role |
|---|---|
| `MetricsRegistry` | Owns all Prometheus `Counter`/`Gauge`/`Family` fields, plus `Mutex<Option<...>>` bridged-snapshot slots and prev-value tracking for delta math |
| `SessionMetrics` impl | Bridges `eggress_server::SessionMetrics` trait to `MetricsRegistry` methods |
| `render_prometheus()` | Delta-promotes all bridged counters, then encodes to Prometheus text format |
| Label structs | `RouteLabels`, `UpstreamLabels`, `DecodeErrorLabels`, `UpstreamOpenLabels`, `UpstreamFailureLabels`, `UnsupportedTransportLabels`, `H2ConnectionLabels`, `H2StreamLabels` |
| `H2MetricsSnapshot` | Returned by `h2_snapshot()` for programmatic H2 metric access |

## Public API surface

### SessionMetrics trait implementation

The `MetricsRegistry` implements `eggress_server::SessionMetrics` (`src/lib.rs:15-63`):

| Trait method | Behavior |
|---|---|
| `record_session(report)` | Increments `connections_total`, conditionally increments `connection_failures` (for `ClientProtocolError`, `AuthenticationFailed`, `HandshakeTimedOut`, `RouteFailed`, `RelayFailed`), adds bytes, decrements `connections_active` |
| `record_session_start()` | Increments `connections_active` |
| `record_route_decision(rule, action, outcome)` | Increments `route_decisions` family |
| `record_upstream_open(protocol, outcome)` | Increments `upstream_open_total` family |
| `record_upstream_failure(protocol, reason)` | Increments `upstream_open_failures_total` family |
| `record_auth_failure()` | Increments `auth_failures` |
| `record_reload(success)` | Increments `reload_total`; if `!success`, also `reload_failures` |
| `set_config_generation(gen)` | Sets `config_generation` gauge (capped at `i64::MAX`) |
| `record_udp_association_created()` | Increments `udp_associations_total` |
| `render_prometheus()` | Delta-promotes bridges, encodes, returns `String` |

### Bridge setup methods

| Method | Source | Bridged fields |
|---|---|---|
| `set_udp_metrics(Arc<UdpMetrics>)` | `eggress_udp::metrics::UdpMetrics` | Associations, packets, bytes, drops, target flows, decode errors, upstream metrics, standalone flow metrics (25 counter/gauge fields) |
| `set_shadowsocks_metrics(Arc<ShadowsocksMetrics>)` | `eggress_protocol_shadowsocks::ShadowsocksMetrics` (feature = `extended`) | TCP sessions/upstream/decrypt/frame-parse/unsupported, UDP packets/bytes/decrypt/unsupported, active flows (12 fields) |
| `set_transparent_counters(Arc<AtomicU64>, Arc<AtomicU64>)` | Supervisor state atomics | `transparent_connections_accepted`, `transparent_original_dst_failed` |

### Direct recording methods

UDP: `record_udp_association_created` / `_closed` / `_failure`, `record_udp_packet_up(bytes)` / `_down(bytes)`, `record_udp_dropped()`, `record_udp_decode_error(kind)`, `record_udp_target_flow_created` / `_closed`, `record_udp_upstream_association_created` / `_closed` / `_failure`, `record_udp_upstream_packet_up(bytes)` / `_down(bytes)`.

Platform: `record_transparent_connection_accepted` / `_original_dst_failed` / `_route_reject`, `record_unix_listener_connection_accepted` / `_bind_failure`, `record_platform_capability_check_failure`.

H2: `record_h2_connection_opened` / `_closed`, `record_h2_stream_opened(id, outcome)` / `_closed`, `record_h2_goaway` / `_handshake_failure` / `_auth_failure` / `_flow_control_stall` / `_pool_exhausted`, `record_h2_bytes_relayed(bytes)`.

Query: `h2_snapshot() -> H2MetricsSnapshot`.

## How it works

### Metric families by subsystem

| Subsystem | Families | Type | Labels |
|---|---|---|---|
| **Connections** | `eggress_connections_active`, `eggress_connections_total`, `eggress_connection_failures_total`, `eggress_auth_failures_total`, `eggress_bytes_upstream_total`, `eggress_bytes_downstream_total` | Gauge/Counter | -- |
| **Routing** | `eggress_route_decisions_total` | Counter | `{rule, action, outcome}` |
| **Upstreams** | `eggress_upstream_health`, `eggress_upstream_open_total`, `eggress_upstream_open_failures_total`, `eggress_unsupported_transport_total` | Gauge/Counter | `{upstream_id, group_id}` / `{protocol, outcome}` / `{protocol, reason}` / `{protocol, transport, reason}` |
| **Reload** | `eggress_reload_total`, `eggress_reload_failures_total`, `eggress_config_generation` | Counter/Gauge | -- |
| **UDP client** | `eggress_udp_associations_{active,total}`, `eggress_udp_association_{failures,timeouts}_total`, `eggress_udp_packets_{up,down}_total`, `eggress_udp_bytes_{up,down}_total`, `eggress_udp_dropped_{packets,encode_errors,send_errors,response_channel_full}_total`, `eggress_udp_target_flows_{active,total}`, `eggress_udp_decode_errors_total{kind}`, `eggress_udp_unsupported_upstream_total` | Gauge/Counter | `{kind}` on decode errors |
| **UDP upstream** | `eggress_udp_upstream_associations_{active,total}`, `eggress_udp_upstream_packets_{up,down}_total`, `eggress_udp_upstream_bytes_{up,down}_total`, `eggress_udp_upstream_failures_total` | Gauge/Counter | -- |
| **UDP standalone** | `eggress_standalone_udp_flows_{active,total}`, `eggress_standalone_udp_packets_{in,out}_total`, `eggress_standalone_udp_bytes_{in,out}_total`, `eggress_standalone_udp_{malformed,rejected}_total`, `eggress_standalone_udp_flow_reaps_total` | Gauge/Counter | -- |
| **Platform** | `eggress_transparent_connections_accepted_total`, `eggress_transparent_original_dst_failed_total`, `eggress_transparent_route_rejects_total`, `eggress_unix_listener_connections_accepted_total`, `eggress_unix_listener_bind_failures_total`, `eggress_platform_capability_check_failures_total` | Counter | -- |
| **Shadowsocks** (extended) | `eggress_shadowsocks_tcp_{sessions_active,sessions_total,upstream_sessions_total,decrypt_failures_total,frame_parse_failures_total,unsupported_method_rejects_total,active_flows}`, `eggress_shadowsocks_udp_{packets_in_total,packets_out_total,bytes_in_total,bytes_out_total,decrypt_failures_total,unsupported_method_rejects_total,active_flows}` | Gauge/Counter | -- |
| **H2** | `eggress_h2_connections_{active,total}`, `eggress_h2_streams_{active,total}`, `eggress_h2_{goaway,handshake_failures,auth_failures,flow_control_stalls,pool_exhausted,bytes_relayed}_total` | Gauge/Counter | `{upstream_id, outcome}` on streams_total |

All families except `eggress_udp_decode_errors_total`, `eggress_upstream_health`, and `eggress_h2_streams_total` have no labels. All bridged sources appear under the "Bridged" column -- their counters are promoted from `AtomicU64` values via delta math; direct methods write Prometheus counters/gauges atomically.

### Bridge delta-promotion mechanics

Bridged subsystems (UDP, Shadowsocks, transparent proxy) use their own `AtomicU64` counters for hot-path recording. The `render_prometheus()` method (`src/lib.rs:1084-1608`) promotes these into Prometheus counters:

1. **Lock the snapshot mutex** (poison-tolerant via `unwrap_or_else(|e| e.into_inner())`).
2. **For each counter field**: `cur = source.load(Relaxed)`, `delta = cur.saturating_sub(prev)`, `if delta > 0 { prometheus_counter.inc_by(delta) }`, `prev = cur`.
3. **For gauges** (active counts): `prometheus_gauge.set(current_value)` directly -- these are current-state, not cumulative.

This pattern guarantees that each scrape increments Prometheus counters by exactly the amount since the last scrape, avoiding double-counting across scrapes.

### H2 bridge from global atomics

H2 metrics originate from `H2_PROTOCOL_METRICS` (`eggress-protocol-http/src/h2_connect.rs:65-66`), a `Lazy<Arc<H2ProtocolMetrics>>` with 10 `AtomicU64` fields (`connections_opened`, `connections_closed`, `streams_opened`, `streams_closed`, `goaway_received`, `handshake_failures`, `auth_failures`, `flow_control_stalls`, `pool_exhausted`, `bytes_relayed`). `render_prometheus()` reads these and applies delta-promotion (`src/lib.rs:1437-1568`). Active counts are `opened - closed`.

### Transparent proxy and decode error bridges

`set_transparent_counters()` (`src/lib.rs:1700-1709`) stores two `Arc<AtomicU64>` from the supervisor. `render_prometheus()` delta-promotes them (`src/lib.rs:1570-1604`).

Bridged UDP decode errors are incremented per `kind` label AND aggregated as `kind="total"` during delta promotion (`src/lib.rs:1203-1213`). Direct `record_udp_decode_error(kind)` calls also create per-kind series.

## Error and failure model

- Mutex poisoning is handled gracefully (`unwrap_or_else(|e| e.into_inner())`) on every `lock()` call. A poisoned mutex from a panic in a bridge setup path will not block `render_prometheus()`.
- `saturating_sub` prevents underflow if a source counter is reset or wraps (should not happen with `AtomicU64`, but defensive).
- `config_generation` is capped at `i64::MAX` before casting (`src/lib.rs:1074`).
- `encode()` is infallible for `String` output (`src/lib.rs:1607`).

## Configuration and features

- The `extended` feature gates `ShadowsocksMetrics` bridging and the `ss_*` metric fields. Without it, all Shadowsocks metrics are compiled out and never appear in Prometheus output.
- No runtime configuration is needed; all metrics are registered in `MetricsRegistry::new()`.

## Security notes

- **No secrets in labels** (enforced by test `labels_no_secrets` at `src/lib.rs:1946-1966`).
- **No IP addresses in bridged metrics** (enforced by test `bridge_no_privacy_leak` at `src/lib.rs:2494-2505`).
- Label cardinality is bounded by construction: `RouteLabels` is bounded by rule count x action x outcome; `UpstreamLabels` by upstream count; `DecodeErrorLabels` by decode error kind set; H2 labels by upstream ID and stream outcome.

## Concurrency and lifecycle

- `MetricsRegistry` is designed for concurrent access. All Prometheus `Counter`/`Gauge` types are internally atomic.
- Bridge slots (`bridged_udp_metrics`, `bridged_shadowsocks_metrics`, transparent counters, H2 prev-values) use `Mutex` with poisoned-lock recovery. Lock contention is minimal because `render_prometheus()` is the sole consumer and is typically called periodically (scrape interval).
- `UdpMetrics` and `ShadowsocksMetrics` use `AtomicU64` counters with `Ordering::Relaxed` on the hot path (relay forwarding), avoiding any lock in the data plane.
- `H2_PROTOCOL_METRICS` is a global `Lazy<Arc<...>>` -- no registration required; the protocol layer writes atomics directly.
- `MetricsRegistry` is typically held by the runtime and admin. The embed handle's `metrics_text()` uses the same registry without HTTP.

## Test coverage map

| Test | What it covers |
|---|---|
| `metric_names_are_stable` (src/lib.rs:1849-1922) | All 70+ Prometheus metric names are asserted present in output -- name stability regression guard |
| `counter_increments` | `record_route_decision` increments correctly |
| `gauge_returns_to_zero` | `set_upstream_health` gauge toggles between 1 and 0 |
| `labels_no_secrets` | Session reports with target data do not leak passwords/secrets/tokens into output |
| `prometheus_output_is_parseable` | Every non-comment line has >= 2 whitespace-separated parts with a numeric last token |
| `session_recording_updates_all_metrics` | `record_session_start` + `record_session` updates active/total/bytes |
| `session_failure_increments_failures` | `SessionOutcome::RouteFailed` increments `connection_failures_total` |
| `reload_success_and_failure` | Reload counters track success/failure correctly |
| `bridge_delta_tracking_across_renders` (src/lib.rs:2462-2492) | First render: no deltas. Second render after recording: deltas appear. Third render with no activity: counters stay at previous value (no double-count) |
| `bridge_*_appear_in_prometheus` (20+ tests) | Each bridged counter family (packets, bytes, drops, decode errors, target flows, upstream, standalone flows, malformed, rejected, reaps) appears in Prometheus output |
| `bridge_active_*_gauge_returns_to_zero` (4 tests) | Gauges for associations, target flows, standalone flows all return to 0 after create+close pairs |
| `transparent_proxy_*` | Transparent proxy counters, bridged counters |
| `h2_protocol_metrics_appear_in_prometheus` (src/lib.rs:2767-2824) | H2 global atomics are promoted into Prometheus output |
| `bridge_no_privacy_leak` | No IP addresses (127.0.0.1, 192.168) in bridged output |
| `upstream_open_metric_records_by_protocol_and_outcome` | Family labels are correctly separated |
| `new_metrics_parseable` | All output lines remain parseable after recording upstream/transport events |

Verify: `cargo test -p eggress-metrics`

## Reviewer gotchas

1. **Delta promotion is NOT lock-free.** The `Mutex` on bridge snapshots is held during `render_prometheus()`. If scrapes are frequent or the snapshot is large, this could become a contention point. In practice, scrape intervals (15-60s) make this negligible.
2. **H2 active counts are derived, not bridged.** `h2_connections_active = connections_opened - connections_closed`. If an `opened` atomic is incremented but the render happens before `closed` is incremented, the active count is correct for that scrape but the `total` counter may lag by one delta.
3. **`record_session` decrements `connections_active`.** This means `connections_active` is incremented by `record_session_start` and decremented by `record_session`. If `record_session` is never called (crash before session end), `connections_active` leaks upward until restart.
4. **`saturating_sub` can mask counter resets.** If a `AtomicU64` counter is reset to 0 (not expected in production), `saturating_sub` produces 0 delta instead of panicking. The Prometheus counter will appear to stall rather than underflow.
5. **Shadowsocks metrics are feature-gated.** Without `extended`, the `ss_*` family is never registered. PromQL queries targeting these metrics will 404 unless the build includes the feature.
6. **`decode_errors` is both per-kind and aggregated.** The bridge increments a `kind="total"` label during delta promotion (`src/lib.rs:1207-1211`) AND direct `record_udp_decode_error(kind)` calls create per-kind series. Both appear in Prometheus output; the `total` is the sum of all kinds seen since last render.
7. **H2 stream labels use a fixed `upstream_id: "h2"`.** The bridge does not carry per-upstream identity because `H2_PROTOCOL_METRICS` is a global singleton, not per-connection.
8. **Transparent proxy has both direct and bridged paths.** `record_transparent_connection_accepted` directly increments the counter. `set_transparent_counters` bridges supervisor atomics. Both paths feed the same Prometheus counter; calling both would double-count.

## See also

- [routing.md](routing.md) -- rule evaluation, schedulers, health, lease lifecycle
- [../docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) -- system-wide architecture
- [../docs/CI_STATUS.md](../docs/CI_STATUS.md) -- CI and verification policy
