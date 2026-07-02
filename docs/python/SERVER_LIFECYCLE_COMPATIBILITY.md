# Python Server Lifecycle Compatibility (Phase 30)

## Overview

Phase 30 adds a pproxy-shaped `Server` wrapper to the Python bindings, enabling
pproxy-style server lifecycle management backed by the Rust runtime.

## Target API

```python
import eggress

# Basic usage
server = eggress.Server(
    listen=["socks5://127.0.0.1:1080"],
    remote=["http://proxy.example.com:8080"],
)
server.start()
print(server.addresses)  # {"pproxy-local-0": "127.0.0.1:1080"}
server.close()

# Context manager (recommended)
with eggress.Server(
    listen=["http://127.0.0.1:0"],
    remote=["socks5://proxy:1080"],
) as server:
    print(server.addresses)
    # ... use server ...
# Automatically stops on exit

# Async context manager
async with eggress.Server(
    listen=["socks5://127.0.0.1:0"],
) as server:
    print(server.addresses)

# Pre-built config
from eggress import EggressConfig, Server

cfg = EggressConfig.from_toml("""
version = 1

[[listeners]]
name = "my-proxy"
bind = "127.0.0.1:0"
protocols = ["socks5"]
""")
with Server(config=cfg) as server:
    print(server.addresses)
```

## Constructor

```python
Server(
    listen: list[str] | None = None,   # List of pproxy listener URIs
    remote: list[str] | None = None,   # List of pproxy upstream URIs
    *,                                 # Keyword-only
    config: EggressConfig | None = None,  # Pre-built config (mutually exclusive with listen/remote)
    allow_partial: bool = False,       # Start despite unsupported features
)
```

- `listen` and `remote` use pproxy URI syntax (e.g., `socks5://:1080`, `http://proxy:8080`)
- `config` accepts a pre-built `EggressConfig` (mutually exclusive with `listen`/`remote`)
- `allow_partial=True` starts the service even when some features are unsupported (e.g., SSH upstreams)

## Lifecycle Methods

| Method | Description |
|--------|-------------|
| `start()` | Start the server. Returns `self` for chaining. Raises `AlreadyStartedError` if already running. |
| `stop()` | Stop the server. Alias for `close()`. |
| `close()` | Stop the server. Idempotent — safe to call multiple times. |
| `run()` | Start and block until interrupted (SIGINT/SIGTERM). |
| `astart()` | Async start via `asyncio.to_thread`. |
| `aclose()` | Async close via `asyncio.to_thread`. |
| `wait_closed()` | Wait for the server to finish shutting down. |

## Properties

| Property | Type | Description |
|----------|------|-------------|
| `addresses` | `dict[str, str]` | Bound listener addresses (empty dict when stopped). Keys are listener names, values are `host:port` strings. |
| `config` | `EggressConfig` | The configuration used to start the server. |

## Exceptions

| Exception | When |
|-----------|------|
| `AlreadyStartedError` | Calling `start()` on an already-running server. |
| `UnsupportedFeatureError` | Constructor receives unsupported URIs (e.g., `ssh://`) and `allow_partial=False`. |
| `ConfigError` | Invalid TOML configuration. |
| `StartupError` | Runtime failure during server startup. |
| `ShutdownError` | Error during shutdown (rare). |

## Comparison with pproxy

| Feature | pproxy | eggress |
|---------|--------|---------|
| Constructor | `Server(rserver=[...], server=[...])` | `Server(listen=[...], remote=[...])` |
| Start | `server.start()` | `server.start()` |
| Close | `server.close()` | `server.close()` / `server.stop()` |
| Addresses | `server.server_name` | `server.addresses` |
| Auth | Per-listener in URI | Per-listener in URI |
| Upstream relay | Built-in | Via TOML config |

## Thread Model

- `start()` creates two OS threads: `"eggress-embed-rt"` (startup) and `"eggress-embed-run"` (supervisor)
- All blocking Rust calls release the GIL via `py.detach()`
- Handle drop performs best-effort cleanup with 5-second timeout
- Context managers (`with`/`async with`) provide deterministic cleanup

## Test Coverage

Lifecycle tests are in `python/tests/test_server_lifecycle.py` (20 tests):

- Construction: basic, config, no-args error, conflicting-args error
- Lifecycle: start, stop, close, idempotent close, double-start error
- Properties: addresses before/after start/stop
- Context managers: sync and async
- Integration: SOCKS5 relay, multiple listeners, repr states
- Error handling: unsupported URI, allow_partial

Oracle comparison tests are in `python/tests/test_pproxy_oracle.py`:

- `TestServerLifecycle`: eggress Server lifecycle patterns
- `TestServerLifecycleOracle`: pproxy vs eggress API comparison
