# Testing

## Normal Local Checks

Run these before any commit. See `AGENTS.md` for the canonical list.

```bash
# Format
cargo fmt --all

# Check formatting
cargo fmt --all -- --check

# Compile check
cargo check --workspace

# Run all tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Security audit
cargo deny check
```

## Focused Test Commands

Per-crate test suites from `AGENTS.md`:

```bash
# UDP-focused tests
cargo test -p eggress-udp
cargo test -p eggress-runtime udp
cargo test -p eggress-config udp

# UDP upstream tests
cargo test -p eggress-udp socks5_upstream
cargo test -p eggress-runtime udp_upstream

# Protocol-specific tests
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-protocol-trojan
cargo test -p eggress-transport-tls

# Upstream protocol tests
cargo test -p eggress-runtime upstream_protocols

# CLI
cargo run --bin eggress -- --help
cargo run --bin eggress -- -l http://:8080
cargo run --bin eggress -- --config path/to/config.toml
```

## Integration Tests

Integration tests live in `crates/eggress-runtime/tests/` and exercise the supervisor end-to-end:

| Test File | Coverage |
|-----------|----------|
| `startup.rs` | Basic supervisor startup and listener binding |
| `routing.rs` | Rule evaluation and route selection |
| `health.rs` | Health probe lifecycle |
| `admin.rs` | Admin endpoint responses |
| `reload.rs` | Config reload success, rejection, failure |
| `shutdown.rs` | Graceful shutdown and drain |
| `pac_static.rs` | PAC file and static content serving |
| `udp.rs` | UDP association lifecycle, TCP control close, echo relay, bind conflict, topology rejection, config reload |
| `udp_upstream.rs` | SOCKS5 upstream relay through full stack |
| `upstream_protocols.rs` | HTTP, SOCKS4, SOCKS5, and unsupported-combo rejection |

Negative-path tests cover: bind conflict, invalid source, oversized identity, reload-time failure.

## Property Tests

Property-based testing with `proptest` is enabled for codec and parser crates:

| Crate | What is tested |
|-------|----------------|
| `eggress-uri` | URI parsing roundtrips |
| `eggress-protocol-socks` | SOCKS4/4a and SOCKS5 codec encode/decode |
| `eggress-protocol-http` | HTTP CONNECT message parsing |
| `eggress-protocol-trojan` | Trojan password/header parsing |
| `eggress-routing` | Rule matching, CIDR parsing, regex compilation |

Run property tests with:

```bash
cargo test -p eggress-uri proptest
cargo test -p eggress-protocol-socks proptest
cargo test -p eggress-protocol-http proptest
cargo test -p eggress-protocol-trojan proptest
cargo test -p eggress-routing proptest
```

## Gated Interoperability Tests

Differential interoperability tests compare eggress behavior against Python `pproxy`. All tests are `#[ignore]` and require:

1. Environment variable: `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`
2. Python 3 with `pproxy` installed (`pip install pproxy`)

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

Tests cover: HTTP CONNECT, SOCKS4, SOCKS5, multi-hop chains, authentication, and binary payload transfer.

## Fuzz Smoke Testing

Fuzz targets live in `fuzz/`:

| Target | What it fuzzes |
|--------|----------------|
| `fuzz_targets/uri_parse.rs` | URI parser with arbitrary input |
| `fuzz_targets/socks5_udp_datagram.rs` | SOCKS5 UDP datagram codec with arbitrary input |

Run fuzz smoke testing:

```bash
cargo fuzz run uri_parse
cargo fuzz run socks5_udp_datagram
```

## Ignored Load Tests

Some tests are marked `#[ignore]` for load or long-running scenarios. Run with:

```bash
cargo test -- --ignored
```

## Benchmark Commands

Benchmarks use `criterion` with HTML reports:

```bash
cargo bench
```

| Benchmark | What it measures |
|-----------|------------------|
| `tcp_relay` | TCP bidirectional relay throughput |
| `udp_relay` | UDP datagram relay throughput |
| `route_match` | Routing rule matching latency |

## Security Invariant Tests

The following tests verify security invariants:

| Test | Crate | What it verifies |
|------|-------|------------------|
| `metric_names_are_stable` | `eggress-metrics` | All metric names appear in output (no accidental removal) |
| `labels_no_secrets` | `eggress-metrics` | No password/secret/token in metric output |
| `bridge_no_privacy_leak` | `eggress-metrics` | No IP addresses in metrics output |
| `prometheus_output_is_parseable` | `eggress-metrics` | Every non-comment line has valid numeric value |
| `test_redacted_display` | `eggress-uri` | `RedactedUri` replaces credentials with `****:****@` |
| `validate_credentials` | `eggress-protocol-http` | Control characters rejected in HTTP auth |

## Observability Tests

| Test | Crate | What it verifies |
|------|-------|------------------|
| `counter_increments` | `eggress-metrics` | Route decision counter increments correctly |
| `gauge_returns_to_zero` | `eggress-metrics` | Upstream health gauge returns to 0 |
| `session_recording_updates_all_metrics` | `eggress-metrics` | Session lifecycle updates all relevant metrics |
| `udp_association_metrics` | `eggress-metrics` | UDP association creation/closure tracked |
| `udp_packet_metrics` | `eggress-metrics` | UDP packet/byte counters update |
| `bridge_*` | `eggress-metrics` | Bridged UdpMetrics appear in Prometheus output |
| `health_returns_200` | `eggress-admin` | `/-/health` returns 200 |
| `ready_returns_200` / `ready_returns_503_when_not_ready` | `eggress-admin` | `/-/ready` reflects readiness state |
| `metrics_returns_prometheus_format` | `eggress-admin` | `/metrics` returns valid Prometheus text |
| `pac_endpoint_returns_pac_when_configured` | `eggress-admin` | PAC file served at configured path |
