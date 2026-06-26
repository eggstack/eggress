# Roadmap: True `pproxy` Parity and Python Bindings

## Purpose

Eggress has reached a stable post-hardening baseline as a Rust-native proxy service: core HTTP/SOCKS/TCP/UDP paths are implemented, major runtime lifecycle invariants are tested, observability is wired, and unsupported protocol claims are no longer overstated.

The next strategic goal is larger:

1. Achieve true practical parity with Python `pproxy` for the protocol and behavior surface Eggress wants to replace.
2. Provide a PyPI-distributed Python package with bindings that lets Python users embed Eggress as a library while offloading networking, protocol parsing, relaying, routing, UDP handling, metrics, and heavy runtime work to Rust.

This roadmap is intentionally stricter than a feature wishlist. A capability counts as parity only when it is implemented, locally tested, documented, and covered by differential or compatibility tests where possible.

---

# Guiding principles

## 1. Parity means behavior, not just protocol names

Do not mark an item complete because a parser or crate exists. Parity requires a functioning end-to-end path:

- inbound accept;
- route selection;
- upstream open if applicable;
- bidirectional relay;
- errors/failures mapped predictably;
- UDP lifecycle if applicable;
- config/URI representation;
- tests and docs.

## 2. Rust remains the source of truth

The Rust implementation should own:

- async networking;
- protocol handshakes;
- UDP relay;
- upstream chaining;
- routing and scheduling;
- metrics;
- config validation;
- runtime lifecycle;
- security constraints.

Python bindings should be a control/configuration API, not a reimplementation.

## 3. Python bindings must not require Python async networking internals

The Python package should expose high-level control surfaces:

- create config;
- start service;
- inspect bound addresses;
- reload config;
- stop service;
- retrieve metrics/status;
- optionally use an async context manager.

The Rust runtime should run independently on its own Tokio runtime or controlled runtime handle.

## 4. Differential testing is the contract

Where Eggress claims `pproxy` compatibility, there should be a gated local differential test that starts both Eggress and `pproxy` against local echo services and compares behavior.

## 5. Be explicit about deliberate non-parity

If Eggress chooses not to replicate a `pproxy` behavior because it is unsafe, ambiguous, obsolete, or too permissive, document that as intentional non-parity.

---

# Current baseline at roadmap start

As of the Phase 6 final polish closeout:

| Area | Status |
|---|---|
| SOCKS5 inbound TCP CONNECT | Supported |
| SOCKS5 inbound UDP ASSOCIATE direct | Supported |
| SOCKS5 UDP one-hop upstream relay | Supported |
| HTTP CONNECT inbound | Supported |
| HTTP forward-proxy path | Supported |
| SOCKS4/SOCKS4a inbound | Supported |
| Direct TCP relay | Supported |
| HTTP CONNECT upstream TCP | Supported |
| SOCKS4/SOCKS4a upstream TCP | Supported |
| SOCKS5 upstream TCP | Supported |
| Trojan TCP upstream | Supported |
| Shadowsocks TCP | Experimental/partial, not supported |
| Shadowsocks UDP | Experimental/non-interop, not supported |
| Multi-hop TCP | Basic chain executor exists; compatibility needs audit |
| Multi-hop UDP | Unsupported |
| Metrics/admin/reload | Supported |
| Python bindings | Not started |
| PyPI distribution | Not started |

---

# Phase 7: Formal `pproxy` parity specification and compatibility matrix

## Goal

Create the contract before adding more protocol surface. This phase defines exactly what `pproxy` behavior Eggress will replicate, what it will intentionally reject, and how parity is tested.

## Workstreams

### WS7.1 Inventory `pproxy` protocol and CLI behavior

Document the `pproxy` feature surface in:

```text
docs/PPROXY_PARITY_SPEC.md
```

Required sections:

- supported local/listener protocols in `pproxy`;
- supported remote/upstream protocols;
- URI grammar and examples;
- chaining grammar;
- scheduler/load-balancing behavior;
- UDP behavior;
- authentication behavior;
- encryption protocol behavior;
- CLI flags and common invocation forms;
- Python library/API surface if used directly;
- error behavior that matters for clients.

Do not rely only on README text. Use local differential probes and source inspection where needed.

### WS7.2 Define Eggress parity tiers

Add explicit support tiers:

