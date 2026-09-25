# Architecture Decision Records

This directory contains durable decisions that affect Eggress architecture across milestones or subsystems.

Pre-existing product/compat decision records live under `docs/adr/` (e.g. SSH upstream parity, QUIC/H3 parity, SSR compatibility) in their original `ADR_<name>.md` convention and stay where they are. New cross-cutting process or architecture decisions that govern multiple subsystems or milestones go here under the `ADR-NNNN` convention below.

Use an ADR when a question cannot be answered safely inside one implementation plan without establishing a reusable architectural contract.

## Naming

```text
ADR-NNNN-short-title.md
```

Numbers are monotonically increasing and never reused.

## Status lifecycle

```text
proposed -> accepted -> deprecated or superseded
         `-> rejected
```

Accepted ADRs are historical records. Do not rewrite an accepted ADR to make a later decision appear original. Create a new ADR and mark the old one superseded.

## ADR template

```markdown
# ADR-NNNN: Title

Status: proposed

Date: YYYY-MM-DD

Decision owners: project maintainers

Related specification sections:

- `plans/000-long-term-specification.md#...`
- `plans/001-terminology-and-domain-model.md#...`

Affected subsystem roadmaps:

- `plans/subsystems/...`

## Context

Describe the architectural problem, existing implementation, constraints, and why the decision is required now.

## Decision drivers

- ...

## Considered options

### Option A — Name

Description, benefits, costs, and failure modes.

### Option B — Name

Description, benefits, costs, and failure modes.

## Decision

State the selected option precisely, including ownership and interface boundaries.

## Consequences

### Positive

- ...

### Negative

- ...

### Neutral or deferred

- ...

## Compatibility and migration

Describe config, protocol, API, and operational migration requirements.

## Security and reliability implications

Describe authorization, secret handling, contention, cancellation, restart, recovery, and denial-of-service effects.

## Verification

Describe the evidence required to prove implementations conform to this decision.

## Supersession

None.
```

## ADR threshold

An ADR is normally required when a decision:

- changes a listener/routing/chain/relay ownership boundary;
- introduces a new listener protocol, transport, or chain-hop contract;
- selects a durable external standard or dependency;
- changes reload, shutdown, or health semantics;
- changes transport-reuse or trust-policy scoping;
- establishes a public compatibility contract or tier semantic;
- materially changes a long-term non-goal.

An ADR is usually unnecessary for local refactors, internal naming cleanup, implementation-specific data structures, or reversible optimizations that preserve established contracts.
