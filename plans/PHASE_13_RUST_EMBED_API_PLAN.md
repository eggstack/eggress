# Phase 13 Detailed Plan: Rust Embed API Stabilization

## Purpose

Phase 13 creates the Rust library surface that Python bindings will wrap. The goal is a deliberate embedded-service API, not exposing CLI internals or forcing Python to manage sockets, async networking, routing, or runtime details.

The Rust API must own the heavy work: config validation, listener binding, Tokio runtime execution, routing, proxy protocol handling, UDP associations, metrics, reload, and shutdown. Python bindings in later phases should be thin wrappers over this stable Rust API.

This phase is a library/API design and implementation phase. Do not start PyO3 bindings yet.

---

# Current context

The runtime already has `ServiceSupervisor`, config loading, listener startup, shutdown token handling, admin/metrics surfaces, UDP services, and route reload behavior. The CLI currently drives those pieces through executable code paths. Phase 13 should extract a clean embedded API so non-CLI callers can safely start and control Eggress in-process.

Known constraints from earlier phases:

- Shadowsocks TCP is experimental/non-standard and must not be marketed as pproxy-compatible.
- Shadowsocks UDP is supported but interop remains gated.
- pproxy differential tests are gated and Python 3.14 has known pproxy issues.
- Hosted CI may still be unavailable; local verification remains source of truth unless status contexts become visible.

---

# Non-goals

Do not implement:

- Python bindings;
- PyPI packaging;
- wheel building;
- new proxy protocols;
- Shadowsocks TCP standard rework;
- new CLI feature surface except optional internal reuse;
- unsafe Rust;
- native TLS/OpenSSL;
- background daemon management outside the process.

---

# Workstream 1: Define the embed crate boundary

## Goal

Create a small public crate dedicated to embedding Eggress in another Rust process.

## Target crate

Preferred:

```text
crates/eggress-embed/
```

Add it to the workspace.

## Crate responsibilities

- accept TOML or typed config;
- validate/compile config;
- start service;
- expose bound listener/admin addresses;
- expose readiness state;
- expose metrics text;
- expose status/snapshot;
- support reload;
- support deterministic shutdown;
- convert internal errors into stable public error types.

## Crate non-responsibilities

- no protocol implementation;
- no CLI parsing;
- no Python-specific behavior;
- no process-global daemonization;
- no logging initialization unless explicitly requested by caller.

## Acceptance criteria

- `eggress-embed` compiles as a normal Rust library crate.
- It does not duplicate runtime internals.

---

# Workstream 2: Public API design

## Goal

Expose a minimal, stable API that can later be wrapped by PyO3.

## Proposed types

```rust
pub struct EggressConfig { ... }
pub struct EggressService { ... }
pub struct EggressHandle { ... }
pub struct BoundAddresses { ... }
pub struct ServiceStatus { ... }
pub struct ListenerStatus { ... }
pub struct ReloadOutcome { ... }
pub enum EggressError { ... }
```

## Proposed constructors

```rust
impl EggressConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, EggressError>;
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, EggressError>;
    pub fn to_redacted_toml(&self) -> Result<String, EggressError>;
}

impl EggressService {
    pub fn new(config: EggressConfig) -> Self;
    pub fn from_toml_str(input: &str) -> Result<Self, EggressError>;
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, EggressError>;
}
```

## Proposed runtime API

Decide whether `start` is async, blocking, or both.

Recommended:

```rust
impl EggressService {
    pub async fn start(self) -> Result<EggressHandle, EggressError>;
    pub fn start_blocking(self) -> Result<EggressHandle, EggressError>;
}

impl EggressHandle {
    pub fn bound_addresses(&self) -> BoundAddresses;
    pub fn status(&self) -> ServiceStatus;
    pub async fn metrics_text(&self) -> Result<String, EggressError>;
    pub async fn reload_toml_str(&self, input: &str) -> Result<ReloadOutcome, EggressError>;
    pub async fn shutdown(self) -> Result<(), EggressError>;
    pub fn shutdown_blocking(self) -> Result<(), EggressError>;
}
```

## API requirements

- no panics for normal user errors;
- errors redact credentials;
- no caller must know about `ServiceSupervisor` internals;
- no caller must manually manage listener tasks;
- lifecycle is deterministic;
- dropping `EggressHandle` should not silently leak a running service.

## Acceptance criteria

- API design is documented in `docs/EMBED_API.md`.
- API compiles and is exercised by integration tests.

---

# Workstream 3: Runtime ownership model

## Goal

Define how the embedded service runs in-process.

## Options

### Option A: caller-provided Tokio runtime

`async fn start()` assumes caller is inside a Tokio runtime.

Pros:

- simple for Rust async callers;
- no hidden threads.

Cons:

- awkward for Python bindings;
- Python would need a dedicated runtime wrapper anyway.

### Option B: owned runtime thread

`start_blocking()` creates a Tokio runtime on a dedicated thread and returns a handle.

Pros:

- ideal for Python;
- simple sync embedding;
- clear shutdown ownership.

Cons:

- extra thread;
- careful synchronization required.

## Required outcome

Implement at least one path suitable for Python. Preferred: both async and owned-runtime blocking path.

## Acceptance criteria

- A synchronous Rust test can start a service without already owning Tokio.
- An async Rust test can start a service inside Tokio.
- Shutdown joins runtime tasks and leaves no background thread alive.

---

# Workstream 4: Bound address and readiness reporting

## Goal

Make port-0 service startup usable from Rust and Python.

## Required API behavior

When config uses `127.0.0.1:0`, the handle must expose actual bound ports.

```rust
let addrs = handle.bound_addresses();
let socks = addrs.listener("socks").unwrap();
```

Support:

- listener name -> socket address;
- admin address if configured;
- UDP relay address if static/known, or explain that UDP relay addresses are association-created;
- readiness boolean or startup error.

## Acceptance criteria

- Tests start listener on port 0 and connect through the returned address.

---

# Workstream 5: Metrics and status APIs

## Goal

Expose observability to embedded callers without requiring HTTP scraping.

## Required API

```rust
impl EggressHandle {
    pub async fn metrics_text(&self) -> Result<String, EggressError>;
    pub fn status(&self) -> ServiceStatus;
}
```

`ServiceStatus` should include:

- generation;
- readiness;
- listener addresses;
- active connection count if available;
- active UDP association count if available;
- upstream summary if available;
- runtime uptime if available.

## Security requirements

- no credentials in status;
- no client IP/target host by default;
- metrics preserve bounded-label policy.

## Acceptance criteria

- Embedded test starts service, proxies traffic, calls `metrics_text`, and sees session/upstream counters.

---

# Workstream 6: Reload API

## Goal

Allow embedded callers to update config without restarting the process.

## API

```rust
pub async fn reload_toml_str(&self, input: &str) -> Result<ReloadOutcome, EggressError>;
pub async fn reload_toml_file(&self, path: impl AsRef<Path>) -> Result<ReloadOutcome, EggressError>;
```

## Requirements

- preserve existing reload restrictions;
- rejected reload leaves old generation active;
- applied reload increments generation;
- errors redact credentials;
- listener bind changes should be rejected or documented as restart-required.

## Tests

- reload routing rule from direct to upstream or reject;
- failed reload keeps old behavior;
- bind change rejected;
- metrics/status generation updates.

## Acceptance criteria

- Reload behavior matches runtime supervisor semantics.

---

# Workstream 7: Error model

## Goal

Create a stable, binding-friendly error surface.

## Error enum

Suggested:

```rust
pub enum EggressError {
    Config { message: String },
    Runtime { message: String },
    Startup { message: String },
    Reload { message: String },
    Shutdown { message: String },
    UnsupportedFeature { feature: String, message: String },
    Internal { message: String },
}
```

Requirements:

- implement `std::error::Error`;
- implement `Display` with redaction;
- no secrets in messages;
- keep variants stable for PyO3 mapping later.

## Acceptance criteria

- Tests verify credential strings do not appear in error display/debug for common config failures.

---

# Workstream 8: Tests

## New tests

Suggested files:

```text
crates/eggress-embed/tests/start_stop.rs
crates/eggress-embed/tests/proxy_traffic.rs
crates/eggress-embed/tests/reload.rs
crates/eggress-embed/tests/metrics_status.rs
crates/eggress-embed/tests/error_redaction.rs
```

## Required scenarios

1. Start and shutdown service with port-0 SOCKS5 listener.
2. Proxy TCP echo through direct route.
3. Proxy HTTP CONNECT through direct route if exposed in config.
4. Read metrics after session.
5. Reload route and verify new generation.
6. Failed reload preserves old generation.
7. Dropping/closing handle shuts down runtime or requires explicit shutdown with documented behavior.
8. Error redaction.

## Acceptance criteria

- Tests do not require public internet.
- Tests are deterministic and avoid long sleeps.

---

# Workstream 9: Docs

## Required docs

Create:

```text
docs/EMBED_API.md
```

Update:

```text
docs/ROADMAP.md
README.md
AGENTS.md
```

## Required content

- API overview;
- sync example;
- async example;
- port-0 example;
- reload example;
- metrics/status example;
- lifecycle/shutdown rules;
- limitations;
- Python-binding readiness notes.

---

# Recommended commit sequence

1. Add `eggress-embed` crate skeleton and public error types.
2. Add config constructors and redaction tests.
3. Add async start/handle/status/bound-address API.
4. Add blocking owned-runtime API.
5. Add metrics/status/reload APIs.
6. Add integration tests.
7. Update docs and completion record.

---

# Required verification

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p eggress-embed
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Focused runtime sanity:

```bash
cargo test -p eggress-runtime lifecycle
cargo test -p eggress-runtime observability
cargo test -p eggress-runtime reload
```

---

# Definition of done

Phase 13 is complete only when:

1. `eggress-embed` crate exists.
2. A stable public embed API is documented.
3. Rust callers can start/stop Eggress without CLI code.
4. Port-0 bound addresses are discoverable.
5. Metrics and status are available without HTTP scraping.
6. Reload is available through the handle.
7. Error types are stable and redacted.
8. Sync/blocking path suitable for Python exists or a clear alternative is documented.
9. Integration tests proxy real local traffic.
10. Workspace checks pass locally.

## Completion record

Add:

```text
docs/PHASE_13_RUST_EMBED_API_COMPLETION.md
```

Include API summary, tests, limitations, and readiness for Phase 14 PyO3 bindings.
