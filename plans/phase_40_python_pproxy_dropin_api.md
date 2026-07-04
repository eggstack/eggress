# Phase 40: Python pproxy-compatible drop-in API

## Goal

Build a Python compatibility layer that lets Python users embed eggress as a Rust-backed pproxy replacement without having to manually construct eggress-native TOML or reason about the lower-level PyO3 service primitives.

The existing PyO3 layer is useful but shaped like eggress: config, service, handle, status, metrics, reload, shutdown, URI translation, diagnostics, and route explanation. This phase adds a pproxy-shaped Python surface on top of that machinery.

## Current context

Relevant current pieces:

- `crates/eggress-python/src/lib.rs` exposes PyO3 classes and functions.
- Existing classes include config/service/handle-like wrappers.
- Existing functions include pproxy arg/URI translation helpers and diagnostic/explanation utilities.
- README claims or plans include Python convenience APIs such as `start_pproxy` and `EggressService.from_pproxy_args`, but the inspected module registration should be checked to confirm the exact exported names.
- The compatibility target is not necessarily pproxy's internal implementation. The target is drop-in usability for supported pproxy-facing Python use cases.

## Primary deliverables

- A public Python compatibility module or submodule with pproxy-shaped APIs.
- High-level `start_pproxy` service constructor.
- Service class with context-manager lifecycle.
- Optional async wrappers if practical and consistent with pproxy usage.
- Stable Python exceptions for unsupported pproxy features, config failures, startup failures, reload failures, and shutdown failures.
- Python docs and examples showing drop-in usage.
- Python tests that import the built package/module and exercise the public API.
- Parity manifest updates for Python capabilities.

## API design principles

1. Keep the Rust-backed service API as the engine.
2. Add a thin compatibility facade rather than contorting internal Rust names into pproxy internals.
3. Preserve pproxy-facing argument forms where feasible.
4. Fail explicitly for unsupported pproxy features.
5. Do not silently ignore pproxy args or Python kwargs.
6. Release the GIL around blocking Rust service work.
7. Redact secrets in reprs, diagnostics, logs, and exceptions.
8. Make shutdown deterministic, especially in context-manager and test scenarios.

## Proposed Python package shape

The exact package name should follow current packaging conventions. A workable target shape:

```python
import eggress

svc = eggress.start_pproxy([
    "-l", "socks5://127.0.0.1:1080",
    "-r", "http://proxy:8080",
])
try:
    print(svc.bound_addresses())
finally:
    svc.shutdown()
```

Context-manager form:

```python
import eggress

with eggress.start_pproxy(["-l", "socks5://127.0.0.1:0"]) as svc:
    addrs = svc.bound_addresses()
    status = svc.status()
```

Class form:

```python
from eggress import PPProxyService

service = PPProxyService.from_args(["-l", "http://127.0.0.1:8080"])
with service.start() as handle:
    print(handle.metrics_text())
```

Translation/check form:

```python
import eggress

result = eggress.check_pproxy_args(["-l", "socks5://:1080", "--ssl", "cert.pem,key.pem"])
print(result.ok)
print(result.tier)
print(result.diagnostics)
print(result.toml)
```

## Required exports

At minimum expose these stable functions/classes:

- `translate_pproxy_args(args: Sequence[str]) -> TranslationResult`
- `translate_pproxy_uri(local: str, remotes: Sequence[str] | None = None) -> TranslationResult`
- `check_pproxy_args(args: Sequence[str]) -> CompatibilityReport`
- `start_pproxy(args: Sequence[str] | None = None, *, local=None, remote=None, config=None, background=True, log_format=None) -> PPProxyHandle`
- `serve(...)` as an alias or convenience wrapper if pproxy users expect that spelling
- `PPProxyService`
- `PPProxyHandle`
- `TranslationResult`
- `CompatibilityReport`
- `Diagnostic`
- `EggressError`
- `ConfigError`
- `StartupError`
- `ReloadError`
- `ShutdownError`
- `UnsupportedFeatureError`

If some names already exist with different casing, either preserve backward compatibility aliases or document the final canonical names.

## `start_pproxy` semantics

`start_pproxy` should support at least these input modes:

1. `args=[...]`: pproxy CLI-style args, excluding argv[0].
2. `local="socks5://127.0.0.1:1080"`: shorthand for one local URI.
3. `remote="http://proxy:8080"` or `remote=[...]`: shorthand for remotes.
4. `config="...toml..."` or `config_path="..."` if compatible with current config classes.

Conflicting input modes should raise `ValueError` or `ConfigError` with clear wording.

The function should:

- parse and translate pproxy args;
- reject unsupported features before starting unless an explicit permissive mode is provided;
- compile config;
- start the runtime service;
- return a handle with shutdown/context-manager behavior;
- expose bound addresses so tests can use port 0 listeners.

## `CompatibilityReport` shape

The report should align with the Phase 37 manifest and `eggress pproxy check --json` output.

