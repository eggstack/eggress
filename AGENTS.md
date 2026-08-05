# AGENTS.md

## Repository purpose

Egress is a Rust-native, embeddable, multi-protocol proxy framework and CLI. It targets practical and behavioral compatibility with Python `pproxy==2.7.9`, including a Rust CLI, an embed API, PyO3 bindings, and a bounded Python drop-in namespace bundled in the main distribution.

Compatibility claims must remain explicit and evidence-backed. Strict full drop-in parity is not assumed merely because a symbol, protocol name, or structural wrapper exists. The active compatibility target is practical parity with `pproxy==2.7.9`. The repository publishes one Python distribution, `eggress`; its wheel provides a bounded top-level `pproxy` package and does not provide a separate `eggress-pproxy-compat` distribution. Treat the native runtime and the compatibility translator as separate surfaces, especially for H2, WS/WSS, raw, and tunnel transports.

The maintained public matrix is `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`, and the optional representative scenario index is `docs/parity/PPROXY_CLOSURE_SCENARIOS.md`. These replace aggregate parity percentages and historical certification claims; routine CI remains focused and does not require the external pproxy oracle.

Key compatibility surface notes:
- `--pac`, `--get`, and `--test` are value-taking options; their values must not be reclassified as positional listeners or remotes. Parsed-but-unsupported fields require precise diagnostics and redacted output.
- H2/WS/WSS remain upstream-only; bounded raw/tunnel listener forms are covered by Phase 5. QUIC/HTTP/3 remains intentionally deferred.
- Bounded fixed-target TCP/UDP raw/tunnel forms, Unix domain TCP upstreams on Unix, and per-connection outbound local binds are supported. These do not establish general multi-hop UDP, macOS PF transparent recovery, backward TLS, daemonization, or connection-reuse parity.

Lean feature builds are truthful: internal workspace edges for `eggress-runtime`,
`eggress-embed`, `eggress-udp`, `eggress-server`, and `eggress-metrics` disable
dependency defaults, while `eggress-cli/common` explicitly forwards
`eggress-runtime/common`. The internal `eggress-udp/shadowsocks` gate is
enabled by `extended`; common UDP routing reports Shadowsocks as unsupported
instead of falling back to direct UDP. The `operations` feature gates the admin
HTTP server, Prometheus metrics export, and system-proxy integration. The
`reverse` feature requires `operations`. The data plane retains a lightweight
metrics registry for counter tracking even in common builds, but the full
admin/metrics export layer is absent.

The HTTP forwarder rejects non-empty `Expect` headers with a bounded 417/close
exchange, follows at most eight informational responses, rejects 101 upgrades,
and bounds body upload with the configured connect timeout. It is intentionally
not a general full-duplex request/response pump.

## Source-of-truth documents

Use these current documents before relying on historical phase or completion records:

- `docs/CI_STATUS.md`: CI and verification policy.
- `docs/TESTING.md`: local, specialized, interoperability, performance, and fuzz testing.
- `docs/release/RELEASE_PROCESS.md`: manual release policy.
- `docs/ARCHITECTURE.md`: system architecture.
- `docs/DIFFERENTIAL_TESTING.md`: pproxy oracle and differential harness.
- `docs/parity/pproxy_capability_manifest.toml`: canonical capability contract.
- `docs/parity/pproxy_2_7_9_strict_manifest.toml`: strict behavioral contract.
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`: maintained public matrix.
- `docs/parity/PPROXY_CLOSURE_SCENARIOS.md`: optional closure scenario index.
- `docs/PPROXY_PARITY_SPEC.md`: compatibility vocabulary and tier definitions.

Files under `plans/` and phase-completion documents are historical implementation records. They may explain why code exists, but they do not override current policy or current source behavior.

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md` governs current parity work.
`plans/PPROXY_FULL_DROP_IN_ROADMAP.md` and older milestone plans are historical
and are not acceptance gates for the bounded roadmap.

## Architecture index

Quick reference to per-subsystem architecture documents under `docs/architecture/`:

| Subsystem | Document | Description |
|-----------|----------|-------------|
| System overview | `docs/architecture/overview.md` | Entry points, crate dependency graph, design principles |
| Core types | `docs/architecture/core.md` | `BoxStream`, `TargetAddr`, `ProtocolId`, relay, chain, detection |
| Routing | `docs/architecture/routing.md` | Rule engine, schedulers, health state machine, route explanation |
| Config | `docs/architecture/config.md` | TOML schema, validation, secret sources, compilation |
| Runtime | `docs/architecture/runtime.md` | Supervisor, snapshot compilation, reload, shutdown ordering |
| Server | `docs/architecture/server.md` | Session lifecycle, accept/route/reply/report |
| Admin | `docs/architecture/admin.md` | Admin API, PAC, static content, route explanation |
| Metrics | `docs/architecture/metrics.md` | Prometheus counters, bounded cardinality |
| HTTP protocols | `docs/architecture/protocols-http.md` | HTTP CONNECT, H2 CONNECT, forward proxy |
| SOCKS protocols | `docs/architecture/protocols-socks.md` | SOCKS4/4a, SOCKS5, UDP ASSOCIATE |
| Shadowsocks | `docs/architecture/protocols-shadowsocks.md` | AEAD ciphers, SIP003 framing |
| Trojan | `docs/architecture/protocols-trojan.md` | Trojan client/server, TLS transport |
| WebSocket | `docs/architecture/protocols-websocket.md` | WS/WSS tunnels, stream-native composition |
| Raw | `docs/architecture/protocols-raw.md` | Fixed-target TCP forwarding; bounded UDP mode is documented under `udp.md` |
| Reverse | `docs/architecture/protocols-reverse.md` | Control-channel backward proxy |
| TLS transport | `docs/architecture/transport-tls.md` | rustls client/server, ALPN, certificate handling |
| UDP | `docs/architecture/udp.md` | Associations, target flows, relay, security |
| URI parsing | `docs/architecture/uri.md` | Typed AST, redacted Display, pproxy grammar |
| Embed API | `docs/architecture/embed.md` | `EggressConfig`, `EggressService`, `EggressHandle` |
| Python bindings | `docs/architecture/python.md` | PyO3 classes, pproxy-shaped migration API, Connection, Server |
| pproxy compat | `docs/architecture/pproxy-compat.md` | URI translation, diagnostics, manifest validation |
| CLI | `docs/architecture/cli.md` | Binary targets, pproxy-style translator, exit codes |
| System proxy | `docs/architecture/system-proxy.md` | macOS/Windows system proxy configuration |
| Testkit | `docs/architecture/testkit.md` | Oracle, differential, fixtures, manifest validation |
| Tools/scripts | `docs/architecture/tools-and-scripts.md` | Helper and validation scripts |

## Skills

Agent skills live in `.skills/` (canonical) and are symlinked from `.agents/skills/`. Each skill provides focused guidance for a subsystem:

| Skill | When to use |
|-------|-------------|
| `rust-proxy-dev` | New protocols, transport wrappers, core relay/chain, Python bindings |
| `config-reload` | Config schema, TOML parsing, hot-reload, supervisor lifecycle |
| `routing-rules` | Routing rules, matchers, schedulers, route selection |
| `testing` | Writing tests, test infrastructure, fuzz, differential, oracle |
| `udp-protocol` | UDP associations, datagram relay, upstream SOCKS5 relay |
| `advanced-transports` | H2 CONNECT, WebSocket tunnels, raw tunnels, TLS/ALPN |
| `release` | Version bumps, PyPI/crates.io publication, release verification |
| `reverse-proxy` | Reverse/backward proxy, NAT traversal, control-channel proxying |
| `security-dev` | Security features, hardening, fuzz targets, invariant tests |

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
.venv/bin/python -m pytest python/tests tests/compat -q
```

For faster iteration during Python development, use `maturin develop` instead of the full build-install cycle:

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
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

Fuzz targets live in the standalone `fuzz/` workspace (separate `Cargo.toml`). Compile-check or run only targets relevant to the changed parser:

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo fuzz run uri_parse -- -runs=1000
```

Test-gating environment variables:

| Variable | Purpose |
|----------|---------|
| `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` | Enable pproxy differential tests |
| `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1` | Enable Shadowsocks wire-format interop |
| `EGRESS_REQUIRE_REVERSE_INTEROP=1` | Enable reverse proxy pproxy interop |
| `EGRESS_REQUIRE_SOAK=1` | Enable soak/performance tests |
| `EGRESS_RUN_PPROXY_DIFFERENTIAL=1` | Enable differential parity harness |

