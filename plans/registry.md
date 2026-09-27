# Eggress Active Planning Registry

This file is the compact control surface for active interim planning. Detailed requirements and completed history remain in source roadmaps, implementation plans, `plans/closure/`, `plans/archive/phase-records/`, and Git history.

Canonical direction remains in:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`
- `plans/003-planning-process.md`

Complementary authority: `docs/ROADMAP.md` (canonical roadmap), `docs/parity/pproxy_capability_manifest.toml` + `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` (compat contract), `docs/CI_STATUS.md` (verification policy), `docs/TESTING.md` (suite inventory).

## Status vocabulary

- **proposed** — roadmap or plan exists but is not approved for execution.
- **ready** — dependencies and interfaces are satisfied; plan may be handed off.
- **active** — implementation or closure work is in progress.
- **blocked** — a named dependency or evidence requirement prevents progress.
- **closing** — implementation landed and closure evidence is being gathered.
- **closed** — closure record accepted.
- **conditionally closed** — substantial work landed, but a named correctness or operational evidence condition remains.
- **superseded** — replaced by another document.
- **archived** — no longer active and retained for traceability.

## Active subsystem roadmaps

| Subsystem | Status | Roadmap | Current milestone | Dependencies or blockers |
|---|---|---|---|---|
| Proxy core and relay | closed | `plans/subsystems/proxy-core-relay-roadmap.md` | All milestones closed; 1.0.10 H2/pool correctives qualified | None. Historical evidence in `plans/archive/phase-records/`. |
| Server, runtime lifecycle, and configuration | closed | `plans/subsystems/server-runtime-config-roadmap.md` | All milestones closed | None. |
| Routing, health, metrics, and admin | closed | `plans/subsystems/routing-health-observability-roadmap.md` | All milestones closed | None. |
| Edge protocols | closed | `plans/subsystems/protocols-edge-roadmap.md` | All milestones closed | None. |
| Transports | closed | `plans/subsystems/transports-roadmap.md` | All milestones closed; SSH/QUIC remain feature-gated by design | None. |
| Outbound chains and connectors | closed | `plans/subsystems/outbound-chains-roadmap.md` | All milestones closed; pooled-transport/H2/ALPN correctives qualified | None. |
| Parity contract and compatibility translation | closed | `plans/subsystems/parity-compat-roadmap.md` | All milestones closed; manifest has no unresolved `gap` outside tiered boundaries | None. |
| Delivery (CLI, embed, Python, release) | active | `plans/subsystems/delivery-roadmap.md` | M003 Eggup archive/pair self-update adoption | Blocked on compatible versioned `eggup-core` + `eggup-archive` registry packages; 1.0.10 publication remains separately maintainer-authorized. |

## Dependency-ready implementation plans

None. Delivery M003 is fully planned but blocked on its external package dependency; it MUST NOT be handed off for final dependency cutover until the Eggup registry-package gate closes.

## Current execution order and dependency gates

**1.0.10 qualification gate:** the 1.0.10 runtime/TLS implementation, package dry-run, and CI are qualified (physical H2 isolation proven by handshake counts with shared-registry control at `c5826b3`; Rust CI `36032622797` success). The workspace is prepared but unpublished. No `v1.0.10` tag or publication is authorized by planning records.

**Compat-contract gate:** claims remain governed by the manifest plus matrix. Changing a claim requires a manifest update and the oracle/differential/interop suite. Generated reports follow the manifest.

**Verification gate:** `docs/CI_STATUS.md` owns verification policy; `docs/TESTING.md` owns the suite inventory. Ordinary changes use the narrowest test first, then the broad gate before merging substantial Rust changes.

**Eggup updater-convergence gate:** Delivery M003 is registered at `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md`. Eggup Archive M001d is runtime-qualified (`eggstack/eggup@18d83de`, hosted run `36335233644`), but the required post-M001d `eggup-core` + `eggup-archive` APIs are not yet available as a compatible registry package pair. Qualification may use an immutable upstream revision; final Cargo dependency cutover/package closure requires registry versions. Eggress release/checksum/version/CLI policy remains local.

## Blocked work

| Subsystem | Milestone | Blocker | Plan |
|---|---|---|---|
| Delivery | M003 Eggup archive/pair self-update adoption | compatible versioned `eggup-core` + `eggup-archive` registry packages containing M001d APIs | `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md` |

## Closure work and current control points

| Subsystem | Status | Controlling evidence |
|---|---|---|
| 1.0.10 H2/pool/ALPN qualification | closed, prepared, unpublished | `plans/archive/phase-records/H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md`, `ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md`, `H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md`, `POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`; implementation `c5826b3`; Rust CI `36032622797` |
| Distribution/release maintenance | closed | `plans/archive/phase-records/DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md`, `PYPI_WHEEL_MATRIX_EXPANSION.md`, `MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md` |
| Eggfetch 0.2 consolidation | closed | `plans/archive/phase-records/EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`, `EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`, `EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md` |
| Pre-transition flat index | archived | `plans/archive/phase-records-README-legacy.md` (the former `plans/README.md`) |
