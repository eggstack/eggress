# AGENTS.md

## Build and Test Commands

```bash
# Check workspace compiles
cargo check --workspace

# Run all tests
cargo test --workspace

# Format code
cargo fmt --all

# Check formatting
cargo fmt --all -- --check

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Security audit
cargo deny check
cargo audit

# Run UDP-focused tests
cargo test -p eggress-udp
cargo test -p eggress-runtime udp
cargo test -p eggress-config udp

# Run standalone UDP tests
cargo test -p eggress-udp standalone
cargo test -p eggress-runtime standalone_udp

# Run UDP upstream tests
cargo test -p eggress-udp socks5_upstream
cargo test -p eggress-runtime udp_upstream

# Run Shadowsocks tests
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-runtime shadowsocks_tcp
cargo test -p eggress-runtime shadowsocks_udp

# Run SSR/legacy rejection tests
cargo test -p eggress-protocol-shadowsocks legacy
cargo test -p eggress-pproxy-compat ssr

# Run Trojan tests
cargo test -p eggress-protocol-trojan

# Run TLS transport tests
cargo test -p eggress-transport-tls

# Run upstream protocol tests
cargo test -p eggress-runtime upstream_protocols

# Run H2 CONNECT tests
cargo test -p eggress-protocol-http h2

# Run H2 upstream integration tests
cargo test -p eggress-runtime --test upstream_protocols h2

# Run WebSocket tunnel tests
cargo test -p eggress-protocol-websocket

# Run raw tunnel tests
cargo test -p eggress-protocol-raw

# Run reverse protocol tests
cargo test -p eggress-protocol-reverse
cargo test -p eggress-protocol-reverse --test integration
cargo test -p eggress-pproxy-compat --lib reverse
cargo test -p eggress-runtime --test reverse_interop
cargo test -p eggress-runtime --test reverse_runtime

# Run gated reverse interop tests (requires pproxy on PATH)
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored

# Run URI corpus integrity validator
cargo test -p eggress-testkit --lib corpus

# Run platform capability tests (transparent, PF)
cargo test -p eggress-runtime --lib platform

# Run sockaddr parser tests (transparent unsafe boundary)
cargo test -p eggress-server --lib transparent

# Run advanced transport integration tests
cargo test -p eggress-runtime advanced_transport

# Run transparent proxy tests
cargo test -p eggress-runtime transparent

# Run Unix socket tests
cargo test -p eggress-runtime unix_socket

# Run system proxy tests
cargo test -p eggress-system-proxy

# Run pproxy compat tests for redir/unix
cargo test -p eggress-pproxy-compat redir
cargo test -p eggress-pproxy-compat unix

# Run CLI exit code tests
cargo test -p eggress-cli --test cli_exit_codes

# Run pproxy run process tests
cargo test -p eggress-cli --test pproxy_run_process

# Run CLI translation golden tests
cargo test -p eggress-cli --test pproxy_translation_golden

# Run diagnostics tests
cargo test -p eggress-pproxy-compat diagnostics

# Run exit codes tests
cargo test -p eggress-pproxy-compat exit_codes

# Run Shadowsocks interop tests (gated)
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored

# Run pproxy differential tests (gated, requires Python 3.11/3.12)
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored

# Run pproxy standalone UDP differential tests only
./scripts/compat_udp_pproxy.sh

# Run pproxy binary drop-in tests
cargo test -p eggress-cli --test pproxy_binary

# Run Phase 41 pproxy differential parity harness tests (gated, requires pproxy==2.7.9)
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored

# Run property tests (proptest)
cargo test -p eggress-protocol-socks --test codec_properties
cargo test -p eggress-protocol-http --test connect_properties
cargo test -p eggress-protocol-trojan --test request_properties
cargo test -p eggress-routing --test properties

# Run fuzz smoke tests
cargo test -p eggress-protocol-socks --test fuzz_smoke

# Run performance smoke tests (Tier 1)
cargo test -p eggress-runtime --test performance_smoke

# Run performance smoke tests with output
cargo test -p eggress-runtime --test performance_smoke -- --nocapture

# Run soak tests (Tier 2, gated)
EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1

# Run pproxy performance comparison (Tier 3, gated)
EGRESS_REQUIRE_PPROXY_PERF=1 ./scripts/perf/run_pproxy_comparison.sh

# Run Python binding performance tests
python3.11 -m pytest python/tests/test_performance_smoke.py -v

# Run Python pproxy drop-in API tests (Phase 40)
python3.11 -m pytest python/tests/test_pproxy_dropin.py -v

# Run Python pproxy differential tests (Phase 41, gated)
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python3.11 -m pytest python/tests/test_pproxy_differential.py -v

# Run lifecycle invariant tests
cargo test -p eggress-runtime --test lifecycle_invariants

# Run observability tests
cargo test -p eggress-runtime --test observability

# Run security invariant tests
cargo test -p eggress-runtime --test security_invariants

# Run load tests (ignored by default)
cargo test -p eggress-runtime --test load -- --ignored

# Run pproxy differential tests (gated)
cargo test -p eggress-cli --test differential_pproxy

# Run pproxy compatibility tests
cargo test -p eggress-pproxy-compat

# Run pproxy oracle tests (Phase 18, requires pproxy==2.7.9)
cargo test -p eggress-testkit pproxy_oracle -- --ignored

# Run scenario-driven oracle harness (gated, requires pproxy==2.7.9)
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle -- --ignored

# Run schema validation tests
cargo test -p eggress-testkit --test oracle_scenario_files

# Run oracle CI tier tests
cargo test -p eggress-testkit --lib oracle::ci

# Run oracle observation tests
cargo test -p eggress-testkit --lib oracle::observations

# Run oracle supervisor tests
cargo test -p eggress-testkit --lib oracle::supervisor

# Run oracle probe tests
cargo test -p eggress-testkit --lib oracle::probes

# Run oracle schema tests
cargo test -p eggress-testkit --lib oracle::schema

# Run pproxy differential tests (gated, requires pproxy==2.7.9)
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored

# Validate manifest invariants (Phase 23)
cargo test -p eggress-testkit validate_real_manifest

# Validate manifest test names exist (Phase 23)
cargo test -p eggress-testkit manifest_test_names_exist

# Run all manifest validation tests (Phase 36)
cargo test -p eggress-testkit --lib manifest

# Validate canonical parity manifest via Rust (Phase 53)
cargo test -p eggress-testkit validate_real_canonical_manifest

# Run all canonical manifest validation tests (Phase 53)
cargo test -p eggress-testkit canonical_manifest

# Cross-check parity report against canonical manifest (Phase 53)
cargo test -p eggress-testkit parity_manifest_consistency

# Validate pproxy parity capability manifest (Phase 37)
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml

# Validate parity manifest (strict mode)
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml

# Regenerate the parity report from the manifest (Phase 42)
python3 scripts/validate_pproxy_parity_manifest.py --write-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml

# Verify the parity report is consistent with the manifest (Phase 42; CI runs this)
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml

# Run composition matrix validation (Phase A2)
cargo test -p eggress-testkit composition

# Validate composition matrix against manifest (Phase A2)
python3 scripts/validate_pproxy_parity_manifest.py --check-matrix docs/parity/composition_matrix.toml docs/parity/pproxy_capability_manifest.toml

# Run validator Rule 14 regression tests (Phase 52)
python3.11 -m pytest tests/scripts/test_validate_pproxy_parity_manifest.py -v

# Run full Phase 36 release audit locally (Python 3.11 required for pproxy 2.7.9 interop)
python3.11 -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1

# Generate the final parity report JSON (Phase 36)
python3 scripts/phase36_report.py   # writes target/compat/final-pproxy-parity-report.json
# Or re-run the parity release audit (see plans/PHASE_36_FINAL_PARITY_RELEASE_AUDIT.md)

# Validate release-doc consistency (R1-R4 checks)
python3 scripts/check_release_docs.py

# Run embed API tests
cargo test -p eggress-embed

# Run scheduler parity tests
cargo test -p eggress-routing scheduler_parity
cargo test -p eggress-runtime scheduler_runtime

# Run multi-hop TCP chain tests
cargo test -p eggress-runtime multihop_tcp

# Run failure semantics tests
cargo test -p eggress-runtime failure_semantics

# Run retry/fallback tests
cargo test -p eggress-runtime retry_fallback

# Run benchmarks
cargo bench --workspace

# Build and test Python bindings
# IMPORTANT: Python tests import the compiled `eggress._eggress` native
# extension. Either install a prebuilt wheel (pip install ...) or build
# the extension in place (maturin develop) BEFORE running pytest, or the
# tests will fail at collection with `ModuleNotFoundError: No module named
# 'eggress._eggress'`. The repository's bare `python` is Python 2.7.18 in
# some environments — use `python3.11` (or your Python 3.x interpreter)
# for all commands below.
cd crates/eggress-python && maturin build --target x86_64-apple-darwin
pip install --force-reinstall target/wheels/eggress-0.1.0-*.whl
python3.11 -m pytest python/tests

