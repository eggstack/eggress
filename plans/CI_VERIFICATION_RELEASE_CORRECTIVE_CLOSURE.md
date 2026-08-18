# CI, Verification, and Release Apparatus — Corrective Closure Pass

## Status

**READY FOR IMPLEMENTATION**

## Baseline

- Repository: `eggstack/eggress`
- Baseline branch: `main`
- Baseline commit: `3e1995f079d83818b4333f37eaee7712918dbd7b`
- Parent plan: `plans/CI_VERIFICATION_RELEASE_REDUCTIVE_FOLLOW_UP.md`
- Parent implementation commit: `fbf8000bd3b399c4a2592b74a7bf1a820f3633df`
- Python smoke correction commit: `3e1995f079d83818b4333f37eaee7712918dbd7b`

## Purpose

Close the remaining defects from the CI, verification, and release reduction without restoring the deleted workflow matrix, release train, evidence platform, or mandatory specialist checks.

The previous implementation achieved the major structural reduction:

- hosted GitHub Actions remains limited to one Rust smoke job and one path-scoped Python smoke job;
- automated crates.io, PyPI, GitHub Release, container, SBOM, signing, checksum, and artifact publication remain removed;
- obsolete release-process documents and release evidence scripts were deleted;
- the old five-tier oracle model was reduced;
- internal Rust crates were marked `publish = false` rather than exposing the entire workspace as a public release train;
- active release guidance now states that publication is manual.

The line of work is not closed because the implementation left several explicit acceptance criteria unmet:

1. `scripts/run_pproxy_certification.sh` is still a 22-gate whole-repository closure audit under a behavior-specific filename.
2. That script still runs formatting, workspace checks, Clippy, full workspace tests, dependency audits, report freshness, wheel construction, and the full Python suite.
3. It still writes `target/closure-audit`, one log per gate, JUnit output, and a Markdown closure report instead of a compact compatibility result.
4. Its visible headings and output still use “Milestones A-C Final Closure Audit” terminology.
5. `docs/PYPI_RELEASE.md` still recommends replacement-style upload semantics with `twine ... --replace`, contradicting immutable roll-forward policy.
6. The Python release example invokes `deactivate` without activating the virtual environment.
7. Smoke-job runtime measurements and a deliberate PR-versus-push trigger decision were not recorded.
8. The Python smoke workflow required a follow-up repair and has no confirmed hosted success at the current baseline.
9. GitHub branch protection, required checks, Actions permissions, publishing environments, trusted publishers, and release secrets remain unverified.
10. CI terminology remains embedded in the compatibility model through `CiTier`, `CiTierConfig`, `ci_tier`, `oracle/ci.rs`, and `generate_ci_summary`.
11. `docs/CI_STATUS.md` still refers to a “strict closure audit.”
12. The parent plan still says `READY FOR IMPLEMENTATION` despite substantial but incomplete implementation.

This corrective pass addresses only those residual defects. It must remain deletion-first and narrowly bounded.

---

# 1. Required final outcome

The line of work is complete only when every statement below is true.

1. The repository still contains at most two automatic GitHub Actions workflows.
2. No automatic workflow publishes any package, release, image, signature, checksum, SBOM, evidence bundle, or GitHub Release.
3. `scripts/run_pproxy_certification.sh` performs only pproxy compatibility setup and behavior validation.
4. The certification command does not invoke `cargo fmt`, `cargo check`, `cargo clippy`, `cargo test --workspace`, `cargo deny`, `cargo audit`, release-document checks, package dry-runs, release artifact construction, or the unrestricted Python test suite.
5. Candidate environment construction required to execute compatibility tests is treated as setup, not as a release packaging gate or retained release artifact.
6. Every retained certification test has a direct pproxy compatibility purpose and an explicit required/optional classification.
7. Certification creates one compact `summary.json` plus failure diagnostics only when needed.
8. No successful certification run is required to produce a Markdown report, JUnit report, per-gate logs, hash index, or large artifact tree.
9. Certification fails closed on wrong oracle version, missing observations, comparator failure, timeout, malformed result, or required interop failure.
10. No script or current document calls the certification command a repository closure audit or release gate.
11. Compatibility selection uses behavior-oriented profile terminology rather than CI-tier terminology.
12. Structural tests that do not need pproxy remain ordinary ungated tests.
13. External pproxy differential and platform-dependent checks remain deliberate and opt-in.
14. `docs/PYPI_RELEASE.md` contains no replacement-upload instruction.
15. A broken PyPI release is handled by yanking when appropriate, incrementing the version, rebuilding from a clean output directory, and publishing the new version.
16. Python release commands do not invoke `deactivate` unless the environment was actually activated.
17. The Rust and Python smoke workflows have measured durations recorded in the implementation pull request or commit summary.
18. The workflow trigger model is intentional and does not create avoidable PR-plus-post-merge duplication.
19. A hosted run proves the corrected Python smoke job can complete on the current workflow definition.
20. A hosted run proves the Rust smoke job can complete on the current workflow definition.
21. Branch protection does not require deleted checks.
22. The path-scoped Python smoke job is not a required check unless it is changed to report on every protected change.
23. Actions default permissions are read-only and ordinary CI requires no release secret.
24. Unused PyPI/TestPyPI environments, trusted-publisher bindings, package-write settings, and release secrets are removed or explicitly confirmed absent.
25. `docs/CI_STATUS.md`, `docs/TESTING.md`, `AGENTS.md`, `.skills/testing/skill.md`, and `docs/DIFFERENTIAL_TESTING.md` agree on the final responsibility split.
26. The parent reductive plan is marked partially implemented and superseded by this corrective closure plan, or otherwise given a truthful terminal status.
27. This plan is marked complete only after repository checks and GitHub settings checks are both proven.
28. Crates.io publication remains truthfully blocked until a separate crate-boundary plan is implemented; this pass does not publish internal crates.
29. No new workflow, matrix, release framework, task runner, generated manifest, or evidence platform is introduced.
30. The corrective pass is net-negative or neutral in process code and terminology.

