# Eggress Python API — Current State

> Phase 29 audit of the existing Python package surface.

## Package Metadata

- **Package name**: `eggress`
- **Version source**: `CARGO_PKG_VERSION` from the Rust crate (embedded in `_eggress.__version__`); Python fallback is `"0.1.0"`
- **Python version requirement**: `>=3.9`
- **Build system**: maturin (`>=1.0,<2.0`) with `pyo3/extension-module` feature
- **Module path**: `eggress._eggress` (native extension)
- **License**: MIT OR Apache-2.0
- **Classifiers**: Alpha, Typed, supports Python 3.9–3.12

## Native Module (`eggress._eggress`)

The native module is defined in `crates/eggress-python/src/lib.rs`. All blocking Rust calls use `py.detach()` to release the GIL.

### Classes

#### `EggressConfig`

| Member | Parameters / Types | Return | GIL Released | Description |
|--------|-------------------|--------|--------------|-------------|
| `from_toml` (static) | `toml_str: str` | `EggressConfig` | Yes | Parse a TOML configuration string |
| `from_file` (static) | `path: str` | `EggressConfig` | Yes | Load and validate a TOML config file |
| `redacted_toml` | `&self` | `str` | Yes | Return TOML with credentials redacted |

#### `EggressService`

| Member | Parameters / Types | Return | GIL Released | Description |
|--------|-------------------|--------|--------------|-------------|
| `__init__` | `config: EggressConfig` | `EggressService` | No | Create from a parsed config |
| `from_toml` (static) | `toml_str: str` | `EggressService` | Yes | Parse TOML and create service |
| `from_file` (static) | `path: str` | `EggressService` | Yes | Load file and create service |
| `start` | `&mut self` | `EggressHandle` | Yes | Start the service (consuming it) |

#### `EggressHandle`

| Member | Parameters / Types | Return | GIL Released | Description |
|--------|-------------------|--------|--------------|-------------|
| `bound_addresses` | `&self` | `dict[str, str]` | Yes | Listener name→addr mapping + `_admin` key |
| `status` | `&self` | `dict` | Yes | Generation, readiness, active connections, uptime, listeners |
| `metrics_text` | `&self` | `str` | Yes | Prometheus-compatible metrics text |
| `reload_toml` | `&self, toml_str: str` | `dict` | Yes | Hot-reload routing/upstreams; returns generation + upstreams |
| `shutdown` | `&mut self` | `None` | Yes | Graceful shutdown (idempotent) |
| `__enter__` / `__exit__` | — | `Self` / `bool` | Yes (`__exit__`) | Context manager; `__exit__` calls shutdown |

#### `TranslationResult`

| Member | Parameters / Types | Return | GIL Released | Description |
|--------|-------------------|--------|--------------|-------------|
| `toml` (getter) | — | `str` | No | Generated TOML output |
| `warnings` (getter) | — | `list[TranslationWarning]` | No | Non-fatal translation warnings |
| `unsupported` (getter) | — | `list[UnsupportedFeature]` | No | Unsupported pproxy features |
| `ok` (getter) | — | `bool` | No | `True` if no unsupported features |
| `config` | `&self` | `EggressConfig` | Yes | Parse the generated TOML into a config |

#### `TranslationWarning`

| Member | Type | Description |
|--------|------|-------------|
| `category` (getter) | `str` | Warning category |
| `message` (getter) | `str` | Human-readable message |

#### `UnsupportedFeature`

| Member | Type | Description |
|--------|------|-------------|
| `feature` (getter) | `str` | Feature name |
| `message` (getter) | `str` | Explanation |

#### `ReverseUriSummary`