# Alternative: build the native extension in place (faster for local dev)
python3.11 -m pip install maturin pytest
python3.11 -m maturin develop -m crates/eggress-python/Cargo.toml
python3.11 -m pytest python/tests

# Run Python pproxy compat tests
python3.11 -m pytest python/tests/test_pproxy_compat.py -v

# Run Python pproxy redaction tests
python3.11 -m pytest python/tests/test_pproxy_redaction.py -v

# Run Python pproxy concurrency tests
python3.11 -m pytest python/tests/test_pproxy_concurrency.py -v

# Run Python utility/fixture tests (Phase 31)
python3.11 -m pytest python/tests/test_pproxy_utility_fixtures.py -v

# Run Python diagnostics tests (Phase 31)
python3.11 -m pytest python/tests/test_pproxy_diagnostics.py -v

# Run Python pproxy differential tests (gated, requires pproxy package)
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python3.11 -m pytest python/tests/test_pproxy_differential.py -v

# Run Python Connection tests
python3.11 -m pytest python/tests/test_connection.py -v

# Run Python Connection behavioral tests (protocol echo, failure scenarios, GIL release)
python3.11 -m pytest python/tests/test_connection_behavioral.py -v

# Run Python Server lifecycle tests (Phase C3)
python3.11 -m pytest python/tests/test_server_lifecycle.py -v

# Run Phase 40 pproxy drop-in API tests
python3.11 -m pytest python/tests/test_pproxy_dropin.py -v

# Run Phase C4 protocol/cipher/plugin tests
python3.11 -m pytest python/tests/test_protocol_cipher.py -v

# Run Phase C4 plugin bridge tests
python3.11 -m pytest python/tests/test_plugin.py -v

# Run Phase C4 wrapper/composition tests
python3.11 -m pytest python/tests/test_wrapper.py -v

# Run Phase C5 asyncio semantic compatibility tests
python3.11 -m pytest python/tests/test_asyncio_semantic.py -v

