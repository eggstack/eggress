# Phase 52: release candidate corrective hardening

## Goal

Correct the remaining release-candidate hygiene issues after Phases 43-51. The repo now has a much stronger parity/release story, but several final certification and workflow claims are stronger than the evidence currently visible. This phase should make the release candidate honest, mechanically checkable, and less brittle.

This is a corrective/hardening pass, not a feature-expansion pass.

## Current state

Recent commits after the Phase 43-51 plan batch delivered meaningful work:

- Parity report now has 109 manifest-backed capabilities and refined caveat sections.
- Trojan inbound/server support appears wired through protocol, server accept, runtime, docs, and manifest.
- SSH and WS/raw/H2 were intentionally classified with ADR-backed non-parity decisions.
- SOCKS BIND was made explicit rather than ambiguous.
- UDP transport validation was added.
- Security docs, release docs, release workflow, fuzz targets, and load tests were added.
- Phase 51 added final certification and go/no-go documents.

The remaining risk is not breadth of capability. The risk is overclaiming certification evidence, workflow fragility, validator coverage gaps, and imprecise security-policy wording.

## Problems to correct

### C1: Final certification overclaims differential evidence

`docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md` says `drop_in` means behavioral parity with pproxy 2.7.9 backed by differential evidence. The generated parity report uses a weaker and more accurate standard: drop-in entries require integration evidence or stronger, not necessarily side-by-side pproxy differential evidence.

This must be corrected. Do not imply every `drop_in` feature has live pproxy differential evidence unless the manifest proves that.

Required wording direction:

- `drop_in`: all required layers complete and evidence is at least integration, with differential evidence where available.
- Distinguish `differential`, `integration`, `unit`, `synthetic`, `docs_only`, and `none` in certification tables.
- Add a certification subsection that lists how many `drop_in` entries are backed by actual `differential` evidence versus `integration` evidence.

### C2: Hosted CI wording conflicts

The certification doc says integration tests pass in CI with isolated network namespaces. The go/no-go checklist says no hosted CI visibility and local verification is the source of truth. These are contradictory.

Correct by choosing one source of truth based on actual evidence:

- If hosted workflow runs are visible and passing, link or reference them.
- If not, state: `Hosted CI status is not verified in this certification; local verification is the source of truth.`

Do not claim tests pass in CI unless the run is visible and checked.

### C3: Release workflow artifact patterns may be invalid

The release workflow uses artifact download patterns like:

```yaml
pattern: "binary-*|wheel-*|sdist"
pattern: "binary-*|wheel-*|sdist|checksums-sbom"
```

`actions/download-artifact` pattern syntax should be verified. If it is minimatch/glob rather than regex alternation, these patterns may be treated literally and miss artifacts.

Hardening options:

- Use multiple explicit download steps.
- Use broader safe patterns and post-filter locally.
- Use artifact names consistently and test with workflow dry-run if available.

### C4: Cross-architecture wheel smoke tests may fail

The release workflow builds wheels for targets that may not match the runner architecture, then installs the generated wheel in the runner's Python environment. Cross-arch install smoke tests should not run unless the built wheel matches the current runner architecture.

Required correction:

- Smoke install only native wheel/runner combinations.
- For cross-built wheels, run metadata checks only unless emulation/native runner exists.
- Document which wheel artifacts are smoke-installed and which are build-only.

### C5: Validator Rule 14 appears scoped incorrectly

The caveat-class validation block appears after the `for cap in capabilities` loop, using the last `cap` value rather than validating each capability. If the observed indentation is accurate, Rule 14 only validates the final manifest entry.

Correct by moving Rule 14 inside the capability loop or by adding a second loop over all capabilities.

Add regression tests proving:

- unknown `caveat_class` is caught on the first, middle, and last capability;
- `config = "refused"` without `caveat_class`/`rationale` is caught on non-last entries;
- `deferred_by_adr` without ADR text is caught;
- `protocol_crate_only` without notes mentioning protocol/crate/refused is caught.

