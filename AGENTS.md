# AGENTS.md

## Repository purpose

Egress is a Rust-native, embeddable, multi-protocol proxy framework and CLI. It targets practical and behavioral compatibility with Python `pproxy`, including a Rust CLI, an embed API, PyO3 bindings, and a separate opt-in Python compatibility distribution.

Compatibility claims must remain explicit and evidence-backed. Strict full drop-in parity is not assumed merely because a symbol, protocol name, or structural wrapper exists.

## Source-of-truth documents

Use these current documents before relying on historical phase or completion records:

- `docs/CI_STATUS.md`: CI and verification policy.
- `docs/TESTING.md`: local, specialized, interoperability, performance, and fuzz testing.
- `docs/release/RELEASE_PROCESS.md`: release policy (manual crates.io; tag-triggered PyPI workflow).
- `architecture/overview.md`: bird's-eye architecture map and index into the per-component deep dives.
- `docs/ARCHITECTURE.md`: long-form system architecture narrative.
- `docs/DIFFERENTIAL_TESTING.md`: pproxy oracle and differential harness.
- `docs/parity/pproxy_capability_manifest.toml`: canonical capability contract.
- `docs/parity/pproxy_2_7_9_strict_manifest.toml`: strict behavioral contract (historical provenance for the active manifest).
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

For Python-facing changes, build the extension and run the relevant Python tests:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q
```

Run pytest from the repo root: `pytest.ini` forces `--import-mode=importlib` specifically so the `python/eggress` source tree cannot shadow the installed wheel's compiled `_eggress` extension.

Do not automatically run security audits, operating-system matrices, ignored interoperability suites, benchmarks, fuzzing, soak tests, parity-report generation, or release-evidence scripts for an unrelated change. Run specialized checks only when the affected subsystem or compatibility claim requires them.

Dependency and advisory checks are expected for dependency changes and release preparation:

```bash
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0009
```

External compatibility checks are opt-in because they install and launch external implementations:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
  cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1

EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 \
  cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1

./scripts/run_pproxy_certification.sh
```

The oracle interpreter must provide `pproxy==2.7.9`; it is resolved from `$EGRESS_ORACLE_PYTHON`, then legacy `$EGRESS_PYTHON_BIN`, then discovery (`find_oracle_python` in `eggress-testkit`). Prebuilt oracle venvs already exist at the repo root (e.g. `.venv-oracle`, `.venv-pproxy-279`).

`fuzz/` is a standalone Cargo workspace; workspace-wide commands do not cover it (`cargo check --manifest-path fuzz/Cargo.toml --bins`).

Ordinary changes do not require generated evidence bundles, uploaded artifacts, screenshots, copied command transcripts, or new completion documents. Record the relevant tests in the commit or pull-request summary when useful.

## CI boundary

There are three hosted workflows, and one of them is a real release path:

- `.github/workflows/ci.yml`: Ubuntu Rust smoke (fmt check, clippy `-D warnings`, `cargo test --workspace --locked`).
- `.github/workflows/python-test.yml`: path-scoped Ubuntu/Python 3.12 smoke for the Python packages.
- `.github/workflows/publish-python.yml`: **fires on every `v*` tag push.** It validates the tag against the workspace version, builds the five-platform wheels plus sdist, smoke-tests them, and publishes to PyPI through the protected `pypi` GitHub environment (TestPyPI only via manual dispatch). Pushing a version tag is a release action, not bookkeeping.

Do not create additional GitHub Actions workflows without an explicit project-level decision. `docs/CI_STATUS.md` and `docs/release/RELEASE_PROCESS.md` describe the tag-triggered PyPI publish path and are kept in sync with the workflow.

## Release policy

Rust crates.io publication is manual: local verification, dry runs, then `cargo publish` in dependency order (top-level facades last). No workflow publishes crates or creates GitHub Releases.

Crates.io versions are immutable. If a release is defective or partially published, increment the version and roll forward; do not attempt to replace an existing version.

A version bump must move several places in lockstep: `[workspace.package]` `version` in the root `Cargo.toml`, every internal `=x.y.z` pin under `[workspace.dependencies]`, `crates/eggress-python/pyproject.toml`, and `python-pproxy-compat/pyproject.toml`. The publish workflow hard-fails on tag/version mismatch.

## Skills

Agent skills live under `.skills/` and provide focused, task-specific guidance.
Each skill contains a `skill.md` with when-to-use context, architecture, key
types, verification commands, and common pitfalls.

| Skill | Purpose |
|-------|---------|
| `rust-proxy-dev` | New proxy protocols, transport wrappers, core relay/chain, embed API, pproxy compatibility binary |
| `python-bindings` | PyO3 extension, `python/eggress` package, pproxy namespace rules, wheel packaging |
| `testing` | Test layers, fuzz harnesses, differential/oracle harnesses, Python tests, benchmarking |
| `security-dev` | DNS rebinding, auth metrics, SSH boundary, legacy crypto, fuzz targets, security invariants |
| `config-reload` | TOML schema, hot-reload behavior, supervisor lifecycle, adding config fields |
| `routing-rules` | Rule matchers, schedulers, route selection, adding new matchers |
| `udp-protocol` | UDP associations, datagram relay, upstream SOCKS5 relay |
| `advanced-transports` | H2 CONNECT, WebSocket, raw tunnels, QUIC/H3, TLS/ALPN |
| `reverse-proxy` | Reverse/backward proxy, control channels, NAT traversal |
| `release` | Version bumps, verification, PyPI/crates.io publishing |