# Run Phase C1 API contract tests
python3.11 -m pytest tests/compat/test_pproxy_api_contract.py -v

# Regenerate the pproxy API contract
python3.11 python/compat/extract_api.py

# Run behavioral probes
python3.11 python/compat/behavioral_probes.py

# Run classification report
python3.11 python/compat/classification.py

# Build and test Python wheel
maturin build --release --out dist
python3.11 -m venv .venv-wheel-test
. .venv-wheel-test/bin/activate
pip install dist/eggress-*.whl
pip install pytest
python3.11 -m pytest python/tests
deactivate

# Or use the helper script
./scripts/test_wheel.sh

# Run Python wheel import smoke tests
python3.11 -m pytest python/tests/test_wheel_import_smoke.py -v

#

# Build sdist
cd crates/eggress-python && maturin sdist --out ../../dist

# Check wheel/sdist metadata
python3.11 -m twine check dist/*

# Run fuzz targets (standalone `fuzz/` workspace; libfuzzer-sys based)
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo test --manifest-path fuzz/Cargo.toml --no-run

# Fuzz targets available:
#   socks5_udp_datagram, socks5_handshake, http_connect_response,
#   trojan_request, route_match, uri_parse
# Smoke examples (require cargo-fuzz):
cargo fuzz run uri_parse -- -runs=1000
cargo fuzz run socks5_udp_datagram -- -runs=1000
cargo fuzz run socks5_handshake -- -runs=1000
cargo fuzz run http_connect_response -- -runs=1000
cargo fuzz run trojan_request -- -runs=1000
cargo fuzz run route_match -- -runs=1000

# Run the CLI
cargo run --bin eggress -- --help
cargo run --bin eggress -- -l http://:8080
cargo run --bin eggress -- --config path/to/config.toml

# Build release binary for current platform
cargo build --release -p eggress-cli

# Build release binary for specific target
cargo build --release --target x86_64-unknown-linux-gnu -p eggress-cli
cargo build --release --target aarch64-unknown-linux-gnu -p eggress-cli
cargo build --release --target x86_64-apple-darwin -p eggress-cli
cargo build --release --target aarch64-apple-darwin -p eggress-cli
cargo build --release --target x86_64-pc-windows-msvc -p eggress-cli

# Build container image
docker build -t eggress:local .

# Validate release docs consistency
python3 scripts/check_release_docs.py

# Validate parity manifest
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml

# Generate parity report
python3 scripts/validate_pproxy_parity_manifest.py --write-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml

# Verify parity report consistency
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

## Project Structure

```text
eggress/
├── Cargo.toml              # Workspace root
├── Containerfile           # Multi-arch container image (Phase 49)
├── .skills/                # Agent skill files for this codebase
├── crates/
│   ├── eggress-core/      # Core types, traits, relay, listener, connector, chain
│   ├── eggress-cli/       # CLI binary (eggress + pproxy drop-in binary)
│   ├── eggress-server/    # Server orchestration: accept, execute, reply, error
│   ├── eggress-runtime/   # Service supervisor, composition layer, signal handling
│   ├── eggress-uri/       # URI parser and AST
│   ├── eggress-routing/   # Rule engine, schedulers, health, leases, route explanation
│   ├── eggress-config/    # TOML configuration, validation, secret sources
│   ├── eggress-metrics/   # Prometheus-compatible metrics registry
│   ├── eggress-admin/     # Admin HTTP server, PAC, static content, snapshot provider trait
│   ├── eggress-protocol-http/   # HTTP CONNECT and forwarding
│   ├── eggress-protocol-socks/  # SOCKS4/4a and SOCKS5
│   ├── eggress-protocol-shadowsocks/ # Shadowsocks AEAD TCP/UDP (TCP: full stream encryption)
│   ├── eggress-protocol-trojan/ # Trojan TLS-based proxy
│   ├── eggress-transport-tls/ # Shared TLS transport layer (builders, connectors, acceptors)
│   ├── eggress-protocol-http/src/h2_connect.rs # HTTP/2 CONNECT client/server/relay
│   ├── eggress-protocol-websocket/ # WebSocket tunnel (server, client, stream adapter)
│   ├── eggress-protocol-raw/ # Raw fixed-target TCP tunnel
│   ├── eggress-protocol-reverse/ # Reverse/backward proxy: raw-relay control channel, server (acceptor), client (control client), metrics
│   ├── eggress-system-proxy/ # System proxy inspection, capability model, dry-run apply
│   ├── eggress-runtime/src/platform.rs # Platform capability model (Linux SO_ORIGINAL_DST, macOS PF)
│   ├── eggress-server/src/listener/transparent.rs # Transparent TCP listener (SO_ORIGINAL_DST)
│   ├── eggress-server/src/listener/unix.rs # Unix domain socket listener
│   ├── eggress-udp/       # UDP association, codec, direct forwarding, upstream SOCKS5 relay
│   ├── eggress-pproxy-compat/ # pproxy compatibility: URI translation, config migration
│   ├── eggress-embed/      # Stable Rust embed API: config, service, handle, errors
│   ├── eggress-python/     # Python bindings via PyO3 (wraps eggress-embed)
│   │   └── pyproject.toml      # Authoritative release build config (maturin)
│   ├── compat/
│   │   ├── extract_api.py              # Phase C1: pproxy API contract extractor
│   │   ├── behavioral_probes.py        # Phase C1: dynamic behavior probes
│   │   ├── classification.py           # Phase C1: tier classification mapper
│   │   ├── pproxy_api_contract.json    # Phase C1: generated API contract
│   │   ├── classification.json         # Phase C1: classification results
│   │   └── behavioral_probe_results.json # Phase C1: probe results
│   ├── test_pproxy_dropin.py       # Phase 40: PPProxyService, CompatibilityReport, start_pproxy tests
│   ├── test_pproxy_api_contract.py     # Phase C1: contract validation tests
│   └── eggress-testkit/   # Test utilities
│       └── src/oracle/
│           ├── mod.rs        # Oracle gate, timeouts, module root
│           ├── scenario.rs   # 31 hardcoded scenarios (backward compat)
│           ├── schema.rs     # TOML scenario schema, loader, validator
│           ├── observations.rs # Semantic observation capture model
│           ├── probes.rs     # Reusable protocol client probes
│           ├── supervisor.rs # Robust process supervisor with artifact retention
│           ├── ci.rs         # CI tier organization and gating
│           └── report.rs     # JSON/Markdown report generation
├── benches/                # Criterion benchmarks (tcp_relay, udp_relay, route_match, http_connect_upstream)
├── fuzz/                   # Fuzz harness smoke targets (socks5_udp_datagram, socks5_handshake, http_connect_response, trojan_request, route_match, uri_parse)
├── scripts/                # Helper scripts (test_wheel.sh)
├── plans/                  # Historical planning documents (reference only)
├── tests/
│   └── interoperability/  # Cross-implementation tests (curl, pproxy)
└── docs/
    ├── ARCHITECTURE.md
    ├── ROADMAP.md
    ├── CI_STATUS.md
    ├── CONFIG_REFERENCE.md
    ├── DEPENDENCY_POLICY.md
    ├── METRICS.md
    ├── OPERATIONS.md
    ├── PARITY_MATRIX.md
    ├── PHASE_2_COMPLETION.md
    ├── PHASE_3_COMPLETION.md
    ├── PHASE_4_UDP_UPSTREAM_RELAY_COMPLETION.md
    ├── PHASE_5_UPSTREAM_PROTOCOL_PARITY_COMPLETION.md
    ├── RELEASE_READINESS.md
    ├── SECURITY_REVIEW.md
    ├── TESTING.md
    ├── PPROXY_PARITY_SPEC.md
    ├── PHASE_7_PPROXY_PARITY_SPEC_COMPLETION.md
    ├── TRANSPORT_TLS_COMPLETION.md
    ├── EMBED_API.md
    ├── PYTHON_BINDINGS.md
    ├── PHASE_14_PYTHON_BINDINGS_COMPLETION.md
    ├── PHASE_16_PYTHON_PPROXY_LIBRARY_PARITY_COMPLETION.md
    ├── PHASE_17_TRUE_PPROXY_PARITY_RELEASE_CANDIDATE_COMPLETION.md
    ├── PHASE_17_RC_POLISH_COMPLETION.md
    ├── TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md
    ├── URI_GRAMMAR.md
    ├── cli/
    │   ├── PPROXY_CLI_INVENTORY.md
    │   └── EXIT_CODES.md
    ├── release/
    │   ├── PARITY_TARGET_FREEZE.md
    │   ├── FINAL_PPROXY_PARITY_REPORT.md
    │   ├── RELEASE_NOTES_PARITY_RC.md
    │   ├── PLATFORM_SUPPORT_MATRIX.md
    │   ├── MIGRATION_FROM_PPROXY_FINAL.md
    │   ├── PARITY_RELEASE_GO_NO_GO.md
    │   ├── RELEASE_PROCESS.md
    │   ├── ARTIFACT_MATRIX.md
    │   ├── BINARY_MATRIX.md
    │   ├── BINARY_INSTALL.md
    │   └── CONTAINER.md
    ├── parity/
    │   ├── pproxy_capability_manifest.toml  # Canonical parity contract (139 capabilities)
    │   ├── composition_schema.toml           # Schema for composition matrix (Phase A2)
    │   ├── composition_matrix.toml           # Protocol×role×traffic_kind graph (Phase A2)
    │   ├── PPROXY_PARITY_REPORT.md           # Generated parity report
    │   └── README.md
    └── protocols/
        ├── HTTP_CONNECT.md
        ├── SOCKS4.md
        ├── SHADOWSOCKS.md
        └── TROJAN.md
```

Integration tests live in `crates/eggress-runtime/tests/` (startup, routing,
health, admin, reload, shutdown, pac_static, udp, udp_upstream, upstream_protocols,
shadowsocks_tcp, shadowsocks_udp, reverse_interop, reverse_runtime, reverse_soak,
lifecycle_invariants, observability, security_invariants, load).
They exercise the supervisor end to end and cover negative-path behaviors (bind
conflict, invalid source, oversized identity, reload-time failure). UDP integration tests
cover association lifecycle, TCP control close, echo relay, bind conflict,
topology rejection, config reload, SOCKS5 upstream relay, and standalone UDP mode. Upstream protocol tests
cover HTTP, SOCKS4, SOCKS5, and unsupported-combo rejection through the full stack.
Property tests live in per-crate `tests/` directories (codec round-trips, parser
round-trips, route match consistency). Fuzz smoke tests exercise seed inputs for
`cargo fuzz` targets. Load tests are `#[ignore]` by default and require explicit opt-in.
Differential tests against `pproxy` are gated and live in `crates/eggress-cli/tests/`.
pproxy compat tests live in `crates/eggress-pproxy-compat/src/tests.rs` and cover protocol aliases, unsupported scheme diagnostics, and credential redaction.
Shadowsocks interop tests live in `crates/eggress-cli/tests/interoperability_shadowsocks.rs` (gated).
Validator regression tests live in `tests/scripts/test_validate_pproxy_parity_manifest.py` (Rule 14 scope, caveat_class validation).
See `docs/TESTING.md` for comprehensive testing guidance.
See `docs/DIFFERENTIAL_TESTING.md` for gated differential and interoperability test details.

## Code Conventions

- Edition: 2021
- MSRV: 1.75
- `unsafe_code = "forbid"` in all workspace crates — never lift this
- `clippy::all` warnings denied
- Async runtime: Tokio
- Errors: `thiserror`
- CLI: `clap` with derive
- Logging: `tracing` + `tracing-subscriber`
- No C dependencies, no OpenSSL
- No `build.rs` files anywhere in the workspace

## CI / Status Visibility

- `.github/workflows/ci.yml` exists with separate visible jobs: fmt, check,
  test, clippy, deny, audit, interoperability.
- `.github/workflows/release.yml` builds CLI binaries (5 targets), Python
  wheels (5 platforms), generates SHA-256 checksums, SBOM, builds container
  image, and creates a GitHub Release — triggered by `v*` tags.
- `.github/workflows/python-wheels.yml` builds Python wheels on `v*` tags
  (standalone, uploaded as artifacts).
- `.github/workflows/publish-pypi.yml` publishes to PyPI/TestPyPI (manual
  workflow_dispatch).
- Hosted CI run status is **not** currently visible via the
  `commits/{sha}/status` endpoint for `main` (returns `state: pending,
  statuses: []`). Recent runs surfaced via `gh run list` are reported as
  `completed failure` with billing-related annotations (no code execution).
- Treat **local verification** (`cargo fmt`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo deny check`,
  `cargo audit`) as the source of truth until hosted CI resumes. Record local
  verification in completion docs; do not claim hosted CI visibility unless a
  workflow run ID is observable on the commit.
- See `docs/CI_STATUS.md` for detailed status, local verification commands,
  and how to interpret completion docs when CI is unavailable.
- `.github/workflows/shadowsocks-interop.yml` runs Shadowsocks interop tests with `ssserver`/`sslocal` from `shadowsocks-rust`.

## Key Architecture Facts

- **Entry point**: `eggress-cli` binary → `eggress-runtime` `ServiceSupervisor::run()` → `eggress-server` `serve_connection()`. The `pproxy` binary is a drop-in compatibility wrapper that translates pproxy-style args to TOML and delegates to the same runtime.
- **Streams are boxed** at protocol/transport boundaries (`BoxStream`) — don't propagate generic stream types
- **Protocol detection** uses ordered `ProtocolDetector` implementations; mixed-protocol listeners are the norm
- **Chain executor** folds over hop list with protocol-specific handlers — validate chain capabilities before executing
- **HopHandler trait** accepts `&ProxyHopSpec` (not just credentials) — handlers extract what they need from the hop
- **Credentials are never logged** — URI display uses redacted format
- **Routing**: compiled rule AST with first-match-wins evaluation; recursive TOML matchers (`all`, `any_of`, `not`)
- **Atomic config reload**: `ArcSwap<Router>` for lock-free reads; only routing/upstreams/groups/health are hot-reloadable, not listener topology
- **Shutdown ordering**: readiness=false → stop listeners → drain connections (force-cancel after grace) → stop admin; admin stays queryable through drain
- **Pre-bind listeners** before readiness to avoid race conditions
- **Shared runtime snapshot**: `CompiledRuntimeSnapshot` — one set of `Arc<UpstreamRuntime>` shared by router, health, admin, metrics
- **Single generation source**: `CompiledRuntimeSnapshot.generation`; admin reads it via `AdminSnapshotProvider` instead of a duplicate atomic
- **Health state machine** with hysteresis and active TCP probes; config per upstream from TOML
- **UDP**: direct forwarding, one-hop SOCKS5 upstream relay, one-hop Shadowsocks upstream relay (standard AEAD format), and standalone UDP relay (`mode = "standalone_pproxy_udp"`); no multi-hop chains, no HTTP/MASQUE. Association owned by TCP control connection (or standalone in pproxy-compatible mode). Client pinning enabled by default. Shadowsocks has inbound TCP listener support and full AEAD stream encryption.
- **WebSocket/Raw/H2 upstream support**: ws/wss/raw/tunnel/h2 schemes are now integrated through the runtime supervisor as upstream protocols (Phase B3, B4). Config compiler, CLI, and chain executor all accept these protocols. See `docs/adr/ADR_b3_ws_raw_runtime_promotion.md`.
- **Scheduler parity**: Round-robin uses global atomic cursor; least-connections uses active+in_flight; first-available returns first eligible; health filtering excludes Unhealthy/Disabled
- **Failure semantics**: SOCKS5/HTTP/SOCKS4 reply codes documented in `docs/FAILURE_SEMANTICS.md`; timeout→504/0x06, refused→502/0x05, policy→403/0x02
- **pproxy parity spec and tier taxonomy** defined in `docs/PPROXY_PARITY_SPEC.md`
- **Differential test harness** has reusable primitives (`ProcessGuard`, `TaskGuard`, `start_tcp_echo`, `start_udp_echo`, `compare_tcp_echo`, etc.)
- **pproxy CLI subcommands**: `pproxy translate` converts pproxy URI arguments to eggress TOML; `pproxy check` reports parity tier; `pproxy run` translates and starts the service
- **pproxy protocol parity**: Phase 11 classified all remaining pproxy protocols/schemes; lightweight aliases (socks4a, https) map to existing protocols; unsupported protocols (SSH) produce structured diagnostics. Transparent TCP proxy (`redir://`, Linux only) and Unix domain socket listeners (`unix://`, Unix only) are now supported (Phase 25).
- **Shadowsocks TCP framing**: Standard SIP003 AEAD (two AEAD operations per chunk, encrypted length). Wire-compatible with standard Shadowsocks implementations. UDP uses standard AEAD format and is interoperable. See `docs/protocols/SHADOWSOCKS.md`.
- **Advanced transports**: HTTP/2 CONNECT, WebSocket tunnels, and raw fixed-target tunnels are implemented in their protocol crates (`eggress-protocol-http/src/h2_connect.rs`, `eggress-protocol-websocket/`, `eggress-protocol-raw/`). WebSocket (ws/wss) and Raw (raw/tunnel) are now promoted to runtime-integrated upstream protocols (Phase B3). H2 CONNECT is now promoted to runtime-integrated upstream (Phase B4). QUIC/HTTP/3 deferred by separate ADR. See `docs/adr/ADR_b3_ws_raw_runtime_promotion.md` and `docs/protocols/ADVANCED_TRANSPORTS.md`.
- **SSR/legacy Shadowsocks**: Intentionally unsupported. SSR URIs (`ssr://`) and legacy stream cipher methods are recognized and rejected with clear diagnostics. See ADR at `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`. Legacy method detection exists in `eggress-protocol-shadowsocks::method::is_legacy_method()`.
- **SSH upstream parity**: SSH is classified as intentional non-parity (Phase 47 ADR). SSH URIs are recognized at parse time for clean diagnostics but rejected with an actionable recommendation to use OpenSSH dynamic forwarding (`ssh -D`). See `docs/adr/ADR_ssh_upstream_parity.md`.
- **Corrective parity audit**: Completed for workstreams 6 (repair capability classifier) and 9 (completion-doc truth pass). Shadowsocks TCP framing standardized to SIP003 AEAD in Phase 21. Completion docs updated with corrective notices and gated-test status.
- **Embed API**: `eggress-embed` provides `EggressConfig`, `EggressService`, and `EggressHandle` for in-process embedding. Thread ownership: async path uses a Tokio blocking-pool thread + dedicated OS thread (`eggress-embed-rt`); blocking path uses an outer startup thread + inner run thread (`eggress-embed-run`). Handle owns state/token and cleans up on drop (5-second timeout on async path). `shutdown()` and `shutdown_blocking()` are idempotent. See `docs/EMBED_API.md`.
- **Python bindings**: `eggress-python` wraps `eggress-embed` via PyO3. GIL is released on all blocking Rust calls via `py.detach()`. Python package lives in `python/eggress/` with maturin build. Version sourced from native module's `CARGO_PKG_VERSION`. Lifecycle: always prefer explicit `shutdown()` or context manager; object destruction is best-effort fallback. Phase 31 added Python utility APIs: `check_pproxy_uri()`, `redact_pproxy_uri()`, `diagnostics_for_uri()`, `supported_features()`, `UriInfo` dataclass, `Diagnostic` dataclass, and `Server` status helpers (`is_ready`, `listener_info`, `metrics_text`). URI corpus fixture tests parametrized from `tests/compat/fixtures/pproxy_uri_corpus.toml` (65 cases). Phase 32 hardening: GIL-release fixes for `parse_toml_config`, `route_explain`, `test_upstream_connect`; tier language normalization (Eggress-native); evidence reclassification; `__all__` completeness fix; `py.typed` presence test. Phase C3 `Server` class: pproxy-compatible server wrapper with full lifecycle, observability (`status()`, `sessions`, `last_error`), hot-reload, resource management (`__del__` warning), thread safety, and 84 lifecycle tests covering TLS, auth, chains, UDP, IPv6, loop affinity, GIL release, FD leak detection, and pproxy example patterns. See `docs/PYTHON_BINDINGS.md` and `docs/PHASE_29_32_PYTHON_HARDENING_COMPLETION.md`.
- **Python Connection object (Phase C2)**: `eggress.Connection` provides a pproxy-compatible low-level connection object backed by Rust-owned networking. Accepts pproxy-style URI arguments, translates to eggress TOML config, creates an embedded service, and manages the connection lifecycle. State machine: Created → Connecting → Connected → Closing → Closed → Failed. Supports sync/async context managers, `__del__` cleanup with `ResourceWarning`, and idempotent `close()`/`wait_closed()`. Exception hierarchy maps Rust errors to `ConnectionError`, `ConnectionClosedError`, `TimeoutError`, `DnsError`, `AuthError`, `TlsError`, `LoopMismatchError`, `ConnectionCancelledError`, `UseAfterCloseError`, `UdpAssociationError`, `UnsupportedCompositionError`. Connection leak detection via `Connection.connection_stats()` / `Connection.reset_connection_stats()`. `AsyncConnection` provides asyncio-native wrapper with loop affinity. See `python/eggress/connection.py` and `python/eggress/async_connection.py`.
- **Python protocol/cipher/plugin objects (Phase C4)**: `eggress.protocol` provides pproxy-compatible protocol objects with `MAPPINGS` dict, `get_protos()` parser, and classes (`Socks5`, `HTTP`, `SS`, `Trojan`, etc.) carrying typed metadata. `eggress.cipher` provides AEAD cipher objects (`AES_256_GCM_Cipher`, `ChaCha20_IETF_POLY1305_Cipher`) delegating to Rust, with `MAP` dict and `get_cipher()` factory. `eggress.plugin` provides a bounded, cancellation-safe plugin callback bridge (`PluginBridge`) between Rust async tasks and Python callbacks with backpressure, timeout enforcement, and reentrancy detection. See `python/eggress/protocol.py`, `python/eggress/cipher.py`, and `python/eggress/plugin.py`.
- **Python asyncio semantic compatibility (Phase C5)**: `_asyncio.py` provides `AsyncBridge` (loop-affinity enforcement, cancellation propagation, contextvars preservation, stable exception conversion), `CloseWaiter` (idempotent, multi-waiter close/shutdown coordination), and `LoopAffinityError`. `_compat.py` provides Python version helpers (`PY_VERSION`, `HAS_TASKGROUP`, `HAS_EXCEPTIONGROUP`, `CANCELLED_ERROR_BASE`). `AsyncConnection` uses `AsyncBridge` + `CloseWaiter` for idempotent close and multi-waiter `await_closed()`. `AsyncEggressHandle` uses `AsyncBridge` for all async operations. `Server.aclose()`/`wait_closed()` use `CloseWaiter` for race-safe shutdown. `PluginBridge` reentrancy detection uses `contextvars.ContextVar` (task-level isolation, not thread-level). 67 semantic tests in `test_asyncio_semantic.py` covering loop affinity, bridge lifecycle, cancellation, close ordering, contextvars, interpreter safety, version compat, stress/race conditions, and API contracts. See `python/eggress/_asyncio.py`, `python/eggress/_compat.py`, and `python/tests/test_asyncio_semantic.py`.
- **PyPI packaging**: Wheels built with maturin for Linux x86_64/aarch64, macOS x86_64/arm64, Windows x86_64. See `docs/PYPI_RELEASE.md`.
- **Python packaging**: Canonical package `eggress` on PyPI. `eggress.pproxy` provides compatibility helpers. No top-level `pproxy` shim (deferred). Wheels built for 5 platforms via maturin. `py.typed` PEP 561 marker included. Version/capability metadata exposed via `eggress.__version__`, `eggress.version()`, `eggress.capabilities()`, `eggress.pproxy.compatibility_version()`. See `docs/adr/ADR_python_import_and_distribution_strategy.md`.
- **Release candidate audit (Phase 17)**: Final parity matrix audit, Rust/Python release audits, security/redaction audit including Python binding surface, documentation consistency pass. Release candidate document at `docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md`. All verification commands pass; go recommendation issued. See `docs/PHASE_17_TRUE_PPROXY_PARITY_RELEASE_CANDIDATE_COMPLETION.md`.
- **Manifest validation**: Two manifests coexist with different schemas. The **canonical parity contract** is `docs/parity/pproxy_capability_manifest.toml` (Phase 37+, 139 capabilities, `[[capability]]` arrays, 5-tier vocabulary). Validated by Rust (`eggress-testkit::canonical_manifest::validate_canonical_manifest()`, 13 rules) and Python (`scripts/validate_pproxy_parity_manifest.py`, 14 rules). The **legacy evidence index** is `tests/compat/pproxy_manifest.toml` (Phase 18, `[[features]]` arrays, different taxonomy) — retained for Rust cross-check tests but superseded for parity claims.
- **pproxy parity manifest (Phase 37)**: `docs/parity/pproxy_capability_manifest.toml` is the authoritative compatibility contract — 139 capabilities across 5 categories (CLI, URI, Protocol, Routing, Python) with tier classification, evidence requirements, and config/runtime/test layers. Frozen as of Phase 51 final parity certification. Validated by Rust (`eggress-testkit::canonical_manifest::validate_canonical_manifest()`, 13 rules) and Python (`scripts/validate_pproxy_parity_manifest.py`, 14 rules, strict mode). The report `docs/parity/PPROXY_PARITY_REPORT.md` is generated from the manifest (Phase 42: `--write-report`/`--check-report`). See `docs/parity/README.md` for design.
- **Composition matrix (Phase A2)**: `docs/parity/composition_matrix.toml` extends the flat capability manifest with an explicit `protocol×role×traffic_kind` graph. 31 cells, 4 chains, 5 constraints. Validates that every supported composition is declared explicitly, preventing false parity claims. Schema at `docs/parity/composition_schema.toml`. Validated by Rust (`eggress-testkit::composition::validate_composition_matrix()`, 33 tests) and Python (`--check-matrix` flag). Config compiler integration produces warnings for unsupported listener/upstream combinations via `validate_config_composition()`. See `docs/parity/README.md` for design.
- **Differential oracle (Phase A3)**: Expanded oracle harness with declarative TOML scenarios (73 scenarios: 31 hardcoded + 42 TOML), semantic observation model, reusable protocol probes (SOCKS4/4a, SOCKS5, HTTP CONNECT/forward), robust process supervisor with process-group ownership and artifact retention, 5-tier CI organization (fast-structural, core-differential, extended-differential, platform-differential, privileged-external), and Markdown report generation. Scenarios map to A2 composition IDs. See `docs/DIFFERENTIAL_TESTING.md` for details.
- **Phase 42 corrective consistency pass**: `CompatibilityReport.tier` now uses the five-tier manifest vocabulary (`drop_in`, `compatible_with_warning`, `native_equivalent`, `intentional_non_parity`, `unsupported`); `PPProxyService.from_args` preserves the full pproxy argument vector through `translate_pproxy_args`; `--ssl` applies TLS to all compatible listeners (matches pproxy, which loads the cert chain into every ssl context). See `docs/PHASE_42_PPROXY_PARITY_CORRECTIVE_CONSISTENCY_PASS_COMPLETION.md`.
- **pproxy CLI native-equivalent closure (Phase 38)**: `--ssl`, `-b`, `--rulefile` generate TOML config (TLS listener, reject rules, rulefile-parsed rules). `-a N` generates `[health] interval = "Ns"` TOML. `--pac` generates `[admin.pac] enabled = true` TOML. `--test` translates config and runs `eggress upstream test -c <config>`, then exits. `--sys` auto-invokes `eggress system-proxy inspect` before starting the service. `--log`, `--get`, `--reuse` emit structured diagnostics. `--daemon` remains unsupported. See `plans/phase_38_pproxy_cli_native_equivalent_closure.md`, `docs/PHASE_38_PPROXY_CLI_NATIVE_EQUIVALENT_CLOSURE_COMPLETION.md`, and `docs/cli/PPROXY_CLI_INVENTORY.md`.\n- **pproxy regex and rulefile compatibility (Phase A.03)**: Dual regex backend (`fast regex` + `fancy_regex` for lookahead/lookbehind/backreferences). `--rulefile` loader validates patterns and enforces `MAX_RULE_ENTRIES = 10_000`. `-b` flag patterns validated at parse time. URI `?rules_file=` query parameter preserved through translation. New diagnostic codes: `RulefileError`, `InvalidRegexPattern`, `FancyRegexBackend`, `UriPreservedUnsupportedComponent`. See `crates/eggress-pproxy-compat/src/regex_compat.rs`.
- **Manifest external dependency checks (Phase 24)**: Compatible entries with differential tests require `external_dependency`; implemented_interop requires dependency or divergence note explaining interop.
- **Exit codes**: Structured exit codes defined in `eggress-pproxy-compat::exit_codes`. CLI uses constants, not ad-hoc returns.
- **Diagnostics**: `DiagnosticCode` enum with stable codes for all pproxy compat errors/warnings. `StructuredDiagnostic` for JSON output.
- **pproxy check --json**: Machine-readable compatibility check output with tier, features, and diagnostics.
- **Phase 25-28 hardening pass**: Verified implementation matches documentation. H1 added SAFETY comments and `read_unaligned` to transparent listener; H3 corrected Linux/macOS platform capability semantics (macOS PF now honestly reports `KernelUnsupported`); H4 hardened Unix listener (`unlink_existing=true` refuses non-socket paths); H5/H6/H7 refused H2/WS/Raw as listener protocols (upstream-only; Phase B4 promoted H2 to runtime-integrated); H8 added QUIC/H3 structured-rejection tests; H9 wired reverse proxy through supervisor with `reverse_runtime.rs` (10 tests); H10 added payload-level reverse differential test; H11 added `ReverseServerConfig::validate()` for non-loopback safety; H13 added URI corpus integrity validator; H14/H15 audited and corrected docs (README, PARITY_MATRIX.md, METRICS.md). See `docs/PHASE_25_28_HARDENING_COMPLETION.md` for the full record.
- **Phase 50 security gate**: DNS rebinding protection (reserved IP rejection after DNS resolution in `DirectConnector`); standalone UDP validation wired to `validate_standalone_target()`; auth failure metrics (`eggress_auth_failures_total`); 8 fuzz targets (uri_parse, socks5_handshake, socks5_udp_datagram, http_connect_response, trojan_request, route_match, shadowsocks_frame, toml_config); soak tests (slowloris, auth failure burst, UDP association churn). Security docs: `SECURITY.md`, `docs/security/SECURE_CONFIGURATION.md`, `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`. See `docs/SECURITY_REVIEW.md` for the full threat model and residual risks.
- **System proxy**: `eggress-system-proxy` provides read-only system proxy inspection, platform capability detection, and explicit dry-run apply. CLI subcommand `eggress system-proxy inspect` reads current settings. No hidden global mutation; apply requires explicit `--apply` flag. Supports macOS (`networksetup`), Windows (registry), Linux (`gsettings`), and environment variables. `CommandRunner` trait enables testable command execution. See `docs/system_proxy/`.
- **Python API contract freeze (Phase C1)**: Machine-readable pproxy 2.7.9 API contract at `python/compat/pproxy_api_contract.json` (105 symbols, 10 constants, 6 aliases). Extracted via `python/compat/extract_api.py`. Behavioral probes at `python/compat/behavioral_probes.py` (46 probes). Classification at `python/compat/classification.json` (3 adapted, 1 intentional non-parity, 87 internal). Contract validator at `tests/compat/test_pproxy_api_contract.py` (56 tests). Namespace strategy ADR at `docs/python/PPROXY_NAMESPACE_STRATEGY.md`. No `import pproxy` shim; compat bundled in main `eggress` package.

## Skills

The `.skills/` directory contains focused reference files for common development tasks:

- `rust-proxy-dev.md` — Adding new protocols, transport wrappers, chain integration
- `udp-protocol.md` — UDP association management, datagram relay, upstream SOCKS5 relay
- `config-reload.md` — TOML config schema, hot-reload vs restart, atomic swaps
- `routing-rules.md` — Rule engine, matchers, schedulers, route explanation
- `testing.md` — Test layers, conventions, running and writing tests, including differential tests
- `advanced-transports.md` — H2 CONNECT, WebSocket tunnels, raw tunnels, TLS/ALPN
- `security-dev.md` — Security testing, DNS rebinding protection, auth failure metrics, fuzz targets, soak tests
- `reverse-proxy.md` — Reverse/backward proxy: NAT traversal, pproxy raw-relay, control channels
