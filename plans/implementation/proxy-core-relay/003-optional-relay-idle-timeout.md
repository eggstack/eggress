# Proxy Core and Relay M003 — Optional Connection-Wide No-Progress Timeout

Status: conditionally closed — source implementation complete; workspace verification and post-publication handoff remain open

Planning baseline: branch cursor/security-hardening, commit 51b37a0933861796af262b533be230419edc0b31

Source roadmap:

- plans/subsystems/proxy-core-relay-roadmap.md
- plans/000-long-term-specification.md §4.11
- plans/001-terminology-and-domain-model.md §3

Applicable ADRs:

- Accepted: plans/adrs/ADR-0003-relay-idle-timeout-semantics.md

Primary class: capability, implemented through relay infrastructure

Upstream request: https://github.com/eggstack/eggress/issues/10

## 1. Objective

Add an additive, opt-in, connection-wide no-progress timeout to the generic eggress-relay API. It must close a stalled relay without moving the bidirectional copy loop into consumers, preserve directional byte accounting and half-close behavior, and remain disabled for all existing calls.

ADR-0003 was accepted before implementation. The user explicitly authorized implementation in this conversation. Source work and focused validation are complete; release publication and downstream issue creation remain separate operational actions and were not performed.

## 2. Why this milestone exists

- Issue #10 identified a concrete consumer need and left the per-direction/per-connection semantic open; accepted ADR-0003 resolves it.
- The current relay owns both streams in one future and already tracks successful directional byte counts.
- The current DrainFor deadline begins only after EOF and is not an idle timer.
- The relay crate is a published primary Rust API; a source-compatible additive entry point is preferable to extending public struct fields or exhaustive enums.
- The repository's direct published dependency list and inspected consumer call sites are known. Downstream issue creation can be sequenced after a releasable API exists.

### Readiness gates

Historical implementation gates:

1. **Complete:** ADR-0003 was accepted, including the additive API/result shape, zero-duration behavior, and exact-deadline precedence.
2. **Complete:** explicit implementation authorization was received from the user in this conversation.
3. **Complete:** the current source baseline was rechecked. The pre-existing modification in `crates/eggress-cli/tests/pproxy_binary.rs` was preserved and remains outside this change.

## 3. Pre-implementation baseline evidence

- RelayOptions contains public buffer_size and half_close fields: crates/eggress-relay/src/lib.rs:62-69.
- RelayFuture owns both streams, directional states, and the optional drain deadline: crates/eggress-relay/src/lib.rs:352-380.
- relay() and relay_with_options() are the public entry points: crates/eggress-relay/src/lib.rs:523-550.
- The relay engine records byte counts only after successful writes and returns them on normal completion, drain expiry, and I/O failure: architecture/relay.md, “How it works” and “Error & failure model.”
- The legacy core facade sets its own bounded drain and maps rich termination into its historical result: crates/eggress-core/src/relay.rs:44-60.
- The maintained embed public-API test exercises relay type paths: crates/eggress-embed/tests/public_api.rs:49-59.
- Registry evidence lists four direct published dependents: eggress-core and eggress-embed in this workspace, plus eggchaos-server and eggtunnel. Eggchaos has a direct RelayOptions literal and report handling; Eggtunnel has server and client relay calls and exact-pins eggress-relay 1.0.8.

## 4. Invariants that must not regress

- All data movement remains in the eggress-relay engine; no consumer duplicates the copy loop.
- Existing relay() and relay_with_options() retain current result types and no-timeout behavior.
- Absence of an idle timeout never closes a quiet connection.
- The timer is connection-wide and resets only after bytes are successfully forwarded in either direction.
- EOF and half-close are not idle activity. Idle timing remains active while the opposite direction drains.
- DrainFor continues to bound only post-half-close draining; it is not replaced, restarted, or reinterpreted as an idle timeout.
- Every terminal outcome preserves accurate bytes_upstream and bytes_downstream.
- Timeout completion drops both streams in the relay future; no detached timer/task survives cancellation.
- No Eggress server/core default, listener topology, protocol behavior, pproxy claim, or feature flag changes as part of this milestone.

## 5. Scope

### In scope

- One additive opt-in API and a distinct result/outcome type, per accepted ADR-0003.
- One idle deadline integrated into RelayFuture and reset by forwarded bytes in either direction.
- Tests, crate README/rustdoc, architecture/relay.md, and docs/RUST_API.md updates.
- A release handoff note and linked downstream issue creation after the API is published.

### Explicitly out of scope

- Adding a public field to RelayOptions or a variant to RelayTermination without a separately accepted breaking-API decision.
- Enabling timeouts in relay(), relay_with_options(), eggress-core, eggress-server, or Eggress runtime defaults.
- Eggchaos or Eggtunnel source/config changes in this repository.
- Per-direction timeouts, TCP keepalive, protocol-specific heartbeat detection, a global timer service, detached tasks, metrics/logging policy, or a buffer/copy-loop rewrite.
- Version bump, tag, crate publication, or release authorization.

