# API Boundary Evidence-State Polish

## Status

**READY FOR IMPLEMENTATION — 2026-09-21**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Baseline commit: `1e408afae496cdf1f45fd438070fbd0557004660`
- Parent campaign: [`API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md) (**IMPLEMENTED**)
- Corrective implementation: `6a1b67cc673b7e98481588299836333b5de1241b`
- Corrective closure record: [`API_BOUNDARY_CORRECTIVE_CLOSURE.md`](API_BOUNDARY_CORRECTIVE_CLOSURE.md) (**IMPLEMENTED**)
- Purpose: polish the evidence state after runtime/API closure without reopening implementation scope.

## Scope

The API-boundary/interoperability line of work is functionally closed. This is an evidence-only polish pass with exactly two substantive goals:

1. reconcile the historical acceptance checkboxes in the five phase plans and corrective closure plan with the evidence that now exists;
2. replace the weak synchronous-native-write regression proof with a deterministic transport-gated test that would actually fail if native `PyOutboundStream.write()` regressed to queue-only semantics.

A small amount of test-only/internal factoring is allowed only when required to make the write semantic directly testable. No supported runtime/API behavior should change.

## Why this plan exists

Post-closure review of current `main` found that implementation, broad verification, and CI are healthy, but the repository's evidence state is not fully self-consistent.

### Evidence-state defect 1 — implemented plans still contain unchecked acceptance criteria

The following plans are marked `IMPLEMENTED` while their acceptance checklists remain unchecked:

- `API_BOUNDARY_PHASE_1_CONFIG_AUTHORITY_CONVERGENCE.md`;
- `API_BOUNDARY_PHASE_2_PYTHON_LIFECYCLE_AND_ERROR_CONVERGENCE.md`;
- `API_BOUNDARY_PHASE_3_ASYNC_OUTBOUND_IO.md`;
- `API_BOUNDARY_PHASE_4_PYTHON_BINDING_OWNERSHIP_AND_STUBS.md`;
- `API_BOUNDARY_PHASE_5_RUST_PUBLIC_API_QUALIFICATION.md`;
- `API_BOUNDARY_CORRECTIVE_CLOSURE.md`.

The corrective closure record already cites focused evidence and broad verification, but the acceptance sections still visually imply that none of the criteria have been satisfied.

The correction is **not** to blindly change every `[ ]` to `[x]`. Each criterion must be mapped to concrete current evidence first. If a criterion cannot be substantiated from code/tests/docs/CI, leave it unchecked and record the precise evidence gap rather than manufacturing closure.

### Evidence-state defect 2 — current native-write regression does not prove its stated property

`python/tests/test_api_boundary_closure.py::TestNativeWriteContract::test_native_write_completes_without_drain` currently writes to a live TCP peer and waits for the peer to observe the bytes without an explicit `drain()`.

Its comment claims a queue-only native write would never reach the peer until `drain()`. That is not true for the current architecture: the Tokio write pump consumes queued `Data` commands independently, so a queue-only implementation can still transmit bytes promptly and satisfy the test.

The runtime implementation is currently correct:

```text
PyOutboundStream.write()
  -> write_blocking()
  -> WritePump::submit()
  -> WritePump::barrier()
```

while the async adapter uses private `_submit_write`.

The evidence needs to prove **completion semantics**, not merely eventual transmission.

---

## Governing constraints

1. Do not reopen the completed API-boundary implementation campaign.
2. Do not alter any supported Rust, Python, CLI, config, URI, protocol, transport, feature-gate, or compatibility surface.
3. Do not change `PyOutboundStream.write()`, `OutboundStream.write()`, or `AsyncOutboundStream.write()` semantics unless a newly strengthened test exposes an actual defect.
4. No new production dependency is authorized.
5. No new crate is authorized.
6. No new public Python/PyO3 method is authorized.
7. Keep private `_submit_write` private/underscored and absent from the maintained public stub/export surface.
8. Do not add benchmark thresholds or wall-clock performance gates.
9. Do not require external network access or a pproxy oracle for this evidence pass.
10. Do not modify the completed performance campaign.
11. Prefer deterministic synchronization over sleeps/timeouts as correctness evidence.
12. Historical plan text may be annotated with closure evidence, but do not rewrite the original problem statements into a post-hoc description of current code.

---

## Workstream 1 — Build a deterministic native-write completion proof

### Required semantic distinction

The test must distinguish these two behaviors:

```text
QUEUE ONLY
write(data)
  -> enqueue Data
  -> return immediately
  -> pump may transmit later

SYNCHRONOUS COMPLETION
write(data)
  -> enqueue Data
  -> wait for ordered transport write completion
  -> return
```

A test that only observes eventual peer receipt cannot distinguish them.

### Preferred test architecture

Add a Rust-level deterministic fixture close to the native write-pump implementation, preferably in `crates/eggress-python/src/outbound.rs` under `#[cfg(test)]` or a focused crate-local test module.

Use a test-only boxed stream whose `AsyncWrite::poll_write` behavior is explicitly gate-controlled:

1. the first write attempt signals `write_polled`;
2. while a test gate is closed, `poll_write` stores/wakes through the normal waker path and returns `Poll::Pending`;
3. after the gate is opened, the same write is allowed to complete;
4. `poll_flush` / `poll_shutdown` have deterministic minimal behavior;
5. `AsyncRead` may be a harmless EOF/minimal implementation because the test concerns write completion only.

The fixture must satisfy the same `eggress_core::BoxStream` trait boundary used in production. Do not special-case production code around the fixture.

### Preferred implementation seam

If direct testing of `PyOutboundStream::write_blocking` would require awkward Python-interpreter orchestration, minimally factor the existing internal sequence into a private Rust helper on `WritePump`, for example conceptually:

```text
submit_and_wait(runtime, data)
  -> submit(data)
  -> barrier(runtime, false)
  -> return len
```

Then:

- native `PyOutboundStream.write()` / `write_blocking()` must delegate to that exact helper;
- the deterministic Rust test exercises that exact helper;
- async private `_submit_write` continues to call queue-only `submit()`.

The helper must remain private Rust implementation detail. Do not expose it through PyO3.

### Required deterministic assertions

The authoritative test should use channels/events/atomics rather than elapsed-time thresholds:

1. start synchronous write execution against the gated stream;
2. wait until the fixture signals that transport `poll_write` has actually been reached;
3. while the write gate remains closed, prove the synchronous completion result has **not** been delivered;
4. open the transport gate and wake the stored task;
5. prove synchronous write then completes successfully with the expected byte count.

Add the inverse proof for the queue-only async submission path:

1. keep the same transport gate closed;
2. call the internal queue-only submit path used by `_submit_write`;
3. prove submission returns the byte count without opening the transport gate;
4. prove the later barrier remains incomplete until the gate opens;
5. release the gate and prove barrier completion.

This directly freezes the semantic difference that the public sync and async adapters rely on.

### Avoid fragile proof techniques

Do not use any of the following as the primary semantic assertion:

- `sleep(0.1)` and “writer thread is still alive”;
- “64 MiB should fill the kernel buffer”;
- loopback TCP socket-buffer assumptions;
- elapsed-time minimums;
- external network throttling;
- peer receipt without a gated completion point.

A short timeout may be used only as a deadlock/test-harness safety bound after deterministic synchronization has established the state being asserted.

### Existing Python closure test

Keep a lightweight Python native-write smoke test if useful, but correct its description so it claims only what it proves.

Acceptable outcomes:

- rename/reword `test_native_write_completes_without_drain` to a smoke/round-trip assertion; or
- replace it with a Python-level contract check that complements, but does not duplicate, the Rust gated proof.

The authoritative synchronous-completion proof must be the deterministic transport-gated test.

---

## Workstream 2 — Reconcile phase acceptance evidence

### General rule

For every unchecked acceptance criterion in the six implemented plans:

1. identify current concrete evidence;
2. add a concise `Closure evidence` section or equivalent evidence mapping to the plan;
3. mark the criterion `[x]` only after the evidence is verified;
4. if evidence is absent or ambiguous, keep `[ ]`, explain the gap, and do not claim the line of work is fully evidenced.

Evidence may be:

- a focused test name/path;
- a source path plus a focused contract test;
- a feature-slice command whose successful run is recorded;
- a documentation path for documentation-only criteria;
- current CI/Python-smoke run evidence for broad gates;
- the implementation commit when the criterion is a structural source fact and focused tests are not meaningful.

Do not use “CI green” alone as evidence for a specific semantic criterion when a focused test exists or is required.

### Phase 1 — Configuration authority convergence

Reconcile all Phase 1 acceptance items against current evidence, including:

- `eggress-config::validate_and_compile_toml` as the single parse/version/validate/compile authority;
- embed/outbound facade error-family preservation;
- reload category/generation/metrics semantics;
- outbound TOML feature topology after direct `toml` dependency removal;
- no constructor/API changes;
- relevant feature slices and workspace gate.

Where an error-family criterion is currently covered only indirectly, cite the exact regression test or add a minimal focused one before checking it.

### Phase 2 — Python lifecycle/error convergence

Map evidence for:

- shared `start()` / `astart()` selection;
- auth-timeout/system-proxy option forwarding equivalence;
- operation exception identity through `AsyncBridge` and `wrap_blocking_call`;
- cancellation/loop-affinity preservation;
- all connection exception names remaining importable;
- runtime class identity/catch hierarchy;
- redaction;
- stubs/docs agreement;
- Python/workspace gates.

Use `python/tests/test_api_boundary_closure.py::TestCompatibilityStartupForwarding` and `TestExceptionIdentityMatrix` where applicable.

### Phase 3 — Async outbound I/O

Do not mark the “sync `OutboundStream` unchanged” / native synchronous-completion portions complete solely from the existing loopback peer-receipt test.

Use the new deterministic gated-write proof for:

- native synchronous completion;
- high-level sync path delegation;
- async queue-only submission distinction;
- `drain()` as ordered completion barrier.

Map the existing focused tests for:

- event-loop schedulability under backpressure;
- ordering;
- retained terminal failure;
- EOF ordering;
- close/wait cleanup;
- write-after-close;
- arbitrary boxed transport support;
- no thread-per-stream design;
- broad gate.

### Phase 4 — Python binding ownership/stubs

Map evidence for:

- every remaining direct `eggress-python` dependency being justified/documented;
- no unnecessary dependency pruning requiring a new facade API;
- runtime/PyO3 exports versus `_eggress.pyi`;
- `exceptions.py` / `exceptions.pyi` identity;
- `__all__` consistency;
- stable `capabilities()` shape/values;
- wheel/import behavior where already covered;
- no new crate/public API introduced.

Do not infer dependency ownership merely from Cargo membership; cite the binding-ownership documentation and actual symbol use where practical.

### Phase 5 — Rust public API qualification

Map evidence for:

- maintained classification in `docs/RUST_API.md`;
- `eggress-embed` config handoff;
- `eggress-outbound` direct/from-chain authority;
- embed outbound re-export compatibility;
- relay/core/routing representative paths;
- required feature slices;
- established `RuntimeConfig` coupling;
- no visibility removal/renaming;
- no mandatory semver/nightly tooling.

Use `crates/eggress-embed/tests/public_api.rs` as the principal representative compile-contract evidence.

### Corrective closure plan

Reconcile its 14 final acceptance criteria after the gated-write proof lands.

The closure record already covers most criteria. Update it only where evidence references change—for example, replace the weak native-write evidence citation with the new deterministic Rust test.

At completion, the corrective plan must contain no unchecked acceptance item unless the plan status is changed away from `IMPLEMENTED` and the unresolved item is explicitly documented.

---

## Workstream 3 — Evidence-document consistency

### Required plan-state outcome

When all criteria are substantiated:

- the five phase plans remain `IMPLEMENTED`;
- the corrective closure plan remains `IMPLEMENTED`;
- all satisfied acceptance boxes are checked;
- each plan contains a compact evidence mapping sufficient for a future reviewer to trace the checkboxes;
- the parent roadmap remains `IMPLEMENTED`;
- this evidence-polish plan becomes `IMPLEMENTED`.

Do not change historical baseline SHAs.

### Canonical roadmap

Update `docs/ROADMAP.md` only enough to state that the API-boundary campaign is implementation-complete and its evidence-state polish is complete.

Do not add a new numbered product milestone for this evidence pass.

### No redundant completion artifact

When this plan closes, update this file in place with:

- implementation/evidence commit;
- deterministic gated-write test path/name;
- number of phase/corrective acceptance criteria reconciled;
- any criteria intentionally left unchecked and why;
- focused verification;
- broad verification.

Do not create a separate “final final closure” document.

---

## Explicit non-goals

This pass must not:

- redesign `WritePump`;
- alter queue capacity or add backpressure configuration;
- change the current unbounded-channel design;
- change `submit_lock` ordering;
- change `AsyncOutboundStream` loop-affinity behavior;
- change exception hierarchy;
- alter startup option semantics;
- add protocols/transports/schedulers;
- modify pproxy parity claims;
- change runtime dependency ownership;
- remove published Rust APIs;
- add API snapshot tooling;
- add benchmarks or benchmark gates;
- change MSRV;
- reopen Eggfetch integration;
- change release automation.

If the stronger write test exposes a real runtime defect, stop this evidence-only pass and write a narrowly scoped corrective implementation plan. Do not silently fix runtime semantics under an evidence-polish commit.

---

## Implementation sequence

1. Add the deterministic gated boxed-stream fixture and synchronous-vs-queue-only write semantic test.
2. Reword or demote the existing loopback native-write test so it no longer overclaims.
3. Run the focused write/Python closure tests.
4. Audit Phase 1 acceptance criteria and add evidence mapping/checkmarks.
5. Repeat for Phases 2–5.
6. Reconcile the corrective closure acceptance criteria and closure record.
7. Run the required feature slices and broad gates.
8. Update this plan and `docs/ROADMAP.md` in place to record evidence closure.

---

## Verification

### Deterministic write evidence

```bash
cargo test -p eggress-python --locked
```

The test name should be explicit enough to identify the semantic contract, for example:

```text
outbound::tests::native_sync_write_waits_for_transport_completion
outbound::tests::async_submit_returns_before_transport_completion
```

Exact names may differ, but there must be one clear synchronous-completion proof and one clear queue-only submission proof.

### Focused Python evidence

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)

