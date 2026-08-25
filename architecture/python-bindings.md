# Python Surface — PyO3 Bindings and the `python/` Tree

Two layers: the compiled `_eggress` extension (PyO3, crate
`crates/eggress-python`) and the canonical pure-Python package `python/eggress`
that wraps it. maturin builds the wheel with `python-source = "../../python"`
and module name `eggress._eggress` (abi3-py39).

## Layout / module map

### Compiled extension (`crates/eggress-python/src/lib.rs`)

Single-file PyO3 module. Registered in `#[pymodule] fn _eggress()` at
`src/lib.rs:1924`.

| Category | Symbols | Notes |
|---|---|---|
| Classes | `PyEggressConfig`, `PyEggressService`, `PyEggressHandle`, `PyConnection`, `PyOutboundConnector`, `PyOutboundStream`, `PyAppliedSystemProxy`, `PyTranslationResult`, `PyTranslationWarning`, `PyUnsupportedFeature`, `PyReverseUriSummary`, `PyUriInfo`, `PyDiagnostic` | 13 classes total |
| Functions | `translate_pproxy_args`, `translate_pproxy_uri`, `check_pproxy_args`, `validate_pproxy_args`, `pproxy_runtime_options`, `init_pproxy_logging`, `run_pproxy_test`, `describe_reverse_pproxy_uri`, `check_pproxy_uri`, `redact_pproxy_uri`, `diagnostics_for_uri`, `supported_features`, `explain_config_toml`, `explain_pproxy_args`, `explain_pproxy_uri`, `route_explain`, `test_upstream_connect`, `apply_system_proxy` | 18 functions |
| Exceptions | `EggressError` (base), `ConfigError`, `StartupError`, `ReloadError`, `ShutdownError`, `UnsupportedFeatureError`, `InternalError`, `ConnectionError`, `ConnectionClosedError`, `TimeoutError`, `DnsError`, `AuthError`, `TlsError`, `LoopMismatchError`, `ConnectionCancelledError`, `UseAfterCloseError`, `UdpAssociationError`, `UnsupportedCompositionError` | 18 exception types |
| Metadata | `__version__` | `env!("CARGO_PKG_VERSION")` |

### Pure-Python package (`python/eggress/`)

| Module | Role |
|---|---|
| `service.py` | `EggressService` (pre-start builder), `EggressHandle` (sync), `AsyncEggressHandle` (async via `AsyncBridge`); `PPProxyHandle` type alias |
| `connection.py` | `Connection` — managed proxy service (listener + relay); wraps `PyConnection` with state machine and `ConnectionState` enum |
| `async_connection.py` | `AsyncConnection` — async wrapper with loop-affinity enforcement via `AsyncBridge`/`CloseWaiter` |
| `outbound.py` | `OutboundConnector`, `OutboundStream`, `AsyncOutboundStream` — native outbound TCP without listener; `from_pproxy_uri`/`from_toml` factories |
| `pproxy.py` | `Server`, `PPProxyService`, `TranslationResult`, `CompatibilityReport`, `Diagnostic`, `UriInfo`, `check_pproxy_uri`, `translate_pproxy_args`, route/test helpers; pproxy-flavored facade |
| `_pproxy_proxy.py` | `ProxyDirect`, `ProxySimple`, `ProxyBackward`, `ProxyH2`, `ProxySSH`, `ProxyQUIC`, `ProxyH3`, `AuthTable` — pproxy 2.7.9 server object model (structural) |
| `protocol.py` | Protocol object model: `BaseProtocol`, `HTTP`, `Socks4`, `Socks5`, `SS`, `SSR`, `Trojan`, `WS`, `H2`, `H3`, `SSH`, `Transparent`, `Redir`, `Pf`, `Tunnel`, `Echo`; `MAPPINGS`/`_PROTOCOL_REGISTRY` dicts; `get_protos`, `accept`, `udp_accept` |
| `cipher.py` | Cipher hierarchy: `BaseCipher`, `StreamCipher`, `AEADCipher`, `PacketCipher`; concrete: `AES_*_GCM`, `ChaCha20_IETF_POLY1305`, `RC4`, `RC4_MD5`, `ChaCha20`, `AES_*_CFB/CFB8/OFB/CTR`, `Salsa20`/`BF`/`CAST5`/`DES` (unsupported); `MAP` dict; `get_cipher` |
| `plugin.py` | `PluginRegistry`, `PluginBridge`, `CallbackWrapper` — bounded async callback bridge with timeout/cancellation/reentrancy detection |
| `wrapper.py` | `TLS`, `Plugin`, `Chain`, `normalize_chain` — composition helpers for protocol wrapping |
| `_asyncio.py` | `AsyncBridge`, `CloseWaiter`, `LoopAffinityError` — core async bridge with loop-affinity enforcement, cancellation propagation, idempotent close |
| `_asyncio_adapter.py` | `CompatibleStreamReader`/`CompatibleStreamWriter` wrapping `AsyncOutboundStream` into asyncio StreamReader/StreamWriter interface |
| `_compat.py` | `get_running_loop`, `HAS_TASKGROUP`, `CANCELLED_ERROR_BASE` — Python version shims |
| `config.py` | `EggressConfig` — wraps `PyEggressConfig` with `from_toml`/`from_file` |
| `exceptions.py` | Re-exports all exception types into a single import point |
| `py.typed` | PEP 561 marker |
| `_eggress.pyi` | Type stubs for the native module |

