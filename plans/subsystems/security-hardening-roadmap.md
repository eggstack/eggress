# Security Hardening Roadmap

Status: closed

Long-term references:

- `plans/000-long-term-specification.md` §2–§4 (fail-closed composition, pproxy practical compatibility, credential redaction)
- `plans/001-terminology-and-domain-model.md` (Listener, Session, OutboundConnector, admin)
- `plans/002-long-term-roadmap.md` (post-milestone hardening as new subsystem milestones)

Related ADRs:

- `plans/adrs/ADR-0002-native-compat-security-hardening.md` (native vs compat harden policy)
- `plans/adrs/ADR-0001-planning-conventions.md` (process)

Audit provenance (research-only, 2026-11-17): repository-wide security audit at `origin/main` tip `ba64e16` (workspace 1.0.12). Findings are residual posture / missing opt-in knobs, not silent parser or redaction bugs.

## 1. Purpose and ownership boundary

Owns the **cross-cutting security remediation workstream** that turns the 2026-11-17 audit findings into ordered, parity-safe milestones: fail-closed config/runtime options, private-egress policy plumbing, embed SSH strict paths, compat residual warnings/docs, and update-trust hardening.

Consumes (does not permanently reassign) interfaces owned by:

- `eggress-config` / `eggress-runtime` / `eggress-server` (listener auth, admin, security warnings);
- `eggress-core` / `eggress-udp` (ConnectOptions, UDP target validation);
- `eggress-outbound` / `eggress-embed` / `eggress-transport-ssh` (SSH host-key policy);
- `eggress-pproxy-compat` / parity docs (compat defaults and claim tiers);
- `eggress-cli` update path (integrity / authenticity).

MUST NOT:

- silently change `matched` pproxy defaults without a manifest + oracle/differential/interop pass;
- weaken credential redaction, DNS-rebinding-on-resolve, or `unsafe_code = deny`;
- treat supply-chain signing as a prerequisite for earlier fail-closed milestones;
- reopen closed subsystem roadmaps as the primary planning home for this work.

Architecture / docs anchors: `docs/security/*`, `architecture/{config,server,runtime,outbound,udp,admin,core}.md`, `.skills/security-dev/skill.md`.

## 2. Work classification

### Invariants

- Compat-path defaults remain warn-compatible unless a milestone explicitly retieres a claim (ADR-0002).
- DNS-resolved private/reserved answers stay blocked on TCP `DirectConnector` defaults.
- Credentials and secret-bearing URIs stay redacted in logs, errors, diagnostics, and evidence.
- Unsupported protocol/transport composition fails closed.
- Workspace `unsafe_code = deny` remains.

### Capabilities

- Native operators can fail closed on security warnings and restrict literal/UDP private egress without leaving the product.
- Embedders can construct outbound chains with verified SSH host keys without using pproxy-compat insecure policy.
- Updater authenticity can be raised above HTTPS+SHA-256 when the delivery milestone ships.

### Infrastructure

- Config/runtime plumbing for `fail_on_security_warnings`, literal-IP enforcement, UDP admission checks.
- Additive embed/outbound APIs for strict SSH policy.
- Optional gating for update base-URL injection.

### Polish

- Hardening docs, threat-model honesty for residual compat risks, CLI check/strict messaging, feature-residual guidance.

## 3. Non-goals

- Breaking drop-in pproxy open-listener / `-ul` / `--auth` reuse semantics by default.
- Requiring TCP SOCKS auth on the standalone UDP wire format (would be a new non-parity protocol).
- Replacing rustls or introducing OpenSSL/C deps for “more secure TLS.”
- Free-threaded Python, automated crates.io publish, or unrelated transport feature work.
- Rewriting closed 1.0.12 release provenance documents.

## 4. Current state

Hardening already present at baseline `ba64e16`:

- `validate_config_security` emits non-fatal `ConfigWarning`s for non-loopback listeners/admin/reverse without auth.
- DNS rebinding checks on resolved names; literal private IPs allowed by default (`enforce_literal_ip_check: false`).
- UDP `validate_standalone_target` with `allow_private_egress` default `true`.
- Runtime SSH insecure host keys env-gated; embed `from_pproxy_uri` uses compatibility policy unconditionally.
- Admin non-loopback requires auth structurally; loopback may be unauthenticated.
- Self-update: GitHub HTTPS + SHA-256 sidecar + staged version checks; `EGRESS_UPDATE_BASE_URL` for fixtures.
- Docs: `PPROXY_COMPAT_SECURITY_DIFFERENCES.md`, `SECURE_CONFIGURATION.md`, `OPEN_PROXY_PREVENTION.md`, `HARDENING_GUIDE.md`, `THREAT_MODEL.md`, `EMBED_API.md`.

