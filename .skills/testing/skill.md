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
Fuzz harnesses live in `fuzz/fuzz_targets/`:
- `uri_parse.rs` — URI parser fuzz target
- `socks5_udp_datagram.rs` — SOCKS5 UDP datagram codec fuzz target

Run with `cargo fuzz run <target>`. Smoke tests in per-crate `tests/` exercise seed inputs
without requiring `cargo-fuzz`:
- `crates/eggress-protocol-socks/tests/fuzz_smoke.rs` — seed corpus for SOCKS codec

### Benchmarks
Criterion benchmarks live in `benches/`:
- `tcp_relay.rs` — TCP relay throughput
- `udp_relay.rs` — UDP relay throughput
- `route_match.rs` — route matching latency

Run with `cargo bench --workspace`.

### Load tests
`#[ignore]`-annotated tests for stress/load scenarios:
- `crates/eggress-runtime/tests/load.rs` — run with `cargo test -p eggress-runtime --test load -- --ignored`

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

### CLI tests
- `crates/eggress-cli/tests/cli_tests.rs` — argument parsing
- `crates/eggress-cli/tests/reply_order.rs` — deferred success reply ordering

## Test utilities (`eggress-testkit`)
- Echo server, half-close server
- Temporary port allocator
- UDP echo server and SOCKS5 UDP test server (`testkit` module in `eggress-udp`)

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

# Fuzz targets (requires cargo-fuzz)
cargo fuzz run uri_parse
cargo fuzz run socks5_udp_datagram

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
