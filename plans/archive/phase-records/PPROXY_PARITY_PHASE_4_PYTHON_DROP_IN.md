# Phase 4 — Bounded Python `pproxy` Drop-In Surface

## Status

Implemented in the bounded public subset. The wheel now ships a real
top-level `pproxy` package alongside `eggress`; remaining private pproxy
internals and excluded protocol families stay explicitly out of scope.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Depends on

Phase 1 URI/CLI representation. Phase 2 and Phase 3 should be substantially complete so the Python API delegates to stable compatibility behavior.

## Objective

Allow ordinary Python programs written against the documented `pproxy==2.7.9` public API to run without source edits after the Eggress distribution is installed.

This phase restores a real top-level `pproxy` namespace while retaining a single Python distribution named `eggress`. It implements the documented public connection and server contracts over the Rust runtime. It does not attempt to clone every private pproxy implementation detail.

## Current gap

The current wheel exposes:

```python
from eggress import pproxy
```

but not:

```python
import pproxy
```

The current public classes also differ materially:

- `eggress.Connection` starts a managed listener rather than representing pproxy's outbound connection factory;
- `ProxyConnection.tcp_connect()` is synchronous and returns an Eggress stream, while pproxy exposes an async contract returning reader/writer-compatible objects;
- no matching public UDP send API exists;
- `eggress.pproxy.Server` has Eggress-oriented constructor and lifecycle behavior rather than the documented pproxy `Server` contract;
- many protocol/cipher classes are structural facades rather than functional public adapters;
- prior compatibility-package documentation is stale because the separate distribution was removed.

## Scope boundary

### Required public namespace

The single Eggress wheel should install:

```text
pproxy/
  __init__.py
  server.py
  proto.py
  cipher.py
```

Add additional tiny modules only when a documented pproxy import or existing upstream example requires them.

The modules should primarily re-export or adapt implementation from `eggress`, avoiding duplicated protocol logic.

### Required top-level symbols

Implement and expose at minimum:

- `Connection`;
- `Server`;
- `Rule`;
- `DIRECT`;
- `proto`;
- `cipher`;
- version metadata expected by normal callers.

Use a small oracle inventory to confirm exact symbol spelling and signatures.

### Required behavioral contracts

Implement the documented public workflows:

- construct an outbound proxy chain from a pproxy URI;
- `await Connection.tcp_connect(host, port)`;
- return a pair compatible with normal `asyncio.StreamReader` and `asyncio.StreamWriter` usage;
- `await Connection.udp_sendto(host, port, data, callback)` or the exact observed pproxy 2.7.9 UDP signature;
- construct `Server` from documented URI/listener inputs;
- start a server using the documented async lifecycle entry point;
- close or stop it using the documented lifecycle;
- compile/use `Rule` in the documented public form;
- expose `DIRECT` with compatible identity/behavior where callers compare or pass it.

### Explicit non-goals

- reproducing every private class, task, closure, or module global;
- matching object `repr` or incidental attribute order unless upstream examples rely on it;
- exposing raw Rust file descriptors;
- implementing unsupported SSH, QUIC/H3, SSR, legacy cipher, or plugin behavior through Python;
- reviving a second `eggress-pproxy-compat` distribution;
- installing under the PyPI project name `pproxy`;
- broad monkey-patching of `sys.modules` from `eggress.__init__`;
- copying upstream Python networking logic when a Rust-backed adapter can satisfy the contract.

## Packaging design

Keep one distribution:

```toml
[project]
name = "eggress"
```

Package both `eggress` and `pproxy` Python source directories in the wheel. The top-level `pproxy` package imports narrow adapters from `eggress`.

Do not dynamically inject `sys.modules["pproxy"]`. A real package tree provides predictable imports, submodule discovery, type checking, and tracebacks.

Document package collision behavior:

- installing upstream `pproxy` and Eggress into the same environment is unsupported because both provide the same import namespace;
- the distribution name remains `eggress`;
- users replacing upstream pproxy should uninstall upstream first, then install Eggress;
- uninstall and upgrade behavior should be checked once with a local wheel, not added as a large CI matrix.

## Architecture

### Shared outbound core

