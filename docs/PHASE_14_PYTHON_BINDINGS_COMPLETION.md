# Phase 14: Python Bindings — Completion

## Summary

Phase 14 adds Python bindings for the eggress proxy via PyO3, wrapping the
stable `eggress-embed` API. The `eggress-python` crate produces a native
extension module (`_eggress`) and the `python/eggress` package provides a
Pythonic API layer.

## What was implemented

### Rust crate (`crates/eggress-python`)

- `PyEggressConfig` — wraps `eggress_embed::EggressConfig` with `from_toml`
  and `from_file` static methods
- `PyEggressService` — wraps `eggress_embed::EggressService` with `start()`
  returning a handle
- `PyEggressHandle` — wraps `eggress_embed::EggressHandle` with
  `bound_addresses`, `status`, `metrics_text`, `reload_toml`, `shutdown`,
  and context manager support
- Exception hierarchy: `EggressError` base with `ConfigError`, `StartupError`,
  `ReloadError`, `ShutdownError`, `UnsupportedFeatureError`, `InternalError`
- GIL release via `py.detach()` on all blocking Rust calls

### Python package (`python/eggress`)

- `EggressConfig` — Python wrapper with `from_toml`, `from_file`,
  `redacted_toml`
- `EggressService` — Python wrapper with `from_toml`, `from_file`, `start`
- `EggressHandle` — Python wrapper with properties, methods, and context
  manager protocol
- Re-exported exceptions from the native module

### Build system

- `Cargo.toml` with `cdylib` target, PyO3 `extension-module` feature
- `pyproject.toml` with maturin build backend, Python source layout

## API surface

| Python class | Rust inner | Key methods |
|---|---|---|
| `EggressConfig` | `PyEggressConfig` | `from_toml`, `from_file`, `redacted_toml` |
| `EggressService` | `PyEggressService` | `from_toml`, `from_file`, `start` |
| `EggressHandle` | `PyEggressHandle` | `bound_addresses`, `status`, `metrics_text`, `reload_toml`, `shutdown`, `__enter__`/`__exit__` |

## Tests

14 Python tests across 5 test files, all passing:

| File | Tests | Coverage |
|---|---|---|
| `test_config.py` | 4 | `from_toml`, `redacted_toml`, invalid TOML, invalid version |
| `test_service.py` | 3 | `from_toml`, start/stop lifecycle, context manager shutdown |
| `test_errors.py` | 3 | `ConfigError` raised, `EggressError` base catch, idempotent shutdown |
| `test_metrics.py` | 2 | Prometheus text output, status dict fields |
| `test_reload.py` | 2 | Applied reload returns generation, bad reload preserves service |

## Limitations

- Blocking only; no async Python API
- No Unix-domain socket support
- Platform-specific wheels required per architecture
- No pproxy URI translation from Python (use CLI or Rust crate)
- No listener hot-reload

## Blockers for Phase 15

None identified. The Python bindings are self-contained and do not depend on
unimplemented Rust features.