```text
Supported
Compatible
Partial
Experimental
Intentional non-parity
Unsupported
```

Definitions:

- **Supported**: Eggress supports this behavior on its own terms.
- **Compatible**: Eggress behavior matches `pproxy` for tested scenarios.
- **Partial**: useful subset exists but not enough for parity.
- **Experimental**: code exists but not promised.
- **Intentional non-parity**: deliberately rejected with rationale.
- **Unsupported**: not implemented.

### WS7.3 Expand parity matrix

Update:

```text
docs/PARITY_MATRIX.md
```

Each row must include:

- feature;
- `pproxy` behavior;
- Eggress behavior;
- support tier;
- test file;
- differential coverage;
- notes.

### WS7.4 Normalize compatibility test harness

Refactor or extend:

```text
crates/eggress-cli/tests/differential_pproxy.rs
```

The harness should support reusable primitives:

- start Eggress from TOML;
- start `pproxy` command;
- start TCP echo;
- start UDP echo;
- perform SOCKS5 CONNECT;
- perform HTTP CONNECT;
- perform SOCKS5 UDP ASSOCIATE;
- compare success payloads;
- compare coarse failure class.

### WS7.5 Document intentional non-parity

Examples that may become intentional non-parity:

- unsafe transparent/system proxy behaviors;
- permissive malformed input acceptance;
- legacy weak ciphers;
- ambiguous URI shorthands;
- public-internet test dependencies;
- unbounded metrics labels.

## Acceptance criteria

- `docs/PPROXY_PARITY_SPEC.md` exists.
- `docs/PARITY_MATRIX.md` is feature-complete enough to drive implementation.
- Differential harness has reusable infrastructure.
- Unsupported vs intentional non-parity is explicit.

---

# Phase 8: pproxy-compatible CLI and URI translation layer

## Goal

Let users migrate common `pproxy` invocations to Eggress with minimal friction.

This phase can be implemented as a compatibility layer. It does not require making Eggress internals mimic `pproxy` exactly.

## Workstreams

### WS8.1 pproxy URI parser/translator

Add a parser that accepts common `pproxy` URI forms and translates them into Eggress `ProxyChainSpec` / TOML structures.

Suggested crate/module:

```text
crates/eggress-pproxy-compat/
```

Responsibilities:

- parse common local URI forms;
- parse common remote URI forms;
- parse user/password auth;
- parse chain separators;
- parse scheduler hints if present;
- emit canonical Eggress config or typed config model;
- preserve redaction.

### WS8.2 CLI compatibility mode

Add one of:

```bash
eggress pproxy ...
eggress --pproxy ...
eggress compat pproxy ...
```

The compatibility command should:

- accept common `pproxy` command shapes;
- produce a validated Eggress runtime config;
- optionally print translated TOML with `--print-config`;
- run Eggress with the translated config;
- give precise errors for unsupported `pproxy` options.

### WS8.3 Migration command

Add:

```bash
eggress pproxy translate '<pproxy args>'
```

or equivalent.

Output:

- Eggress TOML;
- warnings for partial/unsupported behavior;
- exact parity tier for each requested feature.

### WS8.4 Differential CLI tests

For each common invocation:

1. Run `pproxy`.
2. Run Eggress through compatibility mode.
3. Send identical local traffic.
4. Compare payload or coarse failure class.

Minimum scenarios:

- local HTTP CONNECT direct;
- local SOCKS5 direct;
- local SOCKS4 direct;
- local SOCKS5 -> HTTP upstream;
- local SOCKS5 -> SOCKS5 upstream;
- local SOCKS5 UDP direct;
- auth success/failure cases.

## Acceptance criteria

- Common pproxy-style CLI invocations can be translated.
- Unsupported features fail with precise messages.
- Differential tests prove the top migration paths.
- README contains a migration section.

---

# Phase 9: Shadowsocks parity

## Goal

Close the largest protocol gap. Eggress must either support interoperable Shadowsocks or explicitly abandon true `pproxy` protocol parity. For this roadmap, the target is implementation.

## Workstreams

### WS9.1 Shadowsocks spec confirmation

Create:

```text
docs/protocols/SHADOWSOCKS_PARITY.md
```

Document:

- supported SIP(s);
- supported AEAD ciphers;
- key derivation;
- TCP stream framing;
- UDP packet format;
- nonce/salt handling;
- unsupported legacy ciphers;
- interoperability test plan.

### WS9.2 AEAD TCP stream adapter

Implement a real stream adapter that encrypts/decrypts the entire bidirectional stream, not only the target header.

Required behavior:

- client sends salt;
- derives subkey;
- sends encrypted target header as stream data;
- frames payload chunks according to Shadowsocks AEAD stream format;
- maintains independent read/write nonce counters;
- enforces maximum chunk sizes;
- rejects authentication/decryption failures;
- implements `AsyncRead` and `AsyncWrite` over wrapped stream.

Suggested type:

```rust
pub struct ShadowsocksAeadStream<S> { ... }
```

### WS9.3 Supported cipher set

Start with a narrow, secure, pure-Rust-compatible set:

- `aes-128-gcm`;
- `aes-256-gcm`;
- `chacha20-ietf-poly1305` if dependency policy allows.

Do not implement insecure legacy stream ciphers unless explicitly marked intentional compatibility with strong warnings.

### WS9.4 Shadowsocks TCP upstream runtime path

Wire Shadowsocks TCP upstream through the chain executor only after the stream adapter is complete.

Acceptance requires:

- synthetic compatible server test;
- runtime `ServiceSupervisor` TCP echo through Shadowsocks upstream;
- differential/local interop test with a known-good implementation if possible.

### WS9.5 Shadowsocks inbound server mode if pproxy requires it

If `pproxy` supports local Shadowsocks listener mode and true parity requires it, implement listener-side Shadowsocks accept.

Do not implement server mode until upstream mode is solid.

## Acceptance criteria

- No plaintext-after-header bug is possible.
- TCP payload is fully encrypted/decrypted.
- Runtime test proves TCP echo through Shadowsocks upstream.
- Docs move Shadowsocks TCP from experimental to compatible only after tests pass.

---

# Phase 10: Shadowsocks UDP and broader UDP parity

## Goal

Implement interoperable Shadowsocks UDP and close the remaining practical UDP parity gaps.

## Workstreams

### WS10.1 Shadowsocks UDP packet format

Implement standard Shadowsocks UDP packet encode/decode for supported AEAD methods.

Required:

- salt/key derivation as required;
- address + payload encryption;
- response decode;
- target validation;
- tamper rejection;
- wrong-key rejection.

### WS10.2 Shadowsocks UDP upstream relay

Add one-hop Shadowsocks UDP upstream flow support.

Acceptance:

- UDP echo through Shadowsocks upstream;
- target-flow idle cleanup;
- active lease cleanup;
- metrics;
- unsupported combinations still rejected.

### WS10.3 UDP parity matrix expansion

Test and document:

- SOCKS5 UDP direct;
- SOCKS5 UDP through SOCKS5 upstream;
- SOCKS5 UDP through Shadowsocks upstream;
- pproxy-compatible behavior for UDP ASSOCIATE control-lifetime semantics;
- unsupported UDP protocols.

### WS10.4 Multi-hop UDP decision

Decide whether true pproxy parity requires multi-hop UDP.

If yes:

- design UDP chain abstraction;
- support only combinations that have clear semantics;
- avoid silent direct fallback;
- test cleanup and metrics.

If no:

- document as intentional non-parity with rationale.

## Acceptance criteria

- Shadowsocks UDP is interoperable or explicitly scoped.
- UDP parity matrix is complete.
- No unsupported UDP path silently falls back to direct.

---

# Phase 11: Additional pproxy protocol parity

## Goal

Close remaining protocol gaps after HTTP/SOCKS/Shadowsocks/Trojan.

## Candidate protocols

Based on the final parity spec, evaluate:

- SSH upstream if `pproxy` compatibility requires it;
- TLS/SSL wrapping variants;
- WebSocket or HTTP transport variants if applicable;
- additional encrypted protocols supported by `pproxy`;
- inbound encrypted protocols if required.

## Process for each protocol

For each candidate:

1. Confirm `pproxy` behavior.
2. Decide support vs intentional non-parity.
3. Implement protocol crate or module.
4. Add URI/config support.
5. Add chain executor support.
6. Add synthetic server/client tests.
7. Add runtime test.
8. Add differential test if practical.
9. Update parity matrix.

## Acceptance criteria

