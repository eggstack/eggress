# Metrics

Eggress exposes Prometheus-compatible metrics via the admin `/metrics` endpoint.
The metric registry is implemented in `crates/eggress-metrics/src/lib.rs`.

## Metric Names

All metrics are prefixed with `eggress_`.

### Connection Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `eggress_connections_active` | Gauge | Currently active connections |
| `eggress_connections_total` | Counter | Total connections handled |
| `eggress_connection_failures_total` | Counter | Total failed connections |
| `eggress_bytes_upstream_total` | Counter | Total bytes sent upstream |
| `eggress_bytes_downstream_total` | Counter | Total bytes sent downstream |

### Routing Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `eggress_route_decisions_total` | Counter | `rule`, `action`, `outcome` | Route decisions by rule, action, and outcome |
| `eggress_upstream_health` | Gauge | `upstream_id`, `group_id` | Upstream health status (1=healthy, 0=unhealthy) |

### Reload Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `eggress_reload_total` | Counter | Total config reload attempts |
| `eggress_reload_failures_total` | Counter | Total failed config reloads |
| `eggress_config_generation` | Gauge | Current config generation number |

### Upstream Connection Metrics

These metrics are recorded by the TCP chain executor at the upstream-open
boundary. Protocol crates remain metrics-free; the `SessionMetrics` trait
bridges the call into `MetricsRegistry`.

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `eggress_upstream_open_total` | Counter | `protocol`, `outcome` | Upstream connection attempts by protocol and outcome |
| `eggress_upstream_open_failures_total` | Counter | `protocol`, `reason` | Upstream connection failures by protocol and reason |
| `eggress_unsupported_transport_total` | Counter | `protocol`, `transport`, `reason` | Unsupported transport attempts |

### UDP Metrics (Client-Facing)

| Metric | Type | Description |
|--------|------|-------------|
| `eggress_udp_associations_active` | Gauge | Currently active UDP associations |
| `eggress_udp_associations_total` | Counter | Total UDP associations created |
| `eggress_udp_association_failures_total` | Counter | Total UDP association creation failures |
| `eggress_udp_packets_up_total` | Counter | Total UDP packets received from clients |
| `eggress_udp_packets_down_total` | Counter | Total UDP packets sent to clients |
| `eggress_udp_bytes_up_total` | Counter | Total UDP bytes received from clients |
| `eggress_udp_bytes_down_total` | Counter | Total UDP bytes sent to clients |
| `eggress_udp_dropped_packets_total` | Counter | Total UDP packets dropped |
| `eggress_udp_target_flows_active` | Gauge | Currently active UDP target flows |
| `eggress_udp_target_flows_total` | Counter | Total UDP target flows created |
| `eggress_udp_decode_errors_total` | Counter | Total UDP datagram decode errors (label: `kind`) |
| `eggress_udp_unsupported_upstream_total` | Counter | UDP packets routed to unsupported upstream groups |

### UDP Upstream Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `eggress_udp_upstream_associations_active` | Gauge | Currently active UDP upstream associations |
| `eggress_udp_upstream_associations_total` | Counter | Total UDP upstream associations created |
| `eggress_udp_upstream_packets_up_total` | Counter | Total UDP packets sent upstream |
| `eggress_udp_upstream_packets_down_total` | Counter | Total UDP packets received from upstream |
| `eggress_udp_upstream_bytes_up_total` | Counter | Total UDP bytes sent upstream |
| `eggress_udp_upstream_bytes_down_total` | Counter | Total UDP bytes received from upstream |
| `eggress_udp_upstream_failures_total` | Counter | Total UDP upstream failures |

**Shadowsocks UDP upstream traffic** increments the UDP upstream metrics above.
Shadowsocks TCP upstream traffic increments the TCP upstream connection metrics
(`eggress_upstream_open_total` with `protocol="shadowsocks"`) instead. The two
transport modes are distinguished by which metric family receives the counters.

