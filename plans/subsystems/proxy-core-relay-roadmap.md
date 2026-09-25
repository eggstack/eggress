# Proxy Core and Relay Roadmap

Status: closed

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariants 1–2, 6, 10)
- `plans/001-terminology-and-domain-model.md` §2–§3 (BoxStream, Relay, Session)
- `plans/002-long-term-roadmap.md` (Phase 1)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

Owns the leaf byte-relay engine (`eggress-relay`), the legacy core compatibility facade, stream/replay/dispatch primitives, and relay accounting. Consumes accepted sessions from the server and established upstream streams from outbound chains. MUST NOT own listener accept logic, routing decisions, or chain composition.

Architecture: `architecture/relay.md`, `architecture/core.md`.

## 2. Work classification

### Invariants

- Boxed byte streams at protocol/transport boundaries; no generic stream-type leakage.
- 64 KiB relay buffers and bounded post-half-close drain semantics (compat facade; do not retune without evidence).

### Capabilities

- Bidirectional TCP relay with byte counts feeding `SessionReport`/`SessionMetrics`.

### Infrastructure

- `ReplayStream`, `ProtocolDispatcher`, relay helpers, benches (`tcp_relay`).

### Polish

- Relay benchmarks and documentation polish (informational, never a CI threshold).

## 3. Non-goals

- Listener accept/sniff policy (server subsystem); route selection (routing subsystem); upstream hop execution (outbound subsystem).

## 4. Current state

Relay extraction and API stabilization complete (`REUSABLE_RELAY_EXTRACTION_AND_API_STABILIZATION.md` in archive). 1.0.10 line qualified; no active relay blocker.

## 5. Target architecture

All data-plane byte movement flows through the boxed relay boundary with uniform accounting — attained.

## 6. Dependency graph

```text
Stream/relay primitives (hard)
    `--> Reusable relay extraction (hard)
             `--> 1.0.10 qualification (operational)
```

## 7. Milestones

### Milestone 1 — Core stream and relay

Class: infrastructure. Objective: stream/relay primitives + replay/dispatch. Exit: Phase 1 relay path operational. Status: closed.

### Milestone 2 — Reusable relay extraction and API stabilization

Class: infrastructure. Objective: `eggress-relay` leaf engine with stable facade. Exit: extraction landed with benches green. Status: closed. Evidence: archive record + Git history.

## 8. Cross-cutting requirements

Config/migration: none (no config surface). Protocol/compat: relay semantics are part of the behavior baseline; changes require differential evidence. Security: bounded buffers, no secret logging. Concurrency: half-close drain bounded. Observability: byte counts into session metrics. Performance: benches informational. Docs: `architecture/relay.md`, `architecture/core.md`.

## 9. Verification strategy

`cargo test -p eggress-relay`, `cargo bench --bench tcp_relay` (informational), workspace suite for broad changes.

## 10. Risks and decision points

None active. Buffer-size or pooling changes require measured evidence and an ADR-level decision, not a routine milestone.

## 11. Completion definition

Relay engine stable behind the boxed boundary with qualified 1.0.10 evidence — attained.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive phase records | archive phase records | — |
| 2 | closed (historical) | archive `REUSABLE_RELAY_EXTRACTION_AND_API_STABILIZATION.md` | Git history + suite evidence | — |
