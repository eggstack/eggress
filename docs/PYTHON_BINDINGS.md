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
        print("Listening on", await handle.bound_addresses())

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

## Connection object

`eggress.Connection` provides a pproxy-compatible low-level connection object backed by Rust-owned networking.

### Constructor

```python
conn = Connection('socks5://:1080', 'http://proxy:8080')
```

Accepts pproxy-style URI arguments (variadic `*uris`). Translates them to eggress TOML configuration, creates an embedded service, and returns a managed connection object.

### Properties

- `state` — current lifecycle state string: `"created"`, `"connecting"`, `"connected"`, `"closing"`, `"closed"`, `"failed"`
- `closed` — `True` when the connection is closed or failed
- `config` — the TOML configuration used by this connection
- `peername` — remote address as `(host, port)` tuple, or `None`
- `sockname` — local bound address as `(host, port)` tuple, or `None`

### Methods

- `extra_info()` — returns a dict with state, bound address, remote address, and error info
- `close()` — close the connection and shut down the underlying service (idempotent)
- `wait_closed()` — wait for the connection to close (initiates close if needed)

### Async methods

- `aclose()` — async version of `close()`
- `await_closed()` — async version of `wait_closed()`

### Context manager

Supports both sync and async context managers:

```python
with Connection('socks5://:1080') as conn:
    print(conn.sockname)

async with Connection('socks5://:1080') as conn:
    print(conn.state)
```

### Resource management

If a `Connection` object is garbage collected without being closed, a `ResourceWarning` is issued and best-effort cleanup is performed. Always prefer explicit `close()` or context manager usage.

### Exception hierarchy

All connection-specific exceptions inherit from `EggressError`:

- `ConnectionError` — base for all connection errors
- `ConnectionClosedError` — operation on a closed connection
- `TimeoutError` — connection or operation timed out
- `DnsError` — DNS resolution failure
- `AuthError` — authentication failure
- `TlsError` — TLS handshake failure
- `LoopMismatchError` — used from wrong event loop
- `ConnectionCancelledError` — operation was cancelled
- `UseAfterCloseError` — operation attempted on a closed connection
- `UdpAssociationError` — UDP association failure
- `UnsupportedCompositionError` — unsupported protocol/transport composition

### Connection statistics

```python
from eggress.connection import Connection

# Get live/total connection counts (useful for leak detection)
stats = Connection.connection_stats()
print(stats)  # {"live": 2, "total_created": 15}

# Reset counters (useful in tests)
Connection.reset_connection_stats()
```

### Async wrapper

For asyncio-native usage with loop affinity checking:

```python
import asyncio
from eggress.async_connection import AsyncConnection

async def main():
    async with AsyncConnection("socks5://:1080") as conn:
        print(conn.state)
        print(conn.sockname)

asyncio.run(main())
```

`AsyncConnection` wraps `Connection` and adds:
- **Loop affinity**: created on a specific event loop; operations from a different loop raise `LoopMismatchError`
- **`AsyncConnection.open(*uris)`**: async class method for ergonomic creation
- **`aclose()` / `await_closed()`**: native async close/wait
- **`async with` context manager**: automatic cleanup

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
    ├── InternalError      — unexpected internal failure
    ├── ConnectionError    — base for all connection errors
    │   ├── ConnectionClosedError — operation on closed connection
    │   ├── TimeoutError         — connection or operation timed out
    │   ├── DnsError             — DNS resolution failure
    │   ├── AuthError            — authentication failure
    │   ├── TlsError             — TLS handshake failure
    │   ├── ConnectionCancelledError — operation was cancelled
    │   ├── UseAfterCloseError   — operation on closed connection
    │   └── UdpAssociationError  — UDP association failure
    ├── LoopMismatchError  — used from wrong event loop
    └── UnsupportedCompositionError — unsupported protocol/transport composition
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

## Lifecycle management

