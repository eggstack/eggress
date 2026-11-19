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
| Proxy core and relay | conditionally closed | `plans/subsystems/proxy-core-relay-roadmap.md` | M001–M002 closed; M003 source implementation landed | Workspace verification and post-publication downstream handoff remain; see `plans/closure/proxy-core-relay/003-status.md`. |
| Server, runtime lifecycle, and configuration | closed | `plans/subsystems/server-runtime-config-roadmap.md` | All milestones closed | None. |
| Routing, health, metrics, and admin | closed | `plans/subsystems/routing-health-observability-roadmap.md` | All milestones closed | None. |
| Edge protocols | closed | `plans/subsystems/protocols-edge-roadmap.md` | All milestones closed | None. |
| Transports | active | `plans/subsystems/transports-roadmap.md` | M003 downstream-safe bounded WebSocket composition API | Ready on published v1.0.11; additive downstream API seam. |
| Outbound chains and connectors | closed | `plans/subsystems/outbound-chains-roadmap.md` | All milestones closed; pooled-transport/H2/ALPN correctives qualified | None. |
| Parity contract and compatibility translation | closed | `plans/subsystems/parity-compat-roadmap.md` | All milestones closed; manifest has no unresolved `gap` outside tiered boundaries | None. |
| Delivery (CLI, embed, Python, release) | active | `plans/subsystems/delivery-roadmap.md` | M003 Eggup archive/pair self-update adoption landed and shipped in published `v1.0.10` | Closed 2026-09-29 (`plans/closure/delivery/003-status.md`); `v1.0.11` patch release published 2026-09-30; `v1.0.12` patch release published 2026-10-08; next version bump is a separate maintainer-authorized release action. |
| Security hardening | closed | `plans/subsystems/security-hardening-roadmap.md` | M001–M004 closed; M005 Phase A (#10) closed with Outcome B2 deferral for #9 | Closure under `plans/closure/security-hardening/`. Residual: #9 authenticity signing needs a future delivery ADR; parity-locked open-proxy/`-ul`/auth-reuse defaults remain by ADR-0002. |

## Dependency-ready implementation plans

| Subsystem | Milestone | Status | Implementation plan | Dependency / consumer |
|---|---|---|---|---|
| Proxy core and relay | M003 optional connection-wide no-progress timeout | conditionally closed | `plans/implementation/proxy-core-relay/003-optional-relay-idle-timeout.md` | Closure: `plans/closure/proxy-core-relay/003-status.md`; broad verification and post-publication issue handoff remain. |
| Transports | M003 downstream-safe bounded WebSocket composition API | ready | `plans/implementation/transports/003-downstream-safe-bounded-websocket-composition-api.md` | Published v1.0.11 baseline; requested by Eggtunnel M019 to remove direct Tungstenite ownership. |
| Security hardening | M001 native fail-closed and admin posture | closed | `plans/implementation/security-hardening/001-native-fail-closed-and-admin-posture.md` | Closure: `plans/closure/security-hardening/001-native-fail-closed-and-admin-posture.md`. |
| Security hardening | M002 private egress policy plumbing | closed | `plans/implementation/security-hardening/002-private-egress-policy-plumbing.md` | Closure: `plans/closure/security-hardening/002-private-egress-policy-plumbing.md`. |
| Security hardening | M003 embed SSH strict host-key path | closed | `plans/implementation/security-hardening/003-embed-ssh-strict-host-key-path.md` | Closure: `plans/closure/security-hardening/003-embed-ssh-strict-host-key-path.md`. |
| Security hardening | M004 compat residual without parity break | closed | `plans/implementation/security-hardening/004-compat-residual-hardening.md` | Closure: `plans/closure/security-hardening/004-compat-residual-hardening.md`. |
| Security hardening | M005 update trust hardening | closed | `plans/implementation/security-hardening/005-update-trust-hardening.md` | Closure: `plans/closure/security-hardening/005-update-trust-hardening.md` (Phase A + B2). |

Delivery M003 landed and closed 2026-09-29 (`plans/closure/delivery/003-status.md`).

## Proposed work (not authorized for implementation)

None at present. Proxy core and relay M003 source implementation is complete and conditionally closed pending the named verification and post-publication handoff in its closure record.

## Current execution order and dependency gates

**1.0.10 gate (closed — published 2026-09-24):** the 1.0.10 runtime/TLS implementation, package dry-run, and CI were qualified (physical H2 isolation proven by handshake counts with shared-registry control at `c5826b3`; Rust CI `36032622797` success). `v1.0.10` is tagged and published (PyPI, crates.io, GitHub Release with five CLI archives). Archive phase-records retain their original "prepared, unpublished" wording as provenance.

**Compat-contract gate:** claims remain governed by the manifest plus matrix. Changing a claim requires a manifest update and the oracle/differential/interop suite. Generated reports follow the manifest.

**WebSocket downstream-boundary gate:** Transports M003 is ready and additive. It must preserve existing WebSocket methods/defaults while adding an Eggress-owned finite message/frame configuration seam. Source closure does not authorize or imply a version tag; downstream Eggtunnel adoption remains operationally blocked until the API is published through the normal release process.

**Verification gate:** `docs/CI_STATUS.md` owns verification policy; `docs/TESTING.md` owns the suite inventory. Ordinary changes use the narrowest test first, then the broad gate before merging substantial Rust changes.

**Relay M003 gate:** ADR-0003 is accepted and source implementation is conditionally closed. Workspace-wide tests/Clippy have the limitations recorded in the closure record. After a compatible API release is authorized and published, open linked adoption issues for Eggchaos configuration and Eggtunnel exact-pin/adoption; do not create those issues before publication.

**Security-hardening gate (closed 2026-11-17):** ADR-0002 path-split / opt-in harden landed for M001–M004; M005 Phase A gates `EGRESS_UPDATE_BASE_URL` with Outcome B2 deferral of authenticity (#9). Compat defaults remain warn-compatible; parity-locked residuals (#2/#3 wire semantics) stay document/warn-only. Finding #9 signing remains future delivery work under a new scheme ADR.

**Eggup updater-convergence gate (closed 2026-09-28; consumer landed 2026-09-29):** Delivery M003 is registered at `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md` and closed in `plans/closure/delivery/003-status.md`. Eggup Archive M001d is runtime-qualified (`eggstack/eggup@18d83de`, hosted run `36335233644`), and the required post-M001d APIs shipped as compatible registry packages: `eggup-core 0.1.2` + `eggup-archive 0.1.2` (Eggup M009 closure, `eggstack/eggup@v0.1.2`). The final Cargo dependency cutover landed against the registry versions with hosted Linux/macOS/Windows updater evidence. Eggress release/checksum/version/CLI policy remains local.

## Blocked work

No implementation blocker. Relay M003 source work is conditionally closed pending broad verification and post-publication issue handoff; Delivery M003 was unblocked by Eggup M009 publication closure on 2026-09-28 (see ready plans above).

## Closure work and current control points

| Subsystem | Status | Controlling evidence |
|---|---|---|
| Proxy core and relay M003 | conditionally closed | `plans/closure/proxy-core-relay/003-status.md`; workspace verification and post-publication downstream handoff remain |
| 1.0.10 H2/pool/ALPN qualification | closed, published as `v1.0.10` (2026-09-24) | `plans/archive/phase-records/H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md`, `ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md`, `H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md`, `POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`; implementation `c5826b3`; Rust CI `36032622797` |
| Distribution/release maintenance | closed | `plans/archive/phase-records/DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md`, `PYPI_WHEEL_MATRIX_EXPANSION.md`, `MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md` |
| Eggfetch 0.2 consolidation | closed | `plans/archive/phase-records/EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`, `EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`, `EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md` |
| Pre-transition flat index | archived | `plans/archive/phase-records-README-legacy.md` (the former `plans/README.md`) |