Ordinary changes do not require generated evidence bundles, uploaded artifacts, screenshots, copied command transcripts, or new completion documents. Record the relevant tests in the commit or pull-request summary when useful.

## Hosted CI boundary

The repository has two automatic smoke workflows and one manual publish workflow:

- `.github/workflows/ci.yml`: one Ubuntu Rust smoke job running format, Clippy, and workspace tests.
- `.github/workflows/python-test.yml`: one path-scoped Ubuntu/Python 3.12 smoke job.
- `.github/workflows/publish-python.yml`: multi-platform wheel and sdist build, smoke tests, and publication to PyPI/TestPyPI via OIDC trusted publishers. Triggers on tag push (`v*`) or manual dispatch. Builds wheels for Linux (x86_64, aarch64), macOS (x86_64, arm64), and Windows (x86_64) using the Python stable ABI.

The release-only workflow uses a manylinux2014/glibc 2.17 floor, structurally
parses exact TOML version fields, hard-rejects any artifact set other than the
five approved `cp39-abi3` wheels plus one sdist, and runs
`scripts/release_artifact_smoke.py` against installed artifacts. A successful
manual TestPyPI run is required before production tagging.

Do not recreate release artifact matrices, automated GitHub Releases, container publishing, continuous parity evidence generation, or mandatory external interoperability workflows without an explicit project-level decision.

Hosted CI is a smoke signal. It is not the release mechanism and is not a reason to duplicate every available local check.

## Release policy

Python publication to PyPI uses OIDC trusted publishers via `.github/workflows/publish-python.yml`. Push a `v*` tag or use manual dispatch to trigger publication. The workflow requires repository environments `pypi` and `testpypi`. Production publication enforces version coherence among the tag, workspace, binding crate, and pyproject, builds wheels for the approved platform set, runs native and compatibility smoke tests, and fails rather than silently skipping an existing version.

Rust crate publication to crates.io is manual. The release operator performs local verification, package dry runs, and `cargo publish` directly to crates.io.

Git tags and GitHub Releases are optional manual bookkeeping. Pushing a tag triggers the Python publish workflow but must not publish Rust crates or create release artifacts.

Crates.io versions are immutable. If a release is defective or partially published, increment the version and roll forward; do not attempt to replace an existing version.

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
- `python/pproxy`: bounded top-level `pproxy` compatibility namespace bundled in the wheel.
- `eggress-testkit`: oracle, manifest, corpus, and compatibility test utilities.

Note: the root `Cargo.toml` also defines a `eggress-bench` package (not a workspace member) with Criterion benchmarks. Run `cargo bench` from the workspace root.

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

The canonical `eggress` distribution owns both the `eggress.*` namespace and
the bounded `pproxy.*` compatibility namespace. Installing upstream `pproxy`
alongside Eggress is unsupported; uninstall it before replacing it with Eggress.

## Code conventions

- Rust edition 2021; MSRV 1.75 (see `workspace.package.rust-version` in root `Cargo.toml`). `rust-toolchain.toml` pins stable channel with `clippy` and `rustfmt` components.
- `unsafe_code = "deny"` at workspace level; do not add unsafe without explicit justification.
- Tokio is the async runtime; `tokio::main` and `tokio::test` are used throughout.
- Use `thiserror` for structured errors and `tracing` for logging.
- Keep protocol parsing bounded and defensive.
- Prefer deterministic tests over fixed sleeps; use retry loops or readiness signals for process/network startup.
- Preserve stable diagnostic and exit-code semantics where they are part of the compatibility surface.
- Add dependencies only when the maintenance and binary-size cost is justified.
- Do not add OpenSSL, C dependencies, or build scripts without an explicit architectural reason.
- `deny.toml` bans `openssl-sys`, `native-tls`, `aws-lc-sys`, and `cmake` from the dependency graph.
- `cargo check` is not a separate required gate (Clippy and test builds already compile the workspace) but is useful interactively for faster compile-only feedback.

## Change discipline

Keep changes narrowly scoped. Avoid mixing capability implementation, test-infrastructure redesign, documentation mass-generation, and release-process changes in one patch unless they are inseparable.

Do not interpret a historical plan as a mandate to recreate removed verification machinery. Current policy favors fast local iteration, proportional verification, honest compatibility claims, and explicit manual release control.
