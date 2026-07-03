# Testing Conventions

## When to use
Use when writing tests, debugging test failures, or understanding test infrastructure.

## Test layers

### Unit tests
In each crate's `src/` files. Test individual functions and types.

### Integration tests
In `crates/eggress-runtime/tests/`:
- `startup.rs` — listener bind, readiness, negative paths
- `routing.rs` — rule matching, fallback, direct routes
- `health.rs` — health state machine, probe reconciliation
- `admin.rs` — admin endpoints, route explanation
- `reload.rs` — config reload behavior
- `shutdown.rs` — graceful drain, force-cancel, admin-during-drain
- `pac_static.rs` — PAC generation, static content, reload freshness
- `udp.rs` — association lifecycle, echo, bind conflict
- `udp_upstream.rs` — SOCKS5 upstream relay, shutdown, metrics
- `upstream_protocols.rs` — HTTP CONNECT, SOCKS4, SOCKS5, and
  unsupported-combo (HTTP/SOCKS4/Shadowsocks/Trojan + UDP) rejection
- `lifecycle_invariants.rs` — runtime lifecycle invariants
- `observability.rs` — metrics, admin, observability correctness
- `security_invariants.rs` — security constraints and invariants
- `load.rs` — `#[ignore]` load/stress tests (run with `-- --ignored`)

### Property tests (proptest)
Round-trip and invariant tests using `proptest`. Lives in per-crate `tests/` directories:
- `crates/eggress-protocol-socks/tests/codec_properties.rs` — SOCKS codec round-trips
- `crates/eggress-protocol-http/tests/connect_properties.rs` — HTTP CONNECT round-trips
- `crates/eggress-protocol-trojan/tests/request_properties.rs` — Trojan request round-trips
- `crates/eggress-routing/tests/properties.rs` — route match consistency

Property tests generate random inputs and assert invariants hold. Use `proptest!` macro
with `#[proptest]` attribute. Strategies should generate valid-but-random protocol inputs.

### Fuzz testing
Fuzz harnesses live in `fuzz/fuzz_targets/` (standalone workspace, libfuzzer-sys based):
- `uri_parse.rs` — URI parser fuzz target
- `socks5_udp_datagram.rs` — SOCKS5 UDP datagram codec fuzz target
- `socks5_handshake.rs` — SOCKS5 method negotiation + CONNECT / UDP_ASSOCIATE request parsers
- `http_connect_response.rs` — HTTP CONNECT status line, authority, header, basic-auth parsers
- `trojan_request.rs` — Trojan password hash + request encoder
- `route_match.rs` — Route matcher evaluation with constructed routers and requests

Run with `cargo fuzz run <target>`. Smoke tests in per-crate `tests/` exercise seed inputs
without requiring `cargo-fuzz`:
- `crates/eggress-protocol-socks/tests/fuzz_smoke.rs` — seed corpus for SOCKS UDP codec and handshake parsers
- `crates/eggress-uri/tests/fuzz_smoke.rs` — seed corpus for URI parser

Fuzz targets can also be smoke-compiled without `cargo-fuzz`:
```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo test --manifest-path fuzz/Cargo.toml --no-run
```

### Benchmarks
Criterion benchmarks live in `benches/`:
- `tcp_relay.rs` — TCP relay throughput
- `udp_relay.rs` — UDP relay throughput
- `route_match.rs` — route matching latency
- `http_connect_upstream.rs` — HTTP CONNECT upstream open latency (no auth, basic auth, 407 rejection)

Run with `cargo bench --workspace`.

### Load tests
`#[ignore]`-annotated tests for stress/load scenarios:
- `crates/eggress-runtime/tests/load.rs` — run with `cargo test -p eggress-runtime --test load -- --ignored`

### Performance smoke tests
Tier 1 performance and leak detection tests (automated, not `#[ignore]`):
- `crates/eggress-runtime/tests/performance_smoke.rs` — TCP/UDP relay smoke, FD leak check, task cleanup
- `python/tests/test_performance_smoke.py` — Python binding overhead, GIL release