Suggested fields:

- `tier: str`
- `ok: bool`
- `warnings: list[Diagnostic]`
- `unsupported: list[Diagnostic]`
- `diagnostics: list[Diagnostic]`
- `features: list[FeatureInfo]`
- `toml: str | None`
- `parsed_uris: dict`
- `raw_args: list[str]`

Do not expose raw secrets in repr or string conversion.

## Lifecycle requirements

`PPProxyHandle` should support:

- `bound_addresses()`
- `status()`
- `metrics_text()`
- `reload_toml(toml_str)`
- `shutdown()`
- `__enter__` / `__exit__`
- idempotent shutdown
- clear error on use-after-shutdown

`PPProxyService` should support:

- `from_args(...)`
- `from_uri(...)`
- `from_toml(...)`
- `from_file(...)`
- `start()`
- `__enter__` / `__exit__` if the class starts directly, or documented service/handle split if not

## Async compatibility

If pproxy users commonly embed asyncio servers, add optional async wrappers:

- `async_start_pproxy(...)`
- async context-manager support
- `await handle.shutdown_async()`

If this is too invasive, document it as a later phase and make sure blocking calls release the GIL so Python event loops are not unnecessarily starved when service startup/shutdown occurs in executor threads.

## Exception behavior

Map Rust errors to stable Python exceptions:

- invalid user args: `ValueError` or `ConfigError`
- unsupported pproxy features: `UnsupportedFeatureError`
- config compile failures: `ConfigError`
- bind/listen failures: `StartupError`
- runtime failures after startup: `EggressError` or `InternalError`
- reload failures: `ReloadError`
- shutdown failures: `ShutdownError`

Every exception message must be safe for logs. Redact credentials.

## Python typing and docs

Add `.pyi` stubs or inline Python wrapper module with type hints if packaging supports it. Include examples for:

- local direct SOCKS5 proxy
- HTTP listener with upstream proxy
- pproxy CLI args to service
- dynamic port binding for tests
- context-manager cleanup
- compatibility report before startup
- unsupported feature handling

## Tests

Add Python tests that run against the built extension or local editable install.

Minimum tests:

1. Import package/module.
2. `translate_pproxy_args` returns TOML for `-l socks5://127.0.0.1:0`.
3. `check_pproxy_args` returns `ok=True` for a simple supported config.
4. `check_pproxy_args` returns unsupported diagnostics for SSH or SSR.
5. `start_pproxy` starts a service on port 0 and reports bound address.
6. Context-manager exits and shuts down cleanly.
7. Double shutdown is safe or returns a documented error consistently.
8. Unsupported args do not start a service.
9. Credentials are redacted in repr/diagnostics.
10. `reload_toml` works for reload-supported scopes.

If integration networking tests are expensive, keep one small smoke test and mark larger tests accordingly.

## Rust/PyO3 implementation notes

- Avoid duplicating config/service logic in Python if the Rust wrapper can expose it safely.
- Prefer exposing a Python-friendly wrapper class around existing Rust handles.
- Confirm every blocking function uses `py.detach` or the current PyO3 equivalent to release the GIL.
- Do not return borrowed data that can outlive the Rust owner.
- Ensure `Drop` behavior does not hang the interpreter at shutdown.
- Make object reprs credential-safe.

## Packaging notes

If the package currently only exposes `_eggress`, add a Python-level `eggress/__init__.py` wrapper if the packaging layout permits. The public API should not force users to import a private extension module name.

## Documentation updates

Update or add:

- `docs/python/README.md`
- `docs/python/PPROXY_COMPAT.md`
- README Python usage section
- parity manifest Python entries

Document exact non-drop-in surfaces. Avoid implying full pproxy Python API equivalence until the manifest supports it.

## Acceptance criteria

- `import eggress` or the documented package import works in tests.
- `start_pproxy([...])` starts a real Rust-backed proxy service and returns a usable handle.
- Context-manager lifecycle is deterministic.
- Unsupported pproxy features raise `UnsupportedFeatureError` before service startup.
- Python compatibility reports align with CLI JSON check tiers where practical.
- Public docs include at least three complete Python examples.
- Python API entries in the parity manifest are updated with evidence.

## Verification commands

Run at minimum:

```bash
cargo fmt --all -- --check
cargo test -p eggress-python
cargo test --workspace
```

Also run the repository's Python/maturin test flow if present. If not present, add a documented local verification command in the phase implementation.

## Non-goals

- Do not clone pproxy's internal Python implementation details unless users rely on them.
- Do not implement missing protocols solely to satisfy Python tests.
- Do not expose unsafe compatibility defaults without explicit opt-in.
- Do not make the public API depend on private `_eggress` import paths.

## Handoff notes

Treat this as a user-facing API design phase. The implementation should be small, typed, and boring. The compatibility layer should make common embedding easy while preserving eggress's stricter diagnostics for unsupported or unsafe pproxy modes.
