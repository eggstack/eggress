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
cargo audit
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
cargo test -p eggress-protocol-trojan
cargo test -p eggress-transport-tls

# Shadowsocks protocol tests
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-runtime shadowsocks_tcp
cargo test -p eggress-runtime shadowsocks_udp

# Reverse proxy tests
cargo test -p eggress-protocol-reverse
cargo test -p eggress-runtime reverse
cargo test -p eggress-runtime --test reverse_soak
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored

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
| `shadowsocks_tcp.rs` | Shadowsocks TCP relay through full stack |
| `shadowsocks_udp.rs` | Shadowsocks UDP relay through full stack |
| `reverse_interop.rs` | Reverse/backward proxy self-interop and credential redaction (3 un-gated tests) |
| `reverse_runtime.rs` | Runtime reverse server/client lifecycle, supervisor spawning, metrics (13 tests) |
| `reverse_soak.rs` | Soak tests: performance, reconnect churn, auth failure churn (3 gated tests, requires `EGRESS_REQUIRE_SOAK=1`) |

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

Gated reverse interop tests (`EGRESS_REQUIRE_REVERSE_INTEROP=1`) test eggress↔pproxy wire compatibility for the reverse/backward proxy protocol.

### pproxy Compatibility Harness (Phase 18)

The compatibility harness launches real pproxy processes and compares behavior
with eggress. It requires `pproxy==2.7.9` to be installed.

Run with:
```bash
cargo test -p eggress-testkit pproxy_oracle -- --ignored
```

The harness uses:
- `tests/compat/pproxy_target.toml` — pinned pproxy version and timeouts
- `tests/compat/pproxy_manifest.toml` — feature evidence index
- `eggress-testkit` — oracle runner, fixtures, case model, report generator

Parity reports are generated at:
- `target/compat/pproxy-parity-report.json`

### Scenario-Driven Oracle Harness (Track A.01)

The oracle harness provides scenario-driven comparison of eggress vs pproxy
under equivalent conditions. 31 scenarios across 5 categories: CLI/defaults,
HTTP/SOCKS TCP, Chains, Rules, UDP.

Gate: `EGRESS_ORACLE=1`

```bash
# Run all oracle scenarios
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle -- --ignored

# Run a specific scenario
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle oracle_tcp_socks5_connect -- --ignored --nocapture

# Generate a JSON comparison report
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle oracle_generate_report -- --ignored
```

Always-run unit tests (no gate needed):
```bash
cargo test -p eggress-cli --test oracle
cargo test -p eggress-testkit oracle
```

Scenario registry and report types live in `eggress_testkit::oracle`.

### pproxy Chain Tests (Phase 39)

Chain-related tests cover URI parsing and translation of `__`-separated multi-hop chains:

- `crates/eggress-pproxy-compat/src/uri.rs` — URI chain parsing: `__` separator, semicolon/comma rejection, per-hop protocol validation (14 tests)
- `crates/eggress-pproxy-compat/src/translate.rs` — Chain translation: multi-hop TOML generation, unsupported protocol diagnostics per hop (8 tests)

Run with:
```bash
cargo test -p eggress-pproxy-compat
```
- `target/compat/pproxy-parity-report.md`

## Fuzz Smoke Testing

Fuzz targets live in `fuzz/` (standalone workspace, `libfuzzer-sys` based):

| Target | What it fuzzes |
|--------|----------------|
| `fuzz_targets/uri_parse.rs` | URI parser with arbitrary input |
| `fuzz_targets/socks5_udp_datagram.rs` | SOCKS5 UDP datagram codec with arbitrary input |
| `fuzz_targets/socks5_handshake.rs` | SOCKS5 method negotiation + CONNECT/UDP_ASSOCIATE request parsers |
| `fuzz_targets/http_connect_response.rs` | HTTP CONNECT status line, authority, header, basic-auth parsers |
| `fuzz_targets/trojan_request.rs` | Trojan password hash + request encoder across IP / domain targets |
| `fuzz_targets/route_match.rs` | Route matcher evaluation with constructed routers and requests |

Run fuzz smoke testing (requires `cargo-fuzz`):

```bash
cargo fuzz run uri_parse -- -runs=1000
cargo fuzz run socks5_udp_datagram -- -runs=1000
cargo fuzz run socks5_handshake -- -runs=1000
cargo fuzz run http_connect_response -- -runs=1000
cargo fuzz run trojan_request -- -runs=1000
cargo fuzz run route_match -- -runs=1000
```

Fuzz targets can also be smoke-compiled without `cargo-fuzz`:

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo test --manifest-path fuzz/Cargo.toml --no-run
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
| `http_connect_upstream` | HTTP CONNECT upstream open latency (no auth, basic auth, 407 rejection) |

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
