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
- `upstream_protocols.rs` — HTTP CONNECT, SOCKS4, SOCKS5, stream-native WS/Raw/H2 chain tests, and
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
- `crates/eggress-cli/tests/differential_pproxy.rs` — gated differential tests against pproxy (28 scenarios, `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`)
- `crates/eggress-cli/tests/pproxy_differential.rs` — Phase 41 reusable differential parity harness (18 scenarios, `EGRESS_RUN_PPROXY_DIFFERENTIAL=1`)
- `crates/eggress-cli/tests/interoperability_shadowsocks.rs` — gated Shadowsocks interop tests (TCP tests fail due to non-standard framing)
- `crates/eggress-cli/tests/oracle.rs` — scenario-driven oracle harness (31 scenarios, `EGRESS_ORACLE=1`)

Gated tests require environment variables and external tools. See `docs/DIFFERENTIAL_TESTING.md` for prerequisites, environment variables, and running instructions.

### Differential test harness

The reusable harness lives in `eggress_testkit::differential` (protocol-agnostic) and provides:
- `differential_gate_enabled()` / `require_differential_gate()` — Gate check via `EGGRESS_RUN_PPROXY_DIFFERENTIAL=1`
- `find_python_binary()` — Auto-detects Python with pproxy (3.11/3.12/3.13)
- `start_pproxy_server()` / `start_pproxy_server_with_auth()` / `start_pproxy_with_args()` — pproxy process management
- `ProcessGuard` — RAII cleanup for child processes
- `wait_for_port()` / `assert_port_ready()` — Readiness checks
- `read_with_timeout()` — Timeout-based TCP read (avoids half-close issues)
- `start_udp_echo()` — UDP echo server
- `build_socks5_udp_packet()` / `extract_udp_payload()` / `recv_udp_response()` — UDP helpers
- `compare_tcp_echo()` / `compare_udp_echo()` / `assert_coarse_failure_equivalence()` — Comparison primitives

Protocol-specific helpers (SOCKS5/HTTP/SOCKS4 client helpers, eggress server helpers) live in the test file since they depend on `eggress-core` types.
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
- Oracle harness (`oracle` module) — scenario registry, JSON report generation, gate functions (`EGRESS_ORACLE`)

### Oracle infrastructure (Phase A3)

The oracle harness under `eggress-testkit/src/oracle/` provides:

- **`mod.rs`** — Module root, gate checks (`EGRESS_ORACLE`, `EGRESS_ORACLE_EXTENDED`, `EGRESS_ORACLE_PLATFORM`, `EGRESS_ORACLE_PRIVILEGED`), timeout constants
- **`scenario.rs`** — 31 hardcoded scenarios (backward compat, no TOML files needed)
- **`schema.rs`** — TOML scenario schema (version 1), loader, validator. Maps scenarios to A2 composition IDs
- **`observations.rs`** — `ProxyObservation` semantic capture model: bound addresses, exit codes, connection results, protocol replies, bytes transferred, auth results, timing, cleanup status. `compare_observations()` produces structured comparison results
- **`probes.rs`** — Reusable protocol client probes: `socks5_tcp_connect`, `socks5_tcp_connect_auth`, `socks5_connect_refused`, `socks5_auth_failure`, `http_connect`, `http_connect_refused`, `http_forward_get`, `http_forward_post`, `socks4_connect`, `socks4a_connect`. Each returns `ProbeResult`
- **`supervisor.rs`** — `SupervisedProcess` with process-group ownership (Unix), bounded stdout/stderr capture, artifact retention (logs saved on drop), `ReadinessProbe` enum (TcpPort, StdoutPattern, FixedDelay, FileExists), structured `ProcessExit`
- **`ci.rs`** — 5-tier CI organization: FastStructural, CoreDifferential, ExtendedDifferential, PlatformDifferential, PrivilegedExternal. Each tier has its own gate env var
- **`report.rs`** — JSON and Markdown report generation with manifest consistency checks and CI tier filtering

TOML scenario files live under `crates/eggress-testkit/tests/oracle/scenarios/`. Schema validation tests run without pproxy:

```bash
cargo test -p eggress-testkit --test oracle_scenario_files
cargo test -p eggress-testkit --lib oracle
```

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
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored

# Scenario-driven oracle harness (gated, requires pproxy==2.7.9)
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle -- --ignored

# Python tests
python -m pytest python/tests/test_pproxy_dropin.py -v
python -m pytest python/tests/test_pproxy_differential.py -v
python -m pytest python/tests/test_pproxy_compat.py -v
python -m pytest python/tests/test_pproxy_redaction.py -v
python -m pytest python/tests/test_pproxy_concurrency.py -v
python -m pytest python/tests/test_performance_smoke.py -v
python -m pytest python/tests/test_protocol_cipher.py -v
python -m pytest python/tests -v  # all Python tests

# pproxy oracle tests (Phase 18, requires pproxy==2.7.9)
cargo test -p eggress-testkit pproxy_oracle -- --ignored

# Parity manifest validation (Phase 37)
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml

# Regenerate the parity report from the manifest (Phase 42+, frozen Phase 51)
python3 scripts/validate_pproxy_parity_manifest.py --write-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml

# Verify the parity report is consistent with the manifest (Phase 42+, CI runs this)
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml

# Composition matrix validation (Phase A2)
cargo test -p eggress-testkit composition
python3 scripts/validate_pproxy_parity_manifest.py --check-matrix docs/parity/composition_matrix.toml docs/parity/pproxy_capability_manifest.toml

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

## Python tests

Python tests exercise the PyO3 bindings and pproxy compatibility layer:

- `python/tests/test_pproxy_dropin.py` — Phase 40 PPProxyService, CompatibilityReport, start_pproxy tests
- `python/tests/test_pproxy_differential.py` — Phase 41 differential parity structural tests (gated)
- `python/tests/test_pproxy_compat.py` — pproxy translation helpers
- `python/tests/test_pproxy_redaction.py` — credential redaction in repr/diagnostics
- `python/tests/test_pproxy_concurrency.py` — concurrent start/shutdown safety
- `python/tests/test_performance_smoke.py` — Python binding overhead, GIL release
- `python/tests/test_wheel_import_smoke.py` — wheel import verification
- `python/tests/test_proxy_connection.py` — native sync/async outbound stream behavior and no temporary listener regression
- `python/tests/test_connection.py` — Connection contract and lifecycle tests (signatures, attributes, state machine, close semantics, resource ownership, context manager, GIL release)
- `python/tests/test_connection_behavioral.py` — Connection behavioral tests (SOCKS5 proxy echo, multiple protocols, failure scenarios, concurrent lifecycle, resource cleanup, GIL release)
- `python/tests/test_server_lifecycle.py` — Server lifecycle tests (Phase C3: 84 tests covering construction, start/stop, async, context managers, observability, reload, error tracking, resource management, concurrent sessions, thread safety, multi-server coexistence, TLS, auth, chains, UDP, IPv6, loop affinity, GIL release, FD leak detection, pproxy examples)
- `python/tests/test_protocol_cipher.py` — Phase C4 protocol objects, cipher objects, and plugin bridge tests
- `python/tests/test_asyncio_semantic.py` — Phase C5 asyncio semantic compatibility (107 tests: loop affinity including cross-loop detection, bridge lifecycle, cancellation (cancel_wait_closed, cancel during aclose, concurrent bridge.cancel, plugin callback cancellation), close ordering, contextvars, exception chaining, asyncio debug mode with real async operations, interpreter safety (repeated asyncio.run cycles, GC, context managers), version compat, stress/race (AsyncConnection, Server, CloseWaiter, PluginBridge), representative pproxy async patterns, manifest/doc agreement)

The development dependency set includes `pytest-asyncio`; install it before
running the async plugin and asyncio-semantic tests.

Run:
```bash
python -m pytest python/tests -v
```

For installed-wheel verification, use `--import-mode=importlib` and a clean
environment so source-tree imports cannot mask missing native extension or
packaging errors:

```bash
python -m pytest --import-mode=importlib python/tests -q
python -m pip wheel --no-deps --wheel-dir /tmp/eggress-pproxy-compat-wheel ./python-pproxy-compat
python -m pytest --import-mode=importlib python/tests/test_proxy_connection.py python/tests/test_wheel_import_smoke.py -q
```

