# Python Bindings Reference

Python bindings for the eggress proxy, built on PyO3 and the `eggress-embed` crate.

## Overview

The `eggress` Python package provides a Pythonic interface to the Rust-based
eggress proxy. It wraps the `eggress-embed` stable API via PyO3, giving Python
programs access to the full proxy lifecycle: configuration parsing, service
startup, status/metrics, hot-reload, and graceful shutdown.

The native module is built with [maturin](https://github.com/PyO3/maturin) and
distributed as a platform-specific wheel.

## Installation

### Local development

```bash
# From the workspace root
cd crates/eggress-python
maturin build --release --target x86_64-apple-darwin   # adjust target
pip install --force-reinstall target/wheels/eggress-*.whl
```

### From PyPI (recommended)

```bash
pip install eggress
```

### From a wheel file

```bash
pip install dist/eggress-*.whl
```

No Rust toolchain is required when installing from a pre-built wheel.

### Requirements

- Python >= 3.9
- Rust toolchain (for building from source)
- maturin >= 1.0

## Quick start

### Context manager (recommended)

```python
from eggress import EggressService

toml = """
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
"""

with EggressService.from_toml(toml).start() as handle:
    print("Listening on", handle.bound_addresses)
    # ... use the proxy ...
# service is shut down automatically
```

### Async context manager

```python
import asyncio
from eggress import EggressService

TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""

async def main():
    async with await EggressService.from_toml(TOML).astart() as handle:
        print("Listening on", await handle.bound_addresses)

asyncio.run(main())
```

### Explicit start/stop

```python
from eggress import EggressService

svc = EggressService.from_toml(toml)
handle = svc.start()

print(handle.status())
print(handle.metrics_text())

handle.shutdown()
```

### Loading from a file

```python
from eggress import EggressConfig, EggressService

config = EggressConfig.from_file("config.toml")
handle = EggressService(config).start()
```

### Starting from pproxy arguments

```python
from eggress import EggressService

svc = EggressService.from_pproxy_args([
    "-l", "socks5://127.0.0.1:1080",
    "-r", "http://proxy:8080",
])

with svc.start() as handle:
    print("Listening on", handle.bound_addresses)
```

Or use the convenience function:

```python
from eggress import start_pproxy

with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)
```

## API reference

### `EggressConfig`

Parsed and validated TOML configuration.

| Method | Description |
|---|---|
| `EggressConfig.from_toml(toml: str)` | Parse a TOML configuration string |
| `EggressConfig.from_file(path: str \| PathLike)` | Load and validate a TOML file |
| `config.redacted_toml() -> str` | TOML source with credentials redacted |

### `EggressService`

Pre-start service builder. Consumed by `start()`.

| Method | Description |
|---|---|
| `EggressService(config)` | Create from an `EggressConfig` |
| `EggressService.from_toml(toml: str)` | Parse TOML and create a service |
| `EggressService.from_file(path: str \| PathLike)` | Load file and create a service |
| `EggressService.from_pproxy_args(args, allow_partial=False)` | Create from pproxy-style CLI arguments |
| `service.start() -> EggressHandle` | Start the service (blocking) |
| `service.astart() -> AsyncEggressHandle` | Start the service asynchronously |

### `EggressHandle`

Handle to a running service. Supports the context manager protocol.

| Method / Property | Description |
|---|---|
| `handle.bound_addresses` | `dict[str, str]` — listener name to address mapping |
| `handle.status() -> dict` | Generation, readiness, uptime, connections, listeners, UDP, upstreams |
| `handle.metrics_text() -> str` | Prometheus metrics text |
| `handle.reload_toml(toml: str) -> dict` | Hot-reload routing/upstreams; returns `{generation, upstreams}` |
| `handle.shutdown()` | Graceful shutdown (idempotent) |
| `with handle:` | Context manager — calls `shutdown()` on exit |

### `AsyncEggressHandle`

Async handle to a running service. All methods return awaitables.

| Method | Description |
|---|---|
| `await handle.bound_addresses` | Listener name to address mapping |
| `await handle.status() -> dict` | Current service status |
| `await handle.metrics_text() -> str` | Prometheus metrics text |
| `await handle.reload_toml(toml: str) -> dict` | Hot-reload routing |
| `await handle.shutdown()` | Graceful shutdown |
| `async with handle:` | Async context manager |

## Error model

All exceptions inherit from `EggressError`, which inherits from Python's
`Exception`. Subclasses map to specific error categories:

```
Exception
└── EggressError
    ├── ConfigError        — invalid or unsupported configuration
    ├── StartupError       — service failed to start
    ├── ReloadError        — hot-reload failed
    ├── ShutdownError      — shutdown encountered an error
    ├── UnsupportedFeatureError — requested feature not available
    └── InternalError      — unexpected internal failure
```

`ConfigError` is also a subclass of `EggressError`, so catching either works:

```python
from eggress import EggressConfig, ConfigError

try:
    EggressConfig.from_toml("invalid")
except ConfigError:
    print("bad config")
except EggressError:
    print("other eggress error")
```

## Thread and GIL behavior

All Rust calls that perform blocking I/O or CPU work release the GIL via
`py.detach()`. This means:

- Multiple Python threads can call eggress methods without serializing on the GIL.
- The Tokio runtime inside the service runs on dedicated threads and does not
  hold the Python GIL.
- Service startup (`start()`) blocks the calling thread while the Rust runtime
  initializes, but does not hold the GIL.

## Metrics and status

```python
with EggressService.from_toml(toml).start() as handle:
    # Prometheus text format
    print(handle.metrics_text())

    # Structured status
    status = handle.status()
    print(f"Generation: {status['generation']}")
    print(f"Ready: {status['readiness']}")
    print(f"Active connections: {status['active_connections']}")
    print(f"Listeners: {status['listeners']}")
```

## Reload support

Hot-reload replaces routing rules, upstream definitions, upstream groups, and
health configuration. Listener topology (bind addresses, protocol detection) is
not changed and requires a full restart.

```python
new_config = """
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[[upstreams]]
name = "upstream1"
addr = "proxy.example:8080"
"""

result = handle.reload_toml(new_config)
print(f"Generation: {result['generation']}")
```

A failed reload leaves the service running with the previous configuration.

## pproxy compatibility

The Python bindings expose the `eggress-pproxy-compat` translation layer directly.
Translate pproxy-style arguments without subprocesses:

```python
from eggress import translate_pproxy_args, translate_pproxy_uri, check_pproxy_args

# Translate CLI args
result = translate_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
print(result.toml)
print(result.warnings)
print(result.unsupported)

# Translate URI strings
result = translate_pproxy_uri("socks5://:1080", ["http://proxy:8080"])

# Start a service directly from pproxy args
from eggress import start_pproxy
with start_pproxy(["-l", "socks5://:1080"]) as handle:
    ...
```

### Translation result types

| Type | Description |
|---|---|
| `TranslationResult` | Result with `.toml`, `.warnings`, `.unsupported`, `.ok`, `.config()` |
| `TranslationWarning` | `.category` and `.message` describing a partial-behavior note |
| `UnsupportedFeature` | `.feature` and `.message` for unsupported pproxy features |

## Limitations

- **Blocking only**: `start()` is synchronous. Async Python usage requires
  running `start()` in a thread executor (`asyncio.to_thread`).
- **Single-threaded startup**: The service runs one Tokio runtime internally.
  Concurrent `start()` calls on the same `EggressService` will fail.
- **No listener hot-reload**: Adding or removing listeners requires a full
  restart.
- **No Unix-domain sockets**: Not yet supported by the underlying Rust runtime.
- **Platform-specific wheels**: Each platform/architecture requires its own
  built wheel.
- **No embedded async API**: The Python bindings use the blocking `start_blocking`
  path only. An async Python API is not yet available.
- **pproxy compat**: Shadowsocks TCP is experimental/non-standard. No inbound
  Shadowsocks or Trojan listeners. No legacy stream ciphers. No SSH/unix/redir
  transport. No pproxy daemon mode. Multiple remotes default to round-robin.

## Relationship to pproxy compatibility

The Python bindings now expose the `eggress-pproxy-compat` translation layer
directly via `translate_pproxy_args`, `translate_pproxy_uri`, and
`check_pproxy_args`. See the [pproxy compatibility](#pproxy-compatibility)
section above. For CLI-based translation, you can still use:

```bash
python -m eggress pproxy translate -- -l socks5://:1080 -r http://proxy:8080
```