.venv/bin/python -m pytest   python/tests/test_api_boundary_closure.py   python/tests/test_asyncio_semantic.py   python/tests/test_outbound_stream_verification.py   python/tests/test_service.py   python/tests/test_errors.py   python/tests/test_public_exports.py   python/tests/test_pproxy_compat.py -q
```

### Feature slices

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

### Broad closure gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
.venv/bin/python -m pytest python/tests tests/compat -q
```

Run additional fuzz/package/OpenSSH gates only if this evidence pass touches code or feature edges covered by those gates. A test-only gated-stream fixture inside `eggress-python` should not require an OpenSSH regression.

---

## Acceptance criteria

This evidence-polish pass is complete only when:

- [ ] a deterministic in-process gated transport proves native synchronous write does not return before transport completion;
- [ ] a complementary deterministic assertion proves the async/private submit path can return before transport completion while its barrier remains pending;
- [ ] no wall-clock or kernel-buffer assumption is the primary proof of those semantics;
- [ ] the existing Python native-write test no longer claims that peer receipt without `drain()` proves synchronous completion;
- [ ] no supported runtime/API behavior changed during the evidence pass;
- [ ] Phase 1 acceptance criteria are individually mapped to evidence and reconciled;
- [ ] Phase 2 acceptance criteria are individually mapped to evidence and reconciled;
- [ ] Phase 3 acceptance criteria are individually mapped to evidence and reconciled using the new gated-write proof where required;
- [ ] Phase 4 acceptance criteria are individually mapped to evidence and reconciled;
- [ ] Phase 5 acceptance criteria are individually mapped to evidence and reconciled;
- [ ] corrective-closure acceptance criteria are individually mapped to evidence and reconciled;
- [ ] no implemented plan remains with unexplained unchecked acceptance criteria;
- [ ] `docs/ROADMAP.md`, parent roadmap, phase plans, corrective plan, and this plan describe the same closure state;
- [ ] focused Python/PyO3 tests pass;
- [ ] required feature slices pass;
- [ ] formatting, clippy, workspace tests, and full Python/compat suite pass.

## Closure record

Fill this section in place during implementation.

- Evidence-polish commit:
- Deterministic sync-write test:
- Deterministic async-submit test:
- Existing loopback test disposition:
- Phase 1 criteria reconciled:
- Phase 2 criteria reconciled:
- Phase 3 criteria reconciled:
- Phase 4 criteria reconciled:
- Phase 5 criteria reconciled:
- Corrective criteria reconciled:
- Focused verification:
- Broad verification:
- Intentionally unresolved evidence:
