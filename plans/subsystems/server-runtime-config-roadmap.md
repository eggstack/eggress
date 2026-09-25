# Server, Runtime Lifecycle, and Configuration Roadmap

Status: closed

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariants 4–5)
- `plans/001-terminology-and-domain-model.md` §2–§3 (Listener, Session, Snapshot)
- `plans/002-long-term-roadmap.md` (Phase 2)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

Owns listener accept (`eggress-server`), the runtime supervisor (`eggress-runtime`: snapshot compilation, reload, signals, shutdown ordering), and TOML configuration (`eggress-config`). Consumes routing decisions and chain execution. MUST NOT own route selection internals or hop execution.

Architecture: `architecture/server.md`, `architecture/runtime.md`, `architecture/config.md`.

## 2. Work classification

### Invariants

- Listener topology is not hot-reloaded; only routing/upstream/group/health state swaps atomically.
- Shutdown order: readiness false → listener stop → UDP drain → connection drain/cancel → admin last.

### Capabilities

- Accept→route→relay session lifecycle with deferred replies, reload via SIGHUP, graceful drain.

### Infrastructure

- `CompiledRuntimeSnapshot` compiler, `RouteService` integration surface, config validation + secret sources.

### Polish

- Route explanation and `upstream test` ergonomics (shared with routing subsystem).

## 3. Non-goals

- Scheduler algorithms and health probes (routing subsystem); relay internals (core subsystem).

## 4. Current state

Runtime supervisor decomposition, lean-runtime phases, and maintenance phases complete. No active lifecycle blocker; 1.0.10 qualified.

## 5. Target architecture

Snapshot-compiled runtime with atomic reload and ordered shutdown — attained.

## 6. Dependency graph

```text
Config authority (hard)
    `--> Runtime supervisor (hard)
             `--> Server session integration (hard)
                      `--> Maintenance decomposition (soft)
```

## 7. Milestones

### Milestone 1 — Server/routing integration and lifecycle

Class: capability. Objective: accept→route→relay with reload/shutdown. Exit: Phase 2 lifecycle operational. Status: closed.

### Milestone 2 — Runtime decomposition and maintenance convergence

Class: infrastructure/polish. Objective: supervisor/outbound decomposition, ownership cleanup. Exit: maintenance phases closed. Status: closed. Evidence: archive `MAINTENANCE_*`, `LEAN_RUNTIME_*`, `ARCHITECTURE_CONVERGENCE_*` records.

## 8. Cross-cutting requirements

Config: validated TOML with secret sources; failed candidates never swap. Compat: CLI-compat config generation stays in the translator. Security: handshake timeouts, redaction. Concurrency: drain/cancel semantics. Observability: JSON logging, readiness. Docs: `architecture/server.md`, `architecture/runtime.md`, `architecture/config.md`.

## 9. Verification strategy

`cargo test -p eggress-runtime retry_fallback` (narrow example), config validation tests, reload/shutdown integration tests, workspace suite for broad changes.

## 10. Risks and decision points

None active. Topology hot-reload would require an ADR; it is currently a non-goal.

## 11. Completion definition

Lifecycle closed with ordered shutdown and atomic reload proven — attained.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive phase records | archive phase records | — |
| 2 | closed (historical) | archive maintenance records | Git history + suite evidence | — |
