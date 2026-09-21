# API Boundary and Interop Maintenance Roadmap

## Status

**IMPLEMENTED — 2026-09-21**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `2c9d064794a2b74831979f7f36fb25e4f5992707`
- Workspace release line: `1.0.7`
- Governing constraint: reduce feature overlap and maintenance burden while preserving the existing Rust, Python, CLI, configuration, protocol, feature-gate, and compatibility surfaces.

## Purpose

The current repository no longer needs a broad architectural rewrite. The recent extraction of `eggress-outbound` successfully removed the largest listener-bound/listener-free execution overlap, and the existing protocol, transport, routing, relay, runtime, embed, and compatibility crates have defensible ownership.

The remaining maintenance debt is concentrated at boundaries:

1. TOML parse/version/validate/compile logic is independently implemented in `eggress-config`, `eggress-embed`, and `eggress-outbound`;
2. Python synchronous and asynchronous service startup do not select compatibility runtime hooks through the same path;
3. Python connection exceptions have both native and pure-Python representations, while the async bridge currently flattens ordinary operation exceptions into generic `RuntimeError`;
4. `AsyncOutboundStream.write()` performs real native I/O synchronously on the asyncio event-loop thread;
5. the PyO3 crate depends directly on a broad set of internal crates and the Python type-stub/capability metadata has begun to drift from runtime behavior;
6. the published Rust workspace exposes a broad de facto semver surface that should be qualified and documented before further internal cleanup.

This campaign fixes those maintenance and interop defects without changing supported capability or asking downstream users to migrate.

## Governing constraints

1. Do not remove, rename, move, or change the signature of any existing public Rust item, Python symbol, Python method, exception name, CLI option, configuration key, URI form, or documented feature gate.
2. Preserve current protocol behavior, pproxy compatibility claims, listener/UDP semantics, TLS/SSH verification policy, routing behavior, reload behavior, metrics, and shutdown ordering.
3. No new proxy protocol, transport, scheduler, compatibility tier, Python UDP API, listener role, or configuration knob belongs in this campaign.
4. Existing native-extension exception classes remain importable from `eggress._eggress`; existing pure-Python exception names and top-level aliases remain importable from their current modules.
5. Preserve synchronous method shapes. In particular, `AsyncOutboundStream.write(data) -> int` must not become awaitable.
6. Preserve `eggress-embed::outbound::*` as a source-compatible facade over `eggress-outbound`.
7. Do not split the workspace into additional crates merely to reduce dependency edges. Prefer correcting ownership inside existing crates.
8. Do not raise the MSRV, add a git-only dependency, or reopen the deferred Eggfetch HTTP CONNECT migration in this campaign.
9. Do not weaken credential redaction or make error messages include target/proxy secrets.
10. Prefer one implementation authority plus compatibility adapters over multiple “canonical” implementations.
11. If an internal cleanup cannot preserve established observable behavior with focused regression tests, stop and document the boundary rather than forcing the refactor.
12. The open performance-evidence campaign remains independent. Do not mix benchmark tuning or runtime performance policy into this maintenance work.

## Current-state findings

### Configuration authority is duplicated

`eggress-config::validate_and_compile_toml()` already owns the full parse → version check → validation → compilation sequence. `eggress-embed::parse_validate_compile()` and `eggress-outbound::connector::parse_validate_compile()` independently repeat the same pipeline.

The duplicated facade implementations differ in error formatting from `ConfigError::Display`, so consolidation must preserve the existing embed/outbound error strings rather than blindly replacing the calls with `.to_string()`.

### Python compatibility startup diverges between sync and async

`EggressService.start()` consults `_compatibility_options` and selects `PyEggressService.start_with_compatibility_options()` for services built from pproxy arguments.

`EggressService.astart()` currently always runs `self._inner.start` through `AsyncBridge`. As a result, async startup can bypass compatibility runtime hooks such as auth-reuse timeout and system-proxy handling.

### Python error identity and async propagation need convergence

The native module exports connection-related exceptions while `python/eggress/connection.py` defines a second public hierarchy. Outbound operations raise native exceptions directly, while managed `Connection` wraps failures in the pure-Python hierarchy.

Separately, `AsyncBridge.run()` and `wrap_blocking_call()` currently catch ordinary `Exception` and replace it with generic `RuntimeError`, causing sync and async forms of the same operation to expose different exception categories.

### Async outbound writes can block the event-loop thread

`AsyncOutboundStream.write()` is synchronous, as expected for an asyncio-style writer, but delegates to the native `PyOutboundStream.write()`. The native implementation executes `runtime.block_on(stream.write(...))`; releasing the GIL does not prevent the caller's asyncio loop thread from blocking on socket/transport backpressure.

The correction must preserve the synchronous `write() -> int` surface and stream ordering.

### Python binding ownership and metadata are wider than necessary

`eggress-python` directly depends on `eggress-embed`, `eggress-pproxy-compat`, `eggress-config`, `eggress-routing`, `eggress-core`, `eggress-uri`, `eggress-system-proxy`, `eggress-cli`, and `eggress-runtime`.

Some edges are legitimate, but the binding crate should consume stable owner APIs rather than reaching through architectural layers where avoidable. Type stubs also have current drift, including native methods/exception exports not represented consistently in `.pyi` files. `eggress.capabilities()` contains another manually maintained capability list.

### Rust library exposure is broad but should not be broken

The current public crates are usable and the outbound extraction is healthy. The maintenance issue is the size of the published semver surface, not a need to hide it retroactively.

Notable cross-crate coupling such as `EggressConfig::from_compiled(RuntimeConfig, ...)`, `compiled()`, and `into_compiled()` is already public and must be treated as part of the established contract.

## Registered execution plans

