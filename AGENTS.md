# AGENTS.md

Rust-native, embeddable multi-protocol proxy framework + CLI targeting practical compatibility with Python `pproxy==2.7.9` (oracle pin). Strict drop-in parity is never assumed from names/shapes alone.

## Layout

- 26 crates in `crates/`: `eggress-core` / `eggress-uri` / `eggress-config` / `eggress-routing` / `eggress-metrics` (foundation), `eggress-server` / `eggress-runtime` / `eggress-admin` / `eggress-udp` / `eggress-system-proxy`, `eggress-protocol-{http,socks,shadowsocks,trojan,websocket,raw,reverse,h3}`, `eggress-transport-{tls,ssh,quic}`, facades `eggress-cli` (installs both `eggress` and `pproxy` binaries) / `eggress-embed` / `eggress-python` / `eggress-pproxy-compat`, test support `eggress-testkit`. Root package is `eggress-bench` (Criterion in `benches/`).
- `python/eggress` is the canonical Python package; only `python-pproxy-compat/` may own the top-level `pproxy` namespace. Never alias `pproxy` from `eggress`.
- Start subsystem work at `architecture/overview.md` (index into per-component deep dives), not by re-deriving layout from source. Task→deep-dive map: CLI/ops → `cli.md` (+`admin.md`, `metrics.md`, `system-proxy.md`); protocols/transports → `protocols-*.md` / `transports-*.md`; embedding → `embed.md` / `python-bindings.md`; compat claims → `pproxy-compat.md`; config/reload/lifecycle → `config.md` / `runtime.md`; UDP/reverse → `udp.md` / `protocols-reverse.md`; verification → `testing-and-tooling.md`.
- Skills live in `.skills/` (mirrored via symlinks into `.agents/skills/`, `.opencode/skills/` — add new skills in `.skills/` plus a symlink in each mirror). Load via the `skill` tool: `rust-proxy-dev`, `python-bindings`, `testing` are the most general; `cli-ops` covers native/compat CLI, exit codes, diagnostics, admin, metrics, system-proxy; also `security-dev`, `config-reload`, `routing-rules`, `udp-protocol`, `advanced-transports`, `reverse-proxy`, `release`.
- `plans/` and phase-completion docs are historical; `docs/parity/pproxy_capability_manifest.toml` + `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` are the authoritative compat contract. `docs/CI_STATUS.md` is the verification policy; `docs/TESTING.md` has the full suite inventory.

## Verify

Prefer the narrowest test, then the broad gate only before merging substantial Rust changes:

```bash
cargo test -p eggress-routing
cargo test -p eggress-runtime retry_fallback
cargo test -p eggress-cli --test cli_exit_codes
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

- Default `full` feature leaves off `ssh`, `quic`, `pproxy-legacy`, `legacy-crypto`, `pproxy-daemon`: after touching those paths also run `cargo check -p eggress-cli --locked --no-default-features --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins`. Never substitute `--all-features` (drags in test-only `insecure-quic`).
- Python-facing changes: `(cd crates/eggress-python && ../../.venv/bin/maturin develop)` after creating `.venv` with `maturin>=1.0,<2.0`, `pytest`, `pytest-asyncio>=0.23,<1`, `cryptography>=42,<47`; also `pip install --no-deps ./python-pproxy-compat`. Always run pytest from repo root — `pytest.ini` forces `--import-mode=importlib` so `python/eggress` can't shadow the built `_eggress` extension.
- `fuzz/` is a standalone workspace: `cargo check --manifest-path fuzz/Cargo.toml --bins`. Workspace commands don't cover it.
- External suites are opt-in (they install/launch outside implementations); run only when the claim changed. Oracle is resolved from `$EGRESS_ORACLE_PYTHON`, then `$EGRESS_PYTHON_BIN`, then discovery; prebuilt venvs (`.venv-oracle`, `.venv-pproxy-279`) already exist at root:
```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
```
- Dependency changes / release prep only: `cargo deny check` and `cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0009`. Don't run audits, OS matrices, ignored interop, benches, soak, fuzz, or parity-report generation for unrelated changes.

## Invariants agents miss

- Redact credentials/secret-bearing URIs before logging, diagnostics, or evidence output.
- Box streams at protocol/transport boundaries; don't leak generic stream types through the architecture. Validate protocol/transport composition before execution; unsupported transports/roles fail closed with structured diagnostics, never silent fallback.
- Listener topology is not hot-reloaded; only routing/upstream/group/health state swaps atomically. Shutdown order: readiness false → listener stop → connection drain/cancel → admin shutdown.
- Compat claims use tier vocabulary (`matched` / `supported_difference` / `platform_limited` / `intentional_non_parity`); changing a claim means updating the manifest and running the oracle/differential/interop suite. Generated reports follow the manifest, never lead it.
- Edition 2021, MSRV 1.85 (release contract — don't reopen pinning to recover older toolchains), `unsafe_code = "deny"`. Tokio + `thiserror` + `tracing`. No OpenSSL / C deps / build scripts without explicit architectural reason; keep protocol parsing bounded.
- Keep changes narrowly scoped; ordinary work needs a clear message + relevant tests, not evidence bundles, screenshots, transcripts, or new completion docs.

## CI / release boundary

- 3 workflows only; don't add more without a project-level decision. `ci.yml` is Ubuntu Rust smoke; `python-test.yml` is path-scoped 3.12 smoke. **`publish-python.yml` fires on every `v*` tag push** — pushing a version tag publishes to PyPI (protected `pypi` env; TestPyPI via manual dispatch only). Never push tags casually.
- Crates.io is fully manual (`cargo publish --dry-run`, then dependency order, facades last). Versions immutable: roll forward, never replace.
- Version bumps move 4 places in lockstep or the publish workflow hard-fails: root `[workspace.package]` version, every internal `=x.y.z` pin in `[workspace.dependencies]`, `crates/eggress-python/pyproject.toml`, `python-pproxy-compat/pyproject.toml`.
