# API Boundary Corrective Closure

## Status

**IMPLEMENTED — 2026-09-21**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Baseline commit: `c15c60b189d0588d254011e877b78b2ca8a6b3a9`
- Parent roadmap: [`API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md)
- Purpose: close the small set of post-implementation contract defects found after the five API-boundary phases landed, without reopening architecture or expanding capability.

## Scope

This is one narrow corrective pass with four workstreams:

1. restore the established native `PyOutboundStream.write()` contract while keeping `AsyncOutboundStream.write()` event-loop safe through a genuinely private async-submission path;
2. finish Python connection-exception identity convergence so runtime exports, public aliases, and stubs describe the same classes;
3. correct `PyOutboundConnector.preview_connect()["hop_count"]` to report chain hops rather than upstream count;
4. add the missing lifecycle/exception/API-contract evidence and close the stale planning state.

No other API redesign, crate reshaping, protocol work, performance tuning, parity expansion, or feature work is authorized by this plan.

## Why this plan exists

Commit `c15c60b189d0588d254011e877b78b2ca8a6b3a9` substantially implemented the five-phase campaign:

- TOML parse/version/validate/compile now delegates to `eggress-config`;
- `eggress-outbound` dropped its direct optional `toml` dependency;
- service sync/async startup shares startup-operation selection;
- `AsyncBridge` preserves operation exceptions;
- connection errors were mostly moved to one native hierarchy;
- async outbound writes gained an ordered Tokio write pump;
- binding ownership and Rust API documentation were added;
- runtime/stub/export drift was reduced;
- CI and Python smoke are green.

Post-implementation review found four residual issues that prevent clean closure.

### Residual A — native outbound write semantics and surface changed

Before the campaign, native `PyOutboundStream.write(data)` was a synchronous/blocking write operation. The implementation changed it to enqueue data into the async write pump.

To preserve the synchronous high-level `OutboundStream.write()` behavior, the implementation added:

```text
PyOutboundStream.write_blocking_for_sync(...)
```

and declared that method in `python/eggress/_eggress.pyi`.

That is the opposite ownership direction required by Phase 3. The async adapter needed a private submission primitive; the already-existing native `write()` surface should not have been repurposed while adding a new non-underscored native method.

### Residual B — two connection exception identities still drift

Most connection exceptions now directly alias native classes in `python/eggress/connection.py`, but these remain locally defined:

```python
class LoopMismatchError(EggressError):
    ...

class UnsupportedCompositionError(EggressError):
    ...
```

The native extension also exports `LoopMismatchError` and `UnsupportedCompositionError`.

As a result:

- `eggress.connection.LoopMismatchError` is not the same object as `eggress._eggress.LoopMismatchError`;
- `eggress.connection.UnsupportedCompositionError` is not the same object as the native class;
- `eggress.exceptions` re-exports the pure-Python classes;
- `exceptions.pyi` declares the native classes.

Runtime identity and stubs therefore still disagree.

### Residual C — preview hop accounting uses the wrong quantity

`PyOutboundConnector.preview_connect()` currently sets:

```rust
dict.set_item("hop_count", self.inner.upstream_count())?;
```

`OutboundConnector` already exposes the correct `hop_count()`, and `upstream_count()` is a different quantity.

A TOML connector with one upstream containing a two-hop chain can therefore report `hop_count == 1` even though the configured chain has two hops.

### Residual D — closure evidence/planning state is stale

The parent roadmap and all five phase plans still report `READY FOR IMPLEMENTATION` despite the implementation commit.

The implementation also did not leave explicit focused evidence for every Phase 2/5 contract called out by the plans, especially:

- sync vs async compatibility startup forwarding of auth-reuse/system-proxy options;
- connection exception identity/catch relationships after hierarchy convergence;
- representative public API path qualification beyond the initial embed smoke test.

This is documentation/evidence debt, not authorization for another broad implementation campaign.

---

## Workstream 1 — Restore native write compatibility and isolate async submission

### Required target semantics

The maintained surfaces must be:

| Surface | Required semantics |
|---|---|
| `eggress._eggress.PyOutboundStream.write(data) -> int` | established synchronous/blocking write completion semantics |
| `eggress.outbound.OutboundStream.write(data) -> int` | synchronous/blocking, unchanged |
| `eggress.outbound.AsyncOutboundStream.write(data) -> int` | synchronous submission only; must not wait on transport I/O |
| `await AsyncOutboundStream.drain()` | barrier for all writes submitted before the call and propagation point for asynchronous write failure |

The write pump may remain. The correction is which API feeds it.

### Required implementation

1. Restore native `PyOutboundStream.write()` to synchronous completion semantics.
   - It may internally submit to the existing write pump and immediately wait on a barrier.
   - It must not return success merely because bytes were queued.
   - Preserve its existing `int` return shape.

2. Remove `write_blocking_for_sync` from the public/native Python method surface.
   - Remove it from `#[pymethods]` under that name.
   - Remove it from `_eggress.pyi`.
   - Do not retain an undocumented non-underscored replacement.

3. Give `AsyncOutboundStream` a private submission path.
   Preferred order:
   - an underscored PyO3 method such as `_submit_write`, if the Python adapter must call native code directly;
   - or a private Rust/PyO3 helper architecture that is not exposed in the maintained stub/public docs.

   If an underscored native method is required, do **not** list it as part of the supported `_eggress.pyi` public surface unless the repository's stub policy explicitly requires private implementation methods to be typed. If tooling requires a declaration, mark it private and ensure public-export contract tests ignore it.

4. Keep one ordered write pump per connected stream.
   - no thread per stream;
   - arbitrary `BoxStream` transports remain supported;
   - read and write halves remain independently usable;
   - no file-descriptor assumption.

5. Preserve `sendall()`, `drain()`, `write_eof()`, `close()`, and `wait_closed()` semantics.

6. Preserve terminal failure behavior:
   - synchronous native/high-level writes surface write failure before returning;
   - async submissions retain terminal failure and surface it at `drain()` or earlier local misuse;
   - no queued data may be silently reported as durably written.

### Required regression tests

Add/adjust tests proving:

- direct native `PyOutboundStream.write()` does not use queue-only semantics;
- high-level `OutboundStream.write()` remains blocking/completion-based;
- `AsyncOutboundStream.write()` keeps the event loop schedulable under backpressure;
- ordered async writes + drain remain correct;
- write failure is observed by the appropriate synchronous or async surface;
- `write_eof` remains ordered after earlier async submissions;
- close/wait_closed leaves no pump task running;
- the new private helper does not appear in `eggress.__all__` or documented supported API.

Do not use a fragile “must take at least X milliseconds” assertion as the primary proof of synchronous completion. Prefer a deterministic peer/read barrier.

### Stop condition

If preserving native `write()` semantics while keeping `AsyncOutboundStream.write()` non-blocking requires changing the public `BoxStream` contract or adding a thread per stream, stop and document the limitation. Do not change the native method semantics again to force the async adapter.

---

## Workstream 2 — Complete exception identity convergence

### Required implementation

Replace the remaining pure-Python duplicate classes in `python/eggress/connection.py` with the corresponding native classes:

- `LoopMismatchError`;
- `UnsupportedCompositionError`.

The public import paths must remain unchanged:

```python
from eggress.connection import LoopMismatchError, UnsupportedCompositionError
from eggress.exceptions import LoopMismatchError, UnsupportedCompositionError
from eggress import LoopMismatchError, UnsupportedCompositionError
from eggress._eggress import LoopMismatchError, UnsupportedCompositionError
```

The intended identity relationship after correction is:

```python
eggress.connection.LoopMismatchError is eggress._eggress.LoopMismatchError
eggress.connection.UnsupportedCompositionError is eggress._eggress.UnsupportedCompositionError
```

Do the same identity review for every connection exception touched by the campaign so the test covers the complete maintained family, not only these two residual classes.

### Catch hierarchy

Preserve the hierarchy currently established in native PyO3:

- `ConnectionError(EggressError)`;
- `ConnectionClosedError(ConnectionError)`;
- `TimeoutError(ConnectionError)`;
- `DnsError(ConnectionError)`;
- `AuthError(ConnectionError)`;
- `TlsError(ConnectionError)`;
- `ConnectionCancelledError(ConnectionError)`;
- `UseAfterCloseError(ConnectionError)`;
- `UdpAssociationError(ConnectionError)`.

Do not opportunistically move `LoopMismatchError` or `UnsupportedCompositionError` under `ConnectionError` in this corrective pass unless that inheritance already existed in the pre-campaign public contract. Identity convergence and hierarchy redesign are separate concerns.

### Required tests

Add one explicit exception-contract matrix checking:

- identity between native, `connection`, `exceptions`, and top-level aliases where names correspond;
- `issubclass` relationships;
- managed `Connection` catch behavior;
- sync outbound catch behavior;
- async outbound catch behavior;
- bridge propagation retains native exception identity;
- closed-stream operations raise the documented family.

The runtime test must agree with `exceptions.pyi` and `_eggress.pyi`.

---

## Workstream 3 — Correct preview hop count

### Required implementation

Change `PyOutboundConnector.preview_connect()` to use:

```rust
self.inner.hop_count()
```

for the `"hop_count"` field.

Do not rename the field or add an `upstream_count` field as part of this fix. The public dictionary shape remains unchanged.

### Required tests

Cover all three cases:

1. direct connector → `hop_count == 0`;
2. one upstream with one proxy hop → `hop_count == 1`;
3. one upstream containing a multi-hop chain → `hop_count == chain.hops.len()`, proving the result is not `upstream_count()`.

Where a pproxy `__` chain is the simplest deterministic constructor, use it. Also retain a TOML case if the TOML constructor can express multiple hops clearly.

---

## Workstream 4 — Add missing lifecycle/API evidence and close planning state

### Compatibility startup evidence

The shared `EggressService._select_start_operation()` implementation is directionally correct, but closure requires a focused regression showing sync and async startup select the same compatibility options.

Do not modify the host system proxy in ordinary CI merely to test this.

Preferred test seam:

- factor the compatibility-operation selection or native hook construction enough to observe the forwarded values without applying OS policy;
- prove representative pproxy args produce identical:
  - `auth_timeout_seconds`;
  - `system_proxy`;
  under `start()` and `astart()`.

A test that only proves both methods start a listener is insufficient.

### Rust API qualification evidence

Keep `docs/RUST_API.md` and the existing embed compatibility test.

Add only the narrow representative compile/construction tests needed to satisfy the original Phase 5 intent. At minimum verify current import/construction paths for:

- `eggress-outbound::OutboundConnector::direct`;
- `eggress-outbound::OutboundConnector::from_chain`;
- `eggress_embed::outbound::OutboundConnector` compatibility re-export;
- `eggress-config::RuntimeConfig` through `EggressConfig::{from_compiled, compiled, into_compiled}`;
- `eggress-relay::{relay, RelayOptions, RelayReport}` type paths;
- representative `eggress-routing` and `eggress-core` types already documented as supported.

Do not build an exhaustive API snapshot and do not add a new semver tool.

### Planning-state closure

After the corrective implementation and verification pass:

1. update this file to `IMPLEMENTED` with the implementation commit and focused evidence;
2. update all five phase-plan statuses from `IMPLEMENTED; CORRECTIVE CLOSURE OPEN` to `IMPLEMENTED`;
3. update the parent roadmap to `IMPLEMENTED`;
4. mark the corrective row implemented in the parent execution table;
5. ensure `docs/ROADMAP.md` no longer describes this campaign as active/open;
6. do not create another completion document if every criterion below is satisfied.

---

## Explicit non-goals

Do not use this corrective pass to:

- add or remove Python public methods beyond removing the accidental `write_blocking_for_sync` expansion and restoring the pre-campaign contract;
- change `AsyncOutboundStream.write()` into an awaitable;
- add public buffering/high-water controls;
- redesign the write pump into a general actor framework;
- add Python UDP APIs;
- expand listener-free UDP composition;
- change pproxy compatibility tiers or manifest claims;
- change CLI/config/URI behavior;
- change protocol or transport support;
- change Rust crate visibility;
- deprecate or remove published Rust items;
- add new crates;
- raise MSRV;
- reopen Eggfetch HTTP CONNECT integration;
- add mandatory `cargo-semver-checks`/nightly API tooling;
- modify the separate completed performance campaign.

If implementation discovers another independent defect, record it in the handoff. Do not absorb it here unless it blocks one of these four closure workstreams.

## Implementation order

1. Restore native `write()` compatibility and introduce the private async submit boundary.
2. Complete exception identity aliases and their contract matrix.
3. Fix `preview_connect()["hop_count"]` and add multi-hop regression coverage.
4. Add focused compatibility-startup and representative Rust API evidence.
5. Run focused Python/PyO3/outbound tests.
6. Run feature slices and broad repository gates.
7. Update planning statuses in place and close this plan.

## Verification

### Focused Python/PyO3

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)

