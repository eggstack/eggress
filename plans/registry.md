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
| Routing, health, metrics, and admin | closed | `plans/subsystems/routing-health-observability-roadmap.md` | All milestones closed; M003 CIDR/admin exposure qualified | None. |
| Edge protocols | closed | `plans/subsystems/protocols-edge-roadmap.md` | All milestones closed; M003 SOCKS4/UDP/auth-transport qualified | None. |
| Transports | active | `plans/subsystems/transports-roadmap.md` | M003 downstream-safe WebSocket API | M004/M005 closed 2026-11-16; M003 remains ready (additive, no hard blocker). |
| Outbound chains and connectors | closed | `plans/subsystems/outbound-chains-roadmap.md` | All milestones closed; pooled-transport/H2/ALPN correctives qualified | None. |
| Parity contract and compatibility translation | closed | `plans/subsystems/parity-compat-roadmap.md` | All milestones closed; M003 auth reuse/redaction qualified | Manifest claim unchanged; `check --json` raw-field follow-up tracked in the closure record. |
| Delivery (CLI, embed, Python, release) | closed | `plans/subsystems/delivery-roadmap.md` | All milestones closed; M004 updater trust/advisory gate qualified | M003 closed 2026-09-29 (`plans/closure/delivery/003-status.md`); M004 closed 2026-11-16 (`plans/closure/delivery/004-status.md`) with seven `zip` advisories tracked in `deny.toml` (owner/review date recorded); `v1.0.12` published 2026-10-08; next version bump is a separate maintainer-authorized release action. |
| Security hardening (cross-cutting corrective) | closed (2026-11-16) | `plans/subsystems/security-hardening-roadmap.md` | S1–S6 implemented and verified | Seven `zip 6.0.0` advisories plus the unmaintained `chrono` transitive (`russh-util`) remain as narrow, commented `deny.toml` exceptions with owner and review date; no release authorized. |

## Implementation plans and dependency state

| Subsystem | Milestone | Status | Implementation plan | Dependency / consumer |
|---|---|---|---|---|
| Transports | M003 downstream-safe bounded WebSocket composition API | ready | `plans/implementation/transports/003-downstream-safe-bounded-websocket-composition-api.md` | Published v1.0.11 baseline; requested by Eggtunnel M019 to remove direct Tungstenite ownership. |
| Transports | M004 TLS trust/default advisory remediation | closed | `plans/implementation/transports/004-tls-trust-and-dependency-advisory-remediation.md` | `plans/closure/transports/004-status.md` |
| Transports | M005 `russh` 0.65 advisory migration | closed | `plans/implementation/transports/005-russh-065-advisory-migration.md` | `plans/closure/transports/005-status.md` |
| Protocols Edge | M003 SOCKS4 credential enforcement and UDP private-egress policy | closed | `plans/implementation/protocols-edge/003-socks4-auth-and-udp-private-egress.md` | `plans/closure/protocols-edge/003-status.md` |
| Routing, health, metrics, admin | M003 CIDR canonicalization and admin exposure | closed | `plans/implementation/routing-health-observability/003-cidr-canonicalization-and-admin-exposure.md` | `plans/closure/routing-health-observability/003-status.md` |
| Delivery | M004 updater trust, advisory gate, and credential-output hardening | closed | `plans/implementation/delivery/004-updater-trust-and-advisory-gate.md` | `plans/closure/delivery/004-status.md` (tracked `zip` exception) |
| Parity Compat | M003 auth reuse semantics and compatibility redaction | closed | `plans/implementation/parity-compat/003-auth-reuse-and-compat-redaction.md` | `plans/closure/parity-compat/003-status.md` |

The six security-corrective plans above closed on 2026-11-16 after the
coordinator broad gate. Delivery M003 landed and closed 2026-09-29
(`plans/closure/delivery/003-status.md`).

## Current execution order and dependency gates

**1.0.10 gate (closed — published 2026-09-24):** the 1.0.10 runtime/TLS implementation, package dry-run, and CI were qualified (physical H2 isolation proven by handshake counts with shared-registry control at `c5826b3`; Rust CI `36032622797` success). `v1.0.10` is tagged and published (PyPI, crates.io, GitHub Release with five CLI archives). Archive phase-records retain their original "prepared, unpublished" wording as provenance.

**Compat-contract gate:** claims remain governed by the manifest plus matrix. Changing a claim requires a manifest update and the oracle/differential/interop suite. Generated reports follow the manifest.