- Every protocol listed in `PPROXY_PARITY_SPEC.md` is either compatible, partial with plan, or intentional non-parity.
- No protocol is marked compatible without runtime tests.

---

# Phase 12: Scheduler, chaining, and failure semantics parity

## Goal

Match `pproxy` behavior for chain selection, load balancing, retry/fallback, and failure handling where intentionally supported.

## Workstreams

### WS12.1 Scheduler parity

Compare `pproxy` schedulers against Eggress:

- first/first-available;
- round-robin;
- random;
- least connections;
- health-aware selection;
- fallback behavior.

Implement missing scheduler behavior if required.

### WS12.2 Multi-hop TCP parity

Ensure multi-hop TCP chains work across supported combinations:

- SOCKS5 -> HTTP;
- HTTP -> SOCKS5;
- SOCKS4 -> SOCKS5;
- SOCKS5 -> Trojan;
- SOCKS5 -> Shadowsocks after Phase 9;
- TLS wrapping combinations.

### WS12.3 Error mapping parity

Map errors predictably:

- client-visible SOCKS reply codes;
- HTTP CONNECT response codes;
- route rejection;
- auth failure;
- upstream timeout;
- DNS failure;
- refused connection;
- unsupported protocol/transport.

### WS12.4 Differential failure tests

Add coarse failure comparison with `pproxy`:

- auth failure;
- unreachable upstream;
- invalid target;
- unsupported UDP route;
- malformed client request.

## Acceptance criteria

- Chain and scheduler behavior is documented and tested.
- Failure behavior is stable enough for client compatibility.

---

# Phase 13: Rust public library API stabilization

## Goal

Before exposing Python bindings, stabilize a clean Rust library API. Python should bind to a deliberate API, not CLI internals.

## Proposed crate

```text
crates/eggress-embed/
```

or expose from `eggress-runtime` if clean enough.

## Required API concepts

```rust
pub struct EggressConfig { ... }
pub struct EggressService { ... }
pub struct EggressHandle { ... }
pub struct BoundAddresses { ... }
pub struct ServiceStatus { ... }
```

Suggested API:

```rust
impl EggressService {
    pub fn from_toml(toml: &str) -> Result<Self, Error>;
    pub fn from_config(config: EggressConfig) -> Result<Self, Error>;
    pub async fn start(self) -> Result<EggressHandle, Error>;
}

impl EggressHandle {
    pub fn bound_addresses(&self) -> BoundAddresses;
    pub async fn reload_toml(&self, toml: &str) -> Result<(), Error>;
    pub async fn metrics_text(&self) -> Result<String, Error>;
    pub async fn status(&self) -> Result<ServiceStatus, Error>;
    pub async fn shutdown(self) -> Result<(), Error>;
}
```

## Requirements

- no process-global state except optional logging initialization;
- multiple independent services in one process if feasible;
- deterministic shutdown;
- no panics across API boundary;
- structured errors;
- no requirement that caller owns a Tokio runtime unless documented;
- safe lifetime model.

## Acceptance criteria

- Rust integration tests start/stop service through embed API.
- CLI can optionally use embed API internally.
- API is suitable for PyO3 wrapping.

---

# Phase 14: Python bindings architecture

## Goal

Create Python bindings that expose Eggress as an embeddable library while Rust owns networking and runtime execution.

## Proposed package layout

```text
python/
├── pyproject.toml
├── README.md
├── eggress/
│   ├── __init__.py
│   ├── config.py
│   ├── service.py
│   ├── exceptions.py
│   └── py.typed
└── tests/
    ├── test_service.py
    ├── test_config.py
    ├── test_reload.py
    ├── test_metrics.py
    └── test_pproxy_compat.py

crates/eggress-python/
├── Cargo.toml
└── src/lib.rs
```

## Binding technology

Use PyO3 + maturin unless a later review finds a better option.

Recommended crate:

```text
crates/eggress-python
```

`pyproject.toml` should use maturin and produce wheels for common platforms.

## Python API sketch

```python
from eggress import EggressService, EggressConfig

config = EggressConfig.from_toml("""
version = 1
[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
[[rules]]
id = "all"
any = true
direct = true
""")

svc = EggressService(config)
svc.start()
print(svc.bound_addresses())
print(svc.metrics_text())
svc.shutdown()
```