.venv/bin/python -m pytest   python/tests/test_service.py   python/tests/test_errors.py   python/tests/test_public_exports.py   python/tests/test_asyncio_semantic.py   python/tests/test_outbound_stream_verification.py   python/tests/test_pproxy_compat.py -q

cargo test -p eggress-python
cargo test -p eggress-outbound
cargo test -p eggress-embed
```

### Required feature slices

```bash
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp

cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat
```

Run the required OpenSSH regression only if the corrective implementation touches SSH-facing construction/feature topology. It should not need to.

### Broad closure gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
.venv/bin/python -m pytest python/tests tests/compat -q
```

No external pproxy oracle/differential run is required unless the compatibility-startup correction changes actual compatibility behavior beyond proving the already-intended option forwarding.

## Final acceptance criteria

This corrective plan may be marked **IMPLEMENTED** only when all are true:

- [ ] native `PyOutboundStream.write()` again has synchronous completion semantics;
- [ ] `OutboundStream.write()` remains behaviorally unchanged;
- [ ] `AsyncOutboundStream.write()` remains synchronous-to-call but performs only private ordered submission and does not wait on transport I/O;
- [ ] accidental public `write_blocking_for_sync` is removed from runtime/stubs/docs;
- [ ] async write ordering, drain failure, EOF ordering, close, and backpressure-loop-schedulability tests are green;
- [ ] all maintained connection exception aliases point to one runtime class identity for the same named exception;
- [ ] runtime exception identities/hierarchy agree with `.pyi` declarations;
- [ ] `preview_connect()["hop_count"]` uses `hop_count()` and a multi-hop regression proves the difference from upstream count;
- [ ] sync and async compatibility startup forwarding of auth timeout/system-proxy options is explicitly tested without mutating host proxy state;
- [ ] representative Rust public paths listed above compile through maintained tests;
- [ ] no Rust/Python/CLI/config/protocol capability is removed or renamed;
- [ ] no new public API is added as a workaround;
- [ ] required feature slices, Python suite, clippy, formatting, and workspace tests pass;
- [ ] parent/phase/corrective planning statuses accurately reflect closure.