---

# 2. Scope

## In scope

- `scripts/run_pproxy_certification.sh`;
- compatibility setup, execution, output, and failure semantics;
- direct callers and documentation for the certification command;
- `crates/eggress-testkit/src/oracle/ci.rs`;
- `crates/eggress-testkit/src/oracle/report.rs`;
- module exports and tests affected by profile terminology;
- current compatibility environment variables used only inside this repository;
- `.github/workflows/ci.yml`;
- `.github/workflows/python-test.yml`;
- smoke-job runtime measurement and trigger policy;
- `docs/PYPI_RELEASE.md`;
- `docs/CI_STATUS.md`;
- `docs/TESTING.md`;
- `docs/DIFFERENTIAL_TESTING.md`;
- `AGENTS.md`;
- `.skills/testing/skill.md`;
- GitHub branch protection/rulesets;
- GitHub Actions permissions, environments, secrets, and trusted publishing configuration;
- truthful status updates to this plan and its parent plan.

## Out of scope

Do not use this pass to:

- implement new pproxy behavior;
- weaken or delete a behavioral compatibility test merely because it is slow or failing;
- declare full pproxy parity without the applicable differential result;
- restructure the Rust crate graph for crates.io publication;
- publish internal crates;
- publish to crates.io or PyPI;
- add a release workflow;
- add GitHub Release automation;
- add cross-platform matrices;
- add scheduled audits;
- add coverage gates;
- add benchmark gates;
- add `xtask`, `just`, `cargo-make`, release-plz, cargo-release, semantic-release, or another orchestration layer;
- add a replacement evidence uploader;
- create a permanent completion report;
- rewrite every historical phase document;
- remove useful unit, integration, property, fuzz-smoke, differential, or interoperability tests;
- claim GitHub settings changes based solely on repository files.

---

# 3. Non-negotiable corrective rules

1. **Compatibility certification is not repository verification.** Formatting, lint, workspace tests, audits, and package checks remain separate commands.
2. **Setup is not a gate category.** Creating isolated oracle and candidate environments is necessary execution setup, not a release artifact or separately promoted success claim.
3. **No global environment mutation.** The certification command must not install packages into system Python or a user environment.
4. **No implicit prerequisite installation outside isolated environments.** Missing host tools such as Python, Cargo, or maturin fail with a concise preflight message.
5. **No release artifact directory.** Compatibility execution must not write to `dist/` or treat wheels as retained release outputs.
6. **No broad test substitution.** Removing `cargo test --workspace` from certification does not permit removing the targeted compatibility tests that replace it.
7. **Every retained certification gate must answer a compatibility question.** “General confidence” is not sufficient justification.
8. **No Markdown evidence requirement.** Machine-readable summary plus failure diagnostics is sufficient.
9. **Failure must be visible in the exit status.** No required failure may be downgraded to warning or skip.
10. **Optional means technically inapplicable, not inconvenient.** Platform checks may be optional on an incompatible host; ordinary differential failures are not optional.
11. **Structural tests run normally.** Tests that need no oracle should not require an environment variable.
12. **Remove CI vocabulary from manual certification.** A profile describes behavior selection, not a GitHub job.
13. **No upload replacement semantics.** Published package versions are repaired by roll-forward.
14. **Do not require a path-scoped check.** A check that does not run on every protected change cannot be the sole required merge check.
15. **No duplicated automatic execution without a reason.** Choose the trigger model from actual repository policy.
16. **GitHub settings evidence is concise.** Record names and resulting state, never secret values or screenshots containing sensitive data.
17. **Crates.io blocking remains honest.** Do not weaken `publish = false` solely to make a dry run pass.
18. **Update status last.** Documentation cannot mark closure before behavioral, workflow, and settings checks complete.

---

# 4. Execution order

Implement in this order:

1. CC0 — freeze the exact baseline and residual inventory;
2. CC1 — narrow the pproxy certification command;
3. CC2 — remove CI-tier semantics from compatibility selection;
4. CC3 — correct Python release immutability and command accuracy;
5. CC4 — validate and bound the two smoke workflows;
6. CC5 — close GitHub repository settings;
7. CC6 — align active documentation and close plan status;
8. CC7 — run final rejection searches and acceptance checks.

Do not change GitHub required checks before the final workflow job names are known. Do not mark either plan complete before CC5 is performed against repository settings.

---

# Workstream CC0 — Freeze the corrective baseline

## Objective

Create a concise, truthful inventory of the exact residual defects before editing.

## Required inspection

Run from the repository root:

```bash
git rev-parse HEAD
find .github/workflows -maxdepth 1 -type f -print | sort

rg -n \
  "MILESTONES A-C FINAL CLOSURE AUDIT|closure-audit|CLOSURE_AUDIT|strict closure audit|run_pproxy_certification" \
  scripts docs README.md AGENTS.md .skills plans crates

rg -n \
  "cargo fmt|cargo check|cargo clippy|cargo test --workspace|cargo deny|cargo audit|maturin build|python_test_suite|junit|CLOSURE_AUDIT_REPORT" \
  scripts/run_pproxy_certification.sh

rg -n \
  "CiTier|CiTierConfig|ci_tier|oracle::ci|generate_ci_summary|all_tier_configs|tier_gate_enabled|assign_tiers|scenarios_for_tier" \
  crates docs .skills scripts

rg -n -- \
  "--replace|deactivate|publish-pypi\.yml|trusted publisher|Hosted CI.*non-functional|billing issue" \
  docs/PYPI_RELEASE.md docs README.md AGENTS.md .skills
```

## Required inventory output

Record in the implementation PR description or commit summary, not a new permanent file:

- current head SHA;
- current workflow files and job names;
- current required certification gates;
- current output directories and report files;
- current profile enum, serialized fields, module names, and environment variables;
- current PyPI correction instructions;
- current known hosted workflow status;
- GitHub settings that cannot be inferred from the tree.

## Acceptance criteria

- The inventory names the exact current certification gates rather than relying on the prior review.
- Every later deletion or rename maps to an inventoried item.
- No compatibility test is removed before its purpose and caller are identified.
- The baseline SHA is updated if implementation does not start from `3e1995f079d83818b4333f37eaee7712918dbd7b`.

---

# Workstream CC1 — Make pproxy certification behavior-specific

## Objective

Replace the renamed whole-repository closure audit with a direct, fail-closed pproxy behavioral certification command.

## Primary target

- `scripts/run_pproxy_certification.sh`

## Related targets

- `scripts/run_strict_pproxy_api.sh`;
- `scripts/run_strict_pproxy_api.py`;
- `scripts/run_strict_pproxy_interop.sh`;
- `scripts/compat_udp_pproxy.sh`;
- Python strict/differential tests;
- testkit oracle runner entry points;
- any script that hard-codes `target/closure-audit`.

## Required removals from certification

Remove these as certification gates:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo audit
strict generated-report freshness
canonical release wheel build into dist/
compat release wheel build into dist/
full python/tests suite
JUnit report generation
Markdown closure report generation
one-log-per-successful-gate retention
```

These commands remain available in their correct owners:

- formatting, lint, and workspace tests: ordinary repository verification;
- dependency audits: dependency changes and release preparation;
- package builds and metadata: Python release preparation;
- generated compatibility reports: explicit report-generation work, not every certification run.

## Required compatibility stages

The final command should have a small sequence resembling the following. Exact implementation may reuse existing runners, but responsibilities must remain explicit.

### Stage 1 — Host preflight

Check without installing globally:

- `git`;
- `cargo`;
- `python3` at a supported version;
- `maturin` if candidate environment construction requires it;
- shell tools directly used by retained scripts.

A missing host tool must produce one concise message and exit nonzero.

Do not run `pip install` against system Python. Do not use `sudo`.

### Stage 2 — Clean isolated environments

Use a compatibility-specific root:

```text
target/pproxy-certification/
  oracle-venv/
  candidate-venv/
  observations/
  failures/
  summary.json
