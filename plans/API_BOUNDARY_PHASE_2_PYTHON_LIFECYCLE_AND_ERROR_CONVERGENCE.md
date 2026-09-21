# API Boundary Phase 2 — Python Lifecycle and Error Convergence

## Status

**READY FOR IMPLEMENTATION — 2026-09-21**

## Parent

[`API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md)

## Baseline

`2c9d064794a2b74831979f7f36fb25e4f5992707`

## Objective

Make synchronous and asynchronous Python service lifecycles select the same native behavior and preserve useful Eggress exception identity across sync/async/native wrapper paths, without renaming or removing any existing public Python symbol.

## Defect 1 — async service startup bypasses compatibility hooks

`EggressService.start()` selects either:

- `PyEggressService.start()`, or
- `PyEggressService.start_with_compatibility_options(...)` when `_compatibility_options` was captured by `from_pproxy_args()`.

`EggressService.astart()` currently always schedules `self._inner.start`.

That means a service created from identical pproxy arguments can enter different runtime policy depending on whether the caller uses `start()` or `await astart()`.

### Required correction

Create one private Python-level start-selection helper that consumes the service exactly once and returns the native `PyEggressHandle`.

Both public methods must use it:

```text
start()  -> shared selector -> native handle -> EggressHandle
astart() -> AsyncBridge(shared selector) -> native handle -> AsyncEggressHandle
```

The helper must preserve the existing compatibility option extraction and must not configure logging in the supervisor.

Do not add a new public start method.

### Required regression matrix

Exercise native TOML and pproxy-created services through both sync and async start.

For pproxy-created services, prove at minimum that the same values for:

- auth reuse timeout;
- system-proxy request state;

reach the same native compatibility hook construction.

Prefer an observable/focused hook-construction test over manipulating the host OS system proxy during ordinary unit tests.

## Defect 2 — async bridge flattens operation exceptions

`AsyncBridge.run()` and `wrap_blocking_call()` currently convert ordinary exceptions from the called operation into generic `RuntimeError`.

This destroys meaningful categories such as `ConfigError`, `StartupError`, `ReloadError`, connection errors, and user callback exceptions.

### Required correction

Separate bridge-infrastructure failure from operation failure.

Expected exceptions raised by the submitted callable must propagate with their original class and message. The bridge may create its own `RuntimeError` only for failures attributable to the bridge itself, such as:

- attempting to use a closed bridge;
- loop-affinity violation remains `LoopAffinityError`;
- executor submission infrastructure failure before the operation begins;
- an explicitly identified impossible internal bridge state.

Do not catch an arbitrary operation `Exception` merely to wrap it.

Cancellation semantics remain unchanged: cancelling the Python awaitable cancels the executor future best-effort and raises `asyncio.CancelledError`.

### Required tests

For the same failing operation, verify sync and async forms expose the same public exception class/category where the API promises an async analogue.

Include at least:

- service startup/configuration failure reachable through an async facade;
- reload failure;
- outbound connect failure;
- a test callable raising an ordinary Python exception through `AsyncBridge` to prove the bridge does not rewrite it.

## Defect 3 — native and pure-Python connection error families drift

The native extension defines `ConnectionError`, `ConnectionClosedError`, `TimeoutError`, `DnsError`, `AuthError`, `TlsError`, and related types.

`python/egress/connection.py` also defines public connection exceptions. Managed `Connection` raises the pure-Python family while outbound native calls currently raise the native family directly.

### Compatibility-first convergence rules

Do not delete or rename either currently importable family.

The target behavior is:

- existing imports from `egress.connection` continue to work;
- existing imports from `egress._egress` continue to work;
- top-level aliases such as `ConnectionBaseError` keep their names;
- broad `EggressError` catches continue to work;
- public Python facade operations expose a coherent catch hierarchy;
- direct native-extension calls may continue to expose native exception classes.

Use inheritance and/or a private exception translation helper only after proving its MRO/catch behavior in tests. A facade-raised exception should, where feasible, remain catchable by both the documented pure-Python connection base and the corresponding native broad base. Do not rely on string parsing to classify errors.

Do not change exception messages in a way that exposes credentials or full proxy URIs.

### Required contract tests

Freeze:

- `issubclass` relationships expected by the maintained public API;
- catch behavior for managed `Connection`;
- catch behavior for `OutboundConnector.connect_tcp()`;
- catch behavior for `OutboundConnector.aconnect_tcp()`;
- closed-stream read/write failures;
- timeout/auth/TLS categories where deterministic local fixtures exist.

If Python/PyO3 exception-type constraints make dual-family inheritance unsafe, retain both families and translate only at the facade boundary. Document that design rather than attempting a risky C-extension exception MRO.

## Documentation/stubs

Update:

- `architecture/python-bindings.md`;
- `docs/PYTHON_BINDINGS.md`;
- Python exception stubs as needed;
- lifecycle docs that describe `astart()`.

Do not claim `astart()` uses `asyncio.to_thread` if the maintained implementation uses `AsyncBridge`.

## Verification

```bash
(cd crates/egress-python && ../../.venv/bin/maturin develop)

.venv/bin/python -m pytest   python/tests/test_service.py   python/tests/test_errors.py   python/tests/test_asyncio_semantic.py   python/tests/test_outbound_stream_verification.py   python/tests/test_pproxy_compat.py -q

.venv/bin/python -m pytest python/tests tests/compat -q
```

Rust/PyO3 gate:

```bash
cargo test -p eggress-python
cargo test -p eggress-embed
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Stop conditions

Stop and isolate a smaller correction if:

1. making the pure-Python exception family inherit from native extension types creates an unsafe/unsupported extension-type MRO;
2. preserving all existing documented catch relationships conflicts with making every facade error identical;
3. testing `--sys` compatibility would require mutating the host system proxy in ordinary CI.

The mandatory corrections that should still proceed are the shared start selector and async exception-preservation behavior.

## Acceptance criteria

- [ ] `start()` and `astart()` use one private start-selection path.
- [ ] Services created from pproxy args apply the same compatibility runtime hooks under sync and async startup.
- [ ] Async bridge helpers no longer flatten ordinary operation exceptions to generic `RuntimeError`.
- [ ] Cancellation and loop-affinity behavior remain intact.
- [ ] Existing native and pure-Python exception names remain importable.
- [ ] Managed and outbound facade operations have explicit, tested catch semantics.
- [ ] Error messages remain redacted.
- [ ] Python lifecycle/error docs and stubs match runtime behavior.
- [ ] Full Python suite and workspace gate are green.
