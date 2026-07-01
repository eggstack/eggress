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

# Run WebSocket tunnel tests
cargo test -p eggress-protocol-websocket

# Run raw tunnel tests
cargo test -p eggress-protocol-raw

# Run reverse protocol tests
cargo test -p eggress-protocol-reverse
cargo test -p eggress-protocol-reverse --test integration
cargo test -p eggress-pproxy-compat --lib reverse
cargo test -p eggress-runtime --test reverse_interop

# Run gated reverse interop tests (requires pproxy on PATH)
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored

# Run advanced transport integration tests
cargo test -p eggress-runtime advanced_transport

# Run transparent proxy tests
cargo test -p eggress-runtime transparent

# Run Unix socket tests
cargo test -p eggress-runtime unix_socket

# Run pproxy compat tests for redir/unix
cargo test -p eggress-pproxy-compat redir
cargo test -p eggress-pproxy-compat unix

# Run Shadowsocks interop tests (gated)
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored

# Run pproxy differential tests (gated, requires Python 3.11/3.12)
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored

# Run pproxy standalone UDP differential tests only
./scripts/compat_udp_pproxy.sh

# Run property tests (proptest)
cargo test -p eggress-protocol-socks --test codec_properties
cargo test -p eggress-protocol-http --test connect_properties
cargo test -p eggress-protocol-trojan --test request_properties
cargo test -p eggress-routing --test properties

# Run fuzz smoke tests
cargo test -p eggress-protocol-socks --test fuzz_smoke

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

# Run pproxy differential tests (gated, requires pproxy==2.7.9)
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored

# Validate manifest invariants (Phase 23)
cargo test -p eggress-testkit validate_real_manifest

# Validate manifest test names exist (Phase 23)
cargo test -p eggress-testkit manifest_test_names_exist

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
cd crates/eggress-python && maturin build --target x86_64-apple-darwin
pip install --force-reinstall target/wheels/eggress-0.1.0-*.whl
python -m pytest python/tests

# Run Python pproxy compat tests
python -m pytest python/tests/test_pproxy_compat.py -v

# Run Python pproxy redaction tests
python -m pytest python/tests/test_pproxy_redaction.py -v

# Run Python pproxy concurrency tests
python -m pytest python/tests/test_pproxy_concurrency.py -v

# Run Python pproxy differential tests (gated, requires pproxy package)
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python -m pytest python/tests/test_pproxy_differential.py -v

# Build and test Python wheel
maturin build --release --out dist
python -m venv .venv-wheel-test
. .venv-wheel-test/bin/activate
pip install dist/eggress-*.whl
pip install pytest
python -m pytest python/tests
deactivate

# Or use the helper script
./scripts/test_wheel.sh

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
```

## Project Structure

```text
eggress/
├── Cargo.toml              # Workspace root
├── .skills/                # Agent skill files for this codebase
├── crates/
│   ├── eggress-core/      # Core types, traits, relay, listener, connector, chain
│   ├── eggress-cli/       # CLI binary
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
│   ├── eggress-protocol-http/src/h2_connect.rs # HTTP/2 CONNECT bridge
│   ├── eggress-protocol-websocket/ # WebSocket tunnel (server, client, stream adapter)
│   ├── eggress-protocol-raw/ # Raw fixed-target TCP tunnel
│   ├── eggress-protocol-reverse/ # Reverse/backward proxy: raw-relay control channel, server (acceptor), client (control client), metrics
│   ├── eggress-runtime/src/platform.rs # Platform capability model (Linux SO_ORIGINAL_DST, macOS PF)
│   ├── eggress-server/src/listener/transparent.rs # Transparent TCP listener (SO_ORIGINAL_DST)
│   ├── eggress-server/src/listener/unix.rs # Unix domain socket listener
│   ├── eggress-udp/       # UDP association, codec, direct forwarding, upstream SOCKS5 relay
│   ├── eggress-pproxy-compat/ # pproxy compatibility: URI translation, config migration
│   ├── eggress-embed/      # Stable Rust embed API: config, service, handle, errors
│   ├── eggress-python/     # Python bindings via PyO3 (wraps eggress-embed)
│   └── eggress-testkit/   # Test utilities
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
    └── protocols/
        ├── HTTP_CONNECT.md
        ├── SOCKS4.md
        ├── SHADOWSOCKS.md
        └── TROJAN.md
