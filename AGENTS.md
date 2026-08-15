# AGENTS.md

## Repository purpose

Egress is a Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical behavioral parity with Python `pproxy==2.7.9`. One Python distribution (`eggress`) ships both `eggress.*` and a bounded `pproxy.*` compatibility namespace; uninstall upstream `pproxy` before installing.

## Quick reference commands

**Focused test during iteration:**
```bash
cargo test -p eggress-routing
cargo test -p eggress-runtime retry_fallback
cargo test -p eggress-cli --test cli_exit_codes
python3 -m pytest python/tests/test_pproxy_phase4_contract.py -q
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml --check-matrix docs/parity/composition_matrix.toml
```

**Broad pre-merge gate:**
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check -p eggress-cli --no-default-features --features common
```

**Python changes (build + test):**
```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

**Dependency/advisory checks (dependency changes or release prep only):**
```bash
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134
```

**Fuzz targets (standalone `fuzz/` workspace):**
```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo fuzz run uri_parse -- -runs=1000
```

## Workspace structure

Root `Cargo.toml` defines a workspace with 23 member crates under `crates/`. A non-member `eggress-bench` package with Criterion benchmarks lives at the workspace root — run `cargo bench` from root.

Principal crates:
- `eggress-core`: shared types, traits, `BoxStream`, relay abstractions
- `eggress-cli`: `eggress` and `pproxy` binary targets
- `eggress-embed`: stable in-process Rust API (`EggressService`, `EggressConfig`)
- `eggress-python`: PyO3 bindings (builds the Python wheel)
- `eggress-runtime`: supervisor, lifecycle, reload, shutdown
- `eggress-server`: listener and connection orchestration
- `eggress-config`: TOML config and validation
- `eggress-routing`: rules, schedulers, health state, route selection
- `eggress-protocol-*`: HTTP, SOCKS, Shadowsocks, Trojan, WebSocket, raw, reverse
- `eggress-transport-tls`: rustls client/server transport
- `eggress-testkit`: oracle, manifest, corpus, compatibility test utilities
- `eggress-pproxy-compat`: Rust-side URI translation and diagnostics
- `python/eggress`, `python/pproxy`: Python packages bundled in the wheel

The wheel ships the complete Phase 0 `pproxy` module namespace: all ten tracked
modules, the shared `python -m pproxy`/console entry point, and adapters for
verbose helpers and platform system-proxy behavior.

## Feature gates

The `operations` feature gates the admin HTTP server, Prometheus metrics export, and system-proxy integration. The `reverse` feature requires `operations`. Lean builds (`--no-default-features --features common`) exclude extended protocols (Shadowsocks, Trojan, WebSocket), reverse runtime, system-proxy, and compatibility layers.

The `eggress-udp/shadowsocks` gate is enabled by `extended`; common builds report Shadowsocks as unsupported instead of falling back to direct UDP. The `pproxy-legacy` feature enables the isolated SSR/plugin compatibility path; native builds do not need it.

Shadowsocks strict AEAD coverage includes `aes-128-gcm`, `aes-192-gcm`,
`aes-256-gcm`, and `chacha20-ietf-poly1305`. Their pproxy 2.7.9 salt/IV sizes
are respectively 16, 24, 32, and 32 bytes; all use 12-byte nonces and
16-byte tags. The TCP chunk limit follows pproxy's 16 KiB - 1 byte packet
limit. Keep the method inventory and wire-format claims synchronized with
`docs/architecture/protocols-shadowsocks.md` and the phase-0 parity manifest.

## Test locations

- Unit tests: in each crate's `src/` files
- Integration tests: `crates/eggress-runtime/tests/` (startup, routing, health, admin, reload, shutdown, UDP, upstream protocols, load)
- Property tests: per-crate `tests/` (proptest round-trips for SOCKS, HTTP, Trojan, routing)
- Fuzz targets: `fuzz/fuzz_targets/` (standalone workspace)
- Python tests: `python/tests/` and `tests/compat/`

