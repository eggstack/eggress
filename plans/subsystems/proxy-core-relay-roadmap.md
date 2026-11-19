# Proxy Core and Relay Roadmap

Status: conditionally closed — source implementation landed; named verification and post-publication evidence remain

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariants 1–2, 6, 10)
- `plans/001-terminology-and-domain-model.md` §2–§3 (BoxStream, Relay, Session)
- `plans/002-long-term-roadmap.md` (Phase 1)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)
- Accepted: `plans/adrs/ADR-0003-relay-idle-timeout-semantics.md`

## 1. Purpose and ownership boundary

Owns the leaf byte-relay engine (`eggress-relay`), the legacy core compatibility facade, stream/replay/dispatch primitives, and relay accounting. Consumes accepted sessions from the server and established upstream streams from outbound chains. MUST NOT own listener accept logic, routing decisions, or chain composition.

Architecture: `architecture/relay.md`, `architecture/core.md`.

## 2. Work classification

### Invariants

- Boxed byte streams at protocol/transport boundaries; no generic stream-type leakage.
- 64 KiB relay buffers and bounded post-half-close drain semantics (compat facade; do not retune without evidence).

### Capabilities

- Bidirectional TCP relay with byte counts feeding `SessionReport`/`SessionMetrics`.
- Optional, connection-wide no-progress timeout for direct generic-relay consumers; disabled for existing calls.

### Infrastructure

- `ReplayStream`, `ProtocolDispatcher`, relay helpers, benches (`tcp_relay`).
- One optional idle deadline inside the existing relay future; no detached timer or second copy loop.

### Polish

- Relay benchmarks and documentation polish (informational, never a CI threshold).

## 3. Non-goals

- Listener accept/sniff policy (server subsystem); route selection (routing subsystem); upstream hop execution (outbound subsystem).

## 4. Current state

Relay extraction and API stabilization are complete, and the 1.0.10 line is qualified. Issue #10 requested an optional no-progress timeout at the generic relay seam. M003 source implementation and focused qualification are complete under accepted ADR-0003; broader verification and post-publication operational handoff remain outstanding.

## 5. Target architecture

All data-plane byte movement flows through the boxed relay boundary with uniform accounting — attained. The accepted opt-in idle deadline remains inside the leaf relay and preserves the existing relay APIs and defaults.

## 6. Dependency graph

```text
Stream/relay primitives (hard; closed)
    `--> Reusable relay extraction (hard; closed)
             `--> 1.0.10 qualification (operational; closed)
                      `--> M003 optional idle-timeout API (conditionally closed; see closure record)
                               `--> published API (release not performed)
                                        `--> Eggchaos / Eggtunnel adoption issues (post-publication)
```

## 7. Milestones

### Milestone 1 — Core stream and relay

Class: infrastructure. Objective: stream/relay primitives + replay/dispatch. Exit: Phase 1 relay path operational. Status: closed.

### Milestone 2 — Reusable relay extraction and API stabilization

Class: infrastructure. Objective: `eggress-relay` leaf engine with stable facade. Exit: extraction landed with benches green. Status: closed. Evidence: archive record + Git history.

### Milestone 3 — Optional connection-wide no-progress timeout

Class: capability (relay infrastructure).

Objective: offer an additive opt-in idle timeout at the generic relay seam without changing existing API behavior or Eggress defaults.

Dependencies: ADR-0003 is accepted and implementation is complete. Workspace-wide verification is incomplete; publication and downstream adoption remain separate post-publication operational follow-ups.

Deliverable boundary: one shared relay state machine, one additive opt-in API/outcome, focused tests and docs. No downstream source/config changes in this repository.

User or operator value: direct consumers can terminate a truly stalled relay without reimplementing bidirectional copying.

Exit conditions: timeout is connection-wide, starts on the first poll, resets only by successfully forwarded bytes in either direction, and remains distinct from DrainFor. Existing calls stay disabled. Some(Duration::ZERO) expires before stream polling; a due idle deadline beats same-poll progress, while DrainTimedOut wins if both deadlines are due together. Every outcome reports exact directional byte counts.

Deferred work: Eggchaos configuration and Eggtunnel dependency/API adoption are tracked by linked downstream issues after publication.

## 8. Cross-cutting requirements

Config/migration: the leaf relay owns no user config; consumers own optional timeout settings. Protocol/compat: no pproxy claim changes; existing relay behavior stays disabled by default. Security: bounded buffers, no secret logging. Concurrency: preserve fair polling, half-close, cancellation, and independent drain/idle deadlines. Observability: retain exact byte counts and a distinct idle outcome. Performance: avoid a second copy loop or detached timer; benches are informational. Docs: `architecture/relay.md`, `architecture/core.md`, `docs/RUST_API.md`.

## 9. Verification strategy

`cargo test -p eggress-relay`, `cargo test -p eggress-core relay`, public API compile coverage, `cargo fmt --all -- --check`, workspace Clippy and locked tests for the substantial public-crate change. Add deterministic timeout/half-close/cancellation tests; benchmark only if evidence indicates material overhead.

## 10. Risks and decision points

Accepted decision: use a new entry point and outcome type rather than changing RelayOptions or RelayTermination, because those are published public contracts. ADR-0003 fixes zero-duration behavior as immediate expiry before stream polling; at/after a due idle deadline, timeout wins over same-poll progress, and DrainTimedOut wins a tie when both deadlines are due. Implementation is complete; no release or downstream issue was created. Workspace-wide tests and Clippy remain incomplete for the documented reasons in the closure record.

## 11. Completion definition

Relay engine stable behind the boxed boundary with qualified 1.0.10 evidence — attained. M003 source implementation is conditionally closed with a closure record; release publication and linked downstream issue handoff remain post-publication operations.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive phase records | archive phase records | — |
| 2 | closed (historical) | archive `REUSABLE_RELAY_EXTRACTION_AND_API_STABILIZATION.md` | Git history + suite evidence | — |
| 3 | conditionally closed | `plans/implementation/proxy-core-relay/003-optional-relay-idle-timeout.md` | `plans/closure/proxy-core-relay/003-status.md` | Workspace verification; post-publication downstream issue handoff |
