# Python Surface — PyO3 Bindings and the `python/` Tree

Two layers: the compiled `_eggress` extension (PyO3, crate
`crates/eggress-python`) and the canonical pure-Python package `python/eggress`
that wraps it. maturin builds the wheel with `python-source = "../../python"`
and module name `eggress._eggress` (abi3-py39).

## Layout / module map

### Compiled extension (`crates/eggress-python/src/`)

Split by public surface; `lib.rs` is module registration only (`#[pymodule]
fn _eggress()`, unchanged exported names/hierarchy/abi3 metadata).

| Module | Role |
|---|---|
| `errors.rs` | Exception declarations + canonical `map_error()` mapper |
| `service.rs` | `PyEggressConfig` / `PyEggressService` / `PyEggressHandle` |
| `connection.rs` | Compatibility `Connection` state machine + connection counters |
| `compat.rs` | URI inspection, diagnostics, translation, explain/test helpers, reverse summaries |
| `outbound.rs` | `PyOutboundConnector` / `PyOutboundStream` |
| `system_proxy.rs` | `PyAppliedSystemProxy` + `apply_system_proxy` |
| `runtime.rs` | Shared process-wide Tokio runtime helper |

| Category | Symbols | Notes |
|---|---|---|
| Classes | `PyEggressConfig`, `PyEggressService`, `PyEggressHandle`, `PyConnection`, `PyOutboundConnector`, `PyOutboundStream`, `PyAppliedSystemProxy`, `PyTranslationResult`, `PyTranslationWarning`, `PyUnsupportedFeature`, `PyReverseUriSummary`, `PyUriInfo`, `PyDiagnostic` | 13 classes total |
| Functions | `translate_pproxy_args`, `translate_pproxy_uri`, `check_pproxy_args`, `validate_pproxy_args`, `pproxy_runtime_options`, `init_pproxy_logging`, `run_pproxy_test`, `describe_reverse_pproxy_uri`, `check_pproxy_uri`, `redact_pproxy_uri`, `diagnostics_for_uri`, `supported_features`, `explain_config_toml`, `explain_pproxy_args`, `explain_pproxy_uri`, `route_explain`, `test_upstream_connect`, `apply_system_proxy` | 18 functions |
| Exceptions | `EggressError` (base), `ConfigError`, `StartupError`, `ReloadError`, `ShutdownError`, `UnsupportedFeatureError`, `InternalError`, `ConnectionError`, `ConnectionClosedError`, `TimeoutError`, `DnsError`, `AuthError`, `TlsError`, `LoopMismatchError`, `ConnectionCancelledError`, `UseAfterCloseError`, `UdpAssociationError`, `UnsupportedCompositionError` | 18 exception types |
| Metadata | `__version__` | `env!("CARGO_PKG_VERSION")` |

### Pure-Python package (`python/eggress/`)

