# eggress-metrics

`crates/eggress-metrics/`

Prometheus-compatible metrics registry using `prometheus-client`. Bridges metrics from the server, UDP, Shadowsocks, and HTTP subsystems.

## Key Types

| Type | Description |
|---|---|
| `MetricsRegistry` | Central Prometheus registry with session, upstream, and protocol metrics |

## Metric Categories

### Session Metrics

| Metric | Type | Labels |
|---|---|---|
| `eggress_sessions_total` | Counter | protocol, outcome, route |
| `eggress_session_duration_seconds` | Histogram | protocol |
| `eggress_session_bytes_up_total` | Counter | protocol |
| `eggress_session_bytes_down_total` | Counter | protocol |

### Route Decision Metrics

| Metric | Type | Labels |
|---|---|---|
| `eggress_route_decisions_total` | Counter | decision (direct/upstream/reject), rule_id |

### Upstream Metrics

| Metric | Type | Labels |
|---|---|---|
| `eggress_upstream_health` | Gauge | upstream_id, state |
| `eggress_upstream_connections_active` | Gauge | upstream_id |
| `eggress_upstream_connections_total` | Counter | upstream_id |

### Config Metrics

| Metric | Type | Labels |
|---|---|---|
| `eggress_config_generation` | Gauge | generation number |
| `eggress_reload_total` | Counter | outcome (success/failure) |

### UDP Metrics (bridged from `eggress-udp`)

| Metric | Type | Labels |
|---|---|---|
| `eggress_udp_associations_active` | Gauge | listener |
| `eggress_udp_packets_up_total` | Counter | listener |
| `eggress_udp_packets_down_total` | Counter | listener |

### Protocol Metrics (bridged)

- Shadowsocks cipher metrics
- HTTP H2 connection pool metrics

## Label Cardinality

All label values are bounded. Route labels use rule IDs (not arbitrary strings). Upstream labels use upstream IDs.

## Dependencies

- `eggress-server` — `SessionMetrics` trait implementation
- `eggress-udp` — UDP metrics bridging
- `eggress-protocol-shadowsocks` — Shadowsocks metrics bridging
- `eggress-protocol-http` — H2 metrics bridging

See [overview.md](overview.md) for context.