Use `eggress.OutboundConnector` and Rust-owned streams as the connection engine.

Add an asyncio adapter layer that presents the minimum reader/writer protocol expected by pproxy applications:

- `reader.read()`;
- `reader.readexactly()` and `readuntil()` if the oracle object supports them or upstream examples use them;
- `writer.write()`;
- `await writer.drain()`;
- `writer.close()`;
- `await writer.wait_closed()` where available;
- `writer.get_extra_info()` for common keys;
- EOF and half-close behavior consistent with the underlying stream.

Prefer existing compatible reader/writer adapters in `python/eggress/_asyncio_adapter.py`. Extend them rather than creating a parallel stack.

### `Connection`

The top-level `pproxy.Connection` must be a new compatibility class or alias to the correctly shaped adapter. Do not alias it to `eggress.Connection`.

Required behavior:

1. Accept the same constructor inputs as pproxy for common HTTP/SOCKS/direct/modern-SS/Trojan/H2/WS/raw chains.
2. Parse through the shared Phase 1 compatibility AST.
3. Use native outbound connection paths without opening a temporary local listener.
4. Provide the observed coroutine methods and return shapes.
5. Retain references needed to keep streams and UDP associations alive.
6. Close idempotently and cancel outstanding work predictably.

### UDP adapter

Map pproxy's public UDP callback model onto existing Eggress UDP association support.

Minimum requirements:

- direct UDP;
- one-hop SOCKS5 UDP;
- one-hop modern Shadowsocks UDP;
- callback receives source/target data in the shape observed from pproxy;
- close tears down associations;
- unsupported multi-hop UDP raises at call time with a clear compatibility error.

Do not implement multi-hop UDP solely inside Python.

### `Server`

The top-level `pproxy.Server` should adapt to `EggressService` while matching the observed constructor and lifecycle.

Requirements:

- constructor accepts documented pproxy inputs and defaults;
- async start entry point returns the same broad object category as the oracle or an object satisfying the same awaited/close protocol;
- server addresses and handles are exposed where documented;
- close is idempotent;
- cancellation and event-loop shutdown do not leak the Rust runtime thread or listeners;
- no-argument/default server behavior uses the mixed listener from Phase 1.

Keep the existing Eggress-native `eggress.pproxy.Server` API working. It may internally delegate to the new compatibility adapter, but do not break documented Eggress call patterns.

### `Rule` and `DIRECT`

Confirm oracle behavior with focused probes:

- whether `Rule` is a function, class, alias, or factory;
- accepted inputs and return shape;
- how `DIRECT` compares, prints, and participates in connection construction.

Implement only the public behavior observed. Do not recreate unused routing internals.

### `proto` module

Expose public protocol classes and helpers required by documented examples and common imports. Divide symbols into:

1. Functional public adapters — must execute their documented public methods.
2. Metadata/value objects — may be lightweight if pproxy uses them only for construction or inspection.
3. Unsupported protocol families — construct or parse only if the oracle does so without optional dependencies, then raise a stable unsupported error when execution is attempted.

Do not label a structural class drop-in. Functional methods should delegate to Rust or the shared compatibility parser.

### `cipher` module

Expose modern cipher names and lookup behavior already backed by Eggress or `cryptography`:

- AES-128-GCM;
- AES-192-GCM only if the current compatibility module already supports it consistently;
- AES-256-GCM;
- ChaCha20-IETF-Poly1305;
- base/registry symbols required by common imports.

Legacy stream cipher classes may exist as explicit unsupported descriptors only if importing them is necessary for source compatibility. They must not falsely advertise working encryption.

Keep the optional `cipher-api` dependency policy. Missing dependency errors should be clear and lazy.

## Workstream 4.1 — Freeze the public API subset

Create a concise table from:

- pproxy 2.7.9 top-level namespace;
- PyPI-documented examples;
- upstream examples shipped with the frozen release;
- existing Eggress strict inventory, corrected for structural-only claims.

Mark each symbol as:

- required functional;
- required structural/metadata;
- unsupported family descriptor;
- private/not in scope.

Do not inventory every underscore-prefixed implementation detail.

## Workstream 4.2 — Package the namespace