The separate compatibility wheel must also be smoke-tested in an isolated
environment with `import eggress` and `import pproxy`; do not report an
upstream pproxy differential suite as passing when its dependency is absent.
Use `python3 scripts/release_evidence.py` to retain redacted metadata,
scenario results, wheel hashes, and `SHA256SUMS`.

## pproxy compatibility harness (Phase 18)

Compatibility evidence is tracked in `tests/compat/pproxy_manifest.toml`. Each feature
has an evidence level: `unimplemented`, `implemented_synthetic`, `implemented_differential`,
`implemented_interop`, `compatible`, or `intentional_non_parity`.

Only `compatible` or `implemented_interop` evidence levels support compatibility claims.
`implemented_synthetic` means tested without real pproxy.

### pproxy compat unit tests
- `crates/eggress-pproxy-compat/src/tests.rs` — protocol aliases, diagnostics, credential redaction
- `crates/eggress-pproxy-compat/src/uri.rs` — URI chain parsing (`__` separators, semicolon/comma rejection, per-hop validation) — 14 tests
- `crates/eggress-pproxy-compat/src/translate.rs` — chain translation (multi-hop TOML generation, unsupported protocol diagnostics) — 8 tests
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

### Manifest validation (Phase 36)

Phase 36 tightened the manifest validator. New invariants:

- `category` must be one of the 15 allowed values (enumerated in
  `manifest::ALLOWED_CATEGORIES`).
- `intentional_non_parity` status must pair with `intentional_non_parity` or
  `implemented_synthetic` evidence (not `unimplemented`).
- `unsupported` and `experimental` statuses require non-empty `divergence`.
- `platform` category entries must mention a platform keyword in
  `divergence` (Linux, macOS, Windows, FreeBSD, Unix, Solaris, BSD, Android, iOS).
- `tests` entries must not be bare file paths or CI workflow references.
  Use either a group alias (e.g. `cli_tests`, `integration_tests`,
  `deny_audit_gate`, `python_wheel_ci_workflow`) or `file::test_function`
  form.

CLI tier tightened: 17 CLI entries that previously claimed `compatible` with
synthetic evidence are now `supported` with `implemented_synthetic` evidence.

Run the full manifest validation suite:
```bash
cargo test -p eggress-testkit --lib manifest
```

Validate the pproxy parity capability manifest (Phase 37):
```bash
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
```

Regenerate / verify the parity report from the manifest (Phase 42+, frozen Phase 51):
```bash
python3 scripts/validate_pproxy_parity_manifest.py --write-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

Generate the final parity release report JSON:
```bash
python3 scripts/phase36_report.py
# writes target/compat/final-pproxy-parity-report.json
```

The full Phase 36 release audit is gated and requires Python 3.11 (pproxy
2.7.9 is incompatible with Python 3.14). See
`docs/release/PARITY_RELEASE_GO_NO_GO.md` for the gating rationale.
- `target/compat/pproxy-parity-report.md`

## Phase C1: Python API Contract Tests

The pproxy API contract is a machine-readable inventory of every public symbol
in pproxy 2.7.9, with signatures, class hierarchies, async classifications,
and behavioral probes.

### Contract files

| File | Purpose |
|------|---------|
| `python/compat/pproxy_api_contract.json` | Generated API contract (105 symbols) |
| `python/compat/classification.json` | Tier classification for each symbol |
| `python/compat/behavioral_probe_results.json` | Dynamic behavior probe results |
| `python/compat/extract_api.py` | Contract extractor script |
| `python/compat/behavioral_probes.py` | Behavioral probe runner |
| `python/compat/classification.py` | Classification mapper |
| `tests/compat/test_pproxy_api_contract.py` | 56 contract validation tests |

### Running

```bash
# Regenerate contract (requires pproxy==2.7.9 installed)
python3.11 python/compat/extract_api.py

# Run behavioral probes
python3.11 python/compat/behavioral_probes.py

# Run classification
python3.11 python/compat/classification.py

# Run contract validation tests
python3.11 -m pytest tests/compat/test_pproxy_api_contract.py -v
```

### Classification tiers

- `exact_target`: must match directly
- `adapted_target`: same use case via compatibility wrapper
- `unsupported_release_blocker`: required for drop-in parity
- `intentional_non_parity`: with explicit rationale
- `internal_observed`: publicly reachable but not stable API