| Order | Plan | Status | Purpose |
|---|---|---|---|
| 1 | [`API_BOUNDARY_PHASE_1_CONFIG_AUTHORITY_CONVERGENCE.md`](API_BOUNDARY_PHASE_1_CONFIG_AUTHORITY_CONVERGENCE.md) | Implemented | Make `eggress-config` the single TOML compile authority while preserving facade error behavior and feature topology. |
| 2 | [`API_BOUNDARY_PHASE_2_PYTHON_LIFECYCLE_AND_ERROR_CONVERGENCE.md`](API_BOUNDARY_PHASE_2_PYTHON_LIFECYCLE_AND_ERROR_CONVERGENCE.md) | Implemented | Align sync/async compatibility startup and preserve meaningful Python exception identity across native and async paths. |
| 3 | [`API_BOUNDARY_PHASE_3_ASYNC_OUTBOUND_IO.md`](API_BOUNDARY_PHASE_3_ASYNC_OUTBOUND_IO.md) | Implemented | Remove event-loop-blocking outbound writes without changing `AsyncOutboundStream` public method shapes. |
| 4 | [`API_BOUNDARY_PHASE_4_PYTHON_BINDING_OWNERSHIP_AND_STUBS.md`](API_BOUNDARY_PHASE_4_PYTHON_BINDING_OWNERSHIP_AND_STUBS.md) | Implemented | Reduce unnecessary PyO3 dependency reach-through and make runtime/stub/capability metadata agree. |
| 5 | [`API_BOUNDARY_PHASE_5_RUST_PUBLIC_API_QUALIFICATION.md`](API_BOUNDARY_PHASE_5_RUST_PUBLIC_API_QUALIFICATION.md) | Implemented | Freeze and document the existing Rust library exposure so future cleanup does not accidentally regress published consumers. |
| 6 | [`API_BOUNDARY_CORRECTIVE_CLOSURE.md`](API_BOUNDARY_CORRECTIVE_CLOSURE.md) | Implemented | Restore the native outbound write contract, finish exception identity convergence, correct hop-count metadata, and close missing evidence/planning state. |

## Sequencing

Phase 1 is independent and should land first. It removes concrete duplicated implementation with the smallest semantic risk.

Phase 2 should follow because Phase 3 relies on a trustworthy async bridge and explicit exception semantics. Do not redesign outbound async I/O while ordinary async calls are still flattening their errors.

Phase 3 is intentionally separate because preserving a synchronous writer surface while removing event-loop blocking requires careful ordering, failure, cancellation, and close semantics.

Phase 4 should use the stabilized Python behavior from Phases 2–3 to prune dependency reach-through and repair stubs/documentation without mixing behavior changes into metadata cleanup.

Phase 5 may be developed in parallel with Phase 4 but should close last, after the internal ownership changes are known. It is a qualification/documentation pass, not authorization to remove existing public APIs.

## Explicit non-goals

Do not use this campaign to:

- add Python exposure for `UdpAssociation`;
- expand listener-free UDP beyond its current direct + single-hop SOCKS5 contract;
- add SSH listeners;
- change QUIC/H3 support or feature defaults;
- change Shadowsocks/SSR/legacy-crypto compatibility;
- redesign `RuntimeConfig`, `ProxyChainSpec`, `ConnectionConfig`, routing traits, or scheduler policy;
- replace PyO3 with another binding technology;
- merge `eggress-embed`, `eggress-python`, `eggress-runtime`, or `eggress-server`;
- create a new facade crate;
- change top-level `pproxy` namespace ownership;
- implement the deferred Eggfetch HTTP CONNECT dependency migration;
- change the compatibility manifest or parity tier unless implementation uncovers an actual pre-existing claim defect;
- introduce a mandatory external semver tool into ordinary CI.

## Global verification

Every implementation phase should use focused tests first. Before closing the campaign:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp

cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat

(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

Run the required OpenSSH embed regression if a touched feature edge can affect SSH construction. External pproxy differential/oracle suites are required only if a compatibility claim or compatibility execution path changes beyond the internal lifecycle correction explicitly covered by Phase 2.

## Roadmap acceptance criteria

This campaign is complete only when:

1. `eggress-config` is the single implementation authority for TOML parse/version/validation/compilation and embed/outbound preserve their established public errors.
2. Sync and async `EggressService` startup select the same compatibility runtime hooks for the same service.
3. Expected Eggress/native operation exceptions are not flattened to generic `RuntimeError` merely because an async bridge was used.
4. Public connection exception names remain importable and documented, with regression tests covering catch behavior for managed and outbound APIs.
5. `AsyncOutboundStream.write()` no longer waits on native transport I/O on the event-loop thread while retaining its synchronous return shape and ordered `drain()` semantics.
6. The PyO3 dependency graph contains only justified direct edges; removed edges do not reappear as facade reach-through elsewhere.
7. Runtime exports, `.pyi` declarations, package `__all__`, and documented Python API agree for the maintained surface.
8. `capabilities()` retains its current public shape/values unless a pre-existing correctness defect is separately approved, and consistency is mechanically tested against authoritative metadata where practical.
9. Existing Rust public exports and feature slices continue to compile through explicit contract tests/qualification.
10. No protocol capability, CLI behavior, configuration surface, compatibility tier, or published API is removed or renamed.


## Post-implementation corrective closure

The five implementation phases landed in commit `c15c60b189d0588d254011e877b78b2ca8a6b3a9` and passed CI/Python smoke. Post-implementation review found a small number of residual API-contract and evidence defects. They are bounded by [`API_BOUNDARY_CORRECTIVE_CLOSURE.md`](API_BOUNDARY_CORRECTIVE_CLOSURE.md). Do not reopen the five phases beyond work required by that corrective plan.
