# eggress-python

`crates/eggress-python/` and `python/eggress/`

Python bindings via PyO3 wrapping `eggress-embed`. Published as one `eggress`
distribution on PyPI; the wheel also installs a bounded `pproxy` compatibility
namespace (there is no separate compatibility distribution).

## Key Types

| Python Class | Rust Origin | Description |
|---|---|---|
| `Config` | `EggressConfig` | Configuration parsing |
| `Service` | `EggressService` | Pre-start builder |
| `Handle` | `EggressHandle` | Running proxy handle |
| `Connection` | PyO3 custom | pproxy-compatible connection lifecycle |
| `OutboundConnector` | `OutboundConnector` | Native Rust outbound connections |
| `OutboundStream` | `PyOutboundStream` | Read/write/half-close on outbound streams |

The top-level `pproxy` package re-exports the bounded adapters in
`python/pproxy/`. `Connection` and `Server` are aliases for the URI proxy
factory (not the native `eggress.pproxy.Server` lifecycle class), `Rule`
compiles public regex rule inputs, and `DIRECT` is the direct proxy sentinel.
TCP connection methods return asyncio reader/writer-compatible objects.
Unsupported listener roles, multi-hop UDP, and excluded protocol
families fail with explicit ``UnsupportedPProxyFeature`` exceptions
(a subclass of ``PProxyCompatibilityError(RuntimeError)``).

The compatibility server path follows pproxy's connection lifecycle by opening
the raw direct or upstream transport first and invoking `prepare_connection()`
exactly once. Public `Connection.tcp_connect()` retains its destination-ready
reader/writer contract and performs that same preparation once; nested supported
chain hops are prepared in declaration order.

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

`PPProxyService` provides a pproxy-shaped Eggress service builder, not a strict
drop-in implementation of pproxy's `Server` contract:
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

Release artifacts are validated against an exact five-wheel/one-sdist
contract: Linux x86_64/aarch64, macOS x86_64/arm64, and Windows x86_64. Every
wheel must use `cp39-abi3`, and the release-only workflow uses
`scripts/release_artifact_smoke.py` to import both `eggress` and top-level
`pproxy`, start a `127.0.0.1:0` listener, verify readiness and its bound
address, then shut it down and verify readiness is false.

## Dependencies

- `eggress-embed` — Rust embed API
- `eggress-pproxy-compat` — pproxy translation
- `eggress-config` — configuration
- `eggress-core` — types
- `eggress-routing` — route explanation

See [overview.md](overview.md) for context.
