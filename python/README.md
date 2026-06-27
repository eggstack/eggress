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

## API

- `EggressConfig.from_toml(toml)` / `EggressConfig.from_file(path)` — parse config
- `EggressService(config)` / `EggressService.from_toml(toml)` — create service
- `service.start()` — start proxy, returns `EggressHandle`
- `handle.bound_addresses` — dict of listener name -> address
- `handle.status()` — generation, readiness, uptime, connections
- `handle.metrics_text()` — Prometheus metrics
- `handle.reload_toml(toml)` — hot-reload config
- `handle.shutdown()` — graceful shutdown
- Context manager support: `with service.start() as handle: ...`

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