**Always use explicit lifecycle management.** Do not rely on Python garbage
collection or `__del__` to shut down the service. Object destruction is a
best-effort fallback, not the lifecycle API.

### Context manager (recommended)

```python
with EggressService.from_toml(toml).start() as handle:
    print("Listening on", handle.bound_addresses)
    # ... use the proxy ...
# service is shut down automatically on exit
```

### Explicit start/stop

```python
handle = EggressService.from_toml(toml).start()
try:
    print(handle.status())
finally:
    handle.shutdown()
```

### Async context manager

```python
async with await EggressService.from_toml(TOML).astart() as handle:
    print("Listening on", await handle.bound_addresses())
# service is shut down automatically on exit
```

### Why explicit lifecycle?

- `shutdown()` is idempotent: calling it twice is safe.
- `__exit__` calls `shutdown()` and returns `False` (exceptions propagate).
- `__aexit__` awaits `shutdown()` and returns `False`.
- Rust `Drop` cancels the shutdown token and attempts a best-effort join with
  a 5-second timeout. If the process exits before the timeout, cleanup may be
  incomplete. Relying on `Drop` for lifecycle management is unreliable.
- After `shutdown()` or `__exit__`, subsequent calls to `status()`,
  `bound_addresses()`, etc. raise a clear error ("handle consumed").

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

## URI inspection and diagnostics (Phase 31)

### `check_pproxy_uri(uri)`

Parse a pproxy URI and return structured information. Never raises — errors
are captured in the `error` field of the returned `UriInfo`.

```python
from eggress import check_pproxy_uri

info = check_pproxy_uri("socks5://user:pass@example.com:1080+tls")
print(info.scheme)       # "socks5"
print(info.host)         # "example.com"
print(info.port)         # 1080
print(info.tls)          # True
print(info.has_auth)     # True
print(info.ok)           # True (no error)

# Error handling
info = check_pproxy_uri("invalid://")
print(info.ok)           # False
print(info.error)        # error message
```

| Field | Type | Description |
|---|---|---|
| `scheme` | `str` | Protocol scheme (socks5, http, ss, etc.) |
| `host` | `str` | Target host |
| `port` | `int` | Target port |
| `tls` | `bool` | TLS enabled |
| `ssl` | `bool` | SSL suffix present (normalized to +tls in display) |
| `inbound` | `bool` | Inbound listener mode (+in modifier) |
| `backward_num` | `int` | Backward chain depth |
| `has_auth` | `bool` | Credentials present |
| `has_rule` | `bool` | Rule query parameter present |
| `is_reverse_listener` | `bool` | Reverse listener scheme (bind/listen/backward/rebind) |
| `redacted_display` | `str` | URI display with credentials redacted |
| `error` | `str \| None` | Parse error message (None if successful) |
| `ok` | `bool` | True if no parse error |

### `redact_pproxy_uri(uri)`

Return the URI with credentials redacted. Raises `EggressError` on invalid URI.

```python
from eggress import redact_pproxy_uri

print(redact_pproxy_uri("socks5://secret:word@proxy:1080"))
# "socks5://****:****@proxy:1080"
```

### `diagnostics_for_uri(uri)`

Translate a URI and return structured diagnostics for all warnings and
unsupported features. Raises `EggressError` on invalid URI.

```python
from eggress import diagnostics_for_uri

diags = diagnostics_for_uri("ssh://proxy:22")
for d in diags:
    print(f"[{d.code}] {d.message}")
    print(f"  Suggestion: {d.suggestion}")
```

| Field | Type | Description |
|---|---|---|
| `code` | `str` | Stable diagnostic code (e.g., "unsupported_protocol") |
| `feature_id` | `str` | Feature identifier |
| `tier` | `str` | Compatibility tier |
| `message` | `str` | Human-readable message |
| `suggestion` | `str` | Suggested action |

### `supported_features()`

Return a list of all supported pproxy protocol features as strings.

```python
from eggress import supported_features

features = supported_features()
print("socks5" in features)  # True
print("http" in features)    # True
print("ssh" in features)     # False
```

