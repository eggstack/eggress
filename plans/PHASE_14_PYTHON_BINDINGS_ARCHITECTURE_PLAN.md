# Phase 14 Detailed Plan: Python Bindings Architecture

## Purpose

Phase 14 adds Python bindings over the Rust embed API from Phase 13. The Python package should let Python callers start, control, reload, inspect, and stop Eggress while Rust handles all networking, proxy protocols, routing, UDP relay, metrics, and runtime execution.

Python must not reimplement proxy networking. The binding layer is a control surface.

---

# Prerequisites

Required from Phase 13:

- `eggress-embed` crate exists;
- sync/blocking start path suitable for Python exists;
- service handle supports bound addresses, metrics, status, reload, and shutdown;
- errors are structured and redacted;
- embed tests pass.

If Phase 13 is incomplete, do not build bindings around CLI internals. Complete or patch the embed API first.

---

# Non-goals

Do not implement:

- PyPI release automation;
- production wheel matrix;
- Python pproxy helper API beyond minimal examples;
- new proxy protocols;
- Python-side async networking;
- Python packet forwarding;
- unsafe behavior or native TLS/OpenSSL;
- automatic system proxy configuration.

---

# Workstream 1: Package and crate layout

## Target layout

```text
crates/eggress-python/
├── Cargo.toml
└── src/lib.rs

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
    ├── test_config.py
    ├── test_service.py
    ├── test_reload.py
    ├── test_metrics.py
    └── test_errors.py
```

## Binding technology

Use:

- PyO3;
- maturin;
- Python typing files or inline type hints;
- pytest for tests.

## Acceptance criteria

- `maturin develop` builds a local Python module.
- `python -c "import eggress"` works.

---

# Workstream 2: PyO3 Rust binding crate

## Goal

Expose a small native extension module that wraps `eggress-embed`.

## Target crate

```text
crates/eggress-python
```

## Cargo requirements

- crate type `cdylib`;
- dependency on `pyo3`;
- dependency on `eggress-embed`;
- no direct dependency on CLI internals;
- no duplicate runtime implementation.

## PyO3 module sketch

```rust
#[pymodule]
fn _eggress(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyEggressConfig>()?;
    m.add_class::<PyEggressService>()?;
    m.add_class::<PyEggressHandle>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
```

## Acceptance criteria

- Native module exposes config/service/handle classes.
- Build does not require a Python event loop.

---

# Workstream 3: Python object model

## Public Python API

Target user-facing imports:

```python
from eggress import EggressConfig, EggressService
```

## Config API

```python
class EggressConfig:
    @classmethod
    def from_toml(cls, toml: str) -> "EggressConfig": ...

    @classmethod
    def from_file(cls, path: str | PathLike[str]) -> "EggressConfig": ...

    def redacted_toml(self) -> str: ...
```

## Service API

```python
class EggressService:
    def __init__(self, config: EggressConfig): ...

    @classmethod
    def from_toml(cls, toml: str) -> "EggressService": ...

    @classmethod
    def from_file(cls, path: str | PathLike[str]) -> "EggressService": ...

    def start(self) -> "EggressHandle": ...
```

## Handle API

```python
class EggressHandle:
    @property
    def bound_addresses(self) -> dict[str, str]: ...

    def status(self) -> dict[str, object]: ...

    def metrics_text(self) -> str: ...

    def reload_toml(self, toml: str) -> dict[str, object]: ...

    def shutdown(self) -> None: ...
```

## Context manager API

```python
with EggressService.from_toml(toml).start() as handle:
    addr = handle.bound_addresses["socks"]
```

## Acceptance criteria

- Python callers can use context manager lifecycle.
- Explicit `shutdown()` is idempotent or documented as single-use.

---

# Workstream 4: GIL and blocking behavior

## Goal

Prevent Python from blocking the GIL during start, reload, metrics, and shutdown operations.

## Requirements

- release GIL for blocking Rust calls using `py.allow_threads`;
- keep Rust handle thread-safe;
- never call Python callbacks from Rust runtime threads;
- ensure exceptions are converted after reacquiring GIL;
- shutdown joins Rust-owned runtime cleanly.

## Tests

- start service while another Python thread increments counter;
- metrics call does not deadlock;
- shutdown returns promptly;
- exception during reload does not poison handle.

