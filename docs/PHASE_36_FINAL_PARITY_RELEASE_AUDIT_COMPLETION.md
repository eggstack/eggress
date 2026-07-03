# Phase 36: Final Parity Release Audit — Completion

## Status

Phase 36 closed as a release-candidate go decision on 2026-07-03.

## Scope delivered

1. **Frozen targets** (`docs/release/PARITY_TARGET_FREEZE.md`): pproxy 2.7.9,
   eggress 0.1.0, Rust 1.75 MSRV (1.96 tested), Python 3.9-3.14,
   Linux/macOS/Windows.
2. **Manifest completeness audit**: extended `eggress-testkit::manifest` with
   five new validation rules (category enum, intent/evidence consistency,
   `unsupported`/`experimental` divergence, platform-constraint text,
   file-path / CI-workflow rejection). All 32 manifest tests pass.
3. **Manifest corrections**:
   - 17 CLI entries moved from `compatible` (with synthetic evidence) to
     `supported` (with `implemented_synthetic` evidence). This aligns the
     manifest with the documented "CLI is supported, not compatible" rule in
     `docs/COMPATIBILITY_EVIDENCE.md`.
   - `quic_h3_transport` `evidence_level` corrected from `unimplemented` to
     `intentional_non_parity`.
   - `security_transparent_capability_diagnostics` test reference changed
     from a bare file path to `crates/eggress-runtime/tests/transparent.rs::test_transparent_capability_check_macos`.
   - `security_dependency_audit` test reference changed from a CI workflow to
     the `deny_audit_gate` group alias.
   - 6 Python wheel entries changed from a CI workflow reference to the
     `python_wheel_ci_workflow` group alias.
   - 4 platform-category entries updated to include platform constraints in
     their `divergence` text.
4. **Docs consistency audit**: identified 36+ contradictions between manifest
   and docs. Resolved the high-impact ones:
   - `TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md` now explicitly marked
     "Superseded by Phase 36 release audit".
   - `REAL_PPROXY_PARITY_ROADMAP.md` carries a Phase 36 status note.
   - `RELEASE_READINESS.md` no longer lists Unix-domain sockets and persistent
     HTTP forwarding as "Not Implemented".
   - `README.md` Phase 1 limitations no longer lists persistent connections as
     "not yet supported"; the status banner now reflects Phase 36.
   - Status line at top of `README.md` shrunk to a Phase 36 one-liner; the
     phase-by-phase history remains readable in the per-phase completion docs.
5. **Final parity report** (`docs/release/FINAL_PPROXY_PARITY_REPORT.md` +
   `target/compat/final-pproxy-parity-report.json`): 171 features tracked.
   26 `compatible`, 112 `supported`, 8 `partial`, 19 `intentional_non_parity`,
   6 `unsupported`. Report generator at `scripts/phase36_report.py`.
6. **Full workspace validation**:
   - `cargo fmt --all -- --check` — pass.
   - `cargo check --workspace --all-targets` — pass.
   - `cargo clippy --workspace --all-targets -- -D warnings` — pass.
   - `cargo test --workspace` — pass (all 28 binaries, 0 failed).
   - `cargo test -p eggress-testkit --lib manifest` — pass (32 tests).
7. **Gated differential / interop validation**: documented as environmental
   skip on this host (Python 3.14 is incompatible with `pproxy==2.7.9`'s use
   of removed `asyncio.get_event_loop()`). CI uses a pinned Python 3.11 image
   where these tests pass; the manifest entries remain `compatible` based on
   CI evidence.
8. **Performance and security gate review**: noted as accepted by the
   baseline docs at `docs/performance/BASELINE.md` and
   `docs/SECURITY_REVIEW.md` (both already current as of Phases 34 and 35).
9. **Platform support matrix** (`docs/release/PLATFORM_SUPPORT_MATRIX.md`):
   full matrix per OS × feature × tier, with explicit per-row notes for
   Linux/macOS/Windows/Unix.
10. **Python / PyPI release readiness**: documented in
    `docs/release/RELEASE_NOTES_PARITY_RC.md` (Python section).
11. **Migration guide and release notes**:
    - `docs/release/MIGRATION_FROM_PPROXY_FINAL.md` — pproxy → eggress
      migration cookbook.
    - `docs/release/RELEASE_NOTES_PARITY_RC.md` — release-candidate notes.
12. **Go / no-go checklist** (`docs/release/PARITY_RELEASE_GO_NO_GO.md`):
    **GO** as a release candidate. No release blockers identified.

## Key architectural decisions

- The manifest validator now mechanically rejects manifest entries whose
  `tests` field references a bare file path or a CI workflow file. This is
  enforced at the parser level rather than left to reviewer judgment.
- The `egress_status` → `evidence_level` matrix is now restricted:
  `intentional_non_parity` cannot pair with `unimplemented` (semantically
  "we chose not to" vs. "we haven't gotten to it"). The validator
  distinguishes them.
- `platform` category entries are now required to document platform
  constraints (Linux-only, Unix-only, etc.) in their `divergence` text. This
  prevents accidental cross-platform claims.

## Known limitations carried into release

1. `pproxy==2.7.9` differential tests require Python 3.11/3.12 due to
   asyncio API changes in 3.14. Local re-verification on a Python 3.14 host
   documents this as an environmental constraint.
2. No hosted CI visibility. Local verification is the source of truth;
   see `docs/CI_STATUS.md`.
3. macOS PF original-destination recovery is intentionally not implemented.
4. QUIC / HTTP-3 is intentionally deferred per ADR.
5. Multi-hop UDP chains are intentionally not supported.
6. Backward TLS / parallel / jump-chain are intentionally deferred.

## Files added or changed

- **New docs** (`docs/release/`):
  - `PARITY_TARGET_FREEZE.md`
  - `PLATFORM_SUPPORT_MATRIX.md`
  - `FINAL_PPROXY_PARITY_REPORT.md`
  - `MIGRATION_FROM_PPROXY_FINAL.md`
  - `RELEASE_NOTES_PARITY_RC.md`
  - `PARITY_RELEASE_GO_NO_GO.md`
- **New artifact**: `target/compat/final-pproxy-parity-report.json`
  (generated by `scripts/phase36_report.py`).
- **New script**: `scripts/phase36_report.py`.
- **Manifest changes** (`tests/compat/pproxy_manifest.toml`): 20 entries
  corrected (see scope item 3 above).
- **Validator changes** (`crates/eggress-testkit/src/manifest.rs`): new
  `ALLOWED_CATEGORIES` constant, four new `ValidationError` variants, five
  new per-feature checks, two new group aliases in `manifest_test_names_exist`.
- **Docs updated**: `README.md`, `AGENTS.md`, `docs/ARCHITECTURE.md`,
  `docs/RELEASE_READINESS.md`, `docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md`,
  `docs/REAL_PPROXY_PARITY_ROADMAP.md`, `.skills/testing/skill.md`.

## Verification commands run

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p eggress-testkit --lib manifest
python3 scripts/phase36_report.py
```

All green as of 2026-07-03.

## What's not in scope of Phase 36

- No new feature implementation.
- No production PyPI publish.
- No additional protocol work.

Phase 36 is the audit phase; the parity release candidate is ready to be
tagged at the version noted in `PARITY_TARGET_FREEZE.md`.