```

Integration tests live in `crates/eggress-runtime/tests/` (startup, routing,
health, admin, reload, shutdown, pac_static, udp, udp_upstream, upstream_protocols,
shadowsocks_tcp, shadowsocks_udp,
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

- **Entry point**: `eggress-cli` binary → `eggress-runtime` `ServiceSupervisor::run()` → `eggress-server` `serve_connection()`
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
- **Scheduler parity**: Round-robin uses global atomic cursor; least-connections uses active+in_flight; first-available returns first eligible; health filtering excludes Unhealthy/Disabled
- **Failure semantics**: SOCKS5/HTTP/SOCKS4 reply codes documented in `docs/FAILURE_SEMANTICS.md`; timeout→504/0x06, refused→502/0x05, policy→403/0x02
- **pproxy parity spec and tier taxonomy** defined in `docs/PPROXY_PARITY_SPEC.md`
- **Differential test harness** has reusable primitives (`ProcessGuard`, `TaskGuard`, `start_tcp_echo`, `start_udp_echo`, `compare_tcp_echo`, etc.)
- **pproxy CLI subcommands**: `pproxy translate` converts pproxy URI arguments to eggress TOML; `pproxy check` reports parity tier; `pproxy run` translates and starts the service
- **pproxy protocol parity**: Phase 11 classified all remaining pproxy protocols/schemes; lightweight aliases (socks4a, https) map to existing protocols; unsupported protocols (SSH) produce structured diagnostics. Transparent TCP proxy (`redir://`, Linux only) and Unix domain socket listeners (`unix://`, Unix only) are now supported (Phase 25).
- **Shadowsocks TCP framing**: Standard SIP003 AEAD (two AEAD operations per chunk, encrypted length). Wire-compatible with standard Shadowsocks implementations. UDP uses standard AEAD format and is interoperable. See `docs/protocols/SHADOWSOCKS.md`.
- **Advanced transports**: HTTP/2 CONNECT, WebSocket tunnels, and raw fixed-target tunnels supported as inbound/upstream protocols. QUIC/HTTP/3 deferred by ADR. See `docs/protocols/ADVANCED_TRANSPORTS.md`.
- **SSR/legacy Shadowsocks**: Intentionally unsupported. SSR URIs (`ssr://`) and legacy stream cipher methods are recognized and rejected with clear diagnostics. See ADR at `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`. Legacy method detection exists in `eggress-protocol-shadowsocks::method::is_legacy_method()`.
- **Corrective parity audit**: Completed for workstreams 6 (repair capability classifier) and 9 (completion-doc truth pass). Shadowsocks TCP framing standardized to SIP003 AEAD in Phase 21. Completion docs updated with corrective notices and gated-test status.
- **Embed API**: `eggress-embed` provides `EggressConfig`, `EggressService`, and `EggressHandle` for in-process embedding. Thread ownership: async path uses a Tokio blocking-pool thread + dedicated OS thread (`eggress-embed-rt`); blocking path uses an outer startup thread + inner run thread (`eggress-embed-run`). Handle owns state/token and cleans up on drop (5-second timeout on async path). `shutdown()` and `shutdown_blocking()` are idempotent. See `docs/EMBED_API.md`.
- **Python bindings**: `eggress-python` wraps `eggress-embed` via PyO3. GIL is released on all blocking Rust calls via `py.detach()`. Python package lives in `python/eggress/` with maturin build. Version sourced from native module's `CARGO_PKG_VERSION`. Lifecycle: always prefer explicit `shutdown()` or context manager; object destruction is best-effort fallback. See `docs/PYTHON_BINDINGS.md`.
- **PyPI packaging**: Wheels built with maturin for Linux x86_64/aarch64, macOS x86_64/arm64, Windows x86_64. See `docs/PYPI_RELEASE.md`.
- **Release candidate audit (Phase 17)**: Final parity matrix audit, Rust/Python release audits, security/redaction audit including Python binding surface, documentation consistency pass. Release candidate document at `docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md`. All verification commands pass; go recommendation issued. See `docs/PHASE_17_TRUE_PPROXY_PARITY_RELEASE_CANDIDATE_COMPLETION.md`.
- **Manifest validation**: `tests/compat/pproxy_manifest.toml` is the canonical evidence index. `egress_status = "compatible"` requires `evidence_level = "compatible"` backed by real pproxy differential tests. `implemented_synthetic` evidence cannot support compatibility claims. Validation enforced by `eggress-testkit::manifest::validate_manifest()`. `last_updated` field removed in Phase 24; stale warnings no longer emitted.
- **Manifest external dependency checks (Phase 24)**: Compatible entries with differential tests require `external_dependency`; implemented_interop requires dependency or divergence note explaining interop.

## Skills

The `.skills/` directory contains focused reference files for common development tasks:

- `rust-proxy-dev.md` — Adding new protocols, transport wrappers, chain integration
- `udp-protocol.md` — UDP association management, datagram relay, upstream SOCKS5 relay
- `config-reload.md` — TOML config schema, hot-reload vs restart, atomic swaps
- `routing-rules.md` — Rule engine, matchers, schedulers, route explanation
- `testing.md` — Test layers, conventions, running and writing tests, including differential tests
- `advanced-transports.md` — H2 CONNECT, WebSocket tunnels, raw tunnels, TLS/ALPN