### C6: Private-egress/DNS-rebinding policy wording is imprecise

`DirectConnector` checks resolved domain targets for reserved/private IP ranges, but literal IP targets are not checked before direct connect. The comment says reserved/private IPs are unsuitable for direct outbound connections, which implies literal IPs should also be blocked.

Choose one of two paths:

#### Option A: Strict private-egress blocking

Block both literal IPs and resolved domain IPs by default, with an explicit config option to allow private egress for compatibility/local-network use.

This is stronger security but may break pproxy-compatible local/LAN proxy usage.

#### Option B: DNS-rebinding-only protection

Keep literal IPs allowed for pproxy compatibility, but rename/document the function and policy as DNS-rebinding protection only. Update comments/docs so they do not imply all private IPs are blocked.

Preferred default for pproxy compatibility: Option B, unless the project already has a secure-mode policy flag to preserve compatibility.

### C7: Release docs reference older file names

The Phase 51 certification references older release docs such as `PARITY_TARGET_FREEZE.md`, `FINAL_PPROXY_PARITY_REPORT.md`, `PARITY_RELEASE_GO_NO_GO.md`, and `RELEASE_NOTES_PARITY_RC.md`. The current added files also include `FINAL_PPROXY_PARITY_CERTIFICATION.md`, `GO_NO_GO_CHECKLIST.md`, and `RELEASE_NOTES_PHASE_51.md`.

Audit all release docs for stale cross-links and ensure either:

- the referenced legacy files still exist and are intentionally part of the release set; or
- links are updated to the current file names.

### C8: Completion/certification language says no defects despite known corrective items

The final certification says no code defects were identified. Given the validator Rule 14 scope issue and workflow risks, this should be softened until the corrective pass lands.

Use wording like:

- `No release-blocking runtime defects are known after this corrective pass.`
- `Certification is conditional on hosted CI/release workflow validation if not yet executed.`

## Deliverables

### D1: Certification wording correction

Update:

- `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md`
- `docs/release/GO_NO_GO_CHECKLIST.md`
- `docs/release/RELEASE_NOTES_PHASE_51.md`
- any older release go/no-go docs still referenced

Required content:

- accurate evidence language;
- local vs hosted CI source of truth;
- no unsupported claims that every drop-in feature has differential evidence;
- explicit accepted limitations.

### D2: Validator Rule 14 hardening

Update `scripts/validate_pproxy_parity_manifest.py`:

- move Rule 14 into the per-capability loop or implement an explicit second pass over all capabilities;
- make unknown `caveat_class` an error rather than a warning if `--strict` is used; warning in normal mode is acceptable;
- ensure refused config/runtime entries require `caveat_class` or rationale;
- ensure `deferred_by_adr` mentions ADR;
- ensure `protocol_crate_only` notes mention the protocol crate or refused layer.

Add tests. If no Python test harness exists for this script, add a small test module such as:

- `tests/scripts/test_validate_pproxy_parity_manifest.py`

or add a Rust test in `eggress-testkit` that writes temporary manifests and executes the script.

### D3: Release workflow hardening

Update `.github/workflows/release.yml`:

- replace regex-like artifact patterns with verified `actions/download-artifact`-compatible globs or explicit downloads;
- make cross-arch wheel smoke tests conditional;
- add release workflow validation notes in docs;
- ensure checksums include all intended artifacts exactly once;
- ensure `cargo audit` installation is cached or tolerated without masking failures;
- avoid `|| true` for SBOM generation if the release docs claim SBOM is produced; otherwise document SBOM as best-effort.

### D4: Private-egress policy correction

Update code and docs according to chosen policy.

For DNS-rebinding-only path:

