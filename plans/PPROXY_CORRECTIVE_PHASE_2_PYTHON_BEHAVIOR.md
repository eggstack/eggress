# pproxy Corrective Phase 2 — Python Behavioral Honesty

## Status

**IMPLEMENTED**

### Implementation summary

**Exceptions introduced:**
- `PProxyCompatibilityError(RuntimeError)` — base for all known unsupported operations
- `UnsupportedPProxyFeature(PProxyCompatibilityError)` — raised with feature name and alternative

**Methods changed from silent no-ops / generic `NotImplementedError`:**

| Method | Before | After |
|--------|--------|-------|
| `server.check_server_alive` | infinite sleep loop | raises `UnsupportedPProxyFeature` |
| `server.stream_handler` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `server.datagram_handler` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `server.print_server_started` | `return None` | formats and returns startup message |
| `server.test_url` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `proto.sslwrap` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `plugin.get_plugin` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyBackward.close` | empty body | raises `UnsupportedPProxyFeature` |
| `ProxyBackward.start_backward_client` | `return None` | raises `UnsupportedPProxyFeature` |
| `ProxyBackward.start_server_run` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyBackward.udp_start_server` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyBackward.wait_open_connection` | `return None` | raises `UnsupportedPProxyFeature` |
| `ProxyH2.handler` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyH2.udp_start_server` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyH2.wait_h2_connection` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyH2.wait_open_connection` | `return None` | raises `UnsupportedPProxyFeature` |
| `ProxySSH.tcp_connect` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxySSH.start_server` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxySSH.udp_start_server` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxySSH.wait_open_connection` | `return None` | raises `UnsupportedPProxyFeature` |
| `ProxySSH.wait_ssh_connection` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyQUIC.tcp_connect` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyQUIC.start_server` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyQUIC.udp_start_server` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyQUIC.wait_open_connection` | `return None` | raises `UnsupportedPProxyFeature` |
| `ProxyQUIC.wait_quic_connection` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyH3.udp_start_server` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyH3.wait_h3_connection` | `NotImplementedError` | `UnsupportedPProxyFeature` |
| `ProxyH3.wait_open_connection` | `return None` | raises `UnsupportedPProxyFeature` |
| `ProxyH3.wait_quic_connection` | `NotImplementedError` | `UnsupportedPProxyFeature` |

**Structural-only classes documented:**
- `ProxySSH`, `ProxyQUIC`, `ProxyH3` docstrings updated to say "Structural-only"

**Documentation added:**
- `pproxy.Connection` / `pproxy.Server` docstrings clarify they are URI factories, NOT the native `eggress.pproxy.Server`
- `pproxy.__init__` docstring adds note distinguishing factory aliases from lifecycle class

**Type stubs updated:**
- `pproxy.pyi`: `PProxyCompatibilityError`, `UnsupportedPProxyFeature` added
- `__init__.pyi`: re-exports added

## Parent roadmap

[`PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`](PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md)

## Dependency

Phase 1 must establish the final compatibility diagnostic categories. Phase 2 may begin before Phase 1 implementation is complete, but it must use the same distinction among implemented, native-equivalent, unsupported, and non-equivalent behavior.

## Objective

Make the bundled top-level `pproxy` namespace behaviorally honest. Every public method must either perform the documented bounded behavior, delegate to the native Eggress runtime, preserve a proven upstream structural contract, or raise a stable compatibility exception before side effects.

The phase must eliminate silent no-ops and misleading operational facades without rebuilding pproxy's private asyncio server engine in Python.

## Current risk

The repository has two Python surfaces:

1. `eggress.pproxy`, which provides a real Eggress-backed lifecycle API.
2. the bundled top-level `pproxy` namespace, which aims to let ordinary pproxy applications migrate without source edits.

The top-level namespace currently mixes real behavior, metadata adapters, compatibility-shaped factories, generic `NotImplementedError`, and methods that simply return `None`. That makes API shape look stronger than runtime capability.

Confirmed examples include:

- `python/pproxy/server.py::check_server_alive()` sleeping forever without checking any server;
- `python/pproxy/server.py::print_server_started()` returning `None` without behavior;
- `python/pproxy/server.py::stream_handler()` and `datagram_handler()` exposing operational names but failing only at invocation;
- `python/eggress/_pproxy_proxy.py::wait_open_connection()` returning `None` for absent pooling semantics;
- `ProxyBackward.close()` containing no behavior;
- `ProxyBackward.start_backward_client()` returning `None`;
- additional runtime-shaped methods raising generic `NotImplementedError` with inconsistent messages;
- `Connection` and `Server` aliases reflecting upstream factory shape while other documentation may imply native lifecycle objects.

## Scope

### In scope

- public names exported by `python/pproxy/__init__.py`;
- public names exported by `python/pproxy/server.py`, `proto.py`, and `cipher.py`;
- compatibility object methods in `python/eggress/_pproxy_proxy.py`;
- the native facade in `python/eggress/pproxy.py` and its type stubs where delegation is appropriate;
- Python binding exceptions exposed by `crates/eggress-python` when needed;
- compatibility and Python tests;
- concise active Python compatibility documentation.

### Out of scope

- reproducing pproxy's private event-loop internals;
- implementing SSH, QUIC/H3, SSR, legacy cipher, or plugin engines;
- implementing cross-session connection pooling;
- replacing the Rust runtime with Python stream/datagram handlers;
- exact replication of private classes or incidental module globals;
- a new Python package or separate compatibility distribution;
- broad renaming of the native `eggress` API;
- preserving misleading behavior solely because a historical shape test expects a name to exist.

## Required inventory

Before edits, generate a temporary working inventory from these sources:

- `python/pproxy/__init__.py`
- `python/pproxy/server.py`
- `python/pproxy/proto.py`
- `python/pproxy/cipher.py`
- `python/eggress/_pproxy_proxy.py`
- `python/eggress/pproxy.py`
- `python/eggress/pproxy.pyi`
- `tests/compat/test_pproxy_api_contract.py`
- all `tests/compat/test_pproxy_*.py` files
- `python/tests/test_server_lifecycle.py`
- `python/tests/test_asyncio_semantic.py`
- `docs/python/PPROXY_API_INVENTORY.md`
- `docs/python/PPROXY_EMBEDDED_USAGE_PATTERNS.md`

For each public callable, record in implementation notes—not a new permanent report—one classification:

- `behavioral-match`
- `native-delegation`
- `structural-only`
- `intentional-unsupported`
- `remove-from-public-export`

Do not count importability or construction alone as behavioral support.

## Workstream A — Stable compatibility exception

Introduce one stable exception for unsupported compatibility operations. Prefer a narrow hierarchy such as:

```python
class PProxyCompatibilityError(RuntimeError):
    pass

