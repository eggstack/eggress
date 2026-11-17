# Routing, Health, Metrics, and Admin Roadmap

Status: closed — M003 CIDR/admin exposure corrective implemented and verified 2026-11-16

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

A 2026-11-16 security audit found that `destination_cidr`/`source_cidr` matching passes raw `IpAddr` values to `ipnet`, so IPv4 CIDRs do not match IPv4-mapped IPv6 literals, and the docs present CIDR rules as a complete private-network ACL even though domains never match. The admin HTTP surface also has no `Host`/`Origin` validation, so an unauthenticated loopback admin can be read through DNS rebinding; non-loopback admin is plaintext HTTP and the client rejects `https://`. M003 closes the canonical matching gap, makes the literal-only CIDR contract explicit, and adds admin Host/Origin validation plus a plaintext warning.

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

### Milestone 3 — CIDR canonicalization and admin exposure

Class: invariant. Status: closed 2026-11-16 — closure record `plans/closure/routing-health-observability/003-status.md` (coordinator workspace-wide gate passed).

Implementation plan:

- `plans/implementation/routing-health-observability/003-cidr-canonicalization-and-admin-exposure.md`

Objective: canonicalize IPv4-mapped IPv6 before CIDR matching; add a config warning and docs for the literal-only/domain limitation; add `[admin] allowed_hosts` plus Host/Origin validation for DNS-rebinding defense; warn on plaintext non-loopback admin.

Exit: `::ffff:10.0.0.1` matches `10.0.0.0/8` in destination/source matchers; CIDR rules remain synchronous and literal-only with a warning/docs; admin rejects unexpected Host/Origin while default loopback clients still work; non-loopback admin warns; existing route/admin/auth/limiter tests remain green. No release/tag.

## 8. Cross-cutting requirements

Config: rule/group/health TOML validation; M003 adds `admin.allowed_hosts`. Compat: translator-generated `[[rules]]`/`[health]` entries diagnosed. Security: bounded metric cardinality, redaction, canonical CIDR matching, admin Host/Origin validation, plaintext-admin warning. Concurrency: ArcSwap snapshot reads; no DNS or blocking work added to routing. Docs: `architecture/routing.md`, `architecture/metrics.md`, `architecture/admin.md`.

## 9. Verification strategy

`cargo test -p eggress-routing`, `cargo bench --bench route_match` (informational), admin/health integration tests. M003 additionally requires mapped-IPv6 CIDR tests, CIDR-literal-only warning tests, admin Host/Origin/content-type tests, and the plaintext-admin warning.

## 10. Risks and decision points

CIDR remains literal-IP-only by contract; do not add DNS resolution to route evaluation. Domain private-egress protection belongs to the resolved-target guard (protocols-edge) and upstream trust policy. Admin TLS is not implemented in M003; remote admin remains loopback/VPN/TLS-terminator only, with `allowed_hosts` required for reverse proxies. Default loopback admin access must keep working without configuration.

## 11. Completion definition

Routing/observability plane closed with health-aware selection and ordered lifecycle, plus M003 CIDR/admin exposure correction closed — attained when both are true.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive phase records | archive phase records | — |
| 2 | closed (historical) | archive corrective records | Git history + suite evidence | — |
| 3 | closed | `plans/implementation/routing-health-observability/003-cidr-canonicalization-and-admin-exposure.md` | `plans/closure/routing-health-observability/003-status.md` | — |