## 6. Required production changes

### Generic relay API and engine

- Keep RelayOptions, RelayReport, RelayTermination, relay(), and relay_with_options() source-compatible.
- Add `relay_with_idle_timeout(client, server, options, idle_timeout: Option<Duration>) -> Result<RelayIdleTimeoutOutcome, RelayFailure>` with the same stream bounds as `relay_with_options`. The outcome is `Completed(RelayReport)` or `IdleTimedOut { bytes_upstream: u64, bytes_downstream: u64 }`; preserve this API shape from accepted ADR-0003.
- Route old and new entry points through one internal relay future/state machine. Old entry points supply no idle deadline.
- Start the optional deadline on the first poll of the opted-in relay future, not when an unpolled future is merely constructed. Reset it only when a destination write successfully forwards one or more bytes; do not reset on read readiness, bytes merely buffered, EOF, shutdown, or timer wakeup.
- Keep the drain deadline and idle deadline independent. On any poll where the idle deadline is due, IdleTimedOut takes precedence over stream progress or I/O observed on that poll. If idle and DrainFor deadlines are both due on the same poll, preserve existing behavior by returning DrainTimedOut. Cover both rules with deterministic tests.
- Preserve the existing fair polling, write/shutdown handling, I/O failure source/direction, byte totals, and drop/cancellation behavior.
- `None` disables idle timing. `Some(Duration::ZERO)` expires on the future's first poll, before either stream is polled or any bytes are forwarded; document and test this behavior.

### Compatibility facade and consumers

- Do not enable the new timeout in eggress-core's legacy relay facade. Add only compatibility coverage if the new public API could affect its compilation or semantics.
- Do not add Eggress-side config: configuration belongs to the consumer that opts in.
- Add public-API compile coverage proving existing RelayOptions struct literals and existing RelayTermination matches remain valid.

## 7. Ordered work packages

### A — Accept and register the API decision (complete)

Intent: close the public contract before implementation.

Completed: ADR-0003 was accepted on 2026-11-18. It fixes the additive entry point and outcome shape, connection-wide reset semantics, zero-duration behavior, and idle/drain/progress deadline precedence. The ADR is linked from this plan and the subsystem roadmap.

Acceptance evidence: `plans/adrs/ADR-0003-relay-idle-timeout-semantics.md` (accepted). No code work is authorized by completing this work package.

### B — Implement the relay timer and result (complete)

Intent: integrate the optional deadline without changing existing callers.

Completed: added the API/outcome, shared the `RelayFuture`, reset on successful writes in either direction, preserved byte counts and half-close behavior, and retained legacy drain polling order. No detached tasks were introduced.

Acceptance evidence: 23 relay unit tests and 2 relay doctests pass; targeted relay Clippy passes; focused core and embed API tests pass.

### C — Document and qualify (conditionally complete)

Intent: make the contract usable by direct consumers.

Completed: updated crate rustdoc/README, `architecture/relay.md`, `docs/RUST_API.md`, and embed public-API compatibility coverage.

Acceptance evidence: focused checks and the workspace suite with `direct_connect_falls_back_to_next_resolved_address` filtered out pass. The unfiltered workspace test command timed out in that core connector test; workspace Clippy is blocked by `ssh` cfg warnings in `benches/tls_setup.rs`, and workspace formatting reports a pre-existing difference in `crates/eggress-cli/tests/pproxy_binary.rs`; see the closure record.

### D — Downstream issue handoff after publication

Intent: track consumer adoption without importing downstream code into this change.

Required actions after a compatible Eggress release is published:

1. Open a linked issue in eggstack/eggchaos for an optional runtime-config setting using the new relay API. Include absent/disabled default, validation, propagation through its runtime configuration/builder, both-byte-count outcome reporting, and deterministic tests. Eggchaos is the definite feature consumer identified by issue #10.
2. Open a linked issue in eggstack/eggtunnel to update its exact eggress-relay = 1.0.8 pin to the published version and decide whether to expose the new timeout in both its server and client relay paths. Preserve relay_drain as separate half-close behavior; do not silently enable the timeout. If maintainers decline the feature, record that no adoption is required and close the issue accordingly.
3. Put the resulting downstream issue URLs and the published version in the M003 closure record. Do not claim downstream adoption until those repositories complete their work.

These issues were not created during this source implementation; they remain post-publication follow-ups. eggress-core and eggress-embed are in this workspace and do not need separate downstream repository issues.

## 8. Failure, cancellation, and deadline semantics

