# Routing, Health, Metrics, and Admin Roadmap

Status: closed

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariants 4–5)
- `plans/001-terminology-and-domain-model.md` §2–§3 (Route, Upstream, Group, Scheduler)
- `plans/002-long-term-roadmap.md` (Phase 2)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

Owns the routing rule engine (`eggress-routing`), upstream groups/schedulers, health management, Prometheus metrics/JSON logging (`eggress-metrics`), and the admin API/PAC/static surface (`eggress-admin`). Consumed by the server and outbound paths via the shared snapshot. MUST NOT own listener accept or chain-hop execution.

Architecture: `architecture/routing.md`, `architecture/metrics.md`, `architecture/admin.md`.

## 2. Work classification

### Invariants

- First-match-wins routing with explanation; health-aware eligibility; `SessionMetrics` recorded exactly once.

### Capabilities

- Rule/group/scheduler/health behavior, admin endpoints, PAC serving, `upstream test`.

### Infrastructure

- Route compiler, health probes with hysteresis, metrics registry with bounded cardinality.

### Polish

- Human/JSON explanation output, dashboard polish.

## 3. Non-goals

- Protocol framing and transport upgrades (other subsystems).

## 4. Current state

All Phase 2 routing/health/operations milestones plus corrective integration tests complete. No active blocker.

## 5. Target architecture

Snapshot-shared routing/observability plane with atomic state swaps — attained.

## 6. Dependency graph

```text
Rule engine + groups/schedulers (hard)
    `--> Health management (hard)
             `--> Metrics/admin/PAC (soft)
                      `--> Reload integration (interface: snapshot)
```

## 7. Milestones

### Milestone 1 — Routing, health, operations

Class: capability. Objective: full Phase 2 scope (rules through reload). Exit: §Completed Milestones Phase 2 items checked. Status: closed.

### Milestone 2 — Corrective integration hardening

Class: polish. Objective: end-to-end corrective tests (2.11–2.20). Exit: integration evidence green. Status: closed. Evidence: archive phase records + Git history.

## 8. Cross-cutting requirements

Config: rule/group/health TOML validation. Compat: translator-generated `[[rules]]`/`[health]` entries diagnosed. Security: bounded metric cardinality, redaction. Concurrency: ArcSwap snapshot reads. Docs: `architecture/routing.md`, `architecture/metrics.md`, `architecture/admin.md`.

## 9. Verification strategy

`cargo test -p eggress-routing`, `cargo bench --bench route_match` (informational), admin/health integration tests.

## 10. Risks and decision points

None active.

## 11. Completion definition

Routing/observability plane closed with health-aware selection and ordered lifecycle — attained.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive phase records | archive phase records | — |
| 2 | closed (historical) | archive corrective records | Git history + suite evidence | — |
