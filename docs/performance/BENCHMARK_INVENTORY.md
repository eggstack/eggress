# Benchmark Inventory

Phase 34 — Performance, Soak, and Regression Gates

## Categories

### Microbenchmarks (Criterion)

| Benchmark | File | Purpose | Duration | Gating tier |
|-----------|------|---------|----------|-------------|
| `tcp_relay` | `benches/tcp_relay.rs` | TCP echo relay throughput (1KB, 64KB payloads) | ~30s | Tier 0 |
| `udp_codec` | `benches/udp_relay.rs` | SOCKS5 UDP datagram encode/decode/roundtrip | ~15s | Tier 0 |
| `route_match` | `benches/route_match.rs` | Route rule matching latency (7 scenarios, 9 rules) | ~20s | Tier 0 |
| `http_connect_upstream` | `benches/http_connect_upstream.rs` | HTTP CONNECT upstream open latency (no auth, basic auth, 407) | ~20s | Tier 0 |

### Performance Smoke Tests (integration)

| Test | File | Purpose | Duration | Gating tier |
|------|------|---------|----------|-------------|
| `performance_tcp_relay_smoke` | `crates/eggress-runtime/tests/performance_smoke.rs` | TCP echo throughput via HTTP/SOCKS5 listeners, upstream chains | ~5s | Tier 1 |
| `performance_udp_relay_smoke` | `crates/eggress-runtime/tests/performance_smoke.rs` | UDP datagram throughput, flow table cleanup verification | ~5s | Tier 1 |
| `resource_leak_fd_cleanup` | `crates/eggress-runtime/tests/performance_smoke.rs` | File descriptor count before/after high-churn TCP sessions | ~5s | Tier 1 |
| `resource_leak_task_cleanup` | `crates/eggress-runtime/tests/performance_smoke.rs` | Task tracker empty after session drain | ~5s | Tier 1 |

### Soak Tests (gated, long-running)

| Test | File | Purpose | Duration | Gating tier |
|------|------|---------|----------|-------------|
| `performance_reverse_soak` | `crates/eggress-runtime/tests/reverse_soak.rs` | Reverse proxy sustained relay, reconnect churn, auth failure churn | 30–120s | Tier 2 |

### Load Tests (gated, long-running)

| Test | File | Purpose | Duration | Gating tier |
|------|------|---------|----------|-------------|
| `load_test_100_concurrent_tcp_sessions` | `crates/eggress-runtime/tests/load.rs` | 100 concurrent SOCKS5 sessions | ~10s | Tier 2 |
| `load_test_udp_associations_up_to_limit` | `crates/eggress-runtime/tests/load.rs` | UDP association limit enforcement | ~5s | Tier 2 |

### Python Binding Overhead

| Test | File | Purpose | Duration | Gating tier |
|------|------|---------|----------|-------------|
| `test_performance_smoke.py` | `python/tests/test_performance_smoke.py` | Import cost, URI translation, config compile, service start/stop overhead | ~10s | Tier 1 |

### pproxy Comparison (gated)

| Script | File | Purpose | Duration | Gating tier |
|--------|------|---------|----------|-------------|
| HTTP CONNECT relay | `scripts/perf/run_pproxy_comparison.sh` | Compare eggress vs pproxy HTTP CONNECT throughput | ~30s | Tier 3 |
| SOCKS5 relay | `scripts/perf/run_pproxy_comparison.sh` | Compare eggress vs pproxy SOCKS5 throughput | ~30s | Tier 3 |

## Gating Tiers

| Tier | When to run | Command | Blocking? |
|------|-------------|---------|-----------|
| Tier 0 | Every `cargo bench` run | `cargo bench --workspace` | No (informational) |
| Tier 1 | Before release | `cargo test -p eggress-runtime --test performance_smoke` | Yes |
| Tier 2 | Manual soak | `EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored` | No (manual) |
| Tier 3 | Cross-platform RC | `EGRESS_REQUIRE_PPROXY_PERF=1 scripts/perf/run_pproxy_comparison.sh` | No (manual) |

## Duration Estimates

- Tier 0: ~90s total (all Criterion benchmarks)
- Tier 1: ~15s total (performance smoke + leak checks)
- Tier 2: 30–120s per soak test
- Tier 3: 30–60s per pproxy comparison
