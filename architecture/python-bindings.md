# Python Surface — PyO3 Bindings and the `python/` Tree

Two layers: the compiled `_eggress` extension (PyO3, crate
`crates/eggress-python`) and the canonical pure-Python package `python/eggress`
that wraps it. maturin builds the wheel with `python-source = "../../python"`
and module name `eggress._eggress` (abi3-py39).

## Compiled extension (`src/lib.rs`, single file)

- Classes: `PyEggressConfig/Service/Handle`, pproxy-style `Connection`,
  `OutboundConnector` → `PyOutboundStream` (read/write/sendall/recv/drain/
  write_eof/close — no temp local listener), `AppliedSystemProxy`, translation
  result/warning/unsupported types, URI info, diagnostics.
- Functions: translate/check/validate pproxy args & URIs, redaction,
  diagnostics, explain-config/route-explain, upstream test, system proxy.
- Exception hierarchy mirrors `EggressError` exactly (ConfigError,
  StartupError, ReloadError, TimeoutError, DnsError, AuthError, TlsError, ...).
- GIL discipline: every blocking Rust call runs under `py.detach(|| ...)`.
- Stubs: `python/eggress/_eggress.pyi` + `py.typed` (PEP 561).

## Pure-Python package (`python/eggress/`)

| Module | Role |
|---|---|
| `service.py`, `connection.py`, `async_connection.py` | Service/handle wrappers, sync + asyncio (loop-affine) variants |
| `outbound.py`, `pproxy_connection.py` | Native outbound streams; pproxy-flavored facade |
| `pproxy.py`, `_pproxy_proxy.py` | `Server`, `PPProxyService`, `ProxyDirect/Simple/Backward/H2/SSH/QUIC/H3`, `AuthTable` |
| `protocol.py`, `cipher.py`, `plugin.py`, `wrapper.py` | Structural API parity layer (protocol/cipher/plugin objects); ciphers use optional `cryptography` |
| `_asyncio.py`, `_asyncio_adapter.py`, `_compat.py` | Rust→asyncio bridge and version shims |

## Namespace rule (hard boundary)

The `eggress` wheel NEVER installs or aliases top-level `pproxy`. That
namespace belongs to the separate opt-in distribution — see
[pproxy-compat.md](pproxy-compat.md). No `sys.modules` aliasing anywhere.

## Review entry points

- Build + test per AGENTS.md: maturin develop into a venv, then
  `.venv/bin/python -m pytest python/tests tests/compat -q`.
- Test taxonomy in `python/tests/TEST_TAXONOMY.md`: unit → contract →
  differential (oracle-gated) → interop (env-gated) → platform → certification.
