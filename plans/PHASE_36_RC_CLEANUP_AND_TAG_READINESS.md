# Phase 36 RC Cleanup and Tag Readiness Plan

## Purpose

Phase 36 moved the repository into an explicit release-candidate posture, with a final parity report, platform matrix, release notes, migration guide, and GO/NO-GO checklist. The repo is close to tag-ready, but the latest review found a small set of release hygiene issues that should be fixed before tagging `v0.1.0`.

This cleanup pass is deliberately narrow. It should not add features, expand the release scope, or relitigate the parity roadmap. It should make the release artifacts internally consistent, mechanically verifiable, and less confusing for downstream users.

## Current remaining issues

The current RC state is strong, but three pre-tag items remain:

1. Performance baseline link inconsistency:
   - `docs/performance/README.md` links to `BASELINE_2026_07_03.md`.
   - `docs/release/FINAL_PPROXY_PARITY_REPORT.md` and `docs/PHASE_36_FINAL_PARITY_RELEASE_AUDIT_COMPLETION.md` refer to `docs/performance/BASELINE.md`.
   - Either create a `BASELINE.md` index/alias or normalize all references to the dated baseline.

2. Final parity report count/header mismatch:
   - `docs/release/FINAL_PPROXY_PARITY_REPORT.md` says `Compatible features (26)`.
   - The `Inbound TCP (7)` subsection appears to list many more than seven features.
   - The report generator or generated markdown needs grouping/count consistency.

3. Hosted CI caveat must stay prominent:
   - The GO/NO-GO doc correctly says no hosted CI visibility and local verification is the source of truth.
   - Ensure this caveat is repeated in release notes, final report, and any tag checklist so it is not missed.

Optional fourth item if time allows:

4. Verify the final generated JSON artifact path policy:
   - The completion doc references `target/compat/final-pproxy-parity-report.json`.
   - `target/` artifacts are usually build outputs and may not be committed.
   - Decide whether the JSON report should be committed under `docs/release/` or generated-only. Make docs explicit either way.

## Non-goals

Do not add new protocols, CLI flags, Python APIs, system proxy behavior, or performance benchmarks.

Do not change the Phase 36 release decision except to add conditions if a cleanup check fails.

Do not promote any feature tier.

Do not publish or tag the release in this pass.

Do not remove the Python 3.14 / pproxy 2.7.9 compatibility caveat.

## Work items

### R1. Normalize performance baseline references

Choose one canonical approach.

Preferred approach:

- Add `docs/performance/BASELINE.md` as a stable index that points to the latest dated baseline.
- Keep `docs/performance/BASELINE_2026_07_03.md` as the immutable release baseline.
- Update docs to use:
  - `docs/performance/BASELINE.md` when referring to the current/latest baseline.
  - `docs/performance/BASELINE_2026_07_03.md` when referring to the Phase 36 RC baseline specifically.

Tasks:

- Create `docs/performance/BASELINE.md` if absent.
- Include:
  - current release baseline link;
  - date;
  - environment summary;
  - note that historical baselines should be immutable;
  - command used to regenerate or compare.
- Update references in:
  - `docs/release/FINAL_PPROXY_PARITY_REPORT.md`;
  - `docs/PHASE_36_FINAL_PARITY_RELEASE_AUDIT_COMPLETION.md`;
  - `docs/release/PARITY_RELEASE_GO_NO_GO.md`;
  - `docs/performance/README.md` if needed.
- Run a link-grep pass for `BASELINE.md` and `BASELINE_2026_07_03.md`.

Acceptance:

- All performance baseline links resolve.
- Dated and latest/current baseline semantics are clear.

### R2. Fix final parity report compatible-feature grouping

The final report should not have misleading subsection counts.

Tasks:

- Inspect `scripts/phase36_report.py` to determine whether the subsection count is generated or hardcoded.
- Ensure subgroup headings reflect actual rows in each subgroup.
- Options:
  - remove subgroup counts entirely, e.g. `### Inbound TCP` instead of `### Inbound TCP (7)`;
  - or compute exact subgroup counts from manifest category/grouping;
  - or split the current long inbound TCP list into accurate categories.
- Regenerate `docs/release/FINAL_PPROXY_PARITY_REPORT.md` if the script is the source of truth.
- If the markdown was hand-edited, update both markdown and generator so drift does not recur.
- Ensure total compatible count remains consistent with manifest: currently 26.
- Add a lightweight validator or test if practical:
  - parse the generated report headings/counts and compare with manifest counts; or
  - make the generator own all counts and avoid hand-maintained counts.

Acceptance:

- No subsection count contradicts the listed rows.
- Report totals match manifest totals.
- Generated report can be re-run without reintroducing the mismatch.

### R3. Keep hosted-CI caveat prominent

