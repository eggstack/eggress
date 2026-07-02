# Phase 30 Plan: Python pproxy Server Lifecycle Compatibility

## Purpose

Phase 30 implements the first substantial Python drop-in surface: a pproxy-shaped server lifecycle wrapper backed by Eggress's Rust runtime.

The goal is not to expose every pproxy Python API. The goal is to make the common embedded use case work from Python: construct a server-like object, configure listeners/remotes, start it, wait for readiness, use it in tests or applications, and shut it down deterministically.

## Scope

This phase covers:

- A pproxy-shaped Python server object or compatibility wrapper.
- Sync and async lifecycle entry points where feasible.
- Binding to local/ephemeral ports and exposing bound addresses.
- Config generation from pproxy-style listen/remote inputs.
- Start/stop/drain semantics backed by the Rust supervisor.
- Error mapping and redaction.
- Context-manager support.
- Tests comparing common pproxy embedded lifecycle behavior against Eggress.

## Non-goals

Do not implement every pproxy module symbol.

Do not implement pproxy internals that are not needed for server lifecycle.

Do not attempt to fake pproxy's asyncio internals if a simpler compatibility wrapper is safer. Document differences.

Do not block on full Python API namespace/package aliasing. That is Phase 32.

## Design principle

Python code should control lifecycle; Rust should own networking.

Avoid exposing low-level Tokio details to Python. Python users should see a small server object with predictable methods and properties, while Rust manages listeners, routing, chains, metrics, and shutdown.

## Work items

### 30.1 Define target API shape from Phase 29 inventory

Use the Phase 29 API inventory to select the initial compatibility target.

Define exact target names and signatures. Potential shapes:

```python
server = eggress.pproxy.Server(
    listen=["socks5://127.0.0.1:0"],
    remote=["http://127.0.0.1:8080"],
)
await server.start()
print(server.addresses)
await server.close()
```

or a closer pproxy shape if Phase 29 discovers a different constructor.

Document chosen target in:

```text
docs/python/SERVER_LIFECYCLE_COMPATIBILITY.md
```

### 30.2 Build Rust-side service handle for Python

Add or refine a Rust-backed service handle exposed through PyO3.

Requirements:

- owns or references a Rust runtime/supervisor;
- supports start once / stop once semantics;
- exposes readiness state;
- exposes bound listener addresses;
- supports cancellation/shutdown grace;
- prevents use-after-stop;
- maps Rust errors into Python exceptions;
- is thread-safe or explicitly not thread-safe;
- does not leak runtime tasks on deletion.

Potential Rust types:

```rust
#[pyclass]
pub struct PyEggressService { ... }

#[pymethods]
impl PyEggressService {
    #[new]
    fn new(config: PyServiceConfig) -> PyResult<Self>;
    fn start(&self) -> PyResult<()>;
    fn stop(&self) -> PyResult<()>;
    fn is_ready(&self) -> bool;
    fn addresses(&self) -> PyResult<Vec<String>>;
}
```

If async Python methods are exposed, use `pyo3-async-runtimes` or a carefully documented bridge.

### 30.3 Implement pproxy-style Python `Server` wrapper

Implement the public compatibility wrapper in Python or PyO3.

Suggested path:

```text
python/eggress/pproxy.py
```

Possible API:

```python
class Server:
    def __init__(self, listen=None, remote=None, *, config=None, **kwargs): ...
    async def start(self): ...
    async def close(self): ...
    async def wait_closed(self): ...
    def run(self): ...
    def stop(self): ...
    @property
    def addresses(self): ...
    async def __aenter__(self): ...
    async def __aexit__(self, exc_type, exc, tb): ...
```

Use Phase 29 inventory to decide exact naming and compatibility aliases.

Requirements:

- constructor accepts pproxy URI strings where supported;
- unsupported pproxy args produce structured compatibility errors;
- credentials are redacted in exceptions;
- object can be used in tests with ephemeral ports;
- repeated `start()` fails or no-ops consistently with documented semantics;
- repeated `close()` is idempotent;
- deletion/finalizer should not be the primary cleanup mechanism.

### 30.4 Translate pproxy-style constructor arguments

Build or reuse translation from CLI compatibility code.

Inputs:

- listen URI(s);
- remote URI(s);
- UDP listen/remotes if supported;
- scheduler if provided;
- auth;
- config file or TOML object;
- unsupported flags/kwargs.

Output:

- Eggress runtime config in memory;
- generated TOML string for debugging;
- feature classification/warnings;
- structured diagnostics.

Requirements:

- Python and CLI translation use the same parser/classifier as much as possible;
- results are deterministic;
- every warning/error has a diagnostic code;
- generated config can be inspected from Python for debugging.

