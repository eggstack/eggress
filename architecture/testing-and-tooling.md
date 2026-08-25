# Testing, Benchmarking, Fuzzing, and Oracle Tooling

Verification infrastructure spans five crates/directories plus CI policy.
Local fast tests are the default; external-oracle work is opt-in.

## Layout / module map

### eggress-testkit (`crates/eggress-testkit/src/`)

Test-only library consumed as dev-dependency.

| Module | Role |
|---|---|
| `lib.rs` | `get_free_port()`, `start_echo_server()`, `start_half_close_server()` — async test servers and port allocation |
| `oracle/` | Oracle interpreter resolution: `$EGRESS_ORACLE_PYTHON` -> legacy `$EGRESS_PYTHON_BIN` -> `find_oracle_python` discovery |
| `pproxy_oracle.rs` | pproxy 2.7.9 oracle process management |
| `manifest.rs` | Parity manifest loading and validation |
| `canonical_manifest.rs` | Canonical manifest types |
| `strict_manifest.rs` | Strict manifest types |
| `strict_comparators.rs` | Strict comparison logic |
| `strict_observations.rs` | Observation recording |
| `corpus.rs` | Test corpus management |
| `case_model.rs` | Test case model |
| `composition.rs` | Composition test helpers |
| `differential.rs` | Differential test harness plumbing |
| `eggress_runner.rs` | Eggress process runner |
| `fixtures.rs` | Fixture management |
| `report.rs` | Report generation |

### fuzz/ (standalone cargo workspace — NOT covered by workspace commands)

11 libfuzzer targets mapping to every bounded parser:

| Target | Parser exercised |
|---|---|
| `socks5_handshake` | SOCKS5 greeting + auth + CONNECT request |
| `socks5_udp_datagram` | SOCKS5 UDP relay frame |
| `http_connect_response` | HTTP CONNECT response parsing |
| `trojan_request` | Trojan protocol request |
| `trojan_accept` | Trojan protocol accept |
| `route_match` | Route rule matching |
| `uri_parse` | pproxy URI parsing |
| `shadowsocks_frame` | Shadowsocks frame parsing |
| `toml_config` | TOML config deserialization |
| `websocket_handshake` | WebSocket upgrade handshake |
| `h2_connect_authority` | H2 CONNECT authority parsing |

Check with: `cargo check --manifest-path fuzz/Cargo.toml --bins`

`fuzz/` is a standalone workspace; workspace-wide commands (`cargo test
--workspace`) do not cover it.

### benches/ (root package `eggress-bench`, Criterion)

| File | What it measures |
|---|---|
| `route_match.rs` | Route decision latency across domain/IP targets, rule counts, and match strategies |
| `tcp_relay.rs` | TCP echo relay throughput at 1KB and 64KB payload sizes |
| `udp_relay.rs` | SOCKS5 UDP codec encode/decode performance (IPv4/IPv6/domain, small/large payloads) |
| `http_connect_upstream.rs` | HTTP CONNECT upstream open/auth/407-response lifecycle |

Run: `cargo bench`

### scripts/

Grouped by purpose:

| Group | Scripts |
|---|---|
| Strict pproxy probes | `strict_api_probe.py`, `strict_cipher_interop_probe.py`, `strict_cipher_kat_probe.py`, `strict_cipher_roundtrip_probe.py`, `strict_class_probe.py`, `strict_handler_relay_probe.py`, `strict_plugin_lifecycle_probe.py`, `strict_process_lifecycle_probe.py`, `strict_protocol_wire_probe.py`, `strict_runtime_failure_cleanup_probe.py`, `strict_server_internals_probe.py`, `strict_signature_probe.py`, `strict_stream_adapter_probe.py` |
| Interop runners | `compat_shadowsocks.sh`, `compat_udp_pproxy.sh`, `install_shadowsocks_interop.sh` |
| Certification | `run_pproxy_certification.sh`, `run_strict_api_comparison.sh`, `run_strict_pproxy_api.py`, `run_strict_pproxy_api.sh`, `run_strict_pproxy_interop.sh` |
| Evidence / validation | `build_strict_evidence_index.py`, `compare_observations.py`, `validate_pproxy_parity_manifest.py`, `demonstrate_regression_injections.py`, `demonstrate_regression_injections.sh` |
| Release smoke | `release_artifact_smoke.py`, `test_wheel.sh`, `publish-remaining.sh` |
| Perf / soak | `perf/run_local_baseline.sh`, `perf/run_pproxy_comparison.sh`, `perf/run_soak.sh` |
| Snapshot / probe | `snapshot_pproxy_api.py`, `pproxy_surface_probe.py`, `probe_pproxy_chain_topology.py`, `smoke_clients.py` |

### Frozen oracle assets (`compat/pproxy-2.7.9/`)

| File | Role |
|---|---|
| `provenance.toml` | Git tag, commit hash, source URL |
| `hashes.toml` | File integrity hashes |
| `known-defects.toml` | Documented oracle bugs |
| `cli-baseline.json` | CLI output baseline |
| `namespace-baseline.json` | Namespace/import baseline |
| `fixture_manifest.toml` | Fixture file manifest |
| `observations/` | Recorded behavioral observations |
| `tests/` | Oracle test suite |
| `examples/` | Example configurations |
| `requirements-oracle.txt` | Oracle venv dependencies |
| `requirements-optional.txt` | Optional test dependencies |

