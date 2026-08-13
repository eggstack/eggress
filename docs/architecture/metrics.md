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

### Connection Lifecycle

Session accounting is structurally balanced: `record_session_start()` increments `connections_active`, and exactly one `record_session()` call decrements it before the connection handler returns. This covers successful sessions, authentication failures, protocol errors, and handshake timeouts equally. Auth failures additionally increment a specialized `auth_failures` counter.

The structural balance is pinned by two complementary regression layers:

- a trait-boundary test using a `RecordingMetrics` test double in
  `crates/eggress-server/src/lib.rs` proves one `record_session_start()`
  is followed by exactly one `record_session()` across success,
  authentication failure, malformed protocol, handshake timeout, and
  route failure paths;
- a concrete `MetricsRegistry` regression in
  `crates/eggress-runtime/tests/observability.rs` exercises a real
  connection through the existing runtime path and asserts that after
  the failed handshake terminates the actual Prometheus output shows
  `eggress_connections_active == 0`,
  `eggress_connections_total == 1`, and
  `eggress_connection_failures_total == 1`.

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
