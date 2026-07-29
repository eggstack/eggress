# Testing

Egress uses targeted local testing during development, a small workspace gate before merge, and specialized suites only when the affected subsystem requires them. The canonical policy is `docs/CI_STATUS.md`.

## Routine local testing

Run the narrowest test that covers the change:

```bash
cargo test -p eggress-uri
cargo test -p eggress-routing scheduler
cargo test -p eggress-runtime retry_fallback
cargo test -p eggress-cli --test cli_exit_codes
```

Apply formatting as part of normal development:

```bash
cargo fmt --all
```

Do not run every compatibility, security, performance, and platform suite after each edit. That produces long feedback cycles without improving the signal for a focused change.

## Broad pre-merge check

Before merging a substantial Rust change, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

`cargo check` is not a separate required gate because Clippy and the test build already compile the workspace. It remains useful interactively when a faster compile-only pass is desired.

## Python binding and compatibility package

For Python-facing changes:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q
```

Use a focused Python module while iterating, for example:

```bash
.venv/bin/python -m pytest python/tests/test_proxy_connection.py -q
.venv/bin/python -m pytest tests/compat/test_pproxy_api_contract.py -q
```

Routine hosted Python CI uses one Ubuntu/Python 3.12 smoke environment. Additional Python versions and operating systems are release or compatibility checks, not per-change gates.

## Dependency and security checks

Run dependency-policy checks when dependencies, features, licenses, or release inputs change:

```bash
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134
```

These are intentionally not installed and executed by routine CI. Installing auditing tools on every push is slow and duplicates checks whose inputs usually did not change.

## pproxy compatibility

The ordinary workspace suite includes non-ignored compatibility tests. External oracle and differential suites are opt-in because they install and launch Python `pproxy==2.7.9`:

```bash
python3 -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
  cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1

cargo test -p eggress-testkit pproxy_oracle -- --ignored
```

Run these when changing pproxy behavior, URI translation, compatibility manifests, process behavior, or the top-level `pproxy` namespace. See `docs/DIFFERENTIAL_TESTING.md` for the full harness.

The pproxy behavioral certification uses isolated oracle and candidate environments and is reserved for explicit compatibility-certification work:

```bash
./scripts/run_pproxy_certification.sh
```

It performs only pproxy-specific behavioral validation: paired oracle/candidate observations, differential tests, interoperability tests, cipher KAT, plugin probes, and process lifecycle probes. It does not run formatting, linting, workspace tests, dependency audits, or release packaging. Its output is a compact JSON summary.

## Protocol interoperability

External interoperability checks are subsystem-specific:

```bash
# Shadowsocks wire compatibility; requires ssserver and sslocal
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 \
  cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1

# Reverse protocol compatibility; requires pproxy
EGRESS_REQUIRE_REVERSE_INTEROP=1 \
  cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
```

Run them after changes to the relevant wire format, cipher, handshake, relay path, or compatibility claim.

## Performance, soak, and load tests

Performance and endurance tests are manual:

```bash
cargo test -p eggress-runtime --test performance_smoke
cargo bench --workspace
EGRESS_REQUIRE_SOAK=1 \
  cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1
EGRESS_REQUIRE_PPROXY_PERF=1 ./scripts/perf/run_pproxy_comparison.sh
```

Use them for performance-sensitive or concurrency-heavy work and before a release when the changed path warrants it.

## Fuzzing and parser hardening

Fuzz targets are in the standalone `fuzz/` workspace. Compile-smoke or run only the targets relevant to the parser being changed:

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo fuzz run uri_parse -- -runs=1000
cargo fuzz run socks5_udp_datagram -- -runs=1000
```

## Cross-platform verification

Routine CI runs on Ubuntu. Validate macOS or Windows when modifying platform-specific code, process supervision, filesystem behavior, system proxy integration, packaging, or release artifacts. Cross-platform matrices are not required for unrelated changes.

## Test selection rule

A change is adequately verified when:

1. The directly affected unit or integration tests pass.
2. The broad workspace gate passes before merge for substantial changes.
3. Any specialized claim changed by the patch is exercised by its corresponding opt-in suite.

More commands are not inherently stronger evidence. Verification should be proportional to the code and claim being changed.
