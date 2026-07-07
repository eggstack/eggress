# Phase 53: release cleanup pass

## Goal

Close the remaining small release-candidate cleanup issues after Phase 52. The prior corrective pass fixed the main truthfulness and validator-scope problems, but a few release workflow and documentation inconsistencies remain. This phase should be narrow, mechanical, and verification-oriented.

Do not add new proxy capabilities in this phase.

## Current state

Phase 52 landed a useful hardening commit:

- `scripts/validate_pproxy_parity_manifest.py` now runs Rule 14 caveat-class validation inside the per-capability loop.
- `tests/scripts/test_validate_pproxy_parity_manifest.py` adds regression coverage for first, middle, and last manifest entries.
- `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md` no longer claims all drop-in entries have differential evidence.
- Release artifact downloads were split into explicit binary/wheel/sdist downloads instead of pipe-delimited artifact patterns.
- Cross-architecture wheel smoke testing is conditional.
- DNS-rebinding/private-egress wording now explicitly says literal IP targets bypass the domain-resolution guard for pproxy compatibility.

Remaining cleanup items are localized and should be fixed before treating the release workflow as ready.

## Problems to correct

### C1: Go/no-go checklist still says tests pass in CI

`docs/release/GO_NO_GO_CHECKLIST.md` still states that integration tests have an environment limitation but "Tests pass in CI." The same file also says there is no hosted CI visibility and local verification is the source of truth.

This remains contradictory.

Required correction:

- Replace "Tests pass in CI" with "Hosted CI status is not verified in this checklist; local verification is the source of truth."
- Keep integration tests listed as environment-limited unless hosted CI evidence is actually present and linked.
- Ensure the certification doc, go/no-go checklist, release notes, and completion record all use the same wording.

### C2: Release workflow likely generates invalid maturin target triples

The wheel build command currently constructs target triples with:

```yaml
maturin build --release --target ${{ matrix.target }}-unknown-${{ startsWith(matrix.os, 'macos') && 'apple-darwin' || (startsWith(matrix.os, 'windows') && 'pc-windows-msvc' || 'linux-gnu') }} --out dist
```

This likely emits invalid target triples:

- `x86_64-unknown-apple-darwin` instead of `x86_64-apple-darwin`
- `x86_64-unknown-pc-windows-msvc` instead of `x86_64-pc-windows-msvc`

Required correction:

- Replace string interpolation with explicit target triples in the matrix.
- Add fields such as `rust_target`, `wheel_smoke`, and possibly `python_arch`.
- Use `rust_target` for `dtolnay/rust-toolchain` target installation and `maturin build --target`.
- Ensure native smoke-test cases are explicit rather than inferred through fragile expressions.

Suggested matrix shape:

```yaml
matrix:
  include:
    - os: ubuntu-latest
      target: x86_64
      rust_target: x86_64-unknown-linux-gnu
      wheel_smoke: true
    - os: ubuntu-latest
      target: aarch64
      rust_target: aarch64-unknown-linux-gnu
      wheel_smoke: false
    - os: macos-13
      target: x86_64
      rust_target: x86_64-apple-darwin
      wheel_smoke: true
    - os: macos-latest
      target: aarch64
      rust_target: aarch64-apple-darwin
      wheel_smoke: true
    - os: windows-latest
      target: x86_64
      rust_target: x86_64-pc-windows-msvc
      wheel_smoke: true
```

If `macos-latest` is arm64 at the time this is executed, keep the above. If not, choose a runner explicitly and document the choice.

### C3: Release body still references older release notes

`.github/workflows/release.yml` still points the GitHub Release body to `docs/release/RELEASE_NOTES_PARITY_RC.md` rather than `docs/release/RELEASE_NOTES_PHASE_51.md`.

Required correction:

- If `RELEASE_NOTES_PARITY_RC.md` is still intentionally the canonical release-note file, document that and ensure Phase 51 notes point to it.
- Otherwise update the release workflow body to reference `docs/release/RELEASE_NOTES_PHASE_51.md`.
- Ensure docs do not contain stale references to non-canonical release notes.

Preferred: use `RELEASE_NOTES_PHASE_51.md` as the canonical current release-note file unless there is a deliberate final filename migration.

### C4: Strict real-manifest validator test does not assert success

`tests/scripts/test_validate_pproxy_parity_manifest.py` includes `test_real_manifest_strict`, but the test only prints output if strict validation fails. It does not assert `exit_code == 0`.

Required correction:

- Replace the print-only behavior with an assertion:

```python
assert exit_code == 0, f"Strict validator failed on real manifest:\n{output}"
```

- Keep normal-mode manifest test assertion.
- Run the new test module.

### C5: SBOM artifact upload may fail when best-effort SBOM is absent

Phase 52 changed SBOM generation to best-effort, but upload still lists `sbom.json`. If SBOM generation fails and no file exists, `actions/upload-artifact` may warn or fail depending on action behavior/config.

Required correction:

- If SBOM is best-effort, ensure placeholder metadata is created when generation fails, e.g. `sbom.json` containing a clear JSON object with status `skipped` and reason.
- Alternatively configure artifact upload to tolerate missing `sbom.json`, but explicit placeholder is preferable for release auditability.
- Ensure release docs state SBOM is best-effort until workflow execution proves it stable.

### C6: Checksums find expression has duplicate `*.tar.gz`

The checksum generation command includes `-name "*.tar.gz"` twice. This is harmless but sloppy.

Required correction:

- Remove the duplicate pattern.
- Consider including `.sig`, `.cert`, and `.json` only if intentionally checksummed.
- Ensure checksums cover binaries, wheels, and sdist exactly once.

## Deliverables

### D1: Go/no-go wording cleanup

Update:

- `docs/release/GO_NO_GO_CHECKLIST.md`
- `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md` if needed for consistency
- `docs/release/RELEASE_NOTES_PHASE_51.md` if needed for consistency
- `docs/PHASE_52_RELEASE_CANDIDATE_CORRECTIVE_HARDENING_COMPLETION.md` only if the completion record is meant to remain current; otherwise create a Phase 53 completion record instead.

### D2: Release workflow matrix cleanup

Update `.github/workflows/release.yml`:

- add explicit `rust_target` to wheel matrix;
- use `rust_target` for toolchain target install and maturin build;
- add explicit `wheel_smoke` boolean;
- use `if: matrix.wheel_smoke == true` for smoke test;
- fix release-note link in release body;
- create placeholder SBOM when best-effort generation fails;
- clean checksum file patterns.

### D3: Validator strict-test assertion

Update `tests/scripts/test_validate_pproxy_parity_manifest.py`:

- assert strict validation success on real manifest;
- remove print-only non-enforcing behavior;
- ensure tests still pass.

### D4: Completion record

Add:

- `docs/PHASE_53_RELEASE_CLEANUP_PASS_COMPLETION.md`

It should include:

- exact workflow matrix changes;
- go/no-go wording correction;
- release-note canonical-file decision;
- strict-test enforcement;
- SBOM placeholder behavior;
- verification commands run;
- remaining accepted limitations.

## Files likely to change

- `.github/workflows/release.yml`
- `docs/release/GO_NO_GO_CHECKLIST.md`
- `docs/release/RELEASE_NOTES_PHASE_51.md`
- `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md` if consistency requires
- `tests/scripts/test_validate_pproxy_parity_manifest.py`
- `docs/PHASE_53_RELEASE_CLEANUP_PASS_COMPLETION.md`
- `AGENTS.md` / `.skills/testing/skill.md` only if release verification commands change materially

## Acceptance criteria

- Go/no-go checklist no longer says tests pass in CI unless actual hosted CI evidence is linked.
- Release workflow uses explicit valid Rust target triples for wheels.
- Cross-architecture wheel smoke tests are skipped by explicit matrix field, not fragile derived logic.
- GitHub Release body points to the canonical current release notes.
- Real-manifest strict validator test fails if strict validation fails.
- SBOM best-effort behavior still produces a clear artifact or is explicitly tolerated.
- Checksums command has no duplicate patterns and covers intended artifacts.
- Phase 53 completion record exists.

## Verification commands

Run at minimum:

```bash
cargo fmt --all -- --check
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
python -m pytest tests/scripts/test_validate_pproxy_parity_manifest.py -v
```

For workflow syntax, run one of:

```bash
yamllint .github/workflows/release.yml
```

or, if available:

```bash
gh workflow view release.yml
```

If neither is available, manually inspect generated target triples and note that hosted workflow execution remains unverified.

## Non-goals

- Do not add new protocols.
- Do not reopen the Phase 47/48 ADR decisions.
- Do not change manifest capability tiers unless a real mismatch is found.
- Do not publish release artifacts.
- Do not claim hosted CI is passing unless the run is visible and checked.

## Handoff note

This is the last small cleanup layer after Phase 52. Keep it focused. The objective is to make the release workflow and release documents mechanically consistent, not to expand the release scope.