## Test-gating environment variables

| Variable | Purpose |
|----------|---------|
| `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` | Enable pproxy differential tests |
| `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1` | Enable Shadowsocks wire-format interop |
| `EGRESS_REQUIRE_REVERSE_INTEROP=1` | Enable reverse proxy pproxy interop |
| `EGRESS_REQUIRE_SOAK=1` | Enable soak/performance tests |
| `EGRESS_RUN_PPROXY_DIFFERENTIAL=1` | Enable differential parity harness |

Before changing Shadowsocks compatibility claims, run the gated oracle and
maintained-implementation suites locally:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test interoperability_pproxy -- --ignored --test-threads=1
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
```

## Skills

Agent skills live in `.skills/` (canonical) and are symlinked from `.agents/skills/`. The OpenCode config mirrors a subset under `.opencode/skills/`. Use the `skill` tool when a task matches:

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

## Architecture docs

Per-subsystem architecture lives under `docs/architecture/`:

| Subsystem | Document |
|-----------|----------|
| System overview | `docs/architecture/overview.md` |
| Core types | `docs/architecture/core.md` |
| Routing | `docs/architecture/routing.md` |
| Config | `docs/architecture/config.md` |
| Runtime | `docs/architecture/runtime.md` |
| Server | `docs/architecture/server.md` |
| HTTP protocols | `docs/architecture/protocols-http.md` |
| SOCKS protocols | `docs/architecture/protocols-socks.md` |
| Shadowsocks | `docs/architecture/protocols-shadowsocks.md` |
| Trojan | `docs/architecture/protocols-trojan.md` |
| WebSocket | `docs/architecture/protocols-websocket.md` |
| Raw | `docs/architecture/protocols-raw.md` |
| Reverse | `docs/architecture/protocols-reverse.md` |
| TLS transport | `docs/architecture/transport-tls.md` |
| UDP | `docs/architecture/udp.md` |
| URI parsing | `docs/architecture/uri.md` |
| Embed API | `docs/architecture/embed.md` |
| Python bindings | `docs/architecture/python.md` |
| pproxy compat | `docs/architecture/pproxy-compat.md` |
| CLI | `docs/architecture/cli.md` |
| System proxy | `docs/architecture/system-proxy.md` |
| Testkit | `docs/architecture/testkit.md` |

Key source-of-truth docs: `docs/CI_STATUS.md`, `docs/TESTING.md`, `docs/release/RELEASE_PROCESS.md`, `docs/ARCHITECTURE.md`, `docs/parity/pproxy_capability_manifest.toml`, `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`.

The active strict-oracle contract is pinned to the `pproxy==2.7.9` tag at
`09d4752f17ed6787e1a073c93980eec019887ee3` from
`https://github.com/qwj/python-proxy`. The phase-0 manifest and matrix are the
only active strict-target authorities. In particular, `--log`,
`-f/--config`, and `--rulefile` are not tagged-parser flags, and SOCKS4/SOCKS5
BIND are not implemented by the oracle; Eggress extensions or matching
refusals must not be recorded as strict gaps. Use
`scripts/pproxy_surface_probe.py` against isolated oracle and wheel
interpreters when updating the Python module inventory.

Files under `plans/` are historical records; they explain why code exists but do not override current policy or source behavior.

## CI boundary

Two automatic smoke workflows and one manual publish workflow:
- `.github/workflows/ci.yml`: Ubuntu Rust smoke (format, clippy, tests, lean-build check). Push to `main` + manual.
- `.github/workflows/python-test.yml`: path-scoped Ubuntu/Python 3.12 smoke. Push to `main` + manual.
- `.github/workflows/publish-python.yml`: multi-platform wheels + PyPI/TestPyPI via OIDC. Tag push (`v*`) or manual.

Hosted CI is a smoke signal, not a release engine. Do not duplicate every local check in CI, and do not recreate release artifact matrices, automated GitHub Releases, or container publishing without an explicit decision.

