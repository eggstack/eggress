# Security Hardening Milestone 005 — Closure Status (Phase A + Outcome B2)

Status: closed

Source implementation plan:

- `plans/implementation/security-hardening/005-update-trust-hardening.md`

Audit findings disposition:

| Finding | Description | Outcome |
|---------|-------------|---------|
| **#10** | `EGRESS_UPDATE_BASE_URL` env redirect | **Closed** — dual env gate (`EGRESS_UPDATE_ALLOW_BASE_URL_OVERRIDE`) + scheme allowlist (`https://`, `file://`); fail closed without acknowledgment |
| **#9** | No authenticity beyond SHA-256 for self-update | **Deferred (Outcome B2)** — documented residual compromised-release-account risk; no cosign/minisign/TUF/platform signing |

## 1. Executive finding

Phase A lands the base URL override gate so a single ambient env var cannot
redirect production `eggress update` traffic. Outcome B2 records an explicit,
honest deferral of cryptographic release authenticity: integrity remains
SHA-256 + staged-version checks over HTTPS to GitHub Releases.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Override fails without `EGRESS_UPDATE_ALLOW_BASE_URL_OVERRIDE` | `resolve_update_base_override` + unit tests in `update/mod.rs` | pass |
| Override allowed with dual truthy acknowledgment | Unit tests (`1`/`true`/`yes`/`on`) | pass |
| Override schemes limited to `https://` / `file://` | `resolve_update_base_override` + download.rs HTTPS rules unchanged | pass |
| Checksum/version checks unchanged | No edits to `verify.rs` gate logic | pass |
| Operator docs honest about integrity vs authenticity | `docs/INSTALLATION.md`, `docs/security/THREAT_MODEL.md`, module rustdoc | pass |
| No signing/TUF implementation | Scope exclusion respected | pass |

## 3. Deferred work (Outcome B2 — finding #9)

Before implementing authenticity:

- Accept a new delivery ADR selecting cosign / minisign / TUF / platform code
  signing and key custody.
- Extend `release-binaries.yml`, verify path, and install scripts per ADR.
- Decide transition behavior for releases without signatures.

Until then, operators should treat self-update trust as TLS + GitHub account
security + SHA-256 integrity, not publisher authenticity.

## 4. Verification

```bash
cargo test -p eggress-cli --locked update::
```

(Recorded in implementing commit/PR.)

## 5. Residual supply-chain risks

- Compromised GitHub release publishing that ships matching archives and
  SHA-256 sidecars for the advertised tag.
- Hostile env injection that sets both override variables (requires operator
  or process environment control, not remote network alone).

## 6. Roadmap disposition

Milestone 005 Phase A closed; finding #9 remains open on the hardening roadmap
under Outcome B2 until a signing ADR and implementation land.