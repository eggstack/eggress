# eggress Python bindings

Python bindings for the [eggress](https://github.com/eggstack/eggress) proxy framework, powered by PyO3 and the Rust embed API.

## Installation (local development)

```bash
pip install maturin
cd crates/eggress-python
rm -f ../../target/wheels/eggress-*.whl   # stale wheels break the install glob below
maturin build --target x86_64-apple-darwin   # adjust target for your platform
pip install --force-reinstall target/wheels/eggress-*.whl
```

## Prebuilt wheels

Prebuilt wheels are published to PyPI for:
- Linux x86_64 and aarch64 (manylinux)
- macOS x86_64 and arm64
- Windows x86_64

The wheel uses the Python stable ABI (`abi3-py39`), so one wheel per platform supports all declared Python versions (3.9–3.13). Other platforms can install from the source distribution (requires Rust toolchain).

Supported Python versions: 3.9, 3.10, 3.11, 3.12, 3.13.

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

## pproxy Compatibility

Eggress provides a pproxy compatibility surface validated against pproxy==2.7.9
and bounded by the canonical capability manifest. Legacy cipher implementations
and unsupported native transports fail explicitly rather than being silently
substituted:

- **URI Translation**: `translate_pproxy_args()` converts pproxy CLI arguments to eggress TOML
- **Same Protocols**: HTTP, SOCKS4/4a, SOCKS5, Shadowsocks (AEAD), Trojan
- **Same Schedulers**: Round-robin, least-connections, first-available
- **Enhanced Features**: Hot-reload, structured errors, context managers

See `docs/python/PPROXY_EMBEDDED_USAGE_PATTERNS.md` for migration guidance.

### pproxy drop-in API

```python
from eggress import PPProxyService, start_pproxy, check_pproxy_args

# Check compatibility before starting
report = check_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
print(f"Tier: {report.tier}, OK: {report.ok}")

# Start from pproxy args
with start_pproxy(["-l", "socks5://127.0.0.1:0"]) as handle:
    print(handle.bound_addresses)

# Start from local URI
with PPProxyService.from_uri("socks5://127.0.0.1:0") as handle:
    print(handle.bound_addresses)

# Start from TOML
with PPProxyService.from_toml(toml_str) as handle:
    print(handle.bound_addresses)
```

The optional `eggress-pproxy-compat` distribution additionally provides
`python -m pproxy`, a `pproxy` console script, and the top-level `pproxy`
package backed by the same compatibility entry point. `pproxy.server` helper
calls reuse the existing protocol/proxy adapters, while `pproxy.sysproxy`
delegates system-proxy apply/rollback to the native backend where the platform
supports it. See the namespace strategy below before installing it.

## API Contract (Phase C1)

A machine-readable contract of the bounded pproxy adapter is maintained at
`python/compat/pproxy_api_contract.json`. The exact upstream installed-package
inventory is maintained separately in
`compat/pproxy-2.7.9/namespace-baseline.json` and the active parity manifest.
Use `scripts/pproxy_surface_probe.py` with isolated oracle and wheel
interpreters to compare module names, top-level exports, tracked signatures,
async classification, and class bases. Importability is not a claim that
private pproxy internals are functionally reproduced.

### Classification summary

| Tier | Count | Examples |
|------|-------|---------|
| adapted_target | 3 | Connection, Server, Rule → PPProxyService |
| intentional_non_parity | 1 | DIRECT sentinel |
| internal_observed | 87 | Protocol classes, cipher classes, server internals |

### Validation

```bash
# Regenerate the contract
python3.11 python/compat/extract_api.py

# Run 56 contract validation tests
python3.11 -m pytest tests/compat/test_pproxy_api_contract.py -v

# Run 46 behavioral probes
python3.11 python/compat/behavioral_probes.py
```

### Namespace strategy

The optional `eggress-pproxy-compat` distribution installs a bounded top-level
`pproxy` package containing the documented connection/server factories and
protocol/cipher submodules. The canonical `eggress` distribution retains
`eggress.pproxy` and `eggress.start_pproxy()` for translation and managed-service
workflows. Do not install the compatibility distribution alongside upstream
`pproxy` in the same environment.
See `docs/python/PPROXY_NAMESPACE_STRATEGY.md` for the compatibility boundary.

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

- Shadowsocks TCP uses standard SIP003 AEAD framing (wire-compatible with `shadowsocks-rust`/`ssserver`/`sslocal`); single-hop upstream only
- No inbound Shadowsocks or Trojan listeners (upstream-only) — inbound Shadowsocks listener is available in the Rust binary; Python bindings expose the embed API which omits this for now
- No legacy stream ciphers (aes-ctr, aes-cfb, rc4-md5, etc.)
- No SSH, Unix socket, or transparent proxy (redir) transport
- No pproxy daemon mode (`--daemon`)
- No `-ul`/`-ur` standalone UDP relay (uses SOCKS5 UDP ASSOCIATE)
- Multiple remotes default to round-robin (matches pproxy behavior)
- Direct fallback requires explicit config