### Shim distribution (`python/pproxy/`)

Shipped by `eggress-pproxy-compat`, not by the `eggress` wheel:

| Module | Role |
|---|---|
| `__init__.py` | Re-exports `Connection`, `Server`, `DIRECT`, `Rule` + submodules |
| `__main__.py` | `python -m pproxy` entry point → `server.main()` |
| `server.py` | `proxies_by_uri` (= `Connection` = `Server`), `compile_rule`, `schedule`, `main()` — pproxy-shaped URI factories and CLI |
| `proto.py`, `cipher.py`, `cipherpy.py`, `plugin.py` | Re-exports from `eggress.protocol`, `eggress.cipher`, `eggress.plugin` |
| `sysproxy.py`, `verbose.py` | pproxy sysproxy/verbose stubs |

### Stubs

`python/eggress/_eggress.pyi` (192 lines) covers all classes, functions,
exceptions, and `__version__` with full type annotations.

## API surface

### Service lifecycle

```
EggressConfig.from_toml(toml) -> EggressConfig
EggressService(config) -> EggressService
  .start() -> EggressHandle
  .astart() -> AsyncEggressHandle
EggressHandle
  .bound_addresses -> dict
  .status() -> dict
  .metrics_text() -> str
  .reload_toml(toml) -> dict
  .shutdown()
```

### Outbound connector (no listener)

```
OutboundConnector.from_pproxy_uri(uri) -> OutboundConnector
OutboundConnector.from_toml(toml) -> OutboundConnector
  .connect_tcp(host, port, timeout) -> OutboundStream
  .aconnect_tcp(host, port, timeout) -> AsyncOutboundStream
  .validate_config(toml) -> int  (hop count)
```

### pproxy compatibility facade

```
translate_pproxy_args(args) -> TranslationResult
check_pproxy_args(args) -> CompatibilityReport
PPProxyService.from_args(args) / .from_uri(local, remotes)
Server(listen=[...], remote=[...])
  .run() / .astart() / .aclose()
```

## How it works

1. **maturin build**: `pyproject.toml` (`src/lib.rs:pyproject.toml:40-44`)
   declares `module-name = "eggress._eggress"`, `python-source = "../../python"`,
   `abi3-py39`. The `python/eggress/` tree is bundled into the wheel alongside
   the compiled `_eggress.so`.

2. **GIL release**: Every blocking Rust call runs under `py.detach(|| ...)` —
   the GIL is released during network I/O, config parsing, and service startup.
   The single pattern appears at `src/lib.rs:38`, `src/lib.rs:75`,
   `src/lib.rs:123`, `src/lib.rs:178`, `src/lib.rs:207`, etc.

3. **Outbound runtime**: A process-wide `OnceLock<Result<Arc<Runtime>>>`
   (`src/lib.rs:7-22`) provides a shared Tokio runtime for outbound
   connections. `PyOutboundStream` owns an `Arc<Runtime>` clone so it remains
   usable after the connector is dropped.

4. **Connection lifecycle**: `PyConnection` uses `AtomicU8` state machine
   (`src/lib.rs:324-329`) with `begin_close` CAS loop (`src/lib.rs:544-555`).
   `__del__` spawns async shutdown on the outbound runtime; on runtime failure
   the handle is leaked via `std::mem::forget` (`src/lib.rs:510`).

5. **Async bridge**: `AsyncBridge` (`python/eggress/_asyncio.py:322`) binds
   on first use, enforces loop affinity, runs blocking calls via
   `run_in_executor`, and propagates cancellation. `CloseWaiter` provides
   idempotent, race-safe close/wait semantics.

## Namespace / boundary rules

- The `eggress` wheel **never** installs or aliases top-level `pproxy`.
  That namespace belongs to `eggress-pproxy-compat` — see
  [pproxy-compat.md](pproxy-compat.md).
- `python-pproxy-compat/pyproject.toml` declares `package-dir = {pproxy = "../python/pproxy"}`,
  making setuptools own the `pproxy` top-level package. It depends on
  `eggress==<same version>` + `cryptography>=42,<47`.
- No `sys.modules` aliasing exists anywhere in the codebase.

## Test coverage map

| Location | What it covers |
|---|---|
| `python/tests/test_service.py` | EggressService/EggressHandle lifecycle |
| `python/tests/test_connection.py`, `test_connection_behavioral.py` | Connection state machine, context managers |
| `python/tests/test_outbound_stream_verification.py` | OutboundConnector/OutboundStream |
| `python/tests/test_pproxy_compat.py`, `test_pproxy_differential.py` | pproxy translation correctness |
| `python/tests/test_pproxy_diagnostics.py` | Diagnostic output |
| `python/tests/test_protocol_behavioral.py`, `test_protocol_cipher.py` | Protocol/cipher object model |
| `python/tests/test_plugin.py` | PluginBridge/PluginRegistry |
| `python/tests/test_wrapper.py` | Chain/TLS/Plugin wrappers |
| `python/tests/test_asyncio_semantic.py` | AsyncBridge/CloseWaiter semantics |
| `python/tests/test_config.py`, `test_config_explain.py` | Config parsing/explanation |
| `python/tests/test_errors.py` | Exception hierarchy |
| `python/tests/test_milestone_c_*.py` | Implementation detail tests (Tier 0) |
| `tests/compat/test_pproxy_api_contract.py` | API contract validation against extracted pproxy 2.7.9 contract |

## Verification workflow

```bash
# Build + install into venv
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)

# Run tests (importlib mode prevents source tree shadowing)
.venv/bin/python -m pytest python/tests tests/compat -q

# Targeted test
.venv/bin/python -m pytest python/tests -q -k "test_service_starts"
```

## Reviewer gotchas

- `Connection` in `connection.py` starts a **full managed proxy service**,
  not a pproxy-style outbound connection factory. Use `OutboundConnector`
  or `pproxy.Connection` for outbound.
- `pproxy.Connection` and `pproxy.Server` are URI factory aliases
  (`proxies_by_uri`), NOT lifecycle managers. Use `eggress.pproxy.Server`
  for managed service lifecycle.
- `PyConnection.__del__` uses `std::mem::forget` on the handle when the
  outbound runtime is unavailable — this is an intentional leak to avoid
  blocking Python's finalizer thread.
- Cipher `encrypt`/`decrypt` raise `UnsupportedFeatureError` at the Python
  level; actual encryption is delegated to Rust AEAD at the protocol layer.
- `pytest.ini` forces `--import-mode=importlib` so `python/eggress` cannot
  shadow the installed wheel's compiled `_eggress` extension.

## See also

- [pproxy-compat.md](pproxy-compat.md) — compatibility layer architecture
- [testing-and-tooling.md](testing-and-tooling.md) — test infrastructure
- `crates/eggress-python/pyproject.toml` — build configuration
- `python/eggress/_eggress.pyi` — type stubs
- `python/tests/TEST_TAXONOMY.md` — six-tier test classification