```

Before each run:

- remove only `target/pproxy-certification/`;
- recreate the required directories;
- never delete unrelated `.venv` directories;
- never write candidate packages to `dist/`;
- never use a stale installed extension from the source tree.

### Stage 3 — Freeze the oracle

The oracle environment must:

- install exactly `pproxy==2.7.9` from the canonical pinned requirements/provenance source already in the repository;
- verify the installed distribution version before tests begin;
- record Python version, platform, and pproxy version in `summary.json`;
- fail before candidate comparisons if the version is wrong;
- never fall back to a system-installed pproxy.

### Stage 4 — Construct the candidate environment

Build only what is needed to execute candidate behavior.

Preferred approach:

- create the candidate venv under `target/pproxy-certification/`;
- install test dependencies there;
- run `maturin develop` against `crates/eggress-python` using that venv;
- install `python-pproxy-compat` from the local tree without retaining a release wheel;
- prove `import eggress` and `import pproxy` resolve from the intended candidate environment.

If `maturin develop` cannot provide the required isolation, a temporary wheel may be built inside `target/pproxy-certification/` and deleted after installation. It must not be emitted to `dist/`, uploaded, hashed as a release artifact, or presented as a packaging gate.

### Stage 5 — Run paired behavioral observations

Retain the existing paired runner where it directly compares oracle and candidate behavior.

Required properties:

- required records produce both oracle and candidate observations;
- missing observation is failure;
- malformed observation is failure;
- wrong schema/version is failure;
- timeout is failure;
- equal exceptions are not automatically success unless the record explicitly expects that exception behavior;
- output is written beneath `target/pproxy-certification/observations/`.

### Stage 6 — Run targeted compatibility suites

Create a gate table in comments or a compact shell data structure. Every retained command must cite its compatibility purpose.

Expected retained categories are:

- paired public Python API behavior;
- strict Python differential tests consuming paired observations;
- pproxy transport/connection differential cases;
- required TCP interoperability;
- required UDP interoperability for supported UDP claims;
- cipher known-answer or round-trip tests only when they validate a claimed pproxy-compatible cipher path;
- plugin transformation tests only when they validate a claimed pproxy plugin contract;
- listener/process lifecycle tests only when they validate an observable pproxy-compatible lifecycle contract;
- resource cleanup tests only when directly tied to compatibility failure semantics.

Do not retain a test merely because it existed in the old 22-gate script. Link it to a manifest capability, compatibility claim, or named contract. Tests without that link remain ordinary targeted tests outside certification.

### Stage 7 — Produce compact output

Write exactly one required success artifact:

```text
target/pproxy-certification/summary.json
```

Suggested minimal schema:

```json
{
  "schema_version": 1,
  "commit": "<full sha>",
  "profile": "differential",
  "oracle": {
    "distribution": "pproxy",
    "version": "2.7.9",
    "python": "<version>",
    "platform": "<platform>"
  },
  "candidate": {
    "python": "<version>",
    "platform": "<platform>"
  },
  "result": "pass",
  "passed": 0,
  "failed": 0,
  "skipped": 0,
  "checks": []
}
```

Constraints:

- use a stable schema version;
- include full commit SHA;
- include each required check name and result;
- include a skip only for a technically inapplicable optional platform check;
- retain detailed stdout/stderr only for failed checks under `failures/`;
- do not generate Markdown;
- do not generate JUnit unless a direct current consumer is proven;
- do not write successful per-check logs by default;
- a `--verbose` option may retain logs locally but must not be required.

### Stage 8 — Exit truthfully

Exit nonzero when:

- preflight fails;
- oracle version is wrong;
- candidate import fails;
- required observation is missing;
- a required comparator fails;
- a required interop command fails;
- summary generation fails;
- a required check is unexpectedly skipped.

Exit zero only when all required checks pass.

## Required script cleanup

Rename all remaining closure-oriented identifiers:

```text
AUDIT_DIR              -> CERT_DIR
CLOSURE_AUDIT_REPORT   -> remove
MILESTONES A-C ...     -> pproxy behavioral certification
AUDIT PASSED/FAILED    -> CERTIFICATION PASSED/FAILED
```

Remove the unused optional-gate helper if no technically optional profile checks remain.

## Required validation

```bash
bash -n scripts/run_pproxy_certification.sh

! rg -n \
  "cargo fmt|cargo check --workspace|cargo clippy|cargo test --workspace|cargo deny|cargo audit|check_release_docs|twine|cargo publish|SBOM|SHA256|GitHub Release" \
  scripts/run_pproxy_certification.sh

! rg -n \
  "closure-audit|CLOSURE_AUDIT|MILESTONES A-C FINAL CLOSURE|CLOSURE_AUDIT_REPORT|junit" \
  scripts/run_pproxy_certification.sh
```

Run the corrected command once on a supported host and inspect `summary.json`.

## Failure-injection proof

Provide automated tests where practical, or a documented one-time implementation check in the PR summary, for all of these cases:

1. Change the expected oracle version in a temporary copy: certification exits nonzero before comparisons.
2. Remove one required observation in a temporary run directory: certification exits nonzero.
3. Force one comparator to return mismatch in an isolated test: certification exits nonzero.
4. Run with a missing required host tool in a controlled PATH: preflight exits nonzero without global installation.
5. Confirm a successful run creates `summary.json` and no Markdown/JUnit report.

Do not commit deliberately broken source to prove these cases.

## Acceptance criteria

- Certification has one purpose: pproxy behavioral validation.
- The old general repository/release gates are absent.
- Candidate setup does not create retained release artifacts.
- No global Python environment is modified.
- Every retained test has a compatibility mapping.
- Success output is compact.
- Required failures propagate to the process exit status.
- Existing real compatibility defects remain failures rather than skips.

---

# Workstream CC2 — Replace CI tiers with certification profiles

## Objective

Finish the semantic decoupling of compatibility selection from deleted GitHub Actions tiers.

## Target files

- `crates/eggress-testkit/src/oracle/ci.rs`;
- `crates/eggress-testkit/src/oracle/report.rs`;
- `crates/eggress-testkit/src/oracle/mod.rs`;
- direct tests and call sites;
- `docs/DIFFERENTIAL_TESTING.md`;
- `.skills/testing/skill.md`;
- compatibility runner environment-variable documentation.

## Required design

Use at most two explicit certification profiles:

- `differential` — pinned pproxy oracle plus ordinary supported compatibility scenarios;
- `platform` — platform-specific or privileged scenarios selected deliberately.

Structural tests that require no pproxy are normal unit/integration tests and should not be modeled as a gated certification profile.

## Required renames

Where the concepts remain necessary, rename them behaviorally:

```text
CiTier                 -> CertificationProfile
CiTierConfig           -> CertificationProfileConfig, or remove
ci_tier                -> certification_profile, or profile
oracle/ci.rs           -> oracle/profile.rs or oracle/profiles.rs
generate_ci_summary    -> generate_profile_summary, or remove
all_tier_configs       -> all_profiles, or remove
tier_gate_enabled      -> profile_enabled, or remove
assign_tiers           -> assign_profiles, or direct predicate
scenarios_for_tier     -> scenarios_for_profile
```

Prefer deletion over one-for-one renaming when direct predicates are simpler.

## Environment-variable reduction

Audit every current caller of:

```text
EGRESS_ORACLE
EGRESS_ORACLE_EXTENDED
EGRESS_ORACLE_PLATFORM
```

Preferred final model:

```text
EGRESS_PPROXY_CERTIFY=1
EGRESS_PPROXY_PLATFORM=1
```

Rules:

- structural tests run without either variable;
- ordinary differential certification uses `EGRESS_PPROXY_CERTIFY=1`;
- platform-only scenarios additionally use `EGRESS_PPROXY_PLATFORM=1`;
- remove old variables if all callers are repository-internal;
- use a temporary compatibility alias only if a real external consumer is identified;
- do not retain aliases merely out of caution.

## Serialization decision

Search for checked-in or externally consumed JSON containing `ci_tier`.

- If no current consumer exists, rename the serialized field to `profile` and update tests.
- If current backward reading is useful, accept `ci_tier` as a deserialization alias while emitting only `profile`.
- Do not maintain two emitted fields.

## Profile selection rules

- `differential`: scenarios requiring a pproxy oracle or external pproxy-compatible process;
- `platform`: scenarios requiring root, a specific OS, or another platform-only capability;
- structural schema and parser tests: no profile, normal test suite;
- expected duration alone must not create another profile.

## Required tests

- all differential scenarios map to `differential`;
- root/OS-specific scenarios map to `platform`;
- structural tests execute with no environment variable;
- profile selection returns every eligible scenario exactly once;
- old environment variables are absent when no compatibility alias is justified;
- serialized output uses the final field name.

## Rejection searches

```bash
! rg -n \
  "CiTier|CiTierConfig|ci_tier|oracle::ci|generate_ci_summary|all_tier_configs|tier_gate_enabled|assign_tiers|scenarios_for_tier" \
  crates docs .skills scripts
