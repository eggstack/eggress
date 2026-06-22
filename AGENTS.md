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

# Run UDP-focused tests
cargo test -p eggress-udp
cargo test -p eggress-runtime udp
cargo test -p eggress-config udp

# Run UDP upstream tests
cargo test -p eggress-udp socks5_upstream
cargo test -p eggress-runtime udp_upstream

# Run the CLI
cargo run --bin eggress -- --help
cargo run --bin eggress -- -l http://:8080
cargo run --bin eggress -- --config path/to/config.toml
```

## Project Structure

```text
eggress/
├── Cargo.toml              # Workspace root
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
│   ├── eggress-udp/       # UDP association, codec, direct forwarding, upstream SOCKS5 relay
│   └── eggress-testkit/   # Test utilities
├── tests/
│   └── interoperability/  # Cross-implementation tests (curl, pproxy)
└── docs/
    ├── ARCHITECTURE.md
    ├── ROADMAP.md
    ├── PHASE_2_COMPLETION.md
    ├── PHASE_3_COMPLETION.md
    └── URI_GRAMMAR.md
```

Integration tests live in `crates/eggress-runtime/tests/` (startup, routing,
health, admin, reload, shutdown, pac_static, udp). They exercise the supervisor end
to end and cover the negative-path behaviors (bind conflict, invalid source,
oversized identity, reload-time failure). UDP integration tests cover
association lifecycle, TCP control close, echo relay, bind conflict, topology
rejection, and config reload.

## Code Conventions

- Edition: 2021
- MSRV: 1.75
- `unsafe_code = "forbid"` in all workspace crates
- `clippy::all` warnings denied
- Async runtime: Tokio
- Errors: `thiserror`
- CLI: `clap` with derive
- Logging: `tracing` + `tracing-subscriber`
- No C dependencies, no OpenSSL

## Architecture

- Streams are boxed at protocol/transport boundaries (`BoxStream`)
- Protocol detection uses ordered `ProtocolDetector` implementations
- Chain executor folds over hop list with protocol-specific handlers
- Relay uses `tokio::io::split` + `tokio::io::copy` for bidirectional forwarding
- Credentials are never logged; URI display uses redacted format
- Routing uses compiled rule AST with first-match-wins evaluation
- Upstream selection via pluggable schedulers (first, round-robin, random, least-connections)
- Health state machine with hysteresis and active TCP probes
- Atomic config reload via `ArcSwap<Router>` for lock-free reads
- Active connection accounting via `PendingLease`/`ActiveLease` drop guards
- Route explanation for operator debugging without debug logs (supports source, listener, protocol, identity)
- Recursive TOML matcher expressions (all, any_of, not) with leaf matchers
- Session metrics via `SessionMetrics` trait for pluggable backends
- Service supervisor pattern with graceful shutdown and signal handling
- `ServiceSupervisor::run()` returns `Result<(), RuntimeError>`; bind failures and runtime init errors are structured rather than panicking
- Shared runtime snapshot via `CompiledRuntimeSnapshot` — one set of `Arc<UpstreamRuntime>` shared by router, health, admin, metrics
- Separate cancellation tokens for shutdown phases (listeners, connections, health, admin)
- Pre-bind listeners before readiness to avoid race conditions
- Shutdown ordering: readiness=false → stop listeners → drain connections (force-cancel after grace) → stop admin, so /-/ready, /metrics, /-/status remain queryable during drain
- Health config per upstream from TOML
- PAC/static content from TOML config; admin reads them live from current snapshot via `AdminSnapshotProvider`, so reloads are visible without restarting the admin server
- Single generation source: `CompiledRuntimeSnapshot.generation`, exposed by `RuntimeState::generation()`; admin reads it via `AdminSnapshotProvider` instead of a duplicate atomic
