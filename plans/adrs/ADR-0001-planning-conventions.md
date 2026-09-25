# ADR-0001: Adopt codegg-style planning conventions

Status: accepted

Date: 2026-09-25

Decision owners: project maintainers

Related specification sections:

- `plans/000-long-term-specification.md` §6
- `plans/002-long-term-roadmap.md` (Next — no active phase)
- `plans/003-planning-process.md` (all sections)

Affected subsystem roadmaps:

- All roadmaps under `plans/subsystems/` (process-only; no runtime scope)

## Context

Eggress planning lived as ~125 flat `plans/*.md` phase/corrective/closure records with no ADRs, subsystem roadmaps, closure separation, registry, status vocabulary, or work classification. `docs/ROADMAP.md` was canonical but mixed completed history with active state, and `plans/README.md` carried a legacy active-handoff section. The `codegg` repo (`/home/sugarwookie/projects/codegg`, `plans/000`-`003` + `registry.md` + `adrs/` + `subsystems/` + `implementation/` + `closure/` + `archive/`) provides a proven convention separating durable direction from bounded execution handoffs with evidence-gated closure.

## Decision drivers

- Active work must be distinguishable from historical provenance at a glance.
- Corrective passes must be new plans referencing prior closure, not silent rewrites.
- Compat-claim changes must stay manifest-governed with oracle/differential/interop evidence.
- Migration must preserve all existing phase records for traceability.

## Considered options

### Option A — Adopt the codegg planning hierarchy verbatim in structure, Eggress-specific in content

New canonical `000`–`003`, `registry.md`, `adrs/`/`subsystems/`/`implementation/`/`closure/`/`archive/` with Eggress invariants, terminology, and roadmap content; move flat records to `archive/phase-records/` preserving filenames.

### Option B — Keep flat plans/ and add only a registry index

Cheaper, but leaves corrective-vs-roadmap-vs-closure conflation in place and provides no ADR/closure templates or classification discipline.

## Decision

Adopt Option A. Structure follows codegg (`000-long-term-specification`, `001-terminology-and-domain-model`, `002-long-term-roadmap`, `003-planning-process`, `registry.md`, five directories, status vocabulary, invariant/capability/infrastructure/polish classification, hard/interface/soft/operational dependencies, corrective-as-new-plan, closure-evidence gate). All domain content is Eggress-specific and derived from `architecture/overview.md`, `docs/ROADMAP.md`, the parity manifest/matrix, and `AGENTS.md` invariants — no codegg domain requirements are imported.

An implementation handoff is active only when registered in both `plans/registry.md` and `docs/ROADMAP.md`.

## Consequences

### Positive

- Compact control surface (`registry.md`) for active work; history stays reachable in `archive/phase-records/`.
- Bounded, reviewable handoffs with explicit closure evidence.
- Compat and verification authorities stay single-sourced (manifest/matrix, `CI_STATUS.md`, `TESTING.md`).

### Negative

- One-time move of ~125 files (mechanical, history-preserving via `git mv`).

### Neutral or deferred

- No runtime, API, capability, parity, or compatibility behavior changes. No 1.0.10 publication/tagging authorized.

## Compatibility and migration

All pre-transition records moved to `plans/archive/phase-records/` with original filenames; the former `plans/README.md` is preserved at `plans/archive/phase-records-README-legacy.md`. Subsystem roadmaps reference archive entries rather than duplicating them. `docs/ROADMAP.md` and `AGENTS.md` receive minimal pointer updates only.

## Security and reliability implications

None — planning-only change. Credential-redaction and fail-closed rules are restated as invariants, not altered.

## Verification

- `plans/` root contains only `README.md`, `000`–`003`, `registry.md`, and the five directories.
- `plans/archive/phase-records/` holds the moved records.
- `plans/registry.md` and `docs/ROADMAP.md` agree: no active handoff.
- Narrow verification first (structure check), broad suite only if later Rust changes warrant it.

## Supersession

None.