```

Allow historical occurrences under `plans/` only when they are clearly part of the baseline description.

## Acceptance criteria

- Compatibility code no longer describes manual profiles as CI tiers.
- Structural tests are ungated.
- External tests remain opt-in.
- At most two manual profiles remain.
- Selection code and environment variables decrease.
- No workflow is added to consume the profiles.

---

# Workstream CC3 — Correct Python release immutability and command accuracy

## Objective

Make `docs/PYPI_RELEASE.md` a reliable, fully manual, immutable roll-forward procedure.

## Target file

- `docs/PYPI_RELEASE.md`

## Required corrections

1. Delete this command and any equivalent replacement-upload guidance:

```bash
twine upload --repository pypi --replace dist/*
```

2. State the repair sequence precisely:

- determine whether to yank the defective version through the PyPI project interface or supported registry API;
- correct the repository;
- increment the package version;
- remove stale build outputs;
- rebuild both packages;
- test both packages together in a clean environment;
- run `twine check`;
- upload the new version;
- verify installation of the exact new version.

3. Clean output before building:

```bash
rm -rf dist
mkdir -p dist
```

4. Remove `deactivate` because the documented commands invoke the virtual environment by explicit path rather than sourcing `activate`.

5. Ensure the procedure does not upload stale artifacts from an earlier version.

6. Keep TestPyPI optional, not mandatory for every release.

7. Keep Python release separate from crates.io and GitHub tags.

8. Keep credentials local; do not restore trusted publishing or GitHub Actions instructions.

## Preferred command flow

The document should contain one concise sequence equivalent to:

```bash
rm -rf dist .venv-release-test
mkdir -p dist

(cd crates/eggress-python && maturin build --release --out ../../dist)
(cd crates/eggress-python && maturin sdist --out ../../dist)
python3 -m pip wheel --no-deps --wheel-dir dist ./python-pproxy-compat

python3 -m venv .venv-release-test
.venv-release-test/bin/pip install --upgrade pip
.venv-release-test/bin/pip install dist/eggress-*.whl
.venv-release-test/bin/pip install dist/eggress_pproxy_compat-*.whl
.venv-release-test/bin/pip install pytest pytest-asyncio
.venv-release-test/bin/python -m pytest python/tests tests/compat -q
.venv-release-test/bin/python -c "import eggress, pproxy; print(eggress.__version__)"

python3 -m twine check dist/*
python3 -m twine upload dist/*
```

The final exact commands must match actual package filenames and supported interpreter behavior.

## Required rejection searches

```bash
! rg -n -- "--replace|deactivate|publish-pypi\.yml|trusted publisher|GitHub Actions" docs/PYPI_RELEASE.md
```

## Acceptance criteria

- No replacement upload is recommended.
- A defective release always rolls forward to a new version.
- Build output starts clean.
- Both Python distributions are tested together.
- Commands do not assume an activated shell environment.
- No GitHub publishing mechanism is described.

---

# Workstream CC4 — Validate and bound the two smoke workflows

## Objective

Prove that the retained workflows are runnable, proportionate, and triggered intentionally.

## Target files

- `.github/workflows/ci.yml`;
- `.github/workflows/python-test.yml`;
- `docs/CI_STATUS.md` only after the final trigger decision.

## Step 1 — Validate workflow syntax and command ownership

Confirm:

- exactly two workflow files exist;
- both use read-only permissions;
- neither has package or release permissions;
- neither contains a matrix;
- neither uploads artifacts;
- neither invokes pproxy certification;
- neither invokes dependency audits;
- neither invokes release commands.

Use repository review plus a YAML parser available in the implementation environment. Do not add a YAML-validation workflow.

## Step 2 — Validate Python smoke semantics

The current smoke job uses `maturin develop`, which is appropriate for routine native-extension execution and intentionally does not certify a distributable release wheel.

Check that:

- the venv is the interpreter used by `maturin`, `pip`, and `pytest`;
- no stale native extension can be imported;
- the core binding imports from the intended checkout/build;
- the compatibility package imports from the intended local package;
- no system Python package shadows either package;
- the job exits nonzero on test failure;
- the job does not build into `dist/`.

Prefer the simplest compat installation:

```bash
pip install --no-deps ./python-pproxy-compat
```

Retain the explicit temporary compat wheel only if direct installation is proven unreliable. Do not treat it as a release artifact.

## Step 3 — Obtain one hosted proof run

Open or use a pull request that touches:

- one Rust-visible path covered by `ci.yml`;
- one Python-visible path covered by `python-test.yml`.

The changes may be the implementation changes from this plan. Do not create a no-op workflow solely for proof.

Required outcome:

- `Rust smoke` completes successfully;
- `Python 3.12 smoke` completes successfully;
- no deleted check appears pending;
- no release/publish job starts.

Record the run URLs and durations in the implementation PR description or final commit summary. Do not add a permanent evidence document.

## Step 4 — Measure runtime

Record:

- Rust job total duration;
- Python job total duration;
- whether caches were cold or warm when observable;
- any repeated environment-sensitive failure.

Budgets:

- Rust smoke target: under 8 minutes median when representative data exists; hard timeout 20 minutes;
- Python smoke target: under 10 minutes median when representative data exists; hard timeout 20 minutes.

One successful run is enough to establish executability. Use additional recent runs for a median only when already available; do not create repeated proof runs solely to manufacture statistics.

## Step 5 — Choose the trigger model

Inspect branch protection and actual repository practice.

### Model A — protected-main PR workflow

Use when normal changes merge through pull requests:

- automatic `pull_request` runs;
- optional `workflow_dispatch` for manual verification;
- remove automatic `push` to `main` to avoid duplicate post-merge runs.

### Model B — direct-push workflow

Use when direct pushes to `main` are intentionally normal:

- automatic `push` to `main`;
- retain PR runs only when PRs are also a real supported path;
- document why duplication is accepted if both remain.

Preferred result for the current repository, if branch protection confirms PR-only development, is Model A.

Do not guess. Record the selected model and reason in the implementation PR summary.

## Step 6 — Reduce tests only when measured need exists

If both jobs fit the budgets and are reliable, keep the current broad test selection.

If a job exceeds the budget or repeatedly fails for environmental rather than correctness reasons, use explicit commands only.

Permitted Rust reduction:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --bins --locked
cargo test --locked -p eggress-runtime --test startup
cargo test --locked -p eggress-runtime --test routing
cargo test --locked -p eggress-runtime --test shutdown
cargo test --locked -p eggress-cli --test cli_exit_codes
```

Permitted Python reduction must include:

- core import/native-extension smoke;
- one Rust-backed connection path;
- compatibility API contract validation;
- one lifecycle/failure path.

Do not add markers, shards, generated test lists, or path-to-test routing logic.

## Acceptance criteria

- Two workflows maximum.
- Both workflow job names and triggers are intentional.
- Both jobs have a successful hosted run on the final definitions.
- Durations are recorded outside the repository tree.
- No publish or specialist certification work runs automatically.
- No path-scoped job is required unless it reports on every protected change.
- Any test reduction is measurement-driven and explicit.

---

# Workstream CC5 — Close GitHub repository settings

## Objective

Remove settings-side remnants that repository files cannot prove.

## Required inspection surface

Use GitHub repository settings, the GitHub connector, or `gh api` with appropriate permissions. Do not rely on assumptions from workflow YAML.

### Branch protection and rulesets

Inspect the rule that applies to `main`.

Remove required status checks for deleted jobs, including any historical variants of:

```text
check
fmt
clippy
test
cargo-deny
cargo-audit
Python Compatibility
Python wheels
pproxy compatibility
strict differential
Shadowsocks interop
release
publish
platform matrices
```

Final rule:

- may require `Rust smoke` if it runs on every protected change;
- must not require `Python 3.12 smoke` while that workflow remains path-scoped;
- must not reference deleted workflow/job names;
- must not require a release or evidence job;
- should not impose unnecessary administrator bypass ceremony for this project unless intentionally retained.

### Actions permissions

Confirm:

- default workflow token permissions are read-only;
- workflows cannot write packages or contents by default;
- pull requests from forks do not receive release credentials;
- ordinary CI requires no repository secret.

### Environments

Inspect and remove environments used only by deleted automation:

```text
pypi
testpypi
release
production
crates-io
```

Retain an environment only when it has a current manual/non-Actions purpose that is documented. GitHub environments are normally workflow constructs, so deletion is expected when the workflow is gone.

### Secrets and variables

Remove or confirm absence of repository/organization secrets and variables used only by deleted automation, such as:

```text
CARGO_REGISTRY_TOKEN
PYPI_API_TOKEN
TEST_PYPI_API_TOKEN
COSIGN_*
GHCR_*
DOCKER_*
RELEASE_*
```

Do not print secret values. Record only secret names removed or confirmation that no matching entries exist.

### Trusted publishing

Inspect PyPI and TestPyPI project settings outside GitHub when accessible.

- remove GitHub Actions trusted-publisher bindings for deleted workflows;
- do not replace them with another automated binding;
- keep local operator credentials outside the repository.

If the package project does not yet exist or access is unavailable, record that exact limitation and do not claim completion for this sub-item.

## Required implementation record

In the implementation PR description or final commit summary, record a concise table:

| Setting | Final state |
|---|---|
| Main required checks | exact names |
| Python path-scoped check required | no |
| Actions default token | read-only |
| Publishing environments | removed/absent/list retained with reason |
| Publishing secrets | removed/absent; names only |
| Trusted publishers | removed/absent/unverifiable with reason |
| Trigger model | PR-only or direct-push model |

Do not create a repository settings document.

## Acceptance criteria

- A new unrelated PR does not wait for deleted or path-skipped required checks.
- `Rust smoke` is the only required check unless another always-running check has a current justification.
- No Actions environment exists solely for removed publishing workflows.
- No ordinary workflow can access a release secret.
- Default permissions are read-only.
- Trusted-publisher state is truthful.
- No sensitive value appears in commits, logs, or PR text.

---

# Workstream CC6 — Align active documentation and plan status

## Objective

Make the active repository guidance describe the corrected ownership model exactly once.

## Target files

- `docs/CI_STATUS.md`;
- `docs/TESTING.md`;
- `docs/DIFFERENTIAL_TESTING.md`;
- `AGENTS.md`;
- `.skills/testing/skill.md`;
- `plans/CI_VERIFICATION_RELEASE_REDUCTIVE_FOLLOW_UP.md`;
- `plans/CI_VERIFICATION_RELEASE_CORRECTIVE_CLOSURE.md`.

## Required responsibility split

### Ordinary repository verification

Owned by focused local commands plus the two smoke workflows:

- formatting;
- Clippy;
- normal Rust tests;
- Python smoke tests.

### Dependency and release preparation

Owned by explicit local release work:

- `cargo deny`;
- `cargo audit`;
- package metadata checks;
- `cargo publish --dry-run` when crates.io publication becomes architecturally possible;
- Python wheel/sdist builds and `twine check`;
- exact release-surface-specific tests.

### pproxy behavioral certification

Owned by one explicit manual command:

```bash
./scripts/run_pproxy_certification.sh
```

It performs:

- isolated pinned oracle setup;
- isolated candidate setup;
- paired observations;
- required differential and interop checks;
- compact JSON summary;
- failure diagnostics.

It does not perform general repository hygiene or release packaging.

## Required text corrections

- replace “strict closure audit” with “pproxy behavioral certification”;
- remove descriptions that say certification runs format, lint, workspace tests, audits, or release package builds;
- replace “CI tier” with “certification profile” in current guidance;
- document that structural tests are ordinary tests;
- document the final environment variable names;
- document the final workflow trigger model;
- state that Python smoke uses a development build and is not wheel-release certification;
- keep crates.io publication marked blocked pending a separate crate-boundary decision.

## Plan status handling

At implementation start, update the parent plan status to:

```text
PARTIALLY IMPLEMENTED — CORRECTIVE CLOSURE REQUIRED
```

and link this plan.

At final closure:

- mark the parent plan `SUPERSEDED — IMPLEMENTED THROUGH CORRECTIVE CLOSURE`;
- mark this plan `COMPLETE`;
- record the final implementation commit SHA in both;
- do not add a separate completion document.

If CC5 cannot be verified due to permissions, this plan remains `BLOCKED — REPOSITORY SETTINGS EVIDENCE REQUIRED`; do not mark complete.

## Acceptance criteria

- Active guidance agrees on all three responsibility owners.
- No active document calls certification a closure audit or release gate.
- No active document claims GitHub settings were changed without evidence.
- Plan status is truthful.
- Historical plans may retain old terminology as historical content, but current indexes and guidance do not point to them as operational instructions.

---

# Workstream CC7 — Final consistency and rejection pass

## Objective

Prove closure without generating another evidence apparatus.

## Repository rejection searches

```bash
# Only intended automatic workflows
find .github/workflows -maxdepth 1 -type f -print | sort

# No removed release automation in active guidance
! rg -n \
  "release\.yml|publish-pypi\.yml|python-wheels\.yml|python-compat\.yml|strict-differential\.yml|pproxy-compat\.yml|shadowsocks-interop\.yml" \
  README.md AGENTS.md docs .skills scripts

# No whole-repository gates inside behavioral certification
! rg -n \
  "cargo fmt|cargo check --workspace|cargo clippy|cargo test --workspace|cargo deny|cargo audit|cargo publish|twine|SBOM|SHA256SUMS|GitHub Release" \
  scripts/run_pproxy_certification.sh

# No closure-audit output or terminology in current operational surfaces
! rg -n \
  "closure-audit|CLOSURE_AUDIT|MILESTONES A-C FINAL CLOSURE AUDIT|strict closure audit|CLOSURE_AUDIT_REPORT" \
  scripts docs/CI_STATUS.md docs/TESTING.md docs/DIFFERENTIAL_TESTING.md AGENTS.md .skills/testing/skill.md

# No current CI-tier vocabulary in implementation/current docs
! rg -n \
  "CiTier|CiTierConfig|ci_tier|oracle::ci|generate_ci_summary|all_tier_configs|tier_gate_enabled|assign_tiers|scenarios_for_tier" \
  crates scripts docs/CI_STATUS.md docs/TESTING.md docs/DIFFERENTIAL_TESTING.md AGENTS.md .skills/testing/skill.md

# No mutable PyPI replacement guidance
! rg -n -- "--replace|deactivate" docs/PYPI_RELEASE.md

# No implicit publishing restoration
! rg -n \
  "cargo publish|maturin upload|twine upload|gh release create|docker push|cosign" \
  .github/workflows
```

## Ordinary repository checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

These commands verify the implementation but must remain outside the certification script.

## Targeted compatibility checks

```bash
bash -n scripts/run_pproxy_certification.sh
./scripts/run_pproxy_certification.sh
python3 -m json.tool target/pproxy-certification/summary.json >/dev/null
```

Expected:

- command exits zero only when required behavior matches;
- summary commit equals `git rev-parse HEAD`;
- oracle version equals `2.7.9`;
- no `target/closure-audit` is created;
- no Markdown or JUnit success artifact is created.

## Python documentation/package command validation

Execute the documented clean build/test flow in an implementation environment when Python packaging files change. Do not upload.

At minimum:

```bash
rm -rf dist .venv-release-test
mkdir -p dist
(cd crates/eggress-python && maturin build --release --out ../../dist)
(cd crates/eggress-python && maturin sdist --out ../../dist)
python3 -m pip wheel --no-deps --wheel-dir dist ./python-pproxy-compat
python3 -m twine check dist/*
```

Then install both wheels into the clean venv and run the documented smoke suite.

## Hosted checks

Before closure, record:

- successful `Rust smoke` run URL and duration;
- successful `Python 3.12 smoke` run URL and duration;
- final required check names;
- final trigger model.

## Final acceptance matrix

### Certification scope

- [ ] General repository checks removed from certification.
- [ ] Dependency audits removed from certification.
- [ ] Release wheel gates removed from certification.
- [ ] Full unrestricted Python suite removed from certification.
- [ ] All retained tests have compatibility mappings.
- [ ] Oracle is pinned and verified.
- [ ] Candidate environment is isolated.
- [ ] Missing observations fail.
- [ ] Required interop failures fail.
- [ ] Success produces one summary JSON.
- [ ] Failure diagnostics are retained selectively.
- [ ] No closure-audit directory/report remains.

### Profile model

- [ ] Structural tests are ungated.
- [ ] At most differential and platform profiles remain.
- [ ] CI-tier types and fields are removed or behaviorally renamed.
- [ ] Old internal-only environment variables are removed.
- [ ] Profile selection tests pass.

### Python release guidance

- [ ] No `--replace` instruction remains.
- [ ] No invalid `deactivate` instruction remains.
- [ ] Clean `dist/` is required.
- [ ] Both packages are tested together.
- [ ] Broken releases roll forward to a new version.
- [ ] No GitHub publishing path is described.

### Hosted CI

- [ ] Exactly two workflows or fewer.
- [ ] No matrices or publishing jobs.
- [ ] Rust smoke succeeds.
- [ ] Python smoke succeeds.
- [ ] Durations recorded.
- [ ] Trigger model selected intentionally.
- [ ] No avoidable duplicate run model remains.

### GitHub settings

- [ ] Deleted checks removed from branch rules.
- [ ] Path-scoped Python job not required.
- [ ] Actions token defaults read-only.
- [ ] Publishing environments removed or absent.
- [ ] Publishing secrets removed or absent.
- [ ] Trusted-publisher state verified truthfully.

### Documentation and status

- [ ] Active documents agree on responsibility split.
- [ ] No active “strict closure audit” terminology remains.
- [ ] Parent plan has truthful superseded status.
- [ ] This plan records the final implementation SHA.
- [ ] No separate completion document added.
- [ ] Crates.io block remains truthful and out of scope.

The plan is complete only when every applicable checkbox is satisfied. An inaccessible external setting must be recorded as a blocker, not silently treated as complete.

---

# 5. Suggested commit sequence

Keep implementation commits narrow and reviewable.

1. `testkit: narrow pproxy certification to behavioral checks`
2. `testkit: replace CI tiers with certification profiles`
3. `docs: correct immutable manual PyPI release procedure`
4. `ci: validate smoke workflow execution and triggers`
5. `docs: align verification and certification ownership`
6. `plans: close CI and release corrective pass`

GitHub settings changes may not create repository commits. Record them in the implementation PR description before the final status commit.

Do not combine new proxy functionality, crate-boundary restructuring, or release publication with this pass.

---

# 6. Handoff notes for a smaller implementation model

- Start by reading this plan, its parent plan, `docs/CI_STATUS.md`, and the current certification script.
- Preserve behavioral tests. The central correction is moving general checks out of certification, not deleting correctness coverage.
- Treat `maturin develop` inside an isolated candidate venv as execution setup, not release verification.
- Do not use system `pip` or install prerequisites globally from the script.
- Do not write to `dist/` from compatibility certification.
- Do not retain successful logs merely because the old audit did.
- Do not convert a mandatory differential failure into an optional skip.
- Remove one-for-one renamed abstractions when a direct predicate is simpler.
- Keep structural tests in ordinary `cargo test` execution.
- Test the corrected Python workflow on GitHub before claiming it is fixed.
- Never make the path-scoped Python workflow a required check.
- Never reveal secret values while auditing repository settings.
- Do not mark this plan complete when PyPI/TestPyPI trusted-publisher state is unknown; state the blocker.
- Do not attempt crates.io publication in this pass. The CLI publication boundary requires separate architectural work.
- Prefer a smaller diff. No replacement framework is needed.