### 30.5 Event loop and runtime ownership

Define how the wrapper behaves in Python sync and async contexts.

Cases:

- called from normal synchronous script;
- called inside an existing asyncio event loop;
- used as async context manager;
- used from pytest/pytest-asyncio;
- used in a thread;
- process receives Ctrl-C/SIGTERM.

Implementation options:

1. Rust owns a background Tokio runtime and Python methods are thin sync/async wrappers.
2. Python async methods call into Rust async functions via pyo3 async bridge.
3. Separate sync and async objects.

Pick one and document tradeoffs.

Requirements:

- no nested event-loop errors in common cases;
- no hanging background threads after close;
- cancellation propagates to Rust supervisor;
- readiness wait has timeout support.

### 30.6 Bound address and ephemeral port reporting

Embedded users need to know where the proxy bound, especially with port 0.

Expose:

- `server.addresses` or equivalent;
- listener name to address mapping;
- admin address if enabled;
- Unix socket path if used;
- readiness status.

Tests:

- port 0 listener returns actual port;
- multiple listeners return stable order/names;
- address unavailable before start or returns empty according to documented semantics;
- address remains available after stop if useful for diagnostics.

### 30.7 Python exception taxonomy

Expose Python exceptions that preserve structured diagnostics.

Suggested classes:

```python
class EggressError(Exception): ...
class CompatError(EggressError): ...
class UnsupportedFeatureError(CompatError): ...
class ConfigError(EggressError): ...
class RuntimeStartError(EggressError): ...
class AlreadyStartedError(EggressError): ...
class NotStartedError(EggressError): ...
```

Requirements:

- include `.code`, `.feature_id`, `.tier`, `.suggestion` where available;
- `str(error)` redacts credentials;
- repr does not leak credentials;
- Python tests assert redaction.

### 30.8 Lifecycle tests against real networking

Add pytest tests for real Rust-backed lifecycle behavior.

Suggested file:

```text
python/tests/test_server_lifecycle.py
```

Tests:

- import wrapper;
- construct server with HTTP listener on `127.0.0.1:0`;
- start and wait ready;
- connect through proxy to local TCP echo/HTTP target;
- stop and verify listener closes;
- repeated close idempotent;
- double start behavior;
- async context manager;
- unsupported URI raises structured error;
- credentials redacted;
- multiple listeners;
- remote chain smoke if currently supported.

### 30.9 pproxy oracle comparison tests

Use Phase 29 harness to compare lifecycle behavior for selected cases.

Cases:

- constructor accepts same basic inputs;
- start/stop lifecycle maps to same broad behavior;
- HTTP listener relay;
- SOCKS5 listener relay;
- unsupported scheme behavior classified;
- auth failure behavior.

Gate with:

```text
EGRESS_REQUIRE_PPROXY_PYTHON_API=1
```

Do not require byte-for-byte exception strings. Compare behavior classes and connection results.

### 30.10 Documentation updates

Update:

- `docs/python/SERVER_LIFECYCLE_COMPATIBILITY.md`;
- `docs/python/README.md`;
- `docs/python/EGGRESS_PYTHON_API_CURRENT_STATE.md`;
- `docs/COMPATIBILITY_EVIDENCE.md`;
- `docs/PARITY_MATRIX.md`;
- README Python section;
- `tests/compat/pproxy_manifest.toml`.

Docs must distinguish:

- pproxy-shaped lifecycle compatibility;
- exact pproxy API parity not yet complete;
- Eggress-native Python API alternatives.

## Validation commands

```bash
maturin develop
python -m pytest python/tests/test_server_lifecycle.py -q
python -m pytest python/tests/test_pproxy_compat.py -q
cargo test -p eggress-python
cargo test -p eggress-testkit manifest
```

Gated oracle:

```bash
python -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/compat/test_pproxy_api_oracle.py -q
```

## Acceptance criteria

Phase 30 is complete when:

- A pproxy-shaped `Server` wrapper exists or a documented equivalent compatibility object exists.
- It can start and stop a Rust-backed Eggress proxy from Python.
- It reports bound addresses for ephemeral listeners.
- It supports async context-manager lifecycle.
- It rejects unsupported pproxy features with structured redacted errors.
- Real Python networking lifecycle tests pass.
- Gated pproxy oracle tests exist for common lifecycle cases.
- Manifest and docs accurately classify the implemented Python lifecycle surface.

## Handoff notes

Do not let a Python wrapper become a separate runtime. It should drive the same Rust config/supervisor path used by the CLI, so protocol behavior and evidence remain shared.