| Module | Role |
|---|---|
| `__init__.py` | Package root/export surface |
| `service.py` | `EggressService` (pre-start builder), `EggressHandle` (sync), `AsyncEggressHandle` (async via `AsyncBridge`); `PPProxyHandle` type alias |
| `connection.py` | `Connection` — managed proxy service (listener + relay); wraps `PyConnection` with state machine and `ConnectionState` enum; connection exception names are direct aliases of the native `_eggress` classes (single runtime identity) |
| `async_connection.py` | `AsyncConnection` — async wrapper with loop-affinity enforcement via `AsyncBridge`/`CloseWaiter` |
| `outbound.py` | `OutboundConnector`, `OutboundStream`, `AsyncOutboundStream` (bridge-backed: `AsyncBridge` loop-affinity + `CloseWaiter`; reads, `drain`, and `write_eof` use `AsyncBridge.run`; native `PyOutboundStream.write()` is synchronous completion, `OutboundStream.write()` preserves it, while async `write()` only submits via the private native `_submit_write` to the ordered pump) — native outbound TCP without listener; `from_pproxy_uri` (direct native, no TOML) / `from_toml` factories; `preview_connect()["hop_count"]` reports chain hops (`hop_count()`, 0 for direct) |
| `pproxy.py` | `Server`, `PPProxyService`, `TranslationResult`, `CompatibilityReport`, `Diagnostic`, `UriInfo`, `check_pproxy_uri`, `translate_pproxy_args`, route/test helpers; pproxy-flavored facade |
| `pproxy_connection.py` | `ProxyConnection` — pproxy-named outbound facade; thin wrapper over `OutboundConnector` (no listener), `tcp_connect()` returns `OutboundStream` with `sendall`/`recv` aliases |
| `_pproxy_proxy.py` | `ProxyDirect`, `ProxySimple`, `ProxyBackward`, `ProxyH2`, `ProxySSH`, `ProxyQUIC`, `ProxyH3`, `AuthTable` — pproxy 2.7.9 server object model (structural) |
| `protocol.py` | Protocol object model: `BaseProtocol`, `HTTP`, `Socks4`, `Socks5`, `SS`, `SSR`, `Trojan`, `WS`, `H2`, `H3`, `SSH`, `Transparent`, `Redir`, `Pf`, `Tunnel`, `Echo`; `MAPPINGS`/`_PROTOCOL_REGISTRY` dicts; `get_protos`, `accept`, `udp_accept` |
| `cipher.py` | Cipher hierarchy: `BaseCipher`, `StreamCipher`, `AEADCipher`, `PacketCipher`; concrete: `AES_*_GCM`, `ChaCha20_IETF_POLY1305`, `RC4`, `RC4_MD5`, `ChaCha20`, `AES_*_CFB/CFB8/OFB/CTR`, `Salsa20`/`BF`/`CAST5`/`DES` (unsupported); `MAP` dict; `get_cipher` |
| `plugin.py` | `PluginRegistry`, `PluginBridge`, `CallbackWrapper` — bounded async callback bridge with timeout/cancellation/reentrancy detection |
| `wrapper.py` | `TLS`, `Plugin`, `Chain`, `normalize_chain` — composition helpers for protocol wrapping |
| `_asyncio.py` | `AsyncBridge`, `CloseWaiter`, `LoopAffinityError` — core async bridge with loop-affinity enforcement, cancellation propagation, idempotent close |
| `_asyncio_adapter.py` | `CompatibleStreamReader`/`CompatibleStreamWriter` wrapping `AsyncOutboundStream` into asyncio StreamReader/StreamWriter interface (`readline()` matches stdlib EOF semantics; `__aiter__` returns `self` synchronously; `readuntil()` keeps `IncompleteReadError`) |
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
| `__doc__.py` | Package documentation module |
| `__main__.py` | `python -m pproxy` entry point → `server.main()` |
| `server.py` | `proxies_by_uri` (= `Connection` = `Server`), `compile_rule`, `schedule`, `main()` — pproxy-shaped URI factories and CLI |
| `proto.py`, `cipher.py`, `cipherpy.py`, `plugin.py` | Re-exports from `eggress.protocol`, `eggress.cipher`, `eggress.plugin` |
| `sysproxy.py`, `verbose.py` | pproxy sysproxy/verbose stubs |

### Stubs

`python/eggress/_eggress.pyi` (206 lines) covers all classes, functions,
exceptions, and `__version__` with full type annotations. The private native
`_submit_write` submission helper is intentionally untyped: it is not part of
the supported stub surface.

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

1. **maturin build**: `crates/eggress-python/pyproject.toml:41-43`
   declares `module-name = "eggress._eggress"`, `python-source = "../../python"`,
   `abi3-py39`. The `python/eggress/` tree is bundled into the wheel alongside
   the compiled `_eggress.so`.

2. **GIL release**: Every blocking Rust call runs under `py.detach(|| ...)` —
   the GIL is released during network I/O, config parsing, and service startup.
   The pattern lives in `service.rs`, `connection.rs`, `compat.rs`,
   `outbound.rs`, and `system_proxy.rs` (not `lib.rs`, which is module
   registration only).

3. **Outbound runtime**: A process-wide `OnceLock<Result<Arc<Runtime>>>`
   (`runtime.rs:8-9`, `PY_OUTBOUND_RUNTIME` + `outbound_runtime()`) provides a shared Tokio runtime for outbound
   connections. `PyOutboundStream` owns an `Arc<Runtime>` clone so it remains
   usable after the connector is dropped.

4. **Connection lifecycle**: `PyConnection` uses `AtomicU8` state machine
   with `begin_close` CAS loop. `PyConnection::new` uses combined native
   translation (`translate_pproxy_args_to_native` → `EggressConfig::from_compiled`,
   no TOML re-parse for startup; TOML retained only for `config` display).
   `__del__` tries `Handle::try_current().spawn(shutdown)` and falls back to
   `drop(handle)` + `eprintln` (no `std::mem::forget` in `eggress-python`).

5. **Async bridge** (single maintained pattern): `AsyncBridge`
   (`python/eggress/_asyncio.py`) binds on first use, enforces loop affinity,
   runs blocking calls via `run_in_executor` *inside the bridge only*, preserves
   contextvars, and propagates cancellation without rewriting operation
   exceptions. `CloseWaiter` provides idempotent, race-safe, multi-waiter
   close/wait. `AsyncConnection`, `AsyncEggressHandle`, and
   `AsyncOutboundStream` all use it; `AsyncOutboundStream` additionally owns
   a private Tokio write-pump task so synchronous `write()` submission never
   waits on transport I/O (native `PyOutboundStream.write()` keeps synchronous
   completion semantics for the sync `OutboundStream`; only the async adapter
   uses the private `_submit_write` queue-only path). `OutboundConnector.aconnect_tcp`,
   `Connection.aclose`/`await_closed`, and `CompatibleStreamWriter.drain` use
   `wrap_blocking_call`. Direct `run_in_executor` outside `_asyncio.py` is
   limited to the documented `plugin.py` user-callback timeout exception.

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
| `python/tests/test_api_boundary_closure.py` | Native write round-trip smoke + exception/startup/hop-count contracts (authoritative sync-completion proof is the gated Rust test below) |
| `crates/eggress-python/src/outbound.rs` (`outbound::tests`) | Deterministic gated-transport write proof: `native_sync_write_waits_for_transport_completion` (sync withheld until completion) + `async_submit_returns_before_transport_completion` (queue-only submit vs pending barrier) |
| `python/tests/test_connection.py`, `test_connection_behavioral.py` | Connection state machine, context managers |
| `python/tests/test_outbound_stream_verification.py` | OutboundConnector/OutboundStream |
| `python/tests/test_pproxy_compat.py`, `test_pproxy_differential.py` | pproxy translation correctness |
| `python/tests/test_pproxy_diagnostics.py` | Diagnostic output |
| `python/tests/test_protocol_behavioral.py`, `test_protocol_cipher.py` | Protocol/cipher object model |
| `python/tests/test_plugin.py` | PluginBridge/PluginRegistry |
| `python/tests/test_wrapper.py` | Chain/TLS/Plugin wrappers |
| `python/tests/test_asyncio_semantic.py` | AsyncBridge/CloseWaiter semantics |
| `python/tests/test_asyncio_adapter_helpers.py` | `CompatibleStreamReader` pproxy helpers (`read_w`/`read_n`/`read_until`/`rollback`) |
| `python/tests/test_asyncio_readline.py` | `readline()` stdlib EOF parity, `__aiter__`/`__anext__` iteration, `readuntil()` error preservation |
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
- `PyConnection.__del__` tries the current Tokio handle and drops with a
  diagnostic on failure — it never blocks the finalizer thread and never uses
  `std::mem::forget`.
