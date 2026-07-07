# Phase 52: Release Candidate Corrective Hardening — Completion

**Date:** 2026-07-07
**Phase:** 52 (Corrective Hardening)
**Status:** Complete

## Summary

Corrective/hardening pass to make the release candidate honest,
mechanically checkable, and less brittle. No feature work; all changes
address overclaiming, workflow fragility, validator gaps, and imprecise
policy wording.

## Corrections Applied

### C1/D1: Certification wording correction

**File:** `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md`

- Fixed `drop_in` tier description from "backed by differential
  evidence" to "evidence is integration or stronger (differential
  where available)".
- Added "Evidence Breakdown (drop_in)" subsection: 6 differential,
  42 integration, 15 unit (63 total).
- Updated Conclusion to be conditional: "No release-blocking runtime
  defects are known after this corrective pass. Certification is
  conditional on hosted CI/release workflow validation if not yet
  executed."

### C2: Hosted CI wording conflicts

**File:** `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md`

- Changed "tests pass in CI with isolated network namespaces" to
  "Hosted CI status is not verified in this certification; local
  verification is the source of truth." This resolves the
  contradiction with the go/no-go checklist.

### C3/D3: Release workflow hardening

**File:** `.github/workflows/release.yml`

- Replaced single `download-artifact` steps with broken `|`
  regex-like patterns with separate explicit download steps per
  artifact group (binary-\*, wheel-\*, sdist) in both `checksums`
  and `create-release` jobs.
- Added `if` condition on wheel smoke test step to skip cross-arch
  combinations (only smoke-test native wheel/runner pairs).
- Renamed SBOM generation step to "best-effort" with
  `::warning::` annotations instead of silent `|| true`.

### C5/D2: Validator Rule 14 hardening

**File:** `scripts/validate_pproxy_parity_manifest.py`

- Moved the entire Rule 14 block (caveat_class validation) inside
  the `for idx, cap in enumerate(capabilities)` loop. Previously it
  was outside the loop and only validated the last capability.
- Added `rationale` to `protocol.udp_multihop` in the manifest to
  fix a strict-mode warning.

**File:** `tests/scripts/test_validate_pproxy_parity_manifest.py` (new)

- 11 regression tests proving Rule 14 catches issues on first,
  middle, and last capabilities:
  - Unknown caveat_class on first/middle/last entries
  - All three entries with issues simultaneously
  - config="refused" without caveat_class or rationale
  - deferred_by_adr without ADR reference
  - protocol_crate_only without protocol/crate/refused in notes
  - Valid caveat_class produces no warning
  - Strict mode promotes Rule 14 warnings to errors
  - Real manifest passes both normal and strict validation

### C6/D4: Private-egress/DNS-rebinding policy

**Decision:** Option B — DNS-rebinding-only protection (literal private
IPs allowed for pproxy compatibility).

**File:** `crates/eggress-core/src/connector.rs`

- Updated `is_reserved_or_private_ip` doc comment to clarify
  DNS-rebinding-only purpose.
- Added `is_dns_rebinding_risk` wrapper function for semantic clarity.
- Changed `DirectConnector::connect` to call `is_dns_rebinding_risk`.

**File:** `docs/security/SECURE_CONFIGURATION.md`

- Added DNS rebinding protection note explaining that literal private
  IPs are allowed for pproxy compatibility.

**File:** `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`

- Added new "DNS Rebinding Protection" section explaining the policy.

### C7/D5: Release doc link audit

All referenced files in certification, go/no-go, and release notes
docs verified to exist. No broken links found.

### C8: Completion/certification language softening

**File:** `docs/release/GO_NO_GO_CHECKLIST.md`

- Release Blockers: "None identified" → "None identified after
  Phase 52 corrective pass."

**File:** `docs/release/RELEASE_NOTES_PHASE_51.md`

- Updated limitation 1 to say "hosted CI status is not verified;
  local verification is the source of truth."
- Added limitation 5: "Certification is conditional on hosted
  CI/release workflow validation."

## Files Changed

| File | Change |
|------|--------|
| `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md` | Wording corrections, evidence breakdown |
| `docs/release/GO_NO_GO_CHECKLIST.md` | Softened release blockers language |
| `docs/release/RELEASE_NOTES_PHASE_51.md` | Updated limitations, added conditional certification |
| `.github/workflows/release.yml` | Artifact patterns, cross-arch smoke tests, SBOM |
| `scripts/validate_pproxy_parity_manifest.py` | Rule 14 moved inside loop |
| `tests/scripts/test_validate_pproxy_parity_manifest.py` | 11 new regression tests |
| `docs/parity/pproxy_capability_manifest.toml` | Added rationale to udp_multihop |
| `crates/eggress-core/src/connector.rs` | DNS rebinding wrapper, doc updates |
| `docs/security/SECURE_CONFIGURATION.md` | DNS rebinding protection note |
| `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md` | DNS rebinding section |

## Verification Commands

```bash
cargo fmt --all -- --check
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
python -m pytest tests/scripts/test_validate_pproxy_parity_manifest.py -v
```

## Remaining Accepted Limitations

1. Integration tests require port binding; environment-specific, not
   a code defect.
2. Python wheel requires matching architecture; cross-arch install not
   supported.
3. pproxy 2.7.9 differential tests require Python 3.11/3.12 (not 3.14).
4. No hosted CI visibility; local verification is the source of truth.
5. Certification is conditional on hosted CI/release workflow
   validation if not yet executed.
6. SBOM generation is best-effort (cargo-auditable availability
   varies).