## Acceptance criteria

- No Python deadlocks in basic thread tests.

---

# Workstream 5: Error mapping

## Goal

Map Rust errors into Python exception classes.

## Python exceptions

```python
class EggressError(Exception): ...
class ConfigError(EggressError): ...
class StartupError(EggressError): ...
class ReloadError(EggressError): ...
class ShutdownError(EggressError): ...
class UnsupportedFeatureError(EggressError): ...
class InternalError(EggressError): ...
```

## Requirements

- exceptions expose a safe message;
- no credentials in exception strings or reprs;
- Python exception type reflects Rust error variant;
- invalid TOML raises `ConfigError`;
- unsupported feature raises `UnsupportedFeatureError` if available;
- reload failure raises `ReloadError` or returns a structured rejected outcome depending embed API design.

## Tests

- invalid config raises `ConfigError`;
- credential-containing invalid config error is redacted;
- shutdown twice behavior is deterministic;
- reload bad TOML does not kill service.

## Acceptance criteria

- Python error model is stable and documented.

---

# Workstream 6: Python tests with real local traffic

## Test files

```text
python/tests/test_service.py
python/tests/test_reload.py
python/tests/test_metrics.py
python/tests/test_errors.py
```

## Required scenarios

1. Import package.
2. Start SOCKS5 listener on `127.0.0.1:0`.
3. Discover bound address.
4. Send TCP echo through SOCKS5 direct path.
5. Read metrics after traffic.
6. Reload config and observe generation/status update.
7. Bad reload keeps service alive.
8. Context manager shuts down.
9. Error messages redact credentials.
10. Multiple services can start simultaneously if embed API supports it; otherwise document single-service limitation.

## Test helper policy

Python tests may implement simple TCP echo and minimal SOCKS5 client logic, but must not implement proxy server behavior.

## Acceptance criteria

- `python -m pytest python/tests` passes after `maturin develop`.

---

# Workstream 7: Type hints and Python packaging skeleton

## Requirements

- include `py.typed`;
- add Python wrapper classes with type hints;
- expose `__version__`;
- document minimum Python version;
- avoid import-time side effects;
- no logging initialization on import;
- no service start on import.

## pyproject baseline

Use maturin:

```toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "eggress"
requires-python = ">=3.9"
```

The exact Python version floor can be adjusted, but keep it explicit.

## Acceptance criteria

- `python -m mypy python/eggress` or equivalent type check is possible.
- Package has no import-time runtime side effects.

---

# Workstream 8: Docs

## Required docs

Create:

```text
docs/PYTHON_BINDINGS.md
```

Update:

```text
python/README.md
README.md
docs/ROADMAP.md
AGENTS.md
```

## Required content

- install for local development;
- sync context manager example;
- service start/stop example;
- metrics example;
- reload example;
- error model;
- thread/GIL behavior;
- limitations;
- relationship to pproxy compatibility.

---

# Recommended commit sequence

1. Add `eggress-python` crate and `python/` skeleton.
2. Bind config/service/handle classes.
3. Add Python wrapper modules and exceptions.
4. Add lifecycle/context-manager support.
5. Add metrics/status/reload bindings.
6. Add Python traffic tests.
7. Add typing/docs/completion record.

---

# Required verification

Rust:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p eggress-embed
cargo test -p eggress-python
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Python local development:

```bash
maturin develop
python -m pytest python/tests
python -m mypy python/eggress
python -m ruff check python
```

If mypy/ruff are not yet adopted, document that explicitly and at least run pytest.

---

# Definition of done

Phase 14 is complete only when:

1. PyO3 extension crate exists.
2. Python package imports locally.
3. Python can start and stop Eggress through Rust.
4. Python can discover bound addresses.
5. Python can proxy local TCP traffic through Eggress.
6. Python can read metrics/status.
7. Python can reload config or gets documented limitation.
8. Errors map to Python exceptions and redact secrets.
9. GIL is released for blocking Rust calls.
10. Type hints and docs exist.
11. Local Rust and Python tests pass.

## Completion record

Add:

```text
docs/PHASE_14_PYTHON_BINDINGS_COMPLETION.md
```

Include API surface, tests, limitations, and blockers for Phase 15 wheel/PyPI packaging.
