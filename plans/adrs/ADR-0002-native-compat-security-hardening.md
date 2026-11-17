# ADR-0002: Native vs compatibility security hardening

Status: accepted

Date: 2026-11-17

Decision owners: project maintainers

Related specification sections:

- `plans/000-long-term-specification.md` §2 (boxed streams, fail-closed composition), §3 (pproxy practical compatibility), §4 (credential redaction)
- `plans/001-terminology-and-domain-model.md` (Listener, Session, OutboundConnector)
- `plans/002-long-term-roadmap.md` (post-milestone hardening as new subsystem milestones)

Affected subsystem roadmaps:

- `plans/subsystems/security-hardening-roadmap.md`
- Consumes interfaces owned by `server-runtime-config`, `proxy-core-relay`, `outbound-chains`, `parity-compat`, and `delivery` without reopening those closed roadmaps as primary owners

## Context

A repository-wide security audit at published `origin/main` (workspace **1.0.12**, commit `ba64e16`) found no silent implementation bugs beyond documented posture, but identified residual risks:

1. Unauthenticated open listeners under pproxy-compat defaults (warn-only validation).
2. Standalone pproxy UDP (`-ul`) without TCP SOCKS credential auth.
3. pproxy `--auth` IP reuse selecting SOCKS `AUTH_NONE` after cache hit.
4. `OutboundConnector::from_pproxy_uri()` always using SSH `InsecureCompatibility` host keys.
5. Literal private/reserved IP targets allowed while DNS-resolved privates are blocked.
6. UDP `allow_private_egress` defaulting to `true`.
7. UDP domain targets not reserved-IP checked at admission.
8. Loopback admin with `auth: None` readable by any local user.
9. Self-update trust = HTTPS + SHA-256 sidecar only (no signing/TUF).
10. `EGRESS_UPDATE_BASE_URL` can redirect update discovery via process environment.
11. Opt-in `legacy-crypto` / `insecure-tls` / `insecure-quic` residual risk.
12. Operator misuse of `EGRESS_SSH_INSECURE_HOST_KEYS` on the runtime compat path.

Hardening that silently changes pproxy defaults would break `matched` / `supported_difference` claims in `docs/parity/pproxy_capability_manifest.toml`. Operators already have many knobs (`[listeners.auth]`, routing reject, `allow_private_egress = false`, native SSH via `from_toml`, `[admin.auth]`). The gap is primarily **native fail-closed options**, **path-split defaults**, and **supply-chain depth**, not missing DNS-rebinding or redaction machinery.

## Decision drivers

- Preserve practical pproxy 2.7.9 compatibility unless a claim is deliberately retiered with oracle/differential/interop evidence.
- Prefer opt-in fail-closed and native-path harden over changing compat defaults.
- Keep SSH / private-egress / open-proxy policy explicit and documented; never silent fallback.
- Do not reopen closed subsystem roadmaps; route cross-cutting security work through a dedicated hardening roadmap.
- Supply-chain signing is valuable but is a large delivery project, not a silent default flip.

## Considered options

### Option A — Flip insecure-by-compat defaults globally

Make unauthenticated non-loopback binds, private literal egress, UDP private egress, and insecure SSH fail closed for all entry points including `pproxy` / `from_pproxy_uri`.

Benefits: strongest default posture. Costs: high parity break across `cli.listen`, `cli.ul`, `cli.auth`, SSH URI claims; requires manifest retiering and broad oracle suites. Failure mode: breaks drop-in migrations that the product still claims to support.

### Option B — Path-split and opt-in harden (selected)

Keep pproxy-compat defaults and warn-only behavior unless an explicit strict flag, native-path default, or separate API is used. Add missing knobs where APIs already exist (e.g. `enforce_literal_ip_check`) but are not plumbed. Document residual risks that parity forbids changing. Treat code-signing/TUF as a separate delivery milestone.

Benefits: low parity risk, clear operator upgrade path, aligns with existing docs (`PPROXY_COMPAT_SECURITY_DIFFERENCES.md`, `SECURE_CONFIGURATION.md`). Costs: insecure defaults remain for un-hardened compat deployments. Failure mode: operators who never opt in stay exposed — mitigated by docs, `pproxy check`, and native fail-closed options.

### Option C — Document-only

No new knobs; expand hardening guides only.

Benefits: zero code risk. Costs: leaves known gaps (no `fail_on_security_warnings`, no strict embed SSH API, update env injection ungated). Insufficient for the audit remediation set.

## Decision

Adopt **Option B**.

Normative rules for the `security-hardening` workstream:

1. **Compat defaults stay warn-compatible** unless a milestone explicitly updates the parity manifest and runs the required oracle/differential/interop suite.
2. **Native `eggress run` / TOML** MAY gain stricter defaults or fail-closed options that do not apply to the pproxy translator output unless mirrored intentionally.
3. **New APIs** for stricter embed/SSH/private-egress behavior MUST be additive; existing `from_pproxy_uri()` semantics remain unless a milestone retiered that claim.
4. **Findings classified document-only** (parity-locked handshake reuse semantics, standalone UDP protocol shape) get docs + optional warnings/caps, not protocol rewrites.
5. **Self-update authenticity** (signing/TUF) is a delivery capability milestone; checksum+HTTPS remains the interim contract until that milestone closes.
6. Implementation lives under `plans/subsystems/security-hardening-roadmap.md` with milestone plans under `plans/implementation/security-hardening/`. Code changes land in the owning crates (`eggress-config`, `eggress-runtime`, `eggress-server`, `eggress-core`, `eggress-udp`, `eggress-outbound`, `eggress-embed`, `eggress-cli`, docs) without transferring long-term ownership away from those architecture docs.

## Consequences

### Positive

- Clear rule for agents: harden without silent parity breaks.
- Audit findings map to ordered milestones with explicit class A/B/C/D remediation.
- Existing security docs remain authoritative for operator posture.

### Negative

- Unhardened pproxy deployments remain open by design until operators opt in.
- Supply-chain authenticity beyond SHA-256 is deferred until a delivery milestone funds key management.

### Neutral or deferred

- Changing `cli.auth` reuse handshake selection or requiring TCP auth on `-ul` remains out of scope unless a future ADR retieres those claims.
- Free-threaded Python and automated crates.io publication remain unrelated non-goals.

## Compatibility and migration

- No forced config migration for existing TOML or pproxy CLI users.
- New flags/APIs MUST default to current behavior for compat paths.
- Native-only default changes MUST be called out in `docs/CONFIG_REFERENCE.md`, `docs/security/*`, and release notes when shipped.
- Manifest/matrix updates are required only when a milestone changes a claimed behavior or tier.

## Security and reliability implications

- Fail-closed options reduce accidental open proxies and LAN SSRF for native deployments.
- Strict SSH embed APIs reduce MITM on untrusted networks without weakening documented pproxy SSH parity.
- Gating `EGRESS_UPDATE_BASE_URL` reduces env-injection supply-chain risk.
- Signing/TUF (when implemented) raises authenticity above TLS+checksum; until then, threat model docs must stay honest.

## Verification

Each milestone MUST include:

- focused unit/integration tests for the new fail-closed or opt-in path;
- negative tests proving compat defaults unchanged unless explicitly retiered;
- credential-redaction and security-invariant tests where auth/SSH surfaces change;
- for claim changes: manifest update + oracle/differential/interop per `AGENTS.md` / `docs/CI_STATUS.md`;
- documentation updates under `docs/security/` and architecture indexes as ownership requires.

## Supersession

None.