- rename/comment `is_reserved_or_private_ip` use as a domain-resolution guard, or add wrapper `is_reserved_or_private_resolution`;
- update comments in `crates/eggress-core/src/connector.rs`;
- add a doc note explaining literal private IPs remain allowed for pproxy compatibility unless secure policy mode is enabled;
- update `docs/security/SECURE_CONFIGURATION.md` and `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`.

For strict private-egress path:

- add a config flag/policy to preserve compatibility when needed;
- block literal IPs and resolved domains under strict policy;
- add tests for both default and compatibility modes.

Do not accidentally break local test fixtures that use `127.0.0.1` unless strict mode is explicitly enabled.

### D5: Release doc link audit

Add or extend a release-doc checker, if one exists, to validate:

- referenced docs exist;
- capability counts match generated parity report;
- hosted CI caveat language is consistent;
- old and new release notes do not contradict each other;
- release status does not say `CERTIFIED` if required corrective checks fail.

Possible script:

```bash
python3 scripts/check_release_docs.py
```

If the script already exists from earlier phases, extend it.

### D6: Completion record

Add:

- `docs/PHASE_52_RELEASE_CANDIDATE_CORRECTIVE_HARDENING_COMPLETION.md`

It should list:

- exact certification wording changes;
- Rule 14 test coverage;
- workflow hardening changes;
- private-egress policy decision;
- docs audited;
- commands run;
- remaining accepted limitations.

## Files likely to change

- `scripts/validate_pproxy_parity_manifest.py`
- `tests/scripts/test_validate_pproxy_parity_manifest.py` or `crates/eggress-testkit/*`
- `.github/workflows/release.yml`
- `crates/eggress-core/src/connector.rs`
- `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md`
- `docs/release/GO_NO_GO_CHECKLIST.md`
- `docs/release/RELEASE_NOTES_PHASE_51.md`
- `docs/release/RELEASE_PROCESS.md`
- `docs/release/ARTIFACT_MATRIX.md`
- `docs/security/SECURE_CONFIGURATION.md`
- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`
- `docs/parity/README.md`
- `AGENTS.md`
- `.skills/testing/skill.md`

## Acceptance criteria

### Certification accuracy

- Certification docs no longer imply every drop-in feature has differential evidence.
- Certification docs accurately distinguish local verification from hosted CI.
- Go/no-go checklist no longer conflicts with certification text.
- Release status wording is conditional or evidence-backed.

### Validator correctness

- Rule 14 validates every capability, not only the last one.
- Tests prove caveat-class errors/warnings fire for first, middle, and last manifest entries.
- Strict validation still passes on the real manifest.

### Workflow robustness

- Release workflow artifact download patterns are known-valid.
- Cross-arch wheel smoke installs are skipped or emulated appropriately.
- Checksums/SBOM steps either fail on real failure or are documented as best-effort.

### Security-policy clarity

- Private-egress/DNS-rebinding behavior is accurately documented.
- Code comments match actual behavior.
- Tests cover chosen behavior.

### Documentation consistency

- Release docs link to existing files.
- Capability counts match the generated parity report.
- No stale `full differential evidence` or `CI passed` language remains without evidence.

## Verification commands

Run at minimum:

```bash
cargo fmt --all -- --check
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
python -m pytest tests/scripts/test_validate_pproxy_parity_manifest.py -v
python -m pytest python/tests -v
```

If a release-doc checker exists or is added:

```bash
python3 scripts/check_release_docs.py
```

For workflow validation, run the release workflow in dry-run/manual mode where practical or document why it cannot be run locally.

## Non-goals

- Do not add new proxy protocols.
- Do not reopen SSH/QUIC/H3 feature decisions.
- Do not promote additional capabilities to `drop_in`.
- Do not publish artifacts.
- Do not weaken the manifest/report generation loop.

## Handoff notes

Treat this as a release-candidate truthfulness pass. The implementation is now broad enough that the main risk is not missing features; it is release documentation claiming more than the evidence supports. Keep the compatibility claims conservative and source-of-truth driven.
