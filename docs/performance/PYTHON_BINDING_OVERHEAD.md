# Python Binding Overhead

Phase 34 — Performance, Soak, and Regression Gates

## Overview

Measure overhead introduced by the Python/PyO3 wrapper layer compared
to calling the same Rust functions directly.

## Test Scenarios

Tests live in `python/tests/test_performance_smoke.py`.

### Import Cost

Measure `import eggress` time. Expected: under 50ms on modern hardware.

### URI Translation

Compare `eggress.check_pproxy_uri()` call overhead. This calls through
PyO3 into the Rust URI parser and returns a `UriInfo` dataclass.

Expected: under 5ms per call for simple URIs.

### Config Compile

Measure `eggress.parse_toml_config()` with a typical multi-listener
config. This exercises TOML parsing + validation through PyO3.

Expected: under 100ms for a 5-listener config.

### Service Start/Stop

Measure `EggressService` start and stop overhead through the Python API.
This includes Tokio runtime creation and graceful shutdown.

Expected: start under 200ms, stop under 1s (grace period dependent).

### Status/Metrics Polling

Measure `EggressHandle` status query and metrics text retrieval overhead.
This reads shared state without lock contention.

Expected: under 1ms per call.

### GIL Release

Verify GIL is released during blocking Rust calls. Tested by running
concurrent Python threads while a long-running Rust operation executes.

## Running

```bash
# Build Python bindings first
maturin develop

# Run performance smoke tests
python -m pytest python/tests/test_performance_smoke.py -v

# Run with timing output
python -m pytest python/tests/test_performance_smoke.py -v --tb=short
```

## Known Limitations

- PyO3 introduces marshaling overhead for complex return types (e.g.,
  route explanation results).
- GIL release is implemented for all blocking Rust calls via `py.detach()`,
  but very short calls may not benefit measurably.
- Tokio runtime creation cost is one-time per `EggressService` instance;
  subsequent operations share the runtime.
- Benchmark numbers are environment-dependent; record OS/CPU/Rust version
  when comparing across machines.