- Cipher `encrypt`/`decrypt` raise `UnsupportedFeatureError` at the Python
  level; actual encryption is delegated to Rust AEAD at the protocol layer.
- `pytest.ini` forces `--import-mode=importlib` so `python/eggress` cannot
  shadow the installed wheel's compiled `_eggress` extension.

## Binding ownership

The PyO3 crate keeps direct dependencies only where its native surface
intentionally exposes the concept:

| Dependency | Binding owner/use |
|---|---|
| `eggress-embed` | service lifecycle, config facade, outbound compatibility facade, error mapping |
| `eggress-pproxy-compat` | pproxy parsing, translation, diagnostics, and native compilation |
| `eggress-config` / `eggress-routing` / `eggress-core` | lower-level config explanation and route-explain helpers intentionally exposed by the compatibility surface |
| `eggress-uri` | credential-safe URI redaction |
| `eggress-system-proxy` | explicit `apply_system_proxy` binding |
| `eggress-cli` | existing chain-aware upstream-test helpers (`parse_pproxy_test_target`, `run_upstream_test`), not CLI presentation — retained per Maintenance Phase 3 Outcome B |
| `eggress-runtime` | typed compatibility startup hooks and SSH policy capability |

These are live architectural edges. Removing one would require a new owner API
or would change an existing Python return/error contract, so this phase retains
them and records the justification here.

### Maintenance Phase 3 Outcome B — `eggress-python -> eggress-cli` retained

`crates/eggress-python/src/compat.rs::run_pproxy_test()` consumes exactly:

- `eggress_cli::parse_pproxy_test_target()` — URL/`host:port`/IPv4/IPv6 target
  parsing with `http`/`https` defaults (80/443), shared with `pproxy --test`
  and `eggress upstream test`;
- `eggress_cli::run_upstream_test()` — chain-aware proxy test through the
  production executor (same configured upstream chain, same timeout/exit-code/
  redaction semantics, empty-upstream `0`, no listener, no TOML/argv round-trip).

These are typed operational library functions, not argv/presentation
reach-through: they accept compiled `RuntimeConfig` + typed target/timeout and
return exit codes, with `test_upstream_connect()` remaining a distinct raw
endpoint TCP probe.

Removal via existing owner APIs was rejected because:

- `OutboundConnector` / core executor surfaces could not reproduce the exact
  target/timeout/exit-code/redaction/no-upstream contract without adding a new
  public method (forbidden) or copying the full chain tester into the binding
  (forbidden duplication, second tester risk);
- `test_upstream_connect()` intentionally probes endpoint reachability only and
  must not replace chain-aware testing;
- CLI public helpers are preserved by campaign constraint and continue to own
  the tester; the binding shares behavior instead of forking it.

Agreement is by construction (same code) and pinned by three layers:
direct native-binding regression `TestRunPproxyTestNativeBinding` in
`python/tests/test_pproxy_compat.py` (no-upstream `0`, `ValueError` parser
failures, `UnsupportedFeatureError` gate, no network), the shared-owner
`eggress-cli` unit test `pproxy_test_target_parsing_contract` (target parsing
only), and process-level
`test_python_test_mode_uses_native_bridge_without_listener_startup`
(chain-aware `--test` with no listener startup), plus existing CLI
upstream-tester coverage. Future removal requires a stable owner API that
already exposes chain-aware testing with identical result/exit/redaction
semantics and no new public surface.

## See also

- [pproxy-compat.md](pproxy-compat.md) — compatibility layer architecture
- [testing-and-tooling.md](testing-and-tooling.md) — test infrastructure
- `crates/eggress-python/pyproject.toml` — build configuration
- `python/eggress/_eggress.pyi` — type stubs
- `python/tests/TEST_TAXONOMY.md` — six-tier test classification