## Reverse URI inspection (Phase 31)

### `describe_reverse_pproxy_uri(uri)`

Parse a pproxy reverse URI and return a structured :class:`ReverseUriSummary`.
Never raises; returns a summary with safe defaults for invalid input.

```python
from eggress import describe_reverse_pproxy_uri

summary = describe_reverse_pproxy_uri("socks5+in://user:pass@host:1080")
print(summary.role)          # "client"
print(summary.toml_section)  # "reverse_clients"
print(summary.has_auth)      # True
print(summary.tls)           # False
print("+in" in summary.modifiers)  # True
```

| Field | Type | Description |
|---|---|---|
| `role` | `str` | `"server"`, `"client"`, or `"unknown"` |
| `scheme` | `str` | URI scheme |
| `target` | `str` | Redacted `host:port` (never includes credentials) |
| `has_auth` | `bool` | Credentials present in URI |
| `toml_section` | `str` | `"reverse_servers"`, `"reverse_clients"`, or `"unknown"` |
| `tls` | `bool` | TLS modifier present |
| `modifiers` | `tuple[str, ...]` | URI modifiers (e.g., `"+in"`, `"+tls"`) |

## Config explanation (Phase 31)

The `explain_*` functions parse a configuration source and return a structured
dict describing listeners, upstreams, upstream groups, rules, reverse servers,
and reverse clients. They never raise on invalid input.

### `explain_config_toml(toml_str)`

```python
from eggress import explain_config_toml

info = explain_config_toml("""
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
""")
print(info["listeners"])   # [{"name": "socks", "bind": "127.0.0.1:1080", ...}]
print(info["upstreams"])   # []
print(info["rules"])       # []
```

### `explain_pproxy_args(args)`

Translate pproxy CLI arguments and return a dict with the same shape as
`explain_config_toml`, plus `warnings`, `unsupported`, `toml`, and `ok` keys.

```python
from eggress import explain_pproxy_args

info = explain_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
print(info["ok"])        # True
print(info["warnings"])  # []
print(info["toml"])      # full TOML config
```

### `explain_pproxy_uri(uri)`

Translate a single pproxy URI and return a dict with the same shape as
`explain_pproxy_args`.

## Routing and upstream helpers (Phase 31)

### `route_explain(config_toml, target)`

Compile a TOML config and run the routing engine against the target address.
Returns a dict describing the matched rule, action, upstream group, scheduler,
eligible upstreams, and selected upstream.

```python
from eggress import route_explain

info = route_explain(toml_str, "example.com:443")
print(info["matched_rule"])
print(info["action"])            # "direct" | "deny" | "use_upstream"
print(info["upstream_group"])
print(info["scheduler"])
print(info["selected_upstream"])
```

### `check_upstream(uri, timeout=5.0)`

Attempt a TCP connection to the upstream URI. Returns a dict with `host`,
`port`, `scheme`, `has_auth`, `redacted_uri`, `connected`, `latency_us`,
and `error` keys.

```python
from eggress import check_upstream

result = check_upstream("socks5://proxy.example.com:1080", timeout=2.0)
if result["connected"]:
    print(f"latency: {result['latency_us']} us")
else:
    print(f"error: {result['error']}")
```

## Package metadata (Phase 32)

### `version()`

```python
import eggress
print(eggress.version())  # "0.1.0"
```

### `capabilities()`

Return a dict describing eggress capabilities and runtime metadata:

```python
import eggress
caps = eggress.capabilities()
print(caps["version"])                      # "0.1.0"
print(caps["python_version"])               # "3.12.4"
print(caps["pproxy_compatibility_version"]) # "2.7.9"
print(caps["supported_protocols"])          # ["http", "socks4", "socks4a", "socks5", ...]
print(caps["supported_schedulers"])         # ["round_robin", "least_connections", "first_available"]
```

### `compatibility_version()`

Return the pproxy version that eggress targets for compatibility.

```python
from eggress.pproxy import compatibility_version

print(compatibility_version())  # "2.7.9"
```

## Server (Phase C3)

`Server` is a pproxy-compatible server wrapper that manages the full
lifecycle: construction, start, observe, reload, and close. It translates
pproxy-style listen/remote URIs to eggress configuration and delegates to
the underlying Rust supervisor.

```python
from eggress import Server

# Sync usage
with Server(listen=["socks5://127.0.0.1:1080"], remote=["http://proxy:8080"]) as srv:
    print(srv.addresses)     # {"socks5": "127.0.0.1:1080"}
    print(srv.is_ready)      # True
    print(srv.sessions)      # 0 (active connections)
    print(srv.status())      # {"readiness": True, "active_connections": 0, ...}

# Async usage
async with Server(config=my_config) as srv:
    print(srv.addresses)

# Blocking (main thread only)
server = Server(listen=["socks5://:1080"], remote=["http://proxy:8080"])
server.run()  # blocks until SIGINT/SIGTERM
```

### Constructor

```python
Server(
    listen=None,       # list[str] — pproxy listener URIs
    remote=None,       # list[str] — pproxy upstream URIs
    *,                 # keyword-only
    config=None,       # EggressConfig — pre-built config (mutually exclusive with listen/remote)
    allow_partial=False,  # bool — start even with unsupported features
)
```

### Lifecycle methods

| Method | Returns | Description |
|---|---|---|
| `start()` | `self` | Start the server; returns self for chaining |
| `stop()` | `None` | Stop the server (alias for `close()`) |
| `close()` | `None` | Stop the server; idempotent |
| `run()` | `None` | Start and block until SIGINT/SIGTERM (main thread only) |
| `astart()` | `self` | Async start via `asyncio.to_thread` |
| `aclose()` | `None` | Async stop via `asyncio.to_thread` |
| `wait_closed()` | `None` | Async wait until server is closed |
| `reload(toml)` | `dict` | Hot-reload routing/upstreams/health from TOML |

### Properties

| Property | Type | Description |
|---|---|---|
| `addresses` | `dict[str, str]` | Bound listener addresses; empty when stopped |
| `is_ready` | `bool` | True when service is started and ready |
| `listener_info` | `list[dict]` | Listener details from the running service; empty when stopped |
| `metrics_text` | `str` | Prometheus metrics text; empty when stopped |
| `config` | `EggressConfig` | The configuration used to construct/start the server |
| `sessions` | `int` | Number of active connections; 0 when stopped |
| `last_error` | `Exception \| None` | Most recent error from start/reload/shutdown |

### Observability

```python
srv = Server(listen=["socks5://127.0.0.1:0"])
srv.start()

# Structured status
status = srv.status()
print(status["readiness"])           # True
print(status["active_connections"])  # 0
print(status["listeners"])           # [{name, bind, protocols, ...}, ...]

# Active session count
print(srv.sessions)  # 0

# Error tracking
print(srv.last_error)  # None (no errors yet)
```

### Hot reload

Only routing, upstreams, groups, and health settings are hot-reloadable.
Listener topology changes require a restart.

```python
srv.reload(new_toml_config)
```

### Resource management

`Server` supports sync and async context managers, and issues a
`ResourceWarning` if garbage-collected without being properly closed.

```python
# Recommended: use context manager
with Server(listen=["socks5://127.0.0.1:0"]) as srv:
    ...

# Or explicit close
srv = Server(listen=["socks5://127.0.0.1:0"])
srv.start()
try:
    ...
finally:
    srv.close()
```

### Thread safety

- Construction: safe from any thread
- `start()`/`close()`: safe from any thread
- `run()`: must be called from the main thread
- `astart()`/`aclose()`: must be called from an asyncio event loop thread

### Server test coverage

The `Server` class is tested by 84 tests in `python/tests/test_server_lifecycle.py`:

- **Import & construction** (11 tests): argument validation, config property, `allow_partial`
- **Start/stop lifecycle** (13 tests): start, close, idempotent close, context managers, `wait_closed`, `run()`
- **Observability** (15 tests): `status()`, `sessions`, `last_error`, `is_ready`, `listener_info`, `metrics_text`
- **Reload & error paths** (2 tests): reload before start, reload with valid TOML
- **Multi-instance & protocol relay** (8 tests): SOCKS5 relay, multiple listeners, coexistence with `PPProxyService`
- **TLS** (2 tests): TLS listener with self-signed cert, TLS config presence
- **Auth** (3 tests): auth listener start, wrong-password rejection, auth config presence
- **Chains & routing** (2 tests): upstream chain config, chain URI translation
- **UDP** (2 tests): UDP-enabled listener, standalone UDP mode
- **IPv6** (1 test): IPv6 loopback listener (platform-gated)
- **Loop & thread** (2 tests): loop affinity, interpreter shutdown
- **Exception mapping** (4 tests): bind conflict, TLS missing cert, invalid TOML, invalid reload TOML
- **Advanced lifecycle** (8 tests): partial bind rollback, GIL release, FD leak detection, pproxy examples (socks, multi-listener, auth, chain), close with active session, reload with upstream change, sessions with active connection, status listeners, metrics content

## pproxy oracle testing (Phase 29)

The Python bindings include an oracle test harness that verifies eggress
Python API behavior against a frozen pproxy 2.7.9 API snapshot. Tests are
**auto-gated** — they require pproxy to be installed and skip otherwise.
The legacy env var `EGRESS_REQUIRE_PPROXY_ORACLE=1` is accepted for backward
compatibility but is no longer required.

```bash
# Run oracle tests (auto-skips if pproxy is not installed)
python -m pytest python/tests/test_pproxy_oracle.py -v
```

The oracle fixture lives at `tests/compat/fixtures/pproxy_api_snapshot.json`
and is generated by `scripts/snapshot_pproxy_api.py`. The test suite covers:

- Module exports parity
- Protocol class availability
- Translation function behavior
- Snapshot consistency

Phase 29 deliverables include 66 compatibility fixture test cases
(`tests/compat/fixtures/python_api_cases.toml`), 12 new manifest entries,
and comprehensive API inventory documents under `docs/python/`.

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
- **pproxy compat**: Shadowsocks TCP uses standard SIP003 AEAD framing
  (wire-compatible with `shadowsocks-rust`/`ssserver`/`sslocal`). Inbound
  Shadowsocks listeners are available via the Rust binary; the embed API
  (which the Python bindings wrap) exposes TCP/UDP upstream Shadowsocks and
  inbound listeners for SOCKS5/HTTP only in the current release. No Trojan
  inbound listeners. No legacy stream ciphers. No SSH/unix/redir transport.
  No pproxy daemon mode. Multiple remotes default to round-robin.
- **mypy**: PyO3 native types (`_inner` attribute) are invisible to mypy,
  producing ~20 expected false-positive errors. This is known pre-release
  typing debt. A future release will add `.pyi` stubs or type wrappers.

## Relationship to pproxy compatibility

The Python bindings now expose the `eggress-pproxy-compat` translation layer
directly via `translate_pproxy_args`, `translate_pproxy_uri`, and
`check_pproxy_args`. See the [pproxy compatibility](#pproxy-compatibility)
section above. For CLI-based translation, you can still use:

```bash
python -m eggress pproxy translate -- -l socks5://:1080 -r http://proxy:8080
```

## Phase 40: pproxy drop-in API

### PPProxyService

`PPProxyService` is a pproxy-compatible service builder that accepts
pproxy-style arguments and manages the full service lifecycle.

```python
from eggress import PPProxyService

# From pproxy CLI args
with PPProxyService.from_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)

# From local/remote URIs
with PPProxyService.from_uri("socks5://127.0.0.1:0") as handle:
    print(handle.bound_addresses)

# From TOML string
with PPProxyService.from_toml(toml_str) as handle:
    print(handle.bound_addresses)

# From TOML file
with PPProxyService.from_file("config.toml") as handle:
    print(handle.bound_addresses)
```

Factory methods:

- `from_args(args, allow_partial=False)` — pproxy CLI arguments
- `from_uri(local, remotes=(), allow_partial=False)` — local URI and optional remote URIs
- `from_toml(toml)` — TOML configuration string
- `from_file(path)` — path to TOML configuration file
- `start()` — start the service and return an `EggressHandle`

### PPProxyHandle

`PPProxyHandle` is a type alias for `EggressHandle`. All handle
operations (`bound_addresses`, `status`, `metrics_text`, `reload_toml`,
`shutdown`) are available.

### CompatibilityReport

`check_pproxy_args` returns a `CompatibilityReport` instead of a
`TranslationResult`. The report includes tier classification, diagnostics,
parsed URIs, and generated TOML.

```python
from eggress import check_pproxy_args

report = check_pproxy_args(["-l", "socks5://127.0.0.1:0"])
print(report.tier)       # "drop_in", "compatible_with_warning", "native_equivalent", "intentional_non_parity", or "unsupported"
print(report.ok)         # True if no unsupported features
print(report.toml)       # Generated TOML (credentials redacted)
print(report.features)   # List[FeatureInfo]
print(report.parsed_uris)  # Dict[str, UriInfo]
```

Fields:

- `tier: str` — "drop_in", "compatible_with_warning", "native_equivalent", "intentional_non_parity", or "unsupported"
- `ok: bool` — True if no unsupported features
- `warnings: list[Diagnostic]` — translation warnings
- `unsupported: list[Diagnostic]` — unsupported feature diagnostics
- `diagnostics: list[Diagnostic]` — all diagnostics combined
- `features: list[FeatureInfo]` — feature tier classifications
- `toml: str | None` — generated TOML with redacted credentials
- `parsed_uris: dict[str, UriInfo]` — parsed URI info from args
- `raw_args: list[str]` — original input arguments

### FeatureInfo

Each feature from the pproxy compatibility manifest:

- `feature_id: str` — feature identifier
- `tier: str` — "drop_in", "compatible_with_warning", "native_equivalent", "intentional_non_parity", or "unsupported"
- `supported: bool` — whether eggress supports this feature

### Updated start_pproxy

`start_pproxy` now supports multiple input modes (mutually exclusive):

```python
import eggress

# From pproxy CLI args
with eggress.start_pproxy(["-l", "socks5://:1080"]) as handle:
    print(handle.bound_addresses)

# From local URI
with eggress.start_pproxy(local="socks5://127.0.0.1:0") as handle:
    print(handle.bound_addresses)

# From TOML string
with eggress.start_pproxy(config=toml_str) as handle:
    print(handle.bound_addresses)

# From TOML file
with eggress.start_pproxy(config_path="config.toml") as handle:
    print(handle.bound_addresses)
```

Parameters:

- `args` — pproxy CLI-style arguments
- `local` — single local listener URI
- `remote` — remote upstream URI or list of URIs
- `config` — TOML configuration string
- `config_path` — path to TOML configuration file
- `allow_partial` — start even with unsupported features
- `background` — reserved for API compatibility
- `log_format` — reserved for future use

### Type stubs

`.pyi` stub files are provided for all public modules:

- `eggress/_eggress.pyi` — native extension module
- `eggress/__init__.pyi` — public API
- `eggress/pproxy.pyi` — pproxy compatibility layer
- `eggress/service.pyi` — service and handle classes
- `eggress/config.pyi` — configuration
- `eggress/exceptions.pyi` — exception types

### Credential redaction

The `CompatibilityReport.toml` output has credentials automatically
redacted. Password values are replaced with `"****"` and URI credentials
are replaced with `****@host:port`.