Treat as immutable reference data. Prebuilt oracle venvs exist at repo root
(`.venv-oracle`, `.venv-pproxy-279`).

### Python-side tests

#### `tests/compat/` (cross-implementation contract tests)

| File | Role |
|---|---|
| `test_pproxy_api_contract.py` | API contract validation against extracted pproxy 2.7.9 contract; class/method/signal coverage |
| `fixtures/pproxy_api_snapshot.json` | Extracted API snapshot |
| `fixtures/pproxy_cli_cases/` | CLI argument test cases |
| `fixtures/pproxy_uri_corpus.toml` | URI parsing corpus |
| `fixtures/pproxy_phase1_uri_cli.toml` | URI+CLI combined cases |
| `fixtures/python_api_cases.toml` | Python API test cases |
| `fixtures/pproxy_*_behavior.md` | Behavioral documentation (Shadowsocks, SSR, UDP) |
| `fixtures/pproxy_version_snapshot.toml` | Version snapshot |
| `pproxy_target.toml` | Target configuration |

Regression injection modules prove the differential harness catches mutations.

#### `python/tests/` (six-tier taxonomy)

| Tier | Name | Gate | Examples |
|---|---|---|---|
| 0 | Unit implementation | none | `test_milestone_c_functional.py`, `test_milestone_c_properties.py` |
| 1 | Candidate contract | none | `test_asyncio_semantic.py`, `test_protocol_behavioral.py`, `test_cipher_truth.py` |
| 2 | Paired oracle differential | `EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1` | `test_pproxy_differential.py`, `test_pproxy_oracle.py` |
| 3 | External interop | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` | `interoperability_shadowsocks.rs` (Rust) |
| 4 | Platform | none | transparent proxy, PF tests |
| 5 | Release certification | none | strict manifest validation, report freshness |

`pytest.ini` forces `--import-mode=importlib` so `python/eggress` source tree
cannot shadow the installed wheel's compiled `_eggress` extension.

### CI (three workflows, no more without project decision)

| Workflow | Trigger | What it does |
|---|---|---|
| `ci.yml` | push/PR to main | Ubuntu Rust smoke: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --locked` |
| `python-test.yml` | PR/push (path-scoped: `crates/eggress-embed/**`, `crates/eggress-python/**`, `python/**`, `tests/compat/**`, `Cargo.toml`, `Cargo.lock`) | Ubuntu Python 3.12 smoke: build wheel with maturin, install `eggress-pproxy-compat`, run pytest |
| `publish-python.yml` | `v*` tag push or manual dispatch | Validate tag/version coherence, build 5-platform wheels + sdist, smoke test, publish to PyPI via protected `pypi` environment |

Policy docs: `docs/CI_STATUS.md`, `docs/TESTING.md`.

### Containerfile

Multi-stage build: `rust:1.85-slim` builder -> `gcr.io/distroless/cc-debian12:nonroot`.
Exposes ports 8080 (admin), 1080 (proxy), 9090 (metrics). Entry point: `/eggress`.

## Verification workflow

```bash
# Local fast tests (default)
cargo test -p eggress-pproxy-compat
cargo test -p eggress-routing
cargo test -p eggress-runtime retry_fallback
cargo test -p eggress-cli --test cli_exit_codes

# Full workspace gate (before merge)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

# Python tests
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q

# Fuzz check (standalone workspace)
cargo check --manifest-path fuzz/Cargo.toml --bins

# Dependency/advisory checks (dependency changes, release prep)
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134

# External compatibility (opt-in, installs pproxy==2.7.9)
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
  cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
```

## Reviewer gotchas

- `fuzz/` is a standalone workspace — `cargo test --workspace` does NOT
  cover it. Check with `cargo check --manifest-path fuzz/Cargo.toml --bins`.
- `pytest.ini` forces `--import-mode=importlib`. Removing this causes the
  source tree `python/eggress` to shadow the installed wheel's `_eggress`
  extension, producing misleading import errors.
- Oracle venvs at repo root (`.venv-oracle`, `.venv-pproxy-279`) are
  prebuilt. The `find_oracle_python` function in `eggress-testkit` resolves
  `$EGRESS_ORACLE_PYTHON` -> `$EGRESS_PYTHON_BIN` -> discovery.
- `publish-python.yml` fires on every `v*` tag push. Pushing a version tag
  is a release action, not bookkeeping. The workflow validates tag/version
  coherence before publishing.
- Do not create additional GitHub Actions workflows without an explicit
  project-level decision.

## See also

- [python-bindings.md](python-bindings.md) — Python bindings architecture
- [pproxy-compat.md](pproxy-compat.md) — compatibility layer architecture
- `docs/CI_STATUS.md` — CI and verification policy
- `docs/TESTING.md` — testing methodology
- `docs/DIFFERENTIAL_TESTING.md` — pproxy oracle and differential harness
- `python/tests/TEST_TAXONOMY.md` — six-tier test classification
