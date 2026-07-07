# Phase 53 — Release Cleanup Pass Completion

**Date:** 2026-07-07
**Phase:** 53
**Status:** Complete

## Summary

Release cleanup pass addressing 6 correctness issues (C1–C6) in the
release workflow, documentation, and test infrastructure. No behavioral
code changes.

## Fixes Applied

### C1: Go/no-go checklist CI wording contradiction

**File:** `docs/release/GO_NO_GO_CHECKLIST.md`

Line 28 said "Tests pass in CI" which contradicted line 41's "No hosted
CI visibility". Replaced with "Hosted CI status is not verified; local
verification is the source of truth."

### C2: Release workflow maturin target triples

**File:** `.github/workflows/release.yml`

The wheel matrix used bare `target` fields (e.g., `x86_64`) and
constructed target triples via string interpolation, producing invalid
triples for macOS/Windows (e.g., `x86_64-unknown-apple-darwin` instead
of `x86_64-apple-darwin`).

Fix: Added explicit `rust_target` and `wheel_smoke` fields to each wheel
matrix entry. Updated toolchain install, maturin build, and smoke test
steps to use `matrix.rust_target` / `matrix.wheel_smoke`.

### C3: Release body release notes reference

**File:** `.github/workflows/release.yml`

Release body referenced `RELEASE_NOTES_PARITY_RC.md` (the older file)
instead of `RELEASE_NOTES_PHASE_51.md` (the canonical Phase 51 notes).

Fix: Updated reference to `RELEASE_NOTES_PHASE_51.md`.

**File:** `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md`

Updated the References section to point to `RELEASE_NOTES_PHASE_51.md`
instead of `RELEASE_NOTES_PARITY_RC.md` for consistency.

### C4: Strict validator test assertion

**File:** `tests/scripts/test_validate_pproxy_parity_manifest.py`

`test_real_manifest_strict` only printed output on failure but never
asserted success. Added `assert exit_code == 0` so the test properly
fails when the validator returns non-zero.

### C5: SBOM best-effort upload placeholder

**File:** `.github/workflows/release.yml`

The SBOM generation step was best-effort (may fail) but the upload step
expected `sbom.json` to exist. Added a fallback step that creates a
placeholder `sbom.json` when generation fails, ensuring the upload step
always finds a file.

### C6: Duplicate checksum pattern

**File:** `.github/workflows/release.yml`

The checksum step had `find dist -name '*.tar.gz' -o -name '*.tar.gz'`
with a duplicate pattern. Removed the redundant `-o -name '*.tar.gz'`.

## Files Modified

| File | Fixes |
|---|---|
| `.github/workflows/release.yml` | C2, C3, C5, C6 |
| `docs/release/GO_NO_GO_CHECKLIST.md` | C1 |
| `tests/scripts/test_validate_pproxy_parity_manifest.py` | C4 |
| `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md` | C3 (consistency) |

## Verification

All existing tests continue to pass:

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | ✅ PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ PASS |
| `cargo test --workspace` | ✅ PASS |
| `test_real_manifest_strict` (now with assertion) | ✅ PASS |
| YAML syntax validation | ✅ PASS |

## References

- [Phase 53 plan](../plans/phase_53_release_cleanup_pass.md)
- [Phase 51 release notes](release/RELEASE_NOTES_PHASE_51.md)
- [Final parity certification](release/FINAL_PPROXY_PARITY_CERTIFICATION.md)