class UnsupportedPProxyFeature(PProxyCompatibilityError):
    def __init__(self, feature: str, alternative: str | None = None): ...
```

Exact names may follow current package conventions. Requirements:

- exported from the canonical `eggress` Python package;
- available to the bundled `pproxy` namespace where callers need to catch it;
- message identifies the exact method/feature;
- message states a supported Eggress alternative when one exists;
- no secrets, raw credential URIs, or configuration contents in the message;
- generic `NotImplementedError` is not used for known product-level exclusions.

A method may retain standard `NotImplementedError` only for abstract subclass protocol behavior that is not exposed as a supported concrete operation. There should be few or no such cases in this compatibility surface.

## Workstream B — Replace silent no-ops

Search the compatibility packages for:

- empty method bodies;
- `pass`;
- unconditional `return None`;
- infinite sleep loops;
- generic `NotImplementedError`;
- methods whose docstrings claim runtime behavior not present in code.

For every result, choose one of these actions:

### Delegate

Delegate to `eggress.pproxy` or the native extension only when:

- the native method has equivalent lifecycle semantics;
- argument conversion is local and deterministic;
- ownership and shutdown behavior remain clear;
- delegation does not perform the same protocol handshake twice;
- exceptions can be translated without hiding the cause.

### Implement locally

Implement in Python only for small pure behavior such as:

- rule compilation;
- scheduler selection over already-supplied objects;
- metadata/property semantics;
- URI-to-object construction;
- deterministic return formatting.

Do not implement network protocol engines in Python.

### Raise explicitly

Raise `UnsupportedPProxyFeature` for methods requiring:

- pproxy's private listener runtime;
- unavailable backward/reverse lifecycle hooks;
- unavailable pooled connection state;
- SSH, QUIC/H3, SSR, legacy cipher, plugin, or daemon behavior;
- direct mutation of internal Rust runtime objects that the binding does not expose safely.

### Remove from exports

Remove a public export only when it is not part of upstream's documented public surface and no repository user-facing documentation promises it. Preserve import compatibility for documented names, but explicit failure is preferable to silent success.

## Workstream C — Specific method decisions

### `check_server_alive`

Do not retain an infinite sleep loop.

Choose one:

- delegate to an existing native health-check API and update object state in the same shape upstream expects; or
- raise `UnsupportedPProxyFeature("check_server_alive", alternative="use Eggress runtime health checks")` immediately.

Do not create a new health scheduler in Python.

### `stream_handler` and `datagram_handler`

These names imply ownership of the full pproxy listener engine. Eggress intentionally owns wire handling in Rust.

Unless a real Rust-backed adapter already accepts the exact reader/writer/datagram callback contract, raise the stable compatibility exception immediately. Keep the methods importable only if upstream public usage requires it.

Do not add a Python forwarding loop merely to satisfy these symbols.

### `print_server_started`

Determine upstream observable behavior. If it only formats/logs a startup message, implement the small formatting behavior or preserve a documented no-return side effect. An unconditional silent no-op is acceptable only if an oracle test proves upstream also produces no observable effect under the same inputs.

### `test_url`

Delegate to the existing Eggress upstream test path if a safe in-process binding exists. Do not spawn an implicit external process from a library method unless that is already an established API contract.

Otherwise raise the compatibility exception and direct callers to the supported CLI/API alternative.

### `wait_open_connection`

This method is associated with reuse/pooling behavior. Phase 1 corrects `--reuse` to listener `SO_REUSEPORT`; it does not create an upstream connection pool.

If upstream's method is a private helper and not required by documented applications, remove it from public claims. If it remains callable for shape compatibility, raise explicit unsupported behavior rather than returning `None` as though no connection were currently available.

Do not build a connection pool in this phase.

### `ProxyBackward.close` and `start_backward_client`

Use the existing native reverse/backward capability only if the binding already exposes a lifecycle handle with close/wait semantics. Otherwise raise the stable exception.

An empty `close()` method is prohibited because callers rely on cleanup methods for resource ownership.

### Unsupported concrete proxy classes

`ProxySSH`, `ProxyQUIC`, and `ProxyH3` may remain constructible only when construction itself is part of upstream parsing behavior and the object cannot accidentally appear operational.

Requirements:

- class documentation states structural-only status;
- network/lifecycle methods raise the stable compatibility exception;
- parser/factory output does not translate unsupported classes into a different supported protocol;
- practical matrix and API inventory classify them as intentional exclusions;
- tests prove no silent direct fallback.

## Workstream D — Clarify `Connection` and `Server`

Upstream aliases `Connection` and `Server` to URI factory behavior. Preserve that factory contract in the top-level `pproxy` namespace when verified.

Do not conflate those aliases with the native `eggress.pproxy.Server` lifecycle class.

Required documentation distinction:

- `pproxy.Connection` / `pproxy.Server`: pproxy-shaped URI/object factories;
- `eggress.pproxy.Server` or the current native lifecycle type: actual Rust-backed server lifecycle.

Where a compatibility object has `start_server`, use native delegation only if return type, close behavior, `wait_closed`, socket exposure, and exception timing are compatible. Otherwise fail explicitly and direct users to the native lifecycle API.

Do not rename upstream aliases merely to make the distinction easier.

## Workstream E — Preserve working pure-Python compatibility behavior

Do not remove or rewrite behavior already demonstrated to match upstream, including where tests prove it:

- `compile_rule`;
- `schedule` algorithms `fa`, `rr`, `rc`, and `lc`;
- URI factories and chain object construction;
- `AuthTable` behavior where bounded and tested;
- metadata properties;
- direct TCP connection behavior already covered by focused tests;
- protocol handshake ordering that is already known to avoid duplicate handshakes.

When touching these files, add regression tests only for behavior affected by the no-op cleanup. Do not broaden into an exhaustive Python clone.

## Workstream F — Type stubs and exports

Update `.pyi` files and `__all__` alongside implementation changes.

Requirements:

- unsupported methods remain typed with their upstream signature where import compatibility requires them;
- docstrings and stubs note that invocation raises `UnsupportedPProxyFeature`;
- exception types are exported consistently;
- removed private names disappear from `__all__` and active documentation;
- no stub claims a return handle that implementation can never produce;
- native lifecycle types retain accurate close/wait semantics.

Do not create a second set of compatibility stubs outside the canonical package tree.

## Workstream G — Tests

### Contract tests

Revise shape-only tests so they do not treat importability as proof of support.

For each public operational method, test one of:

- real bounded behavior;
- correct delegation and return type;
- immediate stable unsupported exception with feature identity;
- proven structural-only construction followed by explicit failure on execution.

### No-silent-no-op guard

Add one focused static or introspection-based test that protects a small explicit list of operational methods from regressing to unconditional `None`/empty behavior. Do not build a general bytecode linter.

A simple test that invokes the concrete methods with minimal safe inputs and asserts behavior/exception is preferred.

### Lifecycle tests

For delegated server lifecycle:

- startup returns a documented handle;
- readiness is observable without fixed sleeps;
- close is idempotent where native API promises it;
- wait/stop terminates boundedly;
- startup failure does not leak a task or listener;
- no protocol handshake occurs twice.

Use loopback listeners and existing fixtures. Do not use external services.

### Unsupported classes

Tests must prove SSH/QUIC/H3 and other exclusions do not silently fall back to direct HTTP/SOCKS behavior.

## Focused verification

During iteration:

```bash
python -m pytest tests/compat/test_pproxy_api_contract.py -q
python -m pytest tests/compat -q
python -m pytest python/tests/test_server_lifecycle.py python/tests/test_asyncio_semantic.py -q
```

Build the native extension before the final Python gate:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

If Rust bindings change:

```bash
cargo test -p eggress-python
cargo test -p eggress-embed
```

Final broad gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Do not require an external pproxy installation unless one exact method's upstream behavior remains unresolved. Use a focused probe, not the complete certification suite.

## Acceptance criteria

Phase 2 is complete only when all are true:

- every public top-level `pproxy` callable has a behavior classification;
- all empty operational methods, unconditional operational `None` returns, and infinite placeholder loops are removed;
- unsupported operations raise one stable compatibility exception before side effects;
- generic `NotImplementedError` is no longer used for known concrete product exclusions;
- working pure-Python compatibility behavior remains intact;
- `Connection` and `Server` factory semantics are preserved and clearly distinguished from the native lifecycle API;
- native delegation, where used, has tested ownership, shutdown, return-type, and exception semantics;
- unsupported proxy classes cannot silently execute as a supported protocol or direct route;
- type stubs, `__all__`, docstrings, API inventory, and tests agree;
- the complete Python and compatibility suites pass against a freshly built native extension;
- no Python network engine, connection pool, new package, or compatibility framework is added.

## Handoff notes for the implementer

- Begin by listing concrete no-op and `NotImplementedError` locations; do not edit blindly.
- Preserve upstream factory shape where it is harmless, but do not preserve false operational success.
- Prefer one compatibility exception type with structured fields over many method-specific exceptions.
- Use native delegation only when lifecycle semantics are genuinely compatible.
- Keep the unsupported message actionable and stable; avoid embedding full URIs or credentials.
- Update this file in place with the implementation commit range and the final delegated/unsupported method summary. Do not create a separate closure file.