# Migrating from pproxy

## Preserving the top-level import

Applications that intentionally use the certified subset of the pproxy module
can install the explicit compatibility distribution in a clean environment:

```bash
pip install eggress-pproxy-compat
```

This provides `import pproxy` without making the canonical `eggress` wheel
shadow or alias the upstream namespace. It is not strict full pproxy parity;
the canonical manifest documents warning and unsupported features.

## Quick migration with `start_pproxy()`

The fastest way to migrate is to replace `pproxy.main()` with
`start_pproxy()`:

```python
# Before (pproxy)
import pproxy
pproxy.main(['-l', 'socks5://:1080', '-r', 'http://proxy:8080'])

# After (eggress)
from eggress import start_pproxy
with start_pproxy(['-l', 'socks5://:1080', '-r', 'http://proxy:8080']) as handle:
    pass  # service shuts down on exit
```

## Step-by-step migration

### 1. Translate your arguments

Inspect the translation result before creating a service:

```python
from eggress import translate_pproxy_args

result = translate_pproxy_args([
    "-l", "socks5://:1080",
    "-r", "http://proxy:8080",
])
print(result.toml)          # generated eggress TOML
print(result.warnings)      # partial-behavior notes
print(result.unsupported)   # unsupported features
print(result.ok)            # True if no unsupported features
```

### 2. Inspect the generated TOML

```python
config = result.config()    # parse into EggressConfig
print(config.redacted_toml())  # TOML with credentials redacted
```

### 3. Create and start the service

```python
from eggress import EggressService

svc = EggressService.from_pproxy_args([
    "-l", "socks5://:1080",
    "-r", "http://proxy:8080",
])

with svc.start() as handle:
    print("Listening on", handle.bound_addresses)
    # ... use the proxy ...
```

Or use the convenience function (combines steps 1-3):

```python
from eggress import start_pproxy

with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)
```

## What works the same

| Feature | pproxy | eggress |
|---------|--------|---------|
| HTTP CONNECT | `http://` | Supported |
| SOCKS4 | `socks4://` | Supported |
| SOCKS4a | `socks4a://` | Supported |
| SOCKS5 | `socks5://` | Supported |
| Shadowsocks (AEAD) | `ss://` | Supported |
| Trojan (upstream) | `trojan://` | Supported |
| Round-robin scheduling | `rr` | Supported |
| Least-connections scheduling | `lc` | Supported |
| First-available scheduling | `fa` | Supported |
| `-l` / `-r` CLI flags | Supported | Supported |
| `-ul` / `-ur` (standalone UDP) | Supported | Supported |
| Reverse proxy | Supported | Supported |

## Native outbound client connections

For code that previously used `Connection` as a client-side socket, eggress
returns a native stream instead of exposing a temporary localhost listener:

```python
from eggress import ProxyConnection

with ProxyConnection("direct://127.0.0.1:0") as connection:
    stream = connection.tcp_connect("example.com", 443, timeout=10)
    stream.sendall(b"GET / HTTP/1.0\\r\\nHost: example.com\\r\\n\\r\\n")
    data = stream.recv(4096)
    stream.close()
```

Use `await connection.atcp_connect(...)` for the asyncio wrapper. The native
path has no listener bind to discover or clean up; UDP remains listener-based.

## What differs

| Aspect | pproxy | eggress |
|--------|--------|---------|
| Configuration format | URI flags and inline rules | TOML config files (or translated from URI flags) |
| Lifecycle management | `loop.run_forever()` | Context manager or explicit `start()`/`shutdown()` |
| Error model | Exceptions and return codes | Typed exception hierarchy (`EggressError` subclasses) |
| Hot-reload | Not supported | `handle.reload_toml(toml)` |
| Metrics | Not built-in | `handle.metrics_text()` (Prometheus format) |
| Status introspection | Not built-in | `handle.status()` |
| Protocol detection | Mixed-listener auto-detection | Explicit protocol configuration per listener |
| Shadowsocks TCP framing | Stream or AEAD | SIP003 AEAD only (wire-compatible with standard implementations) |
| Scheduling selection | Runtime `salgorithm` arg | Config-driven per upstream group |
| Rule matching | Freeform regex patterns | Structured matchers (`domain_suffix`, `ip_cidr`, `port`, `all`, `any_of`, `not`) |

## What is not yet supported

| Feature | Status |
|---------|--------|
| SSH transport | Not supported — rejected with diagnostic |
| Unix socket listeners (`unix://`) | Supported in Rust binary; embed API (Python) deferred |
| Transparent proxy (`redir://`) | Supported in Rust binary; Linux only; embed API deferred |
| Daemon mode (`--daemon`) | Not supported |
| `pproxy.Rule()` regex rules | Structured TOML matchers instead |
| `pproxy.DIRECT` singleton | Direct connection via absence of upstream config |
| `proxy.open_connection()` (client API) | Not supported — server-side only |
| `proto.*` protocol class access | Not supported — config-driven only |
| Custom cipher configuration | Not supported — AEAD methods only |
| Legacy stream ciphers | Not supported — rejected with diagnostic (see ADR) |
| SSR (`ssr://`) | Not supported — rejected with diagnostic (see ADR) |
| `--sys` (system proxy) | Not supported |
| `--get` (PAC file serving) | Not supported |
| `--pac` (PAC file path) | Not supported |
| `--reuse` (SO_REUSEADDR) | Not supported |

## See also

- [INSTALLATION.md](INSTALLATION.md) — installation methods
- [IMPORT_STRATEGY.md](IMPORT_STRATEGY.md) — canonical import paths
- [PPROXY_EMBEDDED_USAGE_PATTERNS.md](PPROXY_EMBEDDED_USAGE_PATTERNS.md) — detailed pattern-by-pattern comparison
- [PYTHON_BINDINGS.md](../PYTHON_BINDINGS.md) — full Python API reference