**WebSocket downstream-boundary gate:** Transports M003 is ready and additive. It must preserve existing WebSocket methods/defaults while adding an Eggress-owned finite message/frame configuration seam. Source closure does not authorize or imply a version tag; downstream Eggtunnel adoption remains operationally blocked until the API is published through the normal release process.

**Verification gate:** `docs/CI_STATUS.md` owns verification policy; `docs/TESTING.md` owns the suite inventory. Ordinary changes use the narrowest test first, then the broad gate before merging substantial Rust changes.

**Security corrective gate (closed 2026-11-16):** the 2026-11-16 audit corrective program at `plans/subsystems/security-hardening-roadmap.md` is implemented and verified. S1 removed the distrusted-root/webpki exposure and the default-graph TLS/QUIC/tokio advisories; S2 moved optional SSH to `russh 0.65.2`; S3 restored pproxy SOCKS4 full-credential semantics and enforced UDP private-egress policy after DNS; S4 canonicalized CIDR matching and added admin Host/Origin validation; S5 hardened the updater and added the advisory gate; S6 corrected the compatibility auth-reuse contract and redacted compat diagnostics. The remaining audit items are the seven `zip 6.0.0` advisories in the updater graph (owner: Delivery M004) and the unmaintained `chrono` transitive through `russh-util` on the optional `ssh` path (owner: Transports M005), both tracked as narrow, commented `deny.toml` exceptions with review by 2027-05-01. No tag or publication is authorized by these plans.

**Eggup updater-convergence gate (closed 2026-09-28; consumer landed 2026-09-29):** Delivery M003 is registered at `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md` and closed in `plans/closure/delivery/003-status.md`. Eggup Archive M001d is runtime-qualified (`eggstack/eggup@18d83de`, hosted run `36335233644`), and the required post-M001d APIs shipped as compatible registry packages: `eggup-core 0.1.2` + `eggup-archive 0.1.2` (Eggup M009 closure, `eggstack/eggup@v0.1.2`). The final Cargo dependency cutover landed against the registry versions with hosted Linux/macOS/Windows updater evidence. Eggress release/checksum/version/CLI policy remains local.

## Blocked work

- **`zip 6.0.0` / `eggup-archive` (tracked residual)** — RUSTSEC-2026-0429/-0367/-0368/-0369 and, published 2026-11-17, -0545/-0547/-0548 remain in the updater dependency graph. `eggup-archive 0.1.2` and `0.2.0` pin `zip = "6.0.0"`; an upstream Eggup release with `zip >= 9.0.0` is required. The vulnerable `extract`/`ZipStreamReader` APIs are not used by Eggup's bounded extractor and `aes-crypto` is disabled, so the advisory gate carries a narrow, commented exception with owner (Delivery M004) and review date (2027-05-01); delete it when the upstream dependency lands.
- **`chrono` unmaintained / `russh-util` (tracked residual)** — RUSTSEC-2026-0543 is unmaintained (not a vulnerability) and is pulled unconditionally by `russh-util 0.52.0` through `russh 0.65.2`, the latest published and fixed SSH line. Reachable only through the optional `ssh` feature; no safe upgrade exists. Narrow, commented exception with owner (Transports M005) and review by 2027-05-01. `cargo deny --workspace check` passes on 2026-11-17 with exactly these exceptions.

## Closure work and current control points

| Subsystem | Status | Controlling evidence |
|---|---|---|
| 1.0.10 H2/pool/ALPN qualification | closed, published as `v1.0.10` (2026-09-24) | `plans/archive/phase-records/H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md`, `ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md`, `H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md`, `POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`; implementation `c5826b3`; Rust CI `36032622797` |
| Distribution/release maintenance | closed | `plans/archive/phase-records/DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md`, `PYPI_WHEEL_MATRIX_EXPANSION.md`, `MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md` |
| Eggfetch 0.2 consolidation | closed | `plans/archive/phase-records/EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`, `EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`, `EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md` |
| Security corrective program (2026-11-16 audit) | closed 2026-11-16 | `plans/subsystems/security-hardening-roadmap.md`; closure records `transports/004`, `transports/005`, `protocols-edge/003`, `routing-health-observability/003`, `delivery/004`, `parity-compat/003`; tracked `zip` exception |
| Pre-transition flat index | archived | `plans/archive/phase-records-README-legacy.md` (the former `plans/README.md`) |
