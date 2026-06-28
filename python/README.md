# eggress Python bindings

Python bindings for the [eggress](https://github.com/eggstack/eggress) proxy framework, powered by PyO3 and the Rust embed API.

## Installation (local development)

```bash
pip install maturin
cd crates/eggress-python
maturin build --target x86_64-apple-darwin   # adjust target for your platform
pip install --force-reinstall target/wheels/eggress-*.whl
```

> **Note:** `maturin develop` installs the native extension to the wrong module
> path (`_eggress/_eggress.so` instead of `eggress/_eggress.so`). Use
> `maturin build` + `pip install` instead.

## Quick start

```python
from eggress import EggressService

with EggressService.from_toml("""
    version = 1
    [[listeners]]
    name = "socks"
    bind = "127.0.0.1:0"
    protocols = ["socks5"]
""").start() as handle:
    addr = handle.bound_addresses["socks"]
    print(f"SOCKS5 listening on {addr}")
    print(handle.metrics_text())
```

### Async usage

```python
import asyncio
from eggress import EggressService

async def main():
    async with await EggressService.from_toml(TOML).astart() as handle:
        print("Listening on", await handle.bound_addresses)

asyncio.run(main())
```

## API

- `EggressConfig.from_toml(toml)` / `EggressConfig.from_file(path)` — parse config
- `EggressService(config)` / `EggressService.from_toml(toml)` — create service
- `service.start()` — start proxy, returns `EggressHandle`
- `handle.bound_addresses` — dict of listener name -> address
- `handle.status()` — generation, readiness, uptime, connections
- `handle.metrics_text()` — Prometheus metrics
- `handle.reload_toml(toml)` — hot-reload config
- `handle.shutdown()` — graceful shutdown (idempotent; safe to call twice)
- Context manager support: `with service.start() as handle: ...`

**Always use explicit lifecycle management.** Prefer context managers or
explicit `handle.shutdown()` in a `finally` block. Do not rely on Python
garbage collection to shut down the service — object destruction is a
best-effort fallback, not the lifecycle API.

## Migrating from pproxy

```python
from eggress import start_pproxy

# Same arguments you'd pass to pproxy
with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)
```

Or inspect the translation first:

```python
from eggress import translate_pproxy_args

result = translate_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
print(result.toml)         # generated eggress TOML
print(result.warnings)     # partial-behavior notes
print(result.unsupported)  # unsupported features
```

## Error model

| Exception | Meaning |
|-----------|---------|
| `EggressError` | Base exception |
| `ConfigError` | TOML parsing or validation error |
| `StartupError` | Listener bind or readiness timeout |
| `ReloadError` | Config reload failure |
| `ShutdownError` | Runtime shutdown error |
| `UnsupportedFeatureError` | Feature not supported |
| `InternalError` | Unexpected internal error |

## Limitations

- GIL is released for blocking Rust calls
- Requires Python >= 3.9
- Listener bind changes require restart (not reloadable)
- No logging initialization unless configured in TOML

## Non-parity with pproxy

- Shadowsocks TCP is experimental (non-standard AEAD framing)
- No inbound Shadowsocks or Trojan listeners (upstream-only)
- No legacy stream ciphers (aes-ctr, aes-cfb, rc4-md5, etc.)
- No SSH, Unix socket, or transparent proxy (redir) transport
- No pproxy daemon mode (`--daemon`)
- No `-ul`/`-ur` standalone UDP relay (uses SOCKS5 UDP ASSOCIATE)
- Multiple remotes default to round-robin (matches pproxy behavior)
- Direct fallback requires explicit config
