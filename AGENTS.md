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

# Run the CLI
cargo run --bin eggress -- --help
cargo run --bin eggress -- -l http://:8080
```

## Project Structure

```text
eggress/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── eggress-core/      # Core types, traits, relay, listener, connector, chain
│   ├── eggress-cli/       # CLI binary
│   ├── eggress-server/    # Server orchestration: accept, execute, reply, error
│   ├── eggress-uri/       # URI parser and AST
│   ├── eggress-routing/   # Rule engine, schedulers, health, leases, route explanation
│   ├── eggress-config/    # TOML configuration, validation, secret sources
│   ├── eggress-metrics/   # Prometheus-compatible metrics registry
│   ├── eggress-admin/     # Admin HTTP server, PAC, static content
│   ├── eggress-protocol-http/   # HTTP CONNECT and forwarding
│   ├── eggress-protocol-socks/  # SOCKS4/4a and SOCKS5
│   └── eggress-testkit/   # Test utilities
├── tests/
│   ├── integration/       # Internal eggress-to-eggress tests
│   └── interoperability/  # Cross-implementation tests (curl, pproxy)
└── docs/
    ├── ARCHITECTURE.md
    └── ROADMAP.md
```

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
- Route explanation for operator debugging without debug logs