| Member | Type | Description |
|--------|------|-------------|
| `role` (getter) | `str` | `"server"`, `"client"`, or `"unknown"` |
| `scheme` (getter) | `str` | pproxy scheme (e.g. `socks5+in`) |
| `target` (getter) | `str` | Redacted `host:port` |
| `has_auth` (getter) | `bool` | Whether credentials are present |
| `toml_section` (getter) | `str` | `"reverse_servers"`, `"reverse_clients"`, or `"unknown"` |
| `tls` (getter) | `bool` | TLS flag |
| `modifiers` (getter) | `list[str]` | Parsed modifiers (e.g. `["+tls", "+in"]`) |

### Functions

| Function | Parameters / Types | Return | GIL Released | Description |
|----------|-------------------|--------|--------------|-------------|
| `translate_pproxy_args` | `args: Sequence[str]` | `TranslationResult` | Yes | Translate pproxy CLI args to TOML |
| `translate_pproxy_uri` | `local: str, remotes: Sequence[str] \| None` | `TranslationResult` | Yes | Translate pproxy URIs to TOML |
| `check_pproxy_args` | `args: Sequence[str]` | `TranslationResult` | Yes | Alias for `translate_pproxy_args` |
| `describe_reverse_pproxy_uri` | `uri: str` | `ReverseUriSummary` | No | Inspect a reverse pproxy URI |

### Exceptions

All exceptions inherit from `EggressError` which inherits from Python's `Exception`.

```
Exception
  └── EggressError
        ├── ConfigError
        ├── StartupError
        ├── ReloadError
        ├── ShutdownError
        ├── UnsupportedFeatureError
        └── InternalError
```

### Module Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `__version__` | `str` | Crate version from `CARGO_PKG_VERSION` |

## Public Python API (`eggress`)

### `__init__.py` Exports

The `eggress` package re-exports everything from the native module and adds Python-level wrappers. All public symbols are listed in `__all__` (25 items).

### `EggressConfig` — `eggress/config.py`

Python wrapper around the native `PyEggressConfig`.

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `from_toml` (classmethod) | `toml: str` | `EggressConfig` | Parse TOML string |
| `from_file` (classmethod) | `path: str \| PathLike[str]` | `EggressConfig` | Load from file |
| `redacted_toml` | — | `str` | Redacted TOML output |
| `__repr__` | — | `str` | `"EggressConfig(...)"` |

### `EggressService` — `eggress/service.py`

Pre-start service builder.

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `__init__` | `config: EggressConfig` | `EggressService` | Wrap a parsed config |
| `from_toml` (classmethod) | `toml: str` | `EggressService` | Parse TOML + create service |
| `from_file` (classmethod) | `path: str \| PathLike[str]` | `EggressService` | Load file + create service |
| `from_pproxy_args` (classmethod) | `args: Sequence[str], allow_partial: bool = False` | `EggressService` | Translate pproxy args; raises `UnsupportedFeatureError` if `allow_partial=False` and features unsupported |
| `start` | — | `EggressHandle` | Blocking start |
| `astart` | — | `AsyncEggressHandle` | Async start (delegates to `asyncio.to_thread`) |

### `EggressHandle` — `eggress/service.py`

Blocking handle to a running service.

| Member | Signature | Description |
|--------|-----------|-------------|
| `bound_addresses` | `property → dict[str, str]` | Listener name→address map |
| `status()` | `→ dict[str, Any]` | Generation, readiness, connections, uptime, listeners |
| `metrics_text()` | `→ str` | Prometheus metrics |
| `reload_toml(toml)` | `(str) → dict[str, Any]` | Hot-reload; returns generation + upstreams |
| `shutdown()` | `→ None` | Graceful shutdown |
| `__enter__` / `__exit__` | context manager | Calls `shutdown()` on exit |

### `AsyncEggressHandle` — `eggress/service.py`

Async handle. All methods are `async` and delegate to `asyncio.to_thread`.

| Member | Signature | Description |
|--------|-----------|-------------|
| `bound_addresses` | `async property → dict[str, str]` | Listener name→address map |
| `status()` | `async → dict[str, Any]` | Service status |
| `metrics_text()` | `async → str` | Prometheus metrics |
| `reload_toml(toml)` | `async (str) → dict[str, Any]` | Hot-reload |
| `shutdown()` | `async → None` | Graceful shutdown |
| `__aenter__` / `__aexit__` | async context manager | Calls `shutdown()` on exit |

