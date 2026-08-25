# eggress-metrics — Prometheus Registry and Bridges

Single `MetricsRegistry` owning every metric family; implements
`eggress_server::SessionMetrics` so the server records sessions without knowing
about Prometheus. Bridges live atomic counters from subsystems into the
Prometheus render.

## Structure

One main file `src/lib.rs`:

- **Connection family**: active/total connections, failures,
  `auth_failures_total`, bytes up/down.
- **Routing**: `route_decisions_total{rule,action,...}` labels.
- **Upstreams**: health gauges, open success/failure, unsupported-transport.
- **Reload**: reload total/failures, `config_generation` gauge.
- **UDP**: client-side associations/packets/bytes/drops/target flows,
  upstream-side association metrics, standalone UDP flow metrics.
- **Listeners/platform**: transparent proxy counters, Unix listener counters.
- **Feature-gated** (`extended`): Shadowsocks TCP/UDP session metrics.
- **H2**: pool/connection/stream/goaway/flow-control metrics snapshotted from
  global atomics exported by `eggress-protocol-http`.

## Bridge mechanics

`set_udp_metrics(Arc<UdpMetrics>)`, `set_shadowsocks_metrics(...)`,
`set_transparent_counters(...)`: subsystems increment their own cheap atomics;
`render_prometheus()` computes deltas since the last render and promotes them
into Prometheus counters (gauges set directly). This avoids lock contention in
the hot path and double-counting across scrapes.

## Invariants

- Bounded label cardinality by construction (fixed label sets).
- No secrets in label values (enforced by test).

## Interactions

- Held by runtime/admin; `/metrics` endpoint renders through
  `MetricsRegistry::render_prometheus()`.
- Embed handle's `metrics_text()` uses the same registry without HTTP.

## Review entry points

- ~50 inline tests cover naming stability, bridge deltas, parseability.
- Verify: `cargo test -p eggress-metrics`