### Standalone UDP Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `eggress_standalone_udp_flows_active` | Gauge | Currently active standalone UDP flows |
| `eggress_standalone_udp_flows_total` | Counter | Total standalone UDP flows created |
| `eggress_standalone_udp_packets_in_total` | Counter | Total standalone UDP packets received |
| `eggress_standalone_udp_packets_out_total` | Counter | Total standalone UDP packets sent |
| `eggress_standalone_udp_bytes_in_total` | Counter | Total standalone UDP bytes received |
| `eggress_standalone_udp_bytes_out_total` | Counter | Total standalone UDP bytes sent |
| `eggress_standalone_udp_malformed_total` | Counter | Total malformed standalone UDP datagrams |
| `eggress_standalone_udp_rejected_total` | Counter | Total rejected standalone UDP datagrams |
| `eggress_standalone_udp_flow_reaps_total` | Counter | Total standalone UDP flow reaps |

### Shadowsocks Metrics

Shadowsocks-specific counters and gauges for TCP/UDP AEAD lifecycle, decrypt
failures, and key derivation. Populated whenever the runtime supervisor
threads `ShadowsocksMetrics` through the server wiring.

| Metric | Type | Description |
|--------|------|-------------|
| `eggress_shadowsocks_tcp_upstream_sessions_total` | Counter | TCP sessions opened against an external Shadowsocks upstream |
| `eggress_shadowsocks_tcp_inbound_sessions_total` | Counter | TCP sessions accepted as a Shadowsocks inbound listener |
| `eggress_shadowsocks_tcp_sessions_active` | Gauge | Currently active TCP sessions (inbound + upstream) |
| `eggress_shadowsocks_tcp_flow_open_total` | Counter | TCP flow opens (one per upstream connect and inbound accept) |
| `eggress_shadowsocks_tcp_flow_close_total` | Counter | TCP flow closes |
| `eggress_shadowsocks_tcp_active_flows` | Gauge | Currently open TCP flows |
| `eggress_shadowsocks_tcp_decrypt_failures_total` | Counter | TCP AEAD decryption failures (wrong key, tampered ciphertext, etc.) |
| `eggress_shadowsocks_tcp_frame_parse_failures_total` | Counter | TCP frame structure failures (bad plaintext length) |
| `eggress_shadowsocks_tcp_unsupported_method_reject_total` | Counter | TCP sessions rejected due to unknown cipher method |
| `eggress_shadowsocks_tcp_session_closed_total` | Counter | TCP sessions closed (matches `tcp_inbound_sessions_total` over time) |
| `eggress_shadowsocks_udp_packets_in_total` | Counter | UDP datagrams successfully decoded |
| `eggress_shadowsocks_udp_packets_out_total` | Counter | UDP datagrams successfully encoded and sent |
| `eggress_shadowsocks_udp_active_flows` | Gauge | Currently active UDP flows |
| `eggress_shadowsocks_udp_decrypt_failure_total` | Counter | UDP AEAD decryption failures |

### Advanced Transport Metrics

| Family | Name | Type | Description |
|--------|------|------|-------------|
| H2 CONNECT | `eggress_h2_streams_total` | counter | Total H2 CONNECT streams accepted |
| H2 CONNECT | `eggress_h2_streams_active` | gauge | Currently active H2 streams |
| H2 CONNECT | `eggress_h2_stream_errors_total` | counter | H2 stream errors (RST_STREAM, etc.) |
| WebSocket | `eggress_websocket_sessions_total` | counter | Total WebSocket tunnel sessions |
| WebSocket | `eggress_websocket_sessions_active` | gauge | Currently active WebSocket sessions |
| WebSocket | `eggress_websocket_frame_errors_total` | counter | WebSocket frame decode errors |
| Raw Tunnel | `eggress_raw_tunnel_sessions_total` | counter | Total raw tunnel sessions |
| Raw Tunnel | `eggress_raw_tunnel_sessions_active` | gauge | Currently active raw tunnel sessions |
| TLS/ALPN | `eggress_tls_alpn_negotiation_failures_total` | counter | TLS ALPN negotiation failures |

## Labels and Cardinality Policy

### What IS labeled