### pproxy Compatibility Functions — `eggress/pproxy.py`

| Function | Args | Returns | Description |
|----------|------|---------|-------------|
| `translate_pproxy_args` | `args: Sequence[str]` | `TranslationResult` | Translate pproxy CLI args |
| `translate_pproxy_uri` | `local: str, remotes: Sequence[str] = ()` | `TranslationResult` | Translate pproxy URIs |
| `check_pproxy_args` | `args: Sequence[str]` | `TranslationResult` | Alias for `translate_pproxy_args` |
| `describe_reverse_pproxy_uri` | `uri: str` | `ReverseUriSummary` | Inspect reverse pproxy URI |

### `start_pproxy()` — Convenience Function

Defined in `eggress/__init__.py`. Translates pproxy args, creates a service, starts it, and returns a handle. Combines `translate_pproxy_args` + `EggressService` + `start()` in one call.

```python
def start_pproxy(args: Sequence[str], allow_partial: bool = False) -> EggressHandle
```

### Python-Level Dataclasses

| Class | Fields | Description |
|-------|--------|-------------|
| `TranslationWarning` | `category: str, message: str` | Non-fatal translation warning |
| `UnsupportedFeature` | `feature: str, message: str` | Unsupported pproxy feature |
| `ReverseUriSummary` | `role, scheme, target, has_auth, toml_section, tls, modifiers` | Reverse URI inspection result |

### Error Hierarchy

```
eggress.EggressError
  ├── eggress.ConfigError
  ├── eggress.StartupError
  ├── eggress.ReloadError
  ├── eggress.ShutdownError
  ├── eggress.UnsupportedFeatureError
  └── eggress.InternalError
```

All errors are re-exported from `eggress._eggress` via `eggress.exceptions` and `eggress.__init__`.

## Thread Safety

- **GIL release**: All blocking Rust calls use `py.detach()`, which releases the Python GIL during execution. This allows other Python threads to run concurrently while eggress I/O operations are in progress.
- **Concurrent access**: `EggressHandle` methods are safe to call from multiple threads (the underlying Rust handle is `Send + Sync`). However, `shutdown()` consumes the handle (sets inner to `None`); subsequent calls on the same handle are no-ops.
- **Async path**: `AsyncEggressHandle` delegates all operations to `asyncio.to_thread`, keeping the event loop unblocked. The `astart()` method similarly runs `start_blocking()` in a thread executor.

## Example Inventory

| File | Description |
|------|-------------|
| `python/examples/start_socks5.py` | Start a SOCKS5 proxy from TOML, block until Ctrl+C |
| `python/examples/async_service.py` | Async usage with `astart()` and async context manager |
| `python/examples/reload_config.py` | Start a service and hot-reload configuration at runtime |
| `python/examples/pproxy_translate.py` | Translate pproxy CLI args to eggress TOML (no service start) |
| `python/examples/pproxy_run.py` | Start a service directly from pproxy CLI args via `start_pproxy()` |

## Gap Summary vs pproxy

The following pproxy features are **not yet exposed** through the Python bindings. These are deferred to Phase 29.3 tier classification:

- **Direct protocol class access** — No Python API to instantiate or configure individual protocol handlers (SOCKS5, HTTP CONNECT, etc.) independently.
- **Cipher access** — No API to select or configure Shadowsocks cipher suites from Python; cipher selection is config-driven only.
- **Plugin access** — No Python API for pproxy's plugin system (obfs, auth, etc.).
- **Scheduling algorithm constants** — No exported constants for scheduler types (round-robin, least-connections, first-available); these are config-string-driven.
- **`main()` CLI entry point** — No Python equivalent of the Rust `eggress-cli` binary's `main()`; embedding is via `EggressService` / `start_pproxy()` only.