The release is based on local verification and documented CI assumptions. That must remain visible.

Tasks:

- Verify the caveat appears in:
  - `docs/release/PARITY_RELEASE_GO_NO_GO.md`;
  - `docs/release/FINAL_PPROXY_PARITY_REPORT.md`;
  - `docs/release/RELEASE_NOTES_PARITY_RC.md`;
  - `docs/CI_STATUS.md`;
  - README status or release section if appropriate.
- Wording should be precise:
  - Hosted combined status is unavailable/not visible.
  - Local verification commands passed and are recorded.
  - Gated pproxy differential tests require Python 3.11/3.12 due to pproxy 2.7.9 incompatibility with Python 3.14.
  - Do not imply hosted GitHub checks passed if no statuses are visible.
- Ensure GO/NO-GO keeps this as an accepted limitation unless hosted CI is restored and observed green.

Acceptance:

- No release doc implies visible hosted CI is green.
- The no-hosted-CI caveat is visible before any user acts on the RC.

### R4. Clarify generated JSON artifact policy

The Phase 36 docs reference `target/compat/final-pproxy-parity-report.json`. Since `target/` is generally not committed, users may not find it after cloning.

Tasks:

- Decide one of these policies:
  1. Generated-only: `target/compat/final-pproxy-parity-report.json` is not committed; users generate it with `python3 scripts/phase36_report.py`.
  2. Committed release artifact: copy generated JSON to `docs/release/final-pproxy-parity-report.json` or similar.
- Preferred approach: generated-only unless the project wants immutable release artifacts in docs.
- Update:
  - `docs/release/FINAL_PPROXY_PARITY_REPORT.md`;
  - `docs/PHASE_36_FINAL_PARITY_RELEASE_AUDIT_COMPLETION.md`;
  - `docs/release/PARITY_RELEASE_GO_NO_GO.md` if it references local logs/artifacts;
  - `scripts/phase36_report.py` help text or docstring if needed.
- If generated-only, include exact command and output path.

Acceptance:

- Users are not told to expect an uncommitted `target/` artifact in a fresh clone unless generation instructions are adjacent.

### R5. Add a release-doc consistency check

Add a small script or test to prevent these exact problems from recurring.

Potential implementation:

```text
scripts/check_release_docs.py
```

Checks:

- `docs/performance/BASELINE.md` exists if referenced.
- `docs/performance/BASELINE_2026_07_03.md` exists if referenced.
- `docs/release/FINAL_PPROXY_PARITY_REPORT.md` total counts match manifest counts, or at minimum does not include known stale subgroup strings.
- `docs/release/PARITY_RELEASE_GO_NO_GO.md` contains `No hosted CI visibility` or equivalent exact phrase.
- `docs/release/RELEASE_NOTES_PARITY_RC.md` contains the no-hosted-CI caveat.
- `docs/release/FINAL_PPROXY_PARITY_REPORT.md` includes Python 3.14 / pproxy 2.7.9 caveat.

Acceptance:

- A fast local check catches release-doc drift.
- The check is listed in `docs/release/PARITY_RELEASE_GO_NO_GO.md` or `AGENTS.md` for pre-tag use.

### R6. Re-run Phase 36 report generation and local release checks

Commands:

```bash
python3 scripts/phase36_report.py
python3 scripts/check_release_docs.py
cargo test -p eggress-testkit --lib manifest
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

If the cleanup only touches docs/scripts, `cargo test --workspace` is optional but preferred before tagging.

Acceptance:

- Completion record lists exact commands run and outcomes.

### R7. Create cleanup completion record

Create:

```text
docs/PHASE_36_RC_CLEANUP_COMPLETION.md
```

Record:

- baseline link normalization decision;
- report count/grouping fix;
- hosted-CI caveat locations;
- JSON artifact policy;
- release-doc consistency check added;
- commands run;
- remaining pre-tag blockers, if any.

Acceptance:

- Completion record is short, explicit, and suitable for a final handoff before tagging.

## Validation commands

```bash
python3 scripts/phase36_report.py
python3 scripts/check_release_docs.py
cargo test -p eggress-testkit --lib manifest
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Optional full pre-tag check:

```bash
cargo test --workspace
maturin develop
python -m pytest python/tests -q
scripts/test_wheel.sh
cargo deny check
cargo audit
```

## Acceptance criteria

This cleanup pass is complete when:

- Performance baseline links are consistent and resolve.
- The final parity report has no misleading subgroup counts.
- The hosted-CI caveat is prominent in release-facing docs.
- The generated JSON report policy is explicit.
- A release-doc consistency check exists and passes.
- A cleanup completion record exists.
- No new feature scope was introduced.

## Handoff notes

This is the last polish layer before tagging. Keep the diff small. Prefer stable aliases, corrected generated output, and explicit caveats over further feature work.