Gaps: no `fail_on_security_warnings`; literal-IP flag not a first-class TOML/runtime knob for ordinary sessions; UDP domain admission does not mirror TCP rebinding; no strict embed SSH constructor; update env override ungated outside tests; no code signing/TUF.

## 5. Target architecture

```text
Operator / embedder chooses posture
        |
        +-- pproxy compat path ---- warn-only defaults (parity) ---- optional strict check CLI
        |
        +-- native TOML / eggress run ---- opt-in or native-stricter fail-closed knobs
        |
        +-- OutboundConnector ---- from_pproxy_uri (compat SSH) | from_toml / strict API (verified SSH)
        |
        +-- eggress update ---- HTTPS+SHA-256 (interim) --> signed/TUF authenticity (M005)
```

Security warnings become optionally fatal on native paths. Private egress and UDP admission share one policy vocabulary with TCP. SSH trust policy is explicit per construction path. Update authenticity is documented honestly until signing lands.

## 6. Dependency graph

```text
ADR-0002 (hard)
    |
    +--> M001 native fail-closed + admin posture (#1, #8)
    |         |
    |         +--> M002 private egress plumbing (#5, #6, #7)  [soft: shares config/security vocabulary]
    |
    +--> M003 embed SSH strict path (#4, #12)                 [independent of M001]
    |
    +--> M004 compat residual warnings/docs (#2, #3, #11)     [soft after M001 for shared warning surface]
    |
    `--> M005 update trust (#10 then #9)                      [independent; #9 is large/delivery]
```

Dependency classes: ADR-0002 hard for all; M001→M002 soft (shared config language); M004 soft-after-M001; M003/M005 independent.

## 7. Milestones

### Milestone 001 — Native security fail-closed and admin posture

Class: capability / infrastructure

Objective: Let native operators fail closed on `validate_config_security` warnings; tighten admin guidance/opt-in require-auth for shared hosts (findings **#1**, **#8**).

Dependencies: ADR-0002 accepted.

Deliverable boundary: config + runtime + CLI check/strict flag; docs. No pproxy default change.

User or operator value: accidental open proxies and unauthenticated admin on multi-user hosts can be rejected at startup when configured.

Exit conditions: flag/API documented and tested; compat warn-only path unchanged; closure evidence recorded.

Deferred work: forcing auth on all loopback admin by default (optional follow-up if PAC UX allows).

### Milestone 002 — Private egress policy plumbing

Class: capability / infrastructure

Objective: Plumb `enforce_literal_ip_check` and native/compat-split `allow_private_egress` defaults; align UDP domain admission with TCP rebinding policy under opt-in or native-stricter settings (findings **#5**, **#6**, **#7**).

Dependencies: soft on M001 for shared `[process.security]` / listener security vocabulary if introduced there; otherwise may proceed with its own config keys.

Deliverable boundary: `eggress-config` compile/validate, server/outbound connect options, `eggress-udp` security/direct/relay; docs. Compat translator keeps permissive defaults.

Exit conditions: operators can deny literal private TCP/UDP egress without custom routing-only workarounds; domain UDP cannot bypass reserved-IP policy when enforcement is enabled; compat defaults unchanged.

Deferred work: connect-time TOCTOU elimination beyond documented residual.

### Milestone 003 — Embed SSH host-key strict path

Class: capability

Objective: Additive strict constructor / policy parameter for embed outbound SSH; keep `from_pproxy_uri` compat behavior; clarify/harden runtime env misuse (findings **#4**, **#12**).

Dependencies: ADR-0002; `ssh` + `pproxy-compat` feature slices.

Deliverable boundary: `eggress-outbound`, `eggress-embed`, Python binding surface if applicable, OpenSSH regression, `EMBED_API.md`.

Exit conditions: embedders can use verified known-hosts without pproxy URI path; `from_pproxy_uri` still documents and implements insecure compat policy; runtime env gate behavior documented/tested.

Deferred work: changing `from_pproxy_uri` default to KnownHosts (parity break).

### Milestone 004 — Compat residual hardening without parity break

Class: polish / infrastructure

Objective: Warnings/caps/docs for standalone UDP non-loopback, auth-reuse TTL, and insecure feature residual risk without changing wire semantics (findings **#2**, **#3**, **#11**).

Dependencies: soft after M001 if sharing security-warning channels.

Deliverable boundary: validate/security warnings, optional TTL cap/warn in pproxy-compat, docs/security + threat model; **no** AUTH_NONE selection change; **no** TCP auth on `-ul`.

Exit conditions: operators see clear diagnostics; manifest tiers unchanged unless a later ADR retieres claims.

Deferred work: protocol-level UDP auth; always-require `0x02` after reuse cache hit.

### Milestone 005 — Update trust hardening

Class: capability / delivery

Objective: Gate `EGRESS_UPDATE_BASE_URL` outside intentional test/offline use (**#10**); design and optionally implement authenticity beyond SHA-256 (**#9**, large).

Dependencies: independent of M001–M004; signing may depend on release-engineering key ownership.

Deliverable boundary: `eggress-cli` update module + release_contract tests for #10; for #9 either a design ADR + spike closure or full cosign/minisign/TUF integration with INSTALLATION.md.

Exit conditions: production processes cannot redirect update base URL via a single unset-in-prod env alone without explicit dual acknowledgment or compile-time test gating; authenticity story is either implemented or explicitly deferred with threat-model text.

Deferred work: OS notarization matrices may remain operational follow-ups after first signing scheme.

## 8. Cross-cutting requirements

### Config and migration

New keys default to today’s behavior on compat paths. Native-only stricter defaults require release-note callouts. No forced rewrite of existing configs.

### Protocol and compatibility

Manifest/matrix are authoritative. Claim changes require oracle/differential/interop. Prefer `supported_difference` / `intentional_non_parity` over silent divergence.

### Security and authorization

Redaction, constant-time admin auth, auth-failure metrics, and DNS-rebinding-on-resolve must not regress. New fail-closed paths must not log secrets.

### Concurrency, cancellation, and recovery

Security policy is snapshot-scoped with routing/upstream swaps; listener topology still not hot-reloaded. Update authenticity checks remain pre-mutation.

### Observability and audit

Security warnings/errors must be structured and actionable (`eggress pproxy check`, startup logs). Prefer existing `ConfigWarning` channels.

### Performance and resource use

Admission-time DNS for UDP (if enabled) must be bounded; do not add UDP pooling or change 64 KiB relay buffer without evidence.

### Documentation and operations

Update `docs/security/*`, `docs/CONFIG_REFERENCE.md`, `docs/EMBED_API.md`, `docs/INSTALLATION.md`, architecture indexes, and `.skills/security-dev/skill.md` as behavior lands.

## 9. Verification strategy

- Narrow crate tests first (`eggress-config`, `eggress-udp`, `eggress-outbound`, `eggress-cli`, `eggress-runtime` security_invariants).
- Feature-boundary slices for outbound/embed SSH + pproxy-compat per `docs/CI_STATUS.md`.
- Required OpenSSH regression when SSH policy code changes.
- Workspace `fmt` / `clippy -D warnings` / `cargo test --workspace --locked` before merging substantial Rust.
- External oracle/differential only when claims change.
- Closure records per milestone under `plans/closure/security-hardening/`.

## 10. Risks and decision points

| Risk | Mitigation |
|------|------------|
| Accidental parity break on open listener / `-ul` / auth reuse | ADR-0002; negative tests; stop condition if claim would change |
| Config key sprawl across process/listener | Prefer one security vocabulary in M001; M002 reuses it |
| Strict SSH API unused / undiscoverable | Document in EMBED_API; Python mirror if public surface |
| Signing key custody | M005 may close #10 while leaving #9 as design-only if keys unavailable |
| Scope bleed into transport/performance work | Explicit non-goals; stop conditions in each plan |

Unresolved decisions requiring a **new** ADR (not this roadmap alone):

- Selecting cosign vs minisign vs TUF vs platform code signing for M005 authenticity.
- Deliberate retiering of `cli.listen` / `cli.ul` / `cli.auth` to fail-closed defaults.

## 11. Completion definition

Roadmap may close when M001–M004 are closed and M005 has either (a) shipped authenticity beyond SHA-256 or (b) closed #10 with an accepted deferral record for #9 that updates threat-model docs. Residual compat risks that ADR-0002 classifies as document-only do not block closure.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 001 Native fail-closed + admin | closed | `plans/implementation/security-hardening/001-native-fail-closed-and-admin-posture.md` | `plans/closure/security-hardening/001-native-fail-closed-and-admin-posture.md` | None (ADR-0002 accepted) |
| 002 Private egress plumbing | closed | `plans/implementation/security-hardening/002-private-egress-policy-plumbing.md` | `plans/closure/security-hardening/002-private-egress-policy-plumbing.md` | Soft: M001 security vocabulary preferred |
| 003 Embed SSH strict path | closed | `plans/implementation/security-hardening/003-embed-ssh-strict-host-key-path.md` | `plans/closure/security-hardening/003-embed-ssh-strict-host-key-path.md` | None |
| 004 Compat residual without parity break | closed | `plans/implementation/security-hardening/004-compat-residual-hardening.md` | `plans/closure/security-hardening/004-compat-residual-hardening.md` | Soft: M001 warning channels |
| 005 Update trust hardening | closed (Phase A + B2) | `plans/implementation/security-hardening/005-update-trust-hardening.md` | `plans/closure/security-hardening/005-update-trust-hardening.md` | #9 may need signing ADR + key custody |
