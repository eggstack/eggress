# pproxy Strict Phase 4 — Python Drop-in Completion

## Objective

Move the Python compatibility package from a bounded public facade to the exact module/symbol/behavior contract chosen in Phase 0 for `pproxy==2.7.9`.

This phase is about Python source compatibility. It must continue delegating real networking/protocol execution to Eggress rather than copying 50k lines of Python runtime logic.

## Current package boundary

The Eggress wheel currently provides the top-level `pproxy` package with:

- `__init__.py`
- `server.py`
- `proto.py`
- `cipher.py`
- `plugin.py`

The 2.7.9 package also contains:

- `__doc__.py`
- `__main__.py`
- `cipherpy.py`
- `sysproxy.py`
- `verbose.py`

Current `pproxy.server` also exposes several helpers structurally while intentionally raising `UnsupportedPProxyFeature` for operational calls.

## Primary files

- `python/pproxy/*`
- `python/eggress/_pproxy_proxy.py`
- `python/eggress/protocol.py`
- `python/eggress/cipher.py`
- `python/eggress/pproxy.py`
- `crates/eggress-python/src/*` if an in-process runtime adapter is needed
- Python packaging metadata
- `python/tests/*pproxy*`

## Work package A — module namespace completion

Add the missing modules required by the Phase 0 inventory.

### `pproxy.__doc__`

Expose the metadata constants that real code can reasonably import (`__title__`, `__description__`, `__url__`, version metadata as appropriate). Keep Eggress's actual distribution version truthful; do not pretend the wheel itself is pproxy 2.7.9 if that would corrupt packaging metadata.

### `pproxy.__main__`

`python -m pproxy` must execute the same compatibility entry point as the installed `pproxy` console command.

Do not recursively spawn `pproxy` by name. Prefer a shared Python/native entry function or invoke the Eggress compatibility binary by a non-recursive resolved path only if an in-process bridge is unavailable.

### `pproxy.cipherpy`

Provide the exact required names classified in Phase 0. Do not copy upstream pure-Python cryptography merely for module existence. Functional cipher objects may delegate to Rust-backed implementations where available; legacy methods may remain feature-gated until Phase 9.

### `pproxy.sysproxy`

Wrap the same system-proxy behavior implemented in Phase 2. If upstream exposes `MacSetting`, `WindowsSetting`, and `setup`, reproduce constructor/method shape where required, but delegate mutation and rollback to the native system-proxy backend.

### `pproxy.verbose`

Reproduce required `setup`, formatting/stat helpers, or equivalent observable behavior using a compatibility adapter. Avoid introducing a second metrics engine.

## Work package B — top-level contract

Verify exact behavior for:

- `pproxy.Connection`
- `pproxy.Server`
- `pproxy.Rule`
- `pproxy.DIRECT`
- module re-exports

The upstream top-level `Connection` and `Server` aliases are URI factories, not lifecycle classes. Preserve that even if native `eggress.pproxy.Server` has a different lifecycle API.

## Work package C — operational server helpers

Phase 0 should classify each currently stubbed helper as one of:

1. required callable behavior;
2. incidental internal implementation detail not promised by strict source compatibility;
3. callable surface that may fail only when an optional feature is unavailable.

For category 1, replace `UnsupportedPProxyFeature` with adapters for:

- `prepare_ciphers` where the requested method/plugin is implemented;
- `check_server_alive` using Eggress health primitives but matching coroutine behavior;
- `test_url` using the same upstream-check operation as the CLI;
- `stream_handler` / `datagram_handler` only if real downstream usage or the strict inventory requires direct invocation.

Do not implement a duplicate pure-Python proxy runtime merely to make these names callable. If a direct Python handler call cannot be sensibly adapted to the Rust runtime, document that specific boundary and keep it out of the final unqualified source-compat claim.

## Work package D — protocol and proxy object behavior

The compatibility classes in `python/eggress/_pproxy_proxy.py` already model much of pproxy's object shape. Complete only oracle-confirmed differences:

- constructor defaults;
- property values;
- coroutine vs ordinary function status;
- return tuple shapes;
- exception classes;
- lifecycle `close`/`wait_closed` behavior where public;
- `get_extra_info`-style metadata on returned streams where callers observe it;
- repeated/jump connection behavior.

Never expose raw Rust/PyO3 exceptions when pproxy-shaped exceptions are required.

## Work package E — optional dependency behavior

pproxy 2.7.9 uses optional extras for accelerated crypto, SSH, QUIC, and daemonization.

Eggress does not need to reproduce the same Python dependency graph, but the Python compatibility surface must fail in an equivalent place with actionable diagnostics when an optional compiled Eggress feature is absent.

Examples:

- requesting `ssh://` in a wheel built without SSH -> explicit compatibility feature unavailable;
- requesting QUIC/H3 without the optional transport -> explicit diagnostic;
- legacy cipher absent -> named method unsupported, never silently substituted.

## Tests

Add one focused contract test module that compares the exact tracked inventory against pproxy 2.7.9:

- import success;
- symbol existence;
- `inspect.signature` for tracked callables;
- `inspect.iscoroutinefunction` where relevant;
- basic constructor/factory return type/attribute probes;
- exception class and message category;
- `python -m pproxy --version` and a minimal listener smoke.

Add behavioral tests for any helper converted from structural to functional.

## Packaging verification

Build a clean wheel and test in an isolated environment with no source checkout on `PYTHONPATH`.

Required smoke:

```bash
python -c 'import pproxy, pproxy.server, pproxy.proto, pproxy.cipher, pproxy.plugin'
python -c 'import pproxy.__doc__, pproxy.cipherpy, pproxy.sysproxy, pproxy.verbose'
python -m pproxy --version
```

Adjust the imports to the final Phase 0 inventory if any module is intentionally excluded.

## Non-goals

- Byte-for-byte copy of upstream Python sources.
- Maintaining two networking runtimes.
- Replicating private attributes that no tracked contract uses.
- Pure-Python fallback crypto when Rust implementations exist.

## Acceptance criteria

1. A clean Eggress wheel installs the complete Phase 0-required `pproxy` module namespace.
2. `python -m pproxy` and the console `pproxy` entry use the same execution semantics.
3. Tracked top-level symbols have pproxy 2.7.9-compatible signatures and sync/async classification.
4. `Connection`, `Server`, `Rule`, and `DIRECT` match the oracle contract.
5. Every tracked operational helper either functions through Eggress or is explicitly removed from the claimed strict source-compatibility set with evidence that it is not required.
6. Optional transport/cipher requests fail clearly when their compiled feature is absent.
7. Python tests run against a built wheel without relying on repository-local imports.
8. No new duplicate Python proxy engine is introduced.

## Completion record

Phase 4 is complete in the current Eggress wheel. The ten tracked modules are
installed, tracked symbols and callable shapes have a dedicated contract
suite, the Python/native entry points share lifecycle semantics, and the
operational helpers use native Eggress adapters or explicit feature-boundary
errors. The strict manifest and compatibility documentation record the
remaining intentional differences: truthful Eggress version metadata,
native-runtime diagnostics, unsupported legacy pure-Python ciphers, and
platform-specific system-proxy availability.