### Reverse soak tests
Tier 2 soak tests gated behind `EGRESS_REQUIRE_SOAK=1`:
- `crates/eggress-runtime/tests/reverse_soak.rs` — 30s sustained load, reconnect churn, auth failure churn

### Performance scripts
- `scripts/perf/run_local_baseline.sh` — Tier 1 runner
- `scripts/perf/run_soak.sh` — Tier 2 soak runner (requires EGRESS_REQUIRE_SOAK=1)
- `scripts/perf/run_pproxy_comparison.sh` — Tier 3 pproxy comparison (requires EGRESS_REQUIRE_PPROXY_PERF=1)

### Protocol-crate tests
Protocol-specific tests live alongside the implementation:
- `crates/eggress-protocol-trojan/src/tcp.rs` — hash, `encode_trojan_request()`
  layout (domain/IPv4/IPv6), domain-length validation (1-255), and a synthetic
  TLS happy-path test that calls `trojan_connect()` directly and asserts the
  server-observed request bytes

### UDP-specific tests
- `crates/eggress-udp/tests/socks5_upstream.rs` — upstream relay scenarios
- `crates/eggress-runtime/tests/udp_upstream.rs` — runtime UDP upstream

### Interoperability tests
- `crates/eggress-cli/tests/interoperability_curl.rs` — curl-based
- `crates/eggress-cli/tests/interoperability_pproxy.rs` — pproxy-based

### Differential tests
- `crates/eggress-cli/tests/differential_pproxy.rs` — gated differential tests against pproxy
- `crates/eggress-cli/tests/interoperability_shadowsocks.rs` — gated Shadowsocks interop tests (TCP tests fail due to non-standard framing)

Gated tests require environment variables and external tools. See `docs/DIFFERENTIAL_TESTING.md` for prerequisites, environment variables, and running instructions.

### Differential test harness primitives

The differential test file (`crates/eggress-cli/tests/differential_pproxy.rs`) provides reusable primitives:
- `ProcessGuard` / `TaskGuard` — Drop-based cleanup for child processes and tokio tasks
- `start_tcp_echo()` / `start_udp_echo()` — Echo server fixtures
- `start_eggress_from_toml(config_str)` — Start eggress from TOML config
- `compare_tcp_echo()` / `compare_udp_echo()` — Payload comparison helpers
- `assert_coarse_failure_equivalence()` — Assert both succeeded or both failed
- `socks5_udp_associate()` — SOCKS5 handshake + UDP ASSOCIATE helper
- `build_socks5_udp_packet()` / `recv_udp_response()` — UDP datagram helpers

Black-box probe tests document pproxy behavior for ambiguous scenarios (refused replies, auth success shape, chained failure, UDP relay lifetime).

### CLI tests
- `crates/eggress-cli/tests/cli_tests.rs` — argument parsing
- `crates/eggress-cli/tests/cli_exit_codes.rs` — structured exit code verification
- `crates/eggress-cli/tests/pproxy_run_process.rs` — pproxy run subprocess lifecycle
- `crates/eggress-cli/tests/pproxy_translation_golden.rs` — pproxy URI → TOML golden tests
- `crates/eggress-cli/tests/reply_order.rs` — deferred success reply ordering

## Test utilities (`eggress-testkit`)
- Echo server, half-close server
- Temporary port allocator
- UDP echo server and SOCKS5 UDP test server (`testkit` module in `eggress-udp`)
- pproxy oracle runner (`pproxy_oracle` module) — start/supervise real pproxy processes
- eggress runner (`eggress_runner` module) — start eggress from TOML or CLI args
- Fixture servers (`fixtures` module) — TCP/UDP echo, HTTP origin, HTTP CONNECT upstream, SOCKS4/5 upstream, TLS echo
- Differential case model (`case_model` module) — `PproxyCase`, `CaseOutcome`, comparison helpers
- Parity report generator (`report` module) — JSON and markdown reports from manifest + test results

## Running tests
```bash
# Full suite
cargo test --workspace

# Specific subsystem
cargo test -p eggress-runtime udp
cargo test -p eggress-udp socks5_upstream

# Property tests
cargo test -p eggress-protocol-socks --test codec_properties
cargo test -p eggress-routing --test properties

# Fuzz smoke tests
cargo test -p eggress-protocol-socks --test fuzz_smoke

# Benchmarks
cargo bench --workspace

# Load tests (ignored by default)
cargo test -p eggress-runtime --test load -- --ignored

# SSR/legacy rejection tests
cargo test -p eggress-protocol-shadowsocks legacy
cargo test -p eggress-pproxy-compat ssr

# Gated differential/interop tests (requires external tools)
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored

# pproxy oracle tests (Phase 18, requires pproxy==2.7.9)
cargo test -p eggress-testkit pproxy_oracle -- --ignored

# Fuzz targets (requires cargo-fuzz)
cargo fuzz run uri_parse
cargo fuzz run socks5_udp_datagram
cargo fuzz run socks5_handshake
cargo fuzz run http_connect_response
cargo fuzz run trojan_request
cargo fuzz run route_match

# With output
cargo test --workspace -- --nocapture
```

## Writing new tests
- Use `#[tokio::test]` for async tests
- Use the testkit for server/client fixtures
- Use `tempfile` for config files
- Prefer integration tests over unit tests for behavioral coverage
- Test both success and failure paths
- Test negative paths (bind conflict, invalid config, oversized identity)
- For property tests: use `proptest!` macro, define strategies for valid inputs,
  assert round-trip or invariant properties
- For fuzz targets: add seed inputs to `fuzz_smoke.rs` tests for CI coverage
- For load tests: annotate with `#[ignore]` and document the scenario

## Embed API tests

The `eggress-embed` crate has integration tests in `crates/eggress-embed/tests/`:

- `start_stop.rs` — blocking/async start and shutdown, multiple listeners, config errors
- `proxy_traffic.rs` — SOCKS5 TCP echo through embed API, port-0 discovery
- `reload.rs` — reload generation increment, invalid config, bind change rejection
- `metrics_status.rs` — Prometheus counters, status fields, metrics after session
- `error_redaction.rs` — no credentials in error messages, error categories

Run: `cargo test -p eggress-embed`

Tests use local TCP echo servers (no public internet required).

## pproxy compatibility harness (Phase 18)

Compatibility evidence is tracked in `tests/compat/pproxy_manifest.toml`. Each feature
has an evidence level: `unimplemented`, `implemented_synthetic`, `implemented_differential`,
`implemented_interop`, `compatible`, or `intentional_non_parity`.

Only `compatible` or `implemented_interop` evidence levels support compatibility claims.
`implemented_synthetic` means tested without real pproxy.

### pproxy compat unit tests
- `crates/eggress-pproxy-compat/src/tests.rs` — protocol aliases, diagnostics, credential redaction
- Diagnostics tests: `cargo test -p eggress-pproxy-compat diagnostics`
- Exit codes tests: `cargo test -p eggress-pproxy-compat exit_codes`

### Fixtures
- `tests/compat/fixtures/pproxy_uri_corpus.toml` — canonical pproxy URI input corpus
- `tests/compat/fixtures/pproxy_cli_cases/*.toml` — per-case CLI translation golden files

### Subprocess testing patterns
- `pproxy_run_process.rs` spawns eggress as a child process via `Command::new("cargo")` with `run --bin eggress`
- Use `assert_cmd` or raw `std::process::Command` with timeout-based assertions
- Capture stdout/stderr for exit code and output validation
- Clean up child processes via `Drop` guards or explicit `kill()`

### Manifest validation (Phase 24)

Manifest validation enforces:
- `egress_status = "compatible"` requires `evidence_level = "compatible"`
- Compatible entries with differential tests (`differential_*`) require `external_dependency`
- `implemented_interop` requires `external_dependency` or `divergence` explaining interop
- `implemented_synthetic` cannot pair with `compatible` status
- `intentional_non_parity` requires non-empty `divergence`

The `last_updated` field was removed in Phase 24; stale warnings are no longer emitted.

Run the oracle harness:
```bash
cargo test -p eggress-testkit pproxy_oracle -- --ignored
```

Parity reports are generated at:
- `target/compat/pproxy-parity-report.json`
- `target/compat/pproxy-parity-report.md`