Async context manager:

```python
async with EggressService.from_toml(toml) as svc:
    addr = svc.listener_addresses["socks"]
```

Synchronous context manager:

```python
with EggressService.from_toml(toml) as svc:
    ...
```

## Runtime model

Preferred:

- Rust owns a Tokio runtime on a dedicated thread or thread group.
- Python calls into a thread-safe handle.
- Python GIL is released for blocking start/stop/reload operations.
- Shutdown is deterministic.

Document limitations if multiple runtimes/services are restricted.

## Error model

Map Rust errors to Python exceptions:

```python
EggressError
ConfigError
RuntimeError
ReloadError
ProtocolError
UnsupportedFeatureError
```

Do not expose raw Rust error debug strings containing secrets.

## Acceptance criteria

- `pip install -e ./python` or `maturin develop` works locally.
- Python tests can start a SOCKS5 listener on port 0 and proxy to local TCP echo.
- Python tests can read metrics and shut down cleanly.
- Python package includes type hints.

---

# Phase 15: PyPI packaging, wheels, and release pipeline

## Goal

Make Eggress usable from Python without requiring a Rust toolchain for common platforms.

## Workstreams

### WS15.1 maturin packaging

Set up:

```text
pyproject.toml
Cargo metadata
README for PyPI
license metadata
classifiers
```

### WS15.2 Wheel builds

Target initial wheels:

- Linux x86_64 manylinux;
- Linux aarch64 manylinux;
- macOS x86_64;
- macOS arm64;
- Windows x86_64.

Add musllinux only if needed.

### WS15.3 GitHub Actions release pipeline

Add workflows:

- build wheels on tag;
- run Python tests against built wheel;
- upload artifacts;
- publish to TestPyPI;
- publish to PyPI only on explicit release tag/manual approval.

Hosted CI billing must be resolved before relying on this.

### WS15.4 Python docs

Create:

```text
docs/PYTHON_BINDINGS.md
docs/PYPI_RELEASE.md
```

Include:

- install instructions;
- sync API;
- async API;
- pproxy compatibility examples;
- lifecycle/shutdown guidance;
- limitations;
- platform support;
- version compatibility with Rust crate.

### WS15.5 Versioning policy

Decide whether Rust and Python versions are lockstep.

Recommended:

- same semver for Rust crate and Python package;
- Python package exposes `eggress.__version__`;
- Rust exposes version through API;
- pproxy-compat behavior documented by compatibility matrix version.

## Acceptance criteria

- TestPyPI package installs on at least Linux and macOS.
- Built wheel tests pass without Rust toolchain on target platform.
- Release docs are complete.

---

# Phase 16: Python library parity with pproxy use cases

## Goal

Make the Python package useful as a drop-in library replacement for users who previously embedded or orchestrated `pproxy` from Python.

## Workstreams

### WS16.1 Python pproxy-compatible helper API

Expose helpers such as:

```python
EggressService.from_pproxy_args(args: list[str])
EggressConfig.from_pproxy_uri(local: str, remotes: list[str])
translate_pproxy_args(args: list[str]) -> str  # TOML
```

### WS16.2 Python tests against pproxy examples

Add Python tests that mirror common pproxy examples:

- start local SOCKS5 direct;
- start local HTTP CONNECT direct;
- start local SOCKS5 through HTTP upstream;
- start local SOCKS5 through SOCKS5 upstream;
- start UDP ASSOCIATE direct;
- auth success/failure;
- config reload.

### WS16.3 Python concurrency tests

Ensure Python callers can:

- start service in sync context;
- start service in async context;
- run multiple services if supported;
- call metrics/status while traffic is active;
- shutdown without leaked threads;
- tolerate exceptions without leaving background Rust runtime alive.

### WS16.4 Python packaging quality

- type hints;
- docstrings;
- examples;
- no import-time side effects;
- no logging initialization unless requested;
- clean error messages.

## Acceptance criteria

- Python users can embed Eggress without managing sockets in Python.
- The Python package can replace common pproxy subprocess/library usage for supported features.

---

# Phase 17: Final true-parity audit and release candidate

## Goal

Perform a final audit before declaring Eggress a true Rust `pproxy` replacement.

## Required audits