- **Route decisions**: `rule` (config-defined rule ID), `action` (direct/reject/upstream_group), `outcome` (ok/rejected/error)
- **Upstream health**: `upstream_id` (config-defined upstream ID), `group_id` (config-defined group ID)
- **Upstream opens**: `protocol` (http/socks4/socks5/shadowsocks/trojan), `outcome` (success/failure)
- **Upstream failures**: `protocol`, `reason` (dns_resolution/tls_handshake/connection_refused/etc.)
- **Unsupported transport**: `protocol`, `transport` (udp/quic), `reason`
- **UDP decode errors**: `kind` (too_short/etc.)
- **Upstream failure reasons**: `reason` (bounded set: `dns_resolution`, `tls_handshake`, `connection_refused`, `connection_timeout`, `auth_failed`, `io_error`, `protocol_error`, `policy_denied`, `other`)

All label values come from **configuration-defined names** (rule IDs, upstream IDs, group IDs) or **protocol constants** (protocol names). None are derived from network input.

### What is NOT labeled (and why)

| Omitted | Reason |
|---------|--------|
| Client IP addresses | High cardinality; privacy risk; no cardinality bound |
| Target hostnames | Unbounded cardinality from network input |
| Target ports | Unbounded cardinality |
| Usernames/credentials | Security: never expose auth data in metrics |
| Listener names in per-connection metrics | Config-derived but adds cardinality with no diagnostic value |
| Request URIs / paths | Unbounded cardinality; HTTP-specific |

The `SessionReport` struct captures `protocol`, `target`, and `route` for logging, but these are **not** written to metric labels — they appear only in structured log output.

### Cardinality bounds

- Route labels: bounded by number of configured rules (typically <100)
- Upstream labels: bounded by configured upstreams × groups (typically <50)
- Protocol labels: bounded by supported protocol count (5 protocols)
- Decode error kinds: bounded by error variant count

## Example Output

```
# HELP eggress_connections_active Currently active connections.
# TYPE eggress_connections_active gauge
eggress_connections_active 0
# HELP eggress_connections_total Total connections handled.
# TYPE eggress_connections_total counter
eggress_connections_total 142
# HELP eggress_connection_failures_total Total failed connections.
# TYPE eggress_connection_failures_total counter
eggress_connection_failures_total 3
# HELP eggress_bytes_upstream_total Total bytes sent upstream.
# TYPE eggress_bytes_upstream_total counter
eggress_bytes_upstream_total 58291
# HELP eggress_bytes_downstream_total Total bytes sent downstream.
# TYPE eggress_bytes_downstream_total counter
eggress_bytes_downstream_total 204831
# HELP eggress_route_decisions_total Route decisions by rule, action, outcome.
# TYPE eggress_route_decisions_total counter
eggress_route_decisions_total{rule="allow-all",action="Direct",outcome="ok"} 139
# HELP eggress_upstream_health Upstream health status.
# TYPE eggress_upstream_health gauge
eggress_upstream_health{upstream_id="proxy1",group_id="egress"} 1
# HELP eggress_config_generation Current config generation number.
# TYPE eggress_config_generation gauge
eggress_config_generation 1
# HELP eggress_reload_total Total config reload attempts.
# TYPE eggress_reload_total counter
eggress_reload_total 1
# HELP eggress_udp_associations_active Currently active UDP associations.
# TYPE eggress_udp_associations_active gauge
eggress_udp_associations_active 0
# HELP eggress_upstream_open_total Total upstream connection attempts.
# TYPE eggress_upstream_open_total counter
eggress_upstream_open_total{protocol="socks5",outcome="success"} 12
# HELP eggress_upstream_open_failures_total Total upstream connection failures.
# TYPE eggress_upstream_open_failures_total counter
eggress_upstream_open_failures_total{protocol="socks5",reason="connection_refused"} 2
```

## Bridged UDP Metrics

The `MetricsRegistry` bridges a shared `UdpMetrics` instance (`crates/eggress-udp/src/metrics.rs`) so that live relay counters appear in the same `/metrics` endpoint. Bridge synchronization uses delta-tracking: each render computes the difference since the last render and increments Prometheus counters by that delta, preventing double-counting.

## Security Invariants

- Metric names are stable (verified by `metric_names_are_stable` test)
- Prometheus output is parseable (verified by `prometheus_output_is_parseable` test)
- Labels never contain secrets (verified by `labels_no_secrets` test)
- No IP addresses appear in metrics output (verified by `bridge_no_privacy_leak` test)