1. Add `python/pproxy/` or the authoritative Python source equivalent.
2. Update maturin inclusion so both packages ship in wheels.
3. Add type stubs or inline types for the bounded public surface.
4. Add a wheel smoke test for `import pproxy`, `import pproxy.server`, `import pproxy.proto`, and `import pproxy.cipher`.
5. Remove stale references to the deleted separate distribution.

## Workstream 4.3 — Implement connection adapters

1. Introduce the correctly shaped top-level `Connection`.
2. Implement async `tcp_connect` over `OutboundConnector`.
3. Return compatible reader/writer adapters.
4. Implement direct and supported one-hop UDP callback behavior.
5. Implement close/cancellation cleanup.
6. Add negative tests for unsupported transports and use-after-close.

## Workstream 4.4 — Implement server lifecycle

1. Probe constructor and start/close signatures.
2. Adapt constructor inputs into shared compatibility config.
3. Implement async start and close behavior.
4. Preserve no-argument mixed-listener default.
5. Confirm repeated close and cancellation.
6. Keep the Eggress-native server wrapper compatible.

## Workstream 4.5 — Functional public modules

1. Implement `Rule` and `DIRECT` behavior.
2. Re-export functional protocol classes where existing Rust-backed behavior exists.
3. Correctly classify construction-only classes.
4. Expose modern cipher registry/operations.
5. Add precise unsupported errors for excluded families.

## Workstream 4.6 — Documentation and migration

Document:

- installation order when replacing upstream pproxy;
- one-distribution/two-namespace packaging;
- supported Python public API subset;
- intentional exclusions;
- differences that remain in private internals;
- examples copied from upstream public usage with minimal environment-specific changes.

## Acceptance criteria

Phase 4 is complete when:

- the Eggress wheel contains a real top-level `pproxy` package;
- `import pproxy`, `pproxy.server`, `pproxy.proto`, and `pproxy.cipher` succeed in a clean environment without upstream pproxy installed;
- documented common `Connection` construction works;
- `await Connection.tcp_connect()` returns reader/writer-compatible objects and relays a TCP echo payload;
- direct, SOCKS5, and modern Shadowsocks UDP public calls work where supported;
- documented `Server` construction/start/close works on a local echo scenario;
- default server construction uses the correct mixed listener;
- `Rule` and `DIRECT` match observed public behavior;
- supported modern protocol/cipher symbols are functional rather than import-only;
- excluded protocol/cipher families fail explicitly at execution;
- the existing `eggress` namespace remains stable;
- one wheel, not a second compatibility distribution, owns packaging;
- no temporary compatibility listener is used for outbound TCP connections;
- a small set of unchanged upstream public examples passes.

## Focused verification

```bash
python -m build  # or the repository's authoritative maturin build command
python -m pytest python/tests/test_wheel_import_smoke.py
python -m pytest python/tests/test_proxy_connection.py
python -m pytest python/tests/test_server_lifecycle.py
python -m pytest python/tests/test_protocol_cipher.py
```

Add a small `tests/compat_python_public/` or existing-suite equivalent containing no more than the public workflows required above. Run against the oracle optionally and against Eggress routinely.

## Required negative cases

- both upstream pproxy and Eggress installed in one environment;
- unsupported SSH/QUIC/SSR constructor or execution;
- async method called after close;
- event-loop cancellation during connect;
- writer close followed by repeated wait/close;
- UDP multi-hop request;
- missing optional cipher dependency;
- secrets absent from `repr`, warnings, and exceptions.

## Rollback and compatibility notes

The top-level namespace may conflict with upstream pproxy by design. Keep the distribution name `eggress` and document replacement installation. Do not silently import upstream pproxy when Eggress behavior is unavailable; this would make execution environment-dependent.

## Handoff guidance

Recommended commit order:

1. public API subset table;
2. package tree and wheel inclusion;
3. TCP reader/writer adapter;
4. UDP adapter;
5. Server lifecycle;
6. Rule/proto/cipher public surface;
7. docs and clean-wheel smoke.

Stop if implementation begins duplicating pproxy's entire Python networking stack. The Rust engine and existing adapters must remain the execution core.