1. Protocol matrix audit.
2. CLI compatibility audit.
3. Python binding lifecycle audit.
4. Differential test audit.
5. Security review refresh.
6. Dependency and wheel audit.
7. Performance sanity check against Python pproxy.
8. Documentation consistency audit.

## Performance comparison

Add local benchmarks comparing:

- pproxy SOCKS5 direct TCP echo throughput;
- Eggress SOCKS5 direct TCP echo throughput;
- pproxy HTTP CONNECT direct;
- Eggress HTTP CONNECT direct;
- UDP direct throughput where comparable.

Do not overfit benchmarks. Use them to catch obvious regressions and demonstrate Rust offload value.

## Release candidate docs

Create:

```text
docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md
```

Include:

- final matrix;
- supported features;
- intentional non-parity;
- Python package API;
- platform/wheel support;
- known limitations;
- migration guide.

## Acceptance criteria

- Every claimed pproxy-compatible feature has a test reference.
- Python package can start/stop/proxy/reload/metrics through Rust.
- Wheels install on supported platforms.
- Docs are honest and complete.

---

# Cross-cutting requirements

## Security

- No credentials in logs, metrics, admin, errors, or Python reprs.
- No high-cardinality metric labels.
- No unsafe Rust unless separately justified and audited.
- No OpenSSL/native-tls dependency.
- No native build tools in production wheels unless explicitly accepted.
- Python exceptions must redact secrets.

## Testing

Normal Rust checks:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Compatibility checks:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

Python checks:

```bash
maturin develop
python -m pytest python/tests
python -m mypy python/eggress
python -m ruff check python
```

Wheel checks:

```bash
maturin build --release
pip install dist/*.whl
python -m pytest python/tests
```

## Documentation

Docs that must remain current:

```text
docs/PPROXY_PARITY_SPEC.md
docs/PARITY_MATRIX.md
docs/CONFIG_REFERENCE.md
docs/OPERATIONS.md
docs/METRICS.md
docs/SECURITY_REVIEW.md
docs/PYTHON_BINDINGS.md
docs/PYPI_RELEASE.md
docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md
README.md
```

## Release blockers

The project must not claim true parity while any of these are true:

- Shadowsocks remains experimental/partial if pproxy parity includes Shadowsocks.
- Python package cannot start/stop a service cleanly.
- Python package requires a Rust toolchain on common target platforms.
- Differential tests do not cover the top migration paths.
- Docs advertise unsupported protocol combinations.
- Hosted CI/wheel release pipeline is unavailable without a documented fallback.
- Metrics/admin/Python reprs leak credentials.

---

# Suggested milestone sequence

| Milestone | Description | Outcome |
|---|---|---|
| M7 | Parity spec | Exact contract for true pproxy parity |
| M8 | CLI/URI compatibility | Common pproxy invocations translate/run |
| M9 | Shadowsocks TCP | Largest TCP protocol gap closed |
| M10 | Shadowsocks UDP + UDP parity | Largest UDP protocol gap closed |
| M11 | Remaining protocols | Explicit support or intentional non-parity |
| M12 | Scheduler/chain semantics | pproxy-like routing behavior verified |
| M13 | Rust embed API | Stable API for binding layer |
| M14 | Python bindings | Local Python package works via PyO3/maturin |
| M15 | PyPI wheels | Installable without Rust toolchain |
| M16 | Python pproxy library parity | Python users can replace common pproxy usage |
| M17 | Release candidate audit | True parity claim becomes defensible |

---

# Final definition of true parity

Eggress can claim true `pproxy` parity only when:

1. `docs/PPROXY_PARITY_SPEC.md` lists all relevant `pproxy` features.
2. `docs/PARITY_MATRIX.md` marks every relevant item as compatible, partial, unsupported, or intentional non-parity.
3. Every compatible item has runtime tests.
4. Core migration paths have differential tests against Python `pproxy`.
5. Shadowsocks TCP and UDP are interoperable, or explicitly removed from the true-parity claim with rationale.
6. CLI/URI compatibility covers common pproxy invocations.
7. Python bindings expose start/stop/reload/status/metrics and proxy traffic through Rust.
8. PyPI wheels install on supported platforms without requiring a Rust toolchain.
9. Security and observability policies remain intact.
10. Documentation clearly distinguishes Rust-native features, pproxy-compatible features, experimental features, and intentional non-parity.