Load a skill with the `skill` tool when a task matches its description. The
`rust-proxy-dev`, `python-bindings`, and `testing` skills are the most broadly
applicable. Skills are mirrored into `.agents/skills/` and
`.opencode/skills/` via relative symlinks; add a symlink to every mirror when
adding a skill.

## Architecture deep dives

`architecture/overview.md` is the maintained bird's-eye map and component
index; each sibling file is a focused review guide for one component. Start a
subsystem task from the matching deep dive rather than re-deriving layout
from source:

| Deep dive | Scope |
|---|---|
| [architecture/core.md](architecture/core.md) | `eggress-core`: BoxStream, relay, detection/dispatch, ChainExecutor, rebinding guard |
| [architecture/uri.md](architecture/uri.md) | `eggress-uri`: chain AST, `+`/`__` grammar, redaction |
| [architecture/config.md](architecture/config.md) | `eggress-config`: TOML schema, validation, secrets, compilation |
| [architecture/routing.md](architecture/routing.md) | `eggress-routing`: matchers, schedulers, health hysteresis, leases |
| [architecture/metrics.md](architecture/metrics.md) | `eggress-metrics`: registry, subsystem bridges |
| [architecture/server.md](architecture/server.md) | `eggress-server`: serve pipeline, session reports, reply semantics |
| [architecture/runtime.md](architecture/runtime.md) | `eggress-runtime`: snapshots, reload, signals, shutdown ordering |
| [architecture/admin.md](architecture/admin.md) | `eggress-admin`: /-/endpoints, PAC, route-explain |
| [architecture/udp.md](architecture/udp.md) | `eggress-udp`: associations, flows, upstream relay, standalone modes |
| [architecture/system-proxy.md](architecture/system-proxy.md) | `eggress-system-proxy`: OS proxy inspect/apply/rollback |
| [architecture/protocols-http.md](architecture/protocols-http.md) | HTTP CONNECT/forward + H2 pool |
| [architecture/protocols-socks.md](architecture/protocols-socks.md) | SOCKS4/4a + SOCKS5 |
| [architecture/protocols-shadowsocks.md](architecture/protocols-shadowsocks.md) | Shadowsocks AEAD + legacy/SSR gates |
| [architecture/protocols-trojan.md](architecture/protocols-trojan.md) | Trojan over rustls |
| [architecture/protocols-tunnels.md](architecture/protocols-tunnels.md) | WebSocket + raw tunnels |
| [architecture/protocols-reverse.md](architecture/protocols-reverse.md) | Reverse/backward proxy |
| [architecture/transports-tls.md](architecture/transports-tls.md) | TLS/ALPN (rustls only) |
| [architecture/transports-ssh-quic-h3.md](architecture/transports-ssh-quic-h3.md) | SSH, QUIC streams, HTTP/3 CONNECT (opt-in features) |
| [architecture/cli.md](architecture/cli.md) | `eggress-cli`: both binaries, flags, exit codes, lean builds |
| [architecture/embed.md](architecture/embed.md) | `eggress-embed`: in-process service lifecycle, OutboundConnector |
| [architecture/python-bindings.md](architecture/python-bindings.md) | PyO3 extension and `python/eggress` package |
| [architecture/pproxy-compat.md](architecture/pproxy-compat.md) | Translation/check/run surface and the compat distributions |
| [architecture/testing-and-tooling.md](architecture/testing-and-tooling.md) | Testkit, fuzz, benches, scripts, oracle assets |

Keep these files accurate when moving or renaming modules they describe;
they are grounded in current source layout and public APIs.

## Workspace map

The root `Cargo.toml` package is `eggress-bench` (Criterion benches in `benches/`). The workspace itself holds 26 crates under `crates/`, grouped by role (each role maps to a deep dive in the [Architecture deep dives](#architecture-deep-dives) section above):

- Foundation: `eggress-core` (shared types, traits, relay, boxed stream boundaries), `eggress-uri` (URI parsing/compatibility grammar), `eggress-config` (TOML schema and validation), `eggress-routing` (rules, schedulers, health state, route selection), `eggress-metrics`.
- Runtime: `eggress-server` (listener/connection orchestration), `eggress-runtime` (supervisor, lifecycle, reload, shutdown), `eggress-admin` (local admin HTTP: PAC, metrics, status, route explanation), `eggress-udp`, `eggress-system-proxy`.
- Protocols: `eggress-protocol-{http,socks,shadowsocks,trojan,websocket,raw,reverse,h3}` — HTTP CONNECT/forward + H2, SOCKS4/4a + SOCKS5, Shadowsocks AEAD/legacy, Trojan over rustls, WebSocket tunnel, raw passthrough, reverse control channel, HTTP/3 CONNECT.
- Transports: `eggress-transport-{tls,ssh,quic}`; ssh/quic are optional features.
- Facades: `eggress-cli` (installs both `eggress` and compatibility `pproxy` binaries), `eggress-embed` (stable in-process Rust API), `eggress-python` (PyO3 bindings), `eggress-pproxy-compat` (Rust-side URI translation and diagnostics).
- Test support: `eggress-testkit` (oracle resolution, manifests, corpora, differential harnesses).

Top-level `python/eggress` is the canonical Python package; the separate `python-pproxy-compat/` distribution owns the top-level `pproxy` namespace; `tests/` holds cross-implementation Python tests (`tests/compat`).

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

- Rust edition 2021. The workspace MSRV is **1.85** (declared in
  `[workspace.package]` `rust-version`); treat it as a release contract.
  The raised floor follows the maintained `russh` release and is not an
  incidental dependency side effect; do not reopen dependency pinning or
  split-toolchain architecture to recover an older floor without an
  explicit user-requirement or build-failure reason.
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
