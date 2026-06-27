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

# Run UDP upstream tests
cargo test -p eggress-udp socks5_upstream
cargo test -p eggress-runtime udp_upstream

# Run Shadowsocks tests
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-runtime shadowsocks_tcp
cargo test -p eggress-runtime shadowsocks_udp

# Run Trojan tests
cargo test -p eggress-protocol-trojan

# Run TLS transport tests
cargo test -p eggress-transport-tls

# Run upstream protocol tests
cargo test -p eggress-runtime upstream_protocols

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
│   ├── eggress-udp/       # UDP association, codec, direct forwarding, upstream SOCKS5 relay
│   ├── eggress-pproxy-compat/ # pproxy compatibility: URI translation, config migration
│   └── eggress-testkit/   # Test utilities
├── benches/                # Criterion benchmarks (tcp_relay, udp_relay, route_match, http_connect_upstream)
├── fuzz/                   # Fuzz harness smoke targets (socks5_udp_datagram, socks5_handshake, http_connect_response, trojan_request, route_match, uri_parse)
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
topology rejection, config reload, and SOCKS5 upstream relay. Upstream protocol tests
cover HTTP, SOCKS4, SOCKS5, and unsupported-combo rejection through the full stack.
Property tests live in per-crate `tests/` directories (codec round-trips, parser
round-trips, route match consistency). Fuzz smoke tests exercise seed inputs for
`cargo fuzz` targets. Load tests are `#[ignore]` by default and require explicit opt-in.
Differential tests against `pproxy` are gated and live in `crates/eggress-cli/tests/`.
pproxy compat tests live in `crates/eggress-pproxy-compat/src/tests.rs` and cover protocol aliases, unsupported scheme diagnostics, and credential redaction.
See `docs/TESTING.md` for comprehensive testing guidance.

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
- **UDP**: direct forwarding, one-hop SOCKS5 upstream relay, and one-hop Shadowsocks upstream relay (standard AEAD format); no multi-hop chains, no HTTP/MASQUE. Association owned by TCP control connection. Client pinning enabled by default. Shadowsocks TCP upstream now has full AEAD stream encryption.
- **Scheduler parity**: Round-robin uses global atomic cursor; least-connections uses active+in_flight; first-available returns first eligible; health filtering excludes Unhealthy/Disabled
- **Failure semantics**: SOCKS5/HTTP/SOCKS4 reply codes documented in `docs/FAILURE_SEMANTICS.md`; timeout→504/0x06, refused→502/0x05, policy→403/0x02
- **pproxy parity spec and tier taxonomy** defined in `docs/PPROXY_PARITY_SPEC.md`
- **Differential test harness** has reusable primitives (`ProcessGuard`, `TaskGuard`, `start_tcp_echo`, `start_udp_echo`, `compare_tcp_echo`, etc.)
- **pproxy CLI subcommands**: `pproxy translate` converts pproxy URI arguments to eggress TOML; `pproxy check` reports parity tier; `pproxy run` translates and starts the service
- **pproxy protocol parity**: Phase 11 classified all remaining pproxy protocols/schemes; lightweight aliases (socks4a, https) map to existing protocols; unsupported protocols (SSH, Unix, redir) produce structured diagnostics

## Skills

The `.skills/` directory contains focused reference files for common development tasks:

- `rust-proxy-dev.md` — Adding new protocols, transport wrappers, chain integration
- `udp-protocol.md` — UDP association management, datagram relay, upstream SOCKS5 relay
- `config-reload.md` — TOML config schema, hot-reload vs restart, atomic swaps
- `routing-rules.md` — Rule engine, matchers, schedulers, route explanation
- `testing.md` — Test layers, conventions, running and writing tests, including differential tests
