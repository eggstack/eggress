# AGENTS.md

## Repository purpose

Egress is a Rust-native, embeddable, multi-protocol proxy framework and CLI. It targets practical and behavioral compatibility with Python `pproxy`, including a Rust CLI, an embed API, PyO3 bindings, and a separate opt-in Python compatibility distribution.

Compatibility claims must remain explicit and evidence-backed. Strict full drop-in parity is not assumed merely because a symbol, protocol name, or structural wrapper exists.

## Source-of-truth documents

Use these current documents before relying on historical phase or completion records:

- `docs/CI_STATUS.md`: CI and verification policy.
- `docs/TESTING.md`: local, specialized, interoperability, performance, and fuzz testing.
- `docs/release/RELEASE_PROCESS.md`: manual release policy.
- `docs/ARCHITECTURE.md`: system architecture.
- `docs/DIFFERENTIAL_TESTING.md`: pproxy oracle and differential harness.
- `docs/parity/pproxy_capability_manifest.toml`: canonical capability contract.
- `docs/parity/pproxy_2_7_9_strict_manifest.toml`: strict behavioral contract.
- `docs/PPROXY_PARITY_SPEC.md`: compatibility vocabulary and tier definitions.

Files under `plans/` and phase-completion documents are historical implementation records. They may explain why code exists, but they do not override current policy or current source behavior.

## Verification policy

Use focused local tests during iteration. Run the narrowest command that exercises the changed code, for example:

```bash
cargo test -p eggress-routing
cargo test -p eggress-runtime retry_fallback
cargo test -p eggress-cli --test cli_exit_codes
```

Before merging a substantial Rust change, run the broad workspace gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

For Python-facing changes, build the native wheel in a clean environment and run the relevant Python tests:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin build --release --out ../../target/wheels)
.venv/bin/python -m pip install target/wheels/eggress-*.whl
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q
```

Do not automatically run security audits, operating-system matrices, ignored interoperability suites, benchmarks, fuzzing, soak tests, parity-report generation, or release-evidence scripts for an unrelated change. Run specialized checks only when the affected subsystem or compatibility claim requires them.

Dependency and advisory checks are expected for dependency changes and release preparation:

```bash
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134
```

External compatibility checks are opt-in. Examples:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
  cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1

EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 \
  cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1

./scripts/run_pproxy_certification.sh
```

Ordinary changes do not require generated evidence bundles, uploaded artifacts, screenshots, copied command transcripts, or new completion documents. Record the relevant tests in the commit or pull-request summary when useful.

## Hosted CI boundary

The repository intentionally has two automatic workflows:

- `.github/workflows/ci.yml`: one Ubuntu Rust smoke job running format, Clippy, and workspace tests.
- `.github/workflows/python-test.yml`: one path-scoped Ubuntu/Python 3.12 smoke job.

Do not recreate tag-triggered publishing, release artifact matrices, automated GitHub Releases, container publishing, continuous parity evidence generation, or mandatory external interoperability workflows without an explicit project-level decision.

Hosted CI is a smoke signal. It is not the release mechanism and is not a reason to duplicate every available local check.

## Release policy

Release cadence is manual. The release operator performs local verification, package dry runs, and `cargo publish` directly to crates.io.

Git tags and GitHub Releases are optional manual bookkeeping after crates.io publication. Pushing a tag must not publish packages or create artifacts through GitHub Actions.

Crates.io versions are immutable. If a release is defective or partially published, increment the version and roll forward; do not attempt to replace an existing version.

Python/PyPI publication is a separate manual process and is not coupled to the Rust release workflow.

## Workspace map

The principal crates are:

- `eggress-core`: shared types, traits, relay abstractions, and stream boundaries.
- `eggress-uri`: URI parsing and compatibility grammar.
- `eggress-routing`: rules, schedulers, health state, and route selection.
- `eggress-config`: TOML configuration and validation.
- `eggress-server`: listener and connection orchestration.
- `eggress-runtime`: supervisor, lifecycle, composition, reload, and shutdown.
- `eggress-udp`: UDP associations and relays.
- `eggress-protocol-*`: HTTP, SOCKS, Shadowsocks, Trojan, WebSocket, raw, and reverse protocols.
- `eggress-transport-tls`: shared TLS client/server transport.
- `eggress-cli`: `eggress` and compatibility `pproxy` binaries.
- `eggress-pproxy-compat`: Rust-side URI translation and compatibility diagnostics.
- `eggress-embed`: stable in-process Rust API.
- `eggress-python`: PyO3 binding crate.
- `python/eggress`: canonical Python package.
- `python-pproxy-compat`: separate package providing the top-level `pproxy` namespace.
- `eggress-testkit`: oracle, manifest, corpus, and compatibility test utilities.

## Architectural invariants

Preserve these invariants unless a focused design change explicitly replaces them:

- Streams are boxed at protocol and transport boundaries; avoid propagating generic stream types through the architecture.
- Credentials and secret-bearing URIs must be redacted before logging, diagnostics, or evidence output.
- Listener topology is not hot-reloaded; routing, upstream, group, and health state may be replaced atomically.
- Shutdown ordering is readiness false, listener stop, connection drain/cancellation, then admin shutdown.
- Runtime routing, health, admin, and metrics should share the same compiled runtime snapshot rather than duplicate state.
- Protocol and transport composition must be validated before execution.
- Platform-specific behavior must remain explicit and honestly classified.
- Workspace unsafe-code restrictions must not be weakened casually.

## Compatibility discipline

The reference implementation is pinned Python `pproxy==2.7.9` where oracle comparison is required.

A compatibility claim should distinguish among behavioral match, compatible with warning, native equivalent, intentional non-parity, and unsupported behavior. Do not upgrade a tier based only on API shape, type names, imports, or successful construction.

When a compatibility claim changes, update the applicable manifest and run the corresponding oracle, differential, or interoperability suite. Generated reports are consequences of the manifest and evidence; they are not independent sources of truth.

Unsupported transports or roles should fail with structured, actionable diagnostics rather than silent fallback.

The canonical `eggress` Python package must not silently install or alias the top-level `pproxy` namespace. That namespace belongs to the separate `eggress-pproxy-compat` distribution.

## Code conventions

- Rust edition 2021; MSRV is declared in the workspace manifest.
- Tokio is the async runtime.
- Use `thiserror` for structured errors and `tracing` for logging.
- Keep protocol parsing bounded and defensive.
- Prefer deterministic tests over fixed sleeps; use retry loops or readiness signals for process/network startup.
- Preserve stable diagnostic and exit-code semantics where they are part of the compatibility surface.
- Add dependencies only when the maintenance and binary-size cost is justified.
- Do not add OpenSSL, C dependencies, or build scripts without an explicit architectural reason.

## Change discipline

Keep changes narrowly scoped. Avoid mixing capability implementation, test-infrastructure redesign, documentation mass-generation, and release-process changes in one patch unless they are inseparable.

Do not interpret a historical plan as a mandate to recreate removed verification machinery. Current policy favors fast local iteration, proportional verification, honest compatibility claims, and explicit manual release control.