## Architectural invariants

- Streams are boxed at protocol/transport boundaries; never propagate generic stream types through the architecture.
- Credentials and secret-bearing URIs must be redacted before logging, diagnostics, or evidence output.
- Compatibility `--auth` reuse is source-IP keyed, bounded, monotonic, and must
  never be enabled for native listeners implicitly.
- Compatibility `--sys` applies only after listener bind succeeds and restores
  captured settings on every normal or failed startup/shutdown path.
- Listener topology is not hot-reloaded; routing, upstream, group, and health state may be replaced atomically.
- Shutdown ordering: readiness false, listener stop, connection drain/cancellation, then admin shutdown.
- Runtime routing, health, admin, and metrics share the same compiled runtime snapshot.
- Protocol and transport composition must be validated before execution.
- SSR compatibility is limited to pproxy 2.7.9 address framing and six built-in
  plugins; plugin names are closed, ordered, and fail closed when the
  `pproxy-legacy` feature is unavailable.
- `tls1.2_ticket_auth` is obfuscation compatibility, never native rustls TLS or
  a security claim; UDP SSR and external plugins are unsupported.
- Shadowsocks compatibility evidence must include pproxy 2.7.9 and a maintained
  Shadowsocks implementation; Eggress-to-Eggress roundtrips alone are not
  wire-compatibility evidence.
- `unsafe_code = "deny"` at workspace level; do not add unsafe without justification.
- No OpenSSL, no C dependencies, no `build.rs` without explicit reason. `deny.toml` bans `openssl-sys`, `native-tls`, `aws-lc-sys`, and `cmake`.

## Code conventions

- Rust edition 2021, MSRV 1.75 (`workspace.package.rust-version`). `rust-toolchain.toml` pins stable with `clippy` + `rustfmt`.
- Tokio async runtime; `thiserror` for errors; `tracing` for logging; `clap` derive for CLI.
- Prefer deterministic tests over fixed sleeps; use retry loops or readiness signals.
- Preserve stable diagnostic and exit-code semantics (part of the compatibility surface).
- `cargo check` is not a required gate but useful interactively for fast compile feedback.

## Compatibility discipline

The active compatibility target is practical parity with `pproxy==2.7.9`. Claims must distinguish behavioral match, compatible with warning, native equivalent, intentional non-parity, and unsupported. Do not upgrade a tier based only on API shape or successful construction.

When a compatibility claim changes, update the applicable manifest and run the corresponding oracle or interoperability suite. Unsupported transports/roles should fail with structured diagnostics, not silent fallback.

The strict Phase 4 Python package claim covers the ten tracked modules and
their recorded symbols/signatures. Modern `cipherpy` AEAD names delegate to
native implementations; legacy pure-Python cipher names remain importable but
fail explicitly when constructed. `pproxy.__main__` and the installed
`pproxy` script share the in-process Eggress adapter, and `sysproxy` delegates
mutation/rollback to the native backend on supported platforms.

The strict Phase 3 `ssr://` surface accepts `plain`, `origin`, `http_simple`,
`tls1.2_ticket_auth`, `verify_simple`, and `verify_deflate`; unknown plugins
fail during parsing.

Key compatibility notes:
- `--pac`, `--get`, `--test` are value-taking options; their values are not positional listeners.
- H2 listeners accept independent CONNECT streams; WS/WSS compatibility
  listeners require a fixed target (`ws{host:port}://listener`) and use the
  existing TLS transport for WSS. H2/WS/WSS upstream behavior remains supported.
- QUIC/HTTP/3 is intentionally deferred.

## Change discipline

Keep changes narrowly scoped. Avoid mixing capability implementation, test-infrastructure redesign, documentation mass-generation, and release-process changes in one patch. Do not run security audits, interoperability suites, benchmarks, fuzzing, or parity-report generation for unrelated changes.