- A successful idle timeout is a normal terminal outcome for the new API, not an I/O RelayFailure.
- It returns the last committed directional byte counts and drops both streams.
- A directional read/write/shutdown error remains a RelayFailure with its existing source, direction, and byte counts.
- A half-close drain expiry remains DrainTimedOut in the existing RelayReport when it wins; the new idle outcome is distinct when the idle deadline wins.
- Dropping the caller's relay future must release both streams and the timer without spawning or detaching work.
- At or after a due idle deadline, IdleTimedOut wins over stream I/O/progress observed on that poll; a successful write at the deadline cannot rescue the relay. If idle and DrainFor are both due on the same poll, DrainTimedOut wins. Pin both rules with deterministic tests.

## 9. Compatibility and migration

The intended API change is additive. Existing callers do not need a source migration, and old entry points remain disabled by default. The existing Eggress compatibility facade keeps its 64 KiB buffer and one-second DrainFor setting. The API's release/version publication is separate from source closure and follows normal maintainer authorization.

The downstream issue handoff must distinguish:

- Eggchaos: optional feature adoption and config propagation; its ^1.0.12 requirement can resolve a compatible release, subject to its lockfile and API use.
- Eggtunnel: exact pin update is required only to compile/use the new API; the feature remains opt-in and its existing relay_drain remains unchanged.

## 10. Required tests

### Focused relay tests

- New API with no idle duration remains disabled and completes a quiet/slow-response case.
- No forwarded bytes until the deadline produces IdleTimedOut with exact zero/current counters and releases both stream endpoints. `Some(Duration::ZERO)` expires before polling either stream and forwards zero bytes.
- Successful forwarded bytes in the client-to-server direction reset the shared deadline.
- Successful forwarded bytes in the server-to-client direction reset the same shared deadline.
- Buffered input without a successful write does not reset the deadline.
- The timer remains active after one side half-closes; a delayed response within the interval is delivered, while silence beyond it times out.
- DrainFor expiry and idle expiry remain distinguishable; idle expiry wins over same-poll progress at/after its deadline, while DrainTimedOut wins an exact tie between the two deadlines.
- Existing bidirectional transfer, slow half-close response, bounded drain, I/O failure, tiny buffer, and future-drop tests remain passing.

Prefer deterministic Tokio-time tests. The current workspace Tokio feature list omits test-util; add it only as a test/dev feature if needed, not to the relay's runtime dependency surface.

### API and integration coverage

- Public-API compile test for old struct literals and old exhaustive enum matches.
- New API's result type is accessible through the intended published crate path.
- No Eggress server/default behavior or compatibility facade timeout changes.

## 11. Required verification commands

Narrow tests first:

- cargo test -p eggress-relay
- cargo test -p eggress-core relay
- cargo test -p eggress-embed --test public_api

Before merge:

- cargo fmt --all -- --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace --locked

Do not run external pproxy/interoperability suites unless a compatibility claim changes; this plan explicitly forbids such a change.

## 12. Documentation updates

- crates/eggress-relay/README.md and public rustdoc examples
- architecture/relay.md
- docs/RUST_API.md
- plans/closure/proxy-core-relay/003-status.md after implementation/verification
- docs/ROADMAP.md and plans/registry.md only to reconcile actual status and release/issue handoff

## 13. Acceptance criteria

M003 may close only when:

- An additive opt-in API exists and old APIs preserve disabled-by-default behavior.
- Connection-wide activity, reset conditions, first-poll timer start, half-close interaction, due-deadline precedence (including the DrainFor tie), zero-duration behavior, and outcome reporting are documented.
- Idle expiry is distinct from drain expiry and preserves byte counts.
- All focused tests and required Rust gates pass.
- Public API qualification and architecture docs are updated.
- After publication, the Eggchaos issue and Eggtunnel adoption issue (or documented no-action disposition) are linked in closure evidence.
- Release publication and downstream implementation status are stated separately from source-level completion.

## 14. Stop conditions

Stop and report for maintainer review if:

- The selected design requires changing existing RelayOptions fields, RelayTermination variants, signatures, or Eggress defaults.
- The shared timer cannot preserve fair bidirectional polling, exact byte counts, or stream-drop/cancellation semantics.
- A consumer asks to reinterpret relay_drain as idle timeout or to enable the feature by default.
- Supporting the API requires listener, protocol, server-config, parity, or release scope beyond this milestone.
- An external consumer's actual source/manifest contradicts the recorded dependency evidence.

## 15. Closure evidence required

Record baseline/final commits; accepted ADR; before/after API contract; timeout/half-close/cancellation tests; exact focused and broad command outcomes; public API and documentation updates; any residual limitations; release status; and downstream issue URLs/dispositions. Do not state that downstream repos adopted the feature unless their changes are verified.

## 16. Handoff notes

Source implementation is complete and recorded in `plans/closure/proxy-core-relay/003-status.md`. Preserve the pre-existing modification in `crates/eggress-cli/tests/pproxy_binary.rs` from baseline `51b37a0`. No release was published and no downstream issues were created. After a compatible release is explicitly authorized and published, complete the Eggchaos/Eggtunnel handoff described in Work Package D; do not claim downstream adoption until verified.