## Closure record

Fill this section in place during implementation. Do not create another closure plan if the criteria above pass.

- Implementation commit: `6a1b67c`
- Native write contract evidence: `python/tests/test_api_boundary_closure.py::TestNativeWriteContract::test_native_write_completes_without_drain` (peer receipt without drain) + `test_sync_wrapper_write_echoes_without_explicit_drain`; native `PyOutboundStream.write()` restored to submit+barrier completion in `crates/eggress-python/src/outbound.rs`, `write_blocking_for_sync` removed from runtime and `_eggress.pyi`.
- Async write-pump/private-submit evidence: `test_async_write_uses_private_submit_and_drain_completes`, `test_async_ordered_writes_plus_drain`, `test_async_write_keeps_loop_schedulable`, `test_write_eof_ordered_after_async_submissions`, `test_async_drain_surfaces_terminal_failure`, `test_close_wait_closed_leaves_no_pump_running`; async path uses private native `_submit_write` (absent from `eggress.__all__` and `_eggress.pyi` per `test_private_submit_exists_but_not_public`).
- Exception identity evidence: `TestExceptionIdentityMatrix` (11-name identity across native/`connection`/`exceptions`/top-level, hierarchy, stub agreement, managed/sync/async catch, `AsyncBridge` identity preservation, closed-stream family); `LoopMismatchError`/`UnsupportedCompositionError` are native aliases in `python/eggress/connection.py`.
- Hop-count regression evidence: `TestPreviewHopCount` (direct 0, single-hop 1, pproxy `__` two-hop 2, TOML `__` two-hop 2 with `upstream_count == 1`, shape unchanged); `preview_connect` uses `hop_count()` in `crates/eggress-python/src/outbound.rs`.
- Compatibility startup forwarding evidence: `TestCompatibilityStartupForwarding` (parametrized default/`--auth 5`/`--sys` identical selection under sync/async paths via shared `_select_start_operation`, source-sharing assertion, non-compat `None`, representative forwarded values); seam `EggressService._compatibility_start_args()` in `python/eggress/service.py` observes without binding listeners or touching host proxy.
- Rust API qualification evidence: `crates/eggress-embed/tests/public_api.rs` (`embed_config_handoff_and_outbound_facade_paths_compile` with explicit `RuntimeConfig` path, `outbound_authority_and_compat_reexport_paths_compile` for `eggress-outbound` authority + embed re-export, `relay_routing_core_representative_paths_compile` for `relay`/`RelayOptions`/`RelayReport`, `Router`/`RouteActionSpec`, `UpstreamId`/`TargetAddr`).
- Broad verification: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace --locked` (2945 passed, 151 ignored); `.venv/bin/python -m pytest python/tests tests/compat -q` (2308 passed, 115 skipped); outbound/ssh/pproxy/udp + embed ssh/pproxy feature slices; `cargo check --manifest-path fuzz/Cargo.toml --bins`; `cargo check -p eggress-cli --locked --no-default-features --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins`. No external oracle/differential run (no compatibility behavior changed beyond proving intended option forwarding).
- Retained limitations: none new; write-pump `submit_lock` is held across the drain/EOF barrier wait (serializes concurrent writers per stream, preserving ordering); async write-after-bridge-close surfaces bridge `RuntimeError` rather than the native family (native terminal failure covered via still-open-bridge paths).


## Evidence-state follow-up

Runtime/API corrective closure remains complete. Acceptance-checkbox reconciliation and a stronger deterministic proof of synchronous native-write completion are tracked in [`API_BOUNDARY_EVIDENCE_STATE_POLISH.md`](API_BOUNDARY_EVIDENCE_STATE_POLISH.md). This follow-up is evidence-only unless the stronger test exposes a real defect.
