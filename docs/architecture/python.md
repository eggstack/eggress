# eggress-python

`crates/eggress-python/` and `python/eggress/`

Python bindings via PyO3 wrapping `eggress-embed`. Published as `eggress` on PyPI.

## Key Types

| Python Class | Rust Origin | Description |
|---|---|---|
| `Config` | `EggressConfig` | Configuration parsing |
| `Service` | `EggressService` | Pre-start builder |
| `Handle` | `EggressHandle` | Running proxy handle |
| `Connection` | PyO3 custom | pproxy-compatible connection lifecycle |
| `OutboundConnector` | `OutboundConnector` | Native Rust outbound connections |
| `OutboundStream` | `PyOutboundStream` | Read/write/half-close on outbound streams |

## Convenience Functions

| Function | Description |
|---|---|
| `start_pproxy()` | Multi-mode start (args, local/remote, config, config_path) |
| `translate_pproxy_args()` | Translate pproxy CLI args to TOML |
| `translate_pproxy_uri()` | Translate pproxy URI to eggress TOML |
| `check_pproxy_uri()` | Validate pproxy URI |
| `redact_pproxy_uri()` | Redact credentials from URI |
| `diagnostics_for_uri()` | Structured diagnostics for URI |
| `explain_config_toml()` | Explain TOML configuration |
| `explain_pproxy_args()` | Explain pproxy CLI arguments |
| `route_explain()` | Explain routing decision |
| `test_upstream_connect()` | Test upstream reachability |

## pproxy-Compatible Server

`PPProxyService` provides pproxy-compatible service builder:
- `from_args()` — from pproxy CLI arguments
- `from_uri()` — from pproxy URI
- `from_toml()` — from TOML string
- `start()` — start the proxy
- Context manager support (`__enter__`/`__exit__`)

`Server` wraps this with observability (`status()`, `sessions`, `last_error`), hot-reload, and resource management.

## Protocol Client Classes

| Class | Method | Description |
|---|---|---|
| `HTTP` | `connect(target)` | HTTP CONNECT client handshake |
| `Socks4` | `connect(target)` | SOCKS4 CONNECT handshake |
| `Socks5` | `connect(target)` | SOCKS5 CONNECT handshake |

## GIL Release

All blocking Rust calls release the GIL via `py.detach()`.

## Package

- Package name: `eggress`
- Wheels for Linux/macOS/Windows
- `py.typed` PEP 561 marker
- Type stubs (`.pyi`) for all public modules

## Dependencies

- `eggress-embed` — Rust embed API
- `eggress-pproxy-compat` — pproxy translation
- `eggress-config` — configuration
- `eggress-core` — types
- `eggress-routing` — route explanation

See [overview.md](overview.md) for context.
