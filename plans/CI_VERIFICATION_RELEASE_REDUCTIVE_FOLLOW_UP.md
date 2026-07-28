# CI, Verification, and Release Apparatus — Reductive Follow-Up Plan

## Status

**READY FOR IMPLEMENTATION**

## Baseline

- Repository: `eggstack/eggress`
- Baseline branch: `main`
- Baseline commit: `d477ae49f63e0f0aab8fae194fbefb9d9c48e403`
- Parent simplification commits:
  - `7242a345a7475e7de3298345fbcb7ec9aa6087bd` — simplify hosted verification and make releases manual
  - `d477ae49f63e0f0aab8fae194fbefb9d9c48e403` — replace stale agent verification guidance

## Purpose

Complete the reductive pass around CI, verification, release, compatibility evidence, and release-facing documentation without rebuilding the same complexity under new names.

The first simplification pass removed seven workflows, retained only one Rust smoke workflow and one path-scoped Python smoke workflow, and made crates.io publication explicitly manual. That was the highest-value change. Residual process complexity remains in documentation, scripts, compatibility-harness terminology, generated-report coupling, repository settings, and the undefined crates.io publication surface.

This plan removes or consolidates those residual surfaces. It is intentionally deletion-first. It must not create a new release framework, task runner, evidence platform, workflow matrix, policy registry, or generalized orchestration layer.

---

# 1. Current residual complexity

The following issues are present at the baseline and justify this pass.

## 1.1 Current documents still describe deleted automation

`docs/PYPI_RELEASE.md` still:

- mentions trusted publishing as a normal prerequisite;
- recommends the deleted `publish-pypi.yml` workflow;
- treats Git tags and GitHub Releases as part of the normal Python release path;
- contains stale hosted-CI billing caveats;
- contains a rollback example using replacement-style upload semantics that are not an acceptable immutable-release strategy.

Several documents under `docs/release/` still describe an automated artifact train that no longer exists, including:

- `ARTIFACT_MATRIX.md`;
- `BINARY_MATRIX.md`;
- historical go/no-go documents;
- historical release decision documents;
- release-candidate notes and evidence reports tied to deleted workflows.

The current manual release policy supersedes these documents, but they remain discoverable as if they are active instructions.

## 1.2 Release-document verification is coupled to historical state

`scripts/check_release_docs.py` currently hard-codes or validates:

- an obsolete hosted-CI billing caveat;
- fixed compatibility counts such as `26`;
- historical release report headings;
- historical milestone-plan status;
- generated JSON artifact policy;
- old release-candidate documentation;
- broad README phrase policing.

This script no longer protects the current lean release model. It creates synchronization work among historical plans, generated reports, and release documents. It is also called by compatibility-closure tooling, causing compatibility certification to inherit unrelated release-document requirements.

## 1.3 Compatibility certification still contains CI-shaped machinery

The pproxy oracle/testkit surface still contains concepts such as:

- a five-tier CI taxonomy;
- CI-specific gate names and environment variables;
- hosted artifact retention assumptions;
- closure scripts that run general repository, dependency, documentation, packaging, and evidence checks together;
- generated reports whose freshness is treated as an execution gate.

The compatibility harness itself is valuable and must remain. The CI-shaped orchestration around it is not automatically valuable now that compatibility certification is an explicit, manual operation.

## 1.4 The Python and Rust smoke jobs may still be broader than needed

The retained Rust workflow runs the full workspace test suite and Clippy across all targets. The Python workflow builds a native wheel and runs all tests under `python/tests` and `tests/compat` whenever Python-facing paths change.

These may be acceptable if they remain fast and reliable. They should not be reduced merely for aesthetic reasons. They should, however, have explicit time budgets and a documented fallback profile if they continue to impede iteration.

## 1.5 Repository settings may retain deleted checks and publishing configuration

The repository tree cannot remove GitHub-side settings such as:

- branch-protection required checks referring to deleted workflow jobs;
- `pypi` or `testpypi` environments;
- trusted-publishing/OIDC configuration;
- package-write permissions or GHCR release assumptions;
- Actions default permissions broader than read-only.

These can block or confuse development even after workflow files are removed.

## 1.6 The crates.io release surface is not defined

The workspace contains many internal crates. The manual release document currently describes generic dependency-ordered publication without stating exactly which crates are public products and which are implementation details.

Publishing every internal crate merely because the CLI depends on it would recreate a large release train. Conversely, marking internal crates `publish = false` can make a top-level crate impossible to publish if it depends on those crates by path. This needs a deliberate, bounded publication decision rather than additional CI.

## 1.7 Historical phase and release records still dominate search results

Historical completion documents and superseded release plans frequently appear before current policy documents in repository search. They are useful as git history but harmful when treated as current operational guidance.

The solution should not be to update every old document forever. The solution is to reduce the active documentation surface and make history visibly historical.

---

# 2. Required final outcome

At completion, every statement below must be true.

1. The repository still has exactly two automatic GitHub Actions workflows unless a measured runtime result justifies reducing to one.
2. No GitHub Actions workflow publishes crates, Python packages, containers, release assets, checksums, SBOMs, signatures, or GitHub Releases.
3. No non-historical current document references a deleted workflow as an available release mechanism.
4. No current document describes hosted CI billing failure as an enduring repository policy.
5. The manual crates.io process names the exact intended public crates or explicitly states that crates.io publication is blocked pending a separate crate-boundary decision.
6. Internal crates are not added to the public release surface merely to satisfy an automated release train.
7. Python publication instructions are local, immutable, concise, and separate from Rust publication.
8. `scripts/check_release_docs.py` is removed unless a materially smaller replacement protects a current invariant that cannot be expressed clearly in documentation.
9. Compatibility certification no longer invokes general release-document checks, dependency audits, formatting, linting, packaging, or artifact assembly as part of its behavioral oracle result.
10. Compatibility evidence is generated only by an explicit certification command and is not required for routine work.
11. CI-specific oracle tiers are removed, renamed, or collapsed where they exist only to model deleted workflows.
12. The retained smoke jobs have explicit runtime budgets and do not add new matrices, dynamic test-selection frameworks, or custom orchestration.
13. Repository settings no longer require deleted status checks or retain unused publishing environments.
14. Historical release-process documents no longer appear as active guidance from README, AGENTS, testing, or release indexes.
15. The pass produces a net deletion of process code and documentation.

---

# 3. Scope

## In scope

- stale references to deleted workflows;
- current release and PyPI instructions;
- historical release-process documents that no longer describe a supported path;
- `scripts/check_release_docs.py` and its callers;
- release/evidence responsibilities inside strict compatibility scripts;
- CI-specific oracle tier abstractions that are no longer consumed by hosted workflows;
- generated report synchronization requirements;
- the two retained workflow test selections and runtime budgets;
- branch-protection and repository Actions settings;
- crates.io publication-surface classification;
- concise indexing of current versus historical plans and release records.

## Out of scope

Do not use this pass to:

- implement new proxy protocols or pproxy features;
- weaken compatibility claims or remove tests because they expose a real defect;
- combine all workspace crates into one crate;
- redesign the runtime or crate architecture;
- add a task runner, `xtask`, Makefile framework, release bot, changelog bot, or custom CI scheduler;
- add scheduled security scans;
- add code coverage gates;
- add benchmark gates;
- add release signing, provenance, SBOM, checksum, or container automation;
- add a replacement evidence-upload workflow;
- rewrite every historical completion document;
- make PyPI publication a prerequisite for crates.io publication;
- make GitHub tags or GitHub Releases a prerequisite for either package registry.

---

# 4. Non-negotiable reductive rules

1. **Delete before abstracting.** If a script or document exists only for deleted automation, remove it rather than generalizing it.
2. **Do not replace one matrix with another.** A renamed matrix is still a matrix.
3. **Do not replace workflows with a large local orchestrator.** Local commands should remain direct and inspectable.
4. **Git history is the archive.** Process-only historical documents may be deleted from the current tree when they are fully superseded.
5. **Compatibility tests are not release tests.** Behavioral certification must not silently become a complete repository release gate.
6. **Release tests are selected by affected surface.** No universal evidence checklist is required for every release.
7. **Generated reports are outputs, not independent sources of truth.** A generated Markdown count must not create a second policy source.
8. **No fixed-count policing.** Validators must not hard-code transient feature counts such as `26`.
9. **No permanent billing caveats.** Temporary GitHub account conditions do not belong in durable verification policy.
10. **One automatic run per ordinary change is the target.** Avoid both PR and post-merge duplication if repository policy permits it.
11. **No required path-scoped check.** A required branch-protection check must run on every protected change or it can block unrelated changes indefinitely.
12. **No public-crate sprawl by default.** Publication is an API and maintenance commitment.
13. **No evidence ceremony for ordinary changes.** Relevant tests and a clear commit/PR summary are sufficient.
14. **No broad deletion of behavioral tests.** Remove orchestration and duplication, not correctness coverage.
15. **Net complexity must decrease.** New lines, files, commands, and concepts require a specific deletion or consolidation benefit.

---

# 5. Execution order

Implement in this order:

1. RF0 — freeze the inventory and classify current versus historical surfaces;
2. RF1 — correct active release and Python publication guidance;
3. RF2 — remove obsolete release documents from the active tree;
4. RF3 — delete historical release-document synchronization machinery;
5. RF4 — decouple compatibility certification from release orchestration;
6. RF5 — simplify CI-specific oracle tiering where safe;
7. RF6 — measure and, only if needed, reduce retained smoke jobs;
8. RF7 — define the crates.io publication surface;
9. RF8 — clean GitHub repository settings;
10. RF9 — run final consistency and closure checks.

Do not begin workflow reduction based on runtime assumptions before RF6 measurements. Do not change crate publication flags before RF7 has a complete dependency closure.

---

# Workstream RF0 — Freeze the residual inventory

## Objective

Create a bounded inventory so the implementer removes only process overhead and does not delete live compatibility or packaging functionality accidentally.

## Required inventory categories

Classify each relevant file as one of:

- `current-policy`;
- `current-operation`;
- `current-compatibility-tool`;
- `generated-output`;
- `historical-process-record`;
- `obsolete-automation-support`;
- `uncertain`.

## Files and directories to inspect

At minimum:

```text
.github/workflows/
docs/CI_STATUS.md
docs/TESTING.md
docs/PYPI_RELEASE.md
docs/release/
docs/parity/
AGENTS.md
README.md
.skills/testing/skill.md
.skills/security-dev/skill.md
scripts/check_release_docs.py
scripts/release_evidence.py
scripts/run_strict_pproxy_closure_audit.sh
scripts/run_strict_pproxy_api.sh
scripts/run_strict_pproxy_interop.sh
scripts/phase36_report.py
crates/eggress-testkit/src/oracle/ci.rs
crates/eggress-testkit/src/oracle/report.rs
crates/eggress-testkit/src/bin/strict_report.rs
tests/compat/pproxy_manifest.toml
docs/parity/pproxy_capability_manifest.toml
docs/parity/pproxy_2_7_9_strict_manifest.toml
```

## Required commands

Use repository search rather than assumptions:

```bash
rg -n "release\.yml|publish-pypi\.yml|python-wheels\.yml|python-compat\.yml|strict-differential\.yml|pproxy-compat\.yml|shadowsocks-interop\.yml" .
rg -n "Hosted CI.*non-functional|billing|trusted publish|OIDC|GitHub Release|GHCR|SBOM|SHA256SUMS" docs README.md AGENTS.md .skills scripts
rg -n "check_release_docs|release_evidence|strict_report|oracle::ci|CiTier|CI tier" .
rg -n "tests/compat/pproxy_manifest\.toml|pproxy_capability_manifest\.toml|pproxy_2_7_9_strict_manifest\.toml" .
```

## Deliverable

A short inventory table added to the implementation PR description or commit message. Do not add a permanent inventory document unless it remains useful after deletion.

## Acceptance criteria

- Every file deleted or substantially rewritten later in this plan has an inventory classification.
- No live test implementation is classified as obsolete solely because its former workflow was removed.
- Every script in scope has at least one current non-historical caller or is marked for deletion.
- The implementer can state which manifest is authoritative for each compatibility purpose.

---

# Workstream RF1 — Correct active release and Python publication guidance

## Objective

Make all active release instructions consistent with operator-driven, immutable, local publication.

## Target files

- `docs/release/RELEASE_PROCESS.md`
- `docs/PYPI_RELEASE.md`
- `docs/CI_STATUS.md`
- `docs/TESTING.md`
- `AGENTS.md`
- `README.md`
- any current release index or installation document linking to obsolete process documents

## Required changes

### Rust/crates.io

1. Keep `docs/release/RELEASE_PROCESS.md` as the sole Rust release procedure.
2. Replace generic placeholders with the exact public crate list after RF7.
3. Keep these phases only:
   - verify exact release commit;
   - dry-run exact public crates;
   - publish dependency-first when more than one public crate is intentionally required;
   - verify installation from crates.io;
   - optionally create a tag or GitHub Release manually.
4. Remove references to:
   - artifact bundles;
   - checksums and signatures as required release outputs;
   - SBOM generation as a release requirement;
   - GitHub-hosted binary matrices;
   - container publication as part of normal release;
   - GitHub Actions waiting periods;
   - release branches unless the maintainer actually uses them.
5. Keep the immutable-version roll-forward rule.

### Python/PyPI

Rewrite `docs/PYPI_RELEASE.md` to be a short manual procedure. It must:

- state clearly that Python publication is separate from crates.io;
- use local `maturin build`, `maturin sdist`, wheel installation, `twine check`, and upload commands;
- remove the deleted workflow recommendation;
- remove trusted-publisher/OIDC instructions unless an operator explicitly chooses to retain them outside GitHub Actions;
- remove temporary hosted-CI billing language;
- remove replacement-upload instructions;
- state that a bad published version must be yanked when appropriate and replaced by a new version;
- test canonical `eggress` and `eggress-pproxy-compat` together in a clean environment;
- avoid prescribing TestPyPI for every patch release; it is optional for packaging changes or first publication.

Target length: fewer than 120 lines unless a platform-specific wheel procedure is truly maintained.

### Current-policy references

Update current policy documents so they link only to:

- `docs/CI_STATUS.md`;
- `docs/TESTING.md`;
- `docs/release/RELEASE_PROCESS.md`;
- concise Python release instructions;
- current compatibility manifests and testing guides.

## Acceptance criteria

- `rg -n "publish-pypi\.yml|python-wheels\.yml|release\.yml" README.md AGENTS.md docs .skills` returns no matches outside explicitly historical records retained by RF2.
- No active release document recommends GitHub Actions for publishing.
- No active release document contains the old billing caveat.
- No active release document describes overwriting an immutable registry version.
- Rust and Python publication remain independent.
- The combined active Rust and Python release procedures are materially shorter than the baseline documents.

---

# Workstream RF2 — Remove obsolete release-process documents from the active tree

## Objective

Stop obsolete artifact and release-candidate documents from competing with current policy.

## Mandatory review set

Review at least:

```text
docs/release/ARTIFACT_MATRIX.md
docs/release/BINARY_MATRIX.md
docs/release/GO_NO_GO_CHECKLIST.md
docs/release/PARITY_RELEASE_GO_NO_GO.md
docs/release/RELEASE_NOTES_PARITY_RC.md
docs/release/RELEASE_DECISION_v0.1.0-rc.1.md
docs/release/FINAL_PPROXY_PARITY_REPORT.md
docs/release/CONTAINER.md
docs/RELEASE_READINESS.md
docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md
docs/WHEEL_AUDIT.md
```

## Decision rules

### Delete from the current tree when all are true

- the document describes a workflow, artifact matrix, release candidate, or decision that is no longer current;
- no current runtime or user-facing behavior depends on it;
- current policy has a replacement;
- its historical value is available through git history.

### Merge before deletion when useful content remains

Examples:

- merge accurate platform-support information from `BINARY_MATRIX.md` into `PLATFORM_SUPPORT_MATRIX.md`, then delete `BINARY_MATRIX.md`;
- move operational container build/run instructions to an operations document if the `Containerfile` remains supported, then delete release-registry and automated-publish language;
- move a still-valid migration note into the current migration guide, then delete release-candidate framing.

### Do not retain files merely by moving all of them into an archive directory

A bulk move preserves the same search and maintenance burden. Prefer deletion with git history. If an archive index is needed, add one short file listing the relevant commits or tags, not copies of every obsolete document.

## Historical completion records

Do not rewrite all `PHASE_*_COMPLETION.md` files. Instead:

1. remove links to them from active policy and release instructions;
2. add one concise statement in the relevant index that phase-completion files are historical;
3. delete only those completion records whose sole subject is the removed CI/release apparatus and whose content has no remaining operational value.

## Acceptance criteria

- Active release navigation contains no artifact matrix, automated release train, or historical RC decision.
- Platform support has one current source.
- Container operation, if retained, is not presented as a mandatory release channel.
- Search results for “release process” prioritize `docs/release/RELEASE_PROCESS.md`.
- This workstream removes more lines than it adds.

---

# Workstream RF3 — Remove release-document synchronization machinery

## Objective

Delete the script and coupling that require historical documents and generated counts to stay synchronized.

## Primary target

- `scripts/check_release_docs.py`

## Required changes

1. Remove `scripts/check_release_docs.py` unless RF0 identifies a current, unique invariant that cannot be protected elsewhere.
2. Remove all calls to it from:
   - strict closure scripts;
   - agent skills;
   - testing docs;
   - plans that are still presented as active;
   - any local verification examples.
3. Remove fixed-count checks and generated-report heading checks.
4. Remove hosted-CI caveat checks.
5. Remove plan-status policing from executable verification.
6. Remove README prose-regex policing.

## Permitted replacement

A replacement is permitted only if all conditions are met:

- fewer than 80 lines;
- validates only current file existence or broken local links;
- has no fixed feature counts;
- has no historical plan awareness;
- has no release-candidate awareness;
- is not required by routine CI;
- removes more obsolete code than it introduces.

The preferred outcome is no replacement.

## Acceptance criteria

- `rg -n "check_release_docs" .` returns no current references.
- No executable test or script reads historical plans to decide repository correctness.
- No executable test or script requires a hosted-CI billing disclaimer.
- No executable test or script compares duplicated numeric feature counts across Markdown documents.
- Compatibility certification can run without any release-note or release-decision document.

---

# Workstream RF4 — Decouple compatibility certification from release orchestration

## Objective

Preserve strong pproxy behavioral verification while removing unrelated release ceremony from the certification command.

## Target files

At minimum:

```text
scripts/run_strict_pproxy_closure_audit.sh
scripts/run_strict_pproxy_api.sh
scripts/run_strict_pproxy_interop.sh
scripts/release_evidence.py
crates/eggress-testkit/src/bin/strict_report.rs
docs/DIFFERENTIAL_TESTING.md
docs/parity/PPROXY_ORACLE_MAINTENANCE.md
.skills/testing/skill.md
```

## Required responsibility split

### Routine repository verification

Owned by normal local commands and the Rust/Python smoke workflows:

- format;
- Clippy;
- workspace tests;
- Python smoke tests.

### Dependency/release verification

Owned by explicit release preparation:

- `cargo deny`;
- `cargo audit`;
- `cargo publish --dry-run`;
- wheel metadata checks.

### Compatibility certification

Owned by one explicit manual compatibility command:

- create clean oracle and candidate environments;
- verify `pproxy==2.7.9` provenance;
- run paired behavioral probes;
- run differential/interoperability tests required for the selected compatibility profile;
- produce a local machine-readable summary;
- fail on missing required behavioral observations.

The compatibility command must not run:

- formatting;
- Clippy;
- general workspace tests unrelated to compatibility;
- `cargo deny`;
- `cargo audit`;
- release-document consistency checks;
- crate dry runs;
- wheel matrices;
- SBOM, checksums, signatures, or GitHub artifact operations;
- README or plan-status checks.

## Script naming

If `run_strict_pproxy_closure_audit.sh` remains, either:

- rename it to a behavior-specific name such as `run_pproxy_certification.sh`; or
- rewrite its header and scope so “closure audit” cannot be mistaken for a whole-repository release gate.

Do not retain both old and new wrappers.

## Evidence output

Keep evidence minimal:

```text
target/pproxy-certification/
  summary.json
  failures/
```

Raw logs should be retained only for failing scenarios or when an explicit verbose/debug option is used. Do not require a large Markdown evidence bundle for every successful run.

## `release_evidence.py`

Audit actual callers.

- Delete it if it exists only to construct the removed release evidence bundle.
- If compatibility certification needs a small subset, move that functionality into the compatibility runner under a compatibility-specific name.
- Do not preserve release-oriented naming for a compatibility-only tool.

## Acceptance criteria

- One explicit command performs strict pproxy behavioral certification.
- That command has one clearly stated purpose.
- It does not invoke general release or repository hygiene checks.
- Successful certification produces one summary JSON and no mandatory large artifact tree.
- Failed certification retains enough diagnostics to reproduce the failure.
- No routine CI workflow invokes the certification command.
- Existing behavioral defects still fail certification; this pass must not convert failures into skips.

---

# Workstream RF5 — Collapse CI-specific oracle tiering

## Objective

Remove abstractions that exist primarily to map compatibility tests onto deleted hosted workflow tiers.

## Target files

- `crates/eggress-testkit/src/oracle/ci.rs`
- `crates/eggress-testkit/src/oracle/mod.rs`
- `crates/eggress-testkit/src/oracle/report.rs`
- scenario metadata that refers to CI tiers
- `docs/DIFFERENTIAL_TESTING.md`
- `.skills/testing/skill.md`

## Required audit questions

1. Is `oracle::ci` used by behavior selection independent of GitHub Actions?
2. Are five tiers materially different, or are they historical workflow labels?
3. Can tests be selected directly by scenario attributes such as:
   - external dependency;
   - privileged requirement;
   - platform requirement;
   - expected duration?
4. Are multiple environment variables selecting nearly identical suites?

## Preferred reduced model

At most three profiles:

- `structural` — no external process; runs normally;
- `differential` — clean pproxy oracle and ordinary external interoperability;
- `platform` — privileged or platform-specific checks, explicitly selected.

A two-profile model is preferable if platform selection can remain an ordinary test filter.

## Implementation constraints

- Do not create a generic profile engine.
- Use a small enum or direct predicates.
- Preserve scenario metadata needed for correctness.
- Provide compatibility aliases for old environment variables only temporarily if external users plausibly depend on them.
- Remove aliases before closure if they are only repository-internal.

## Acceptance criteria

- No five-tier CI taxonomy remains unless each tier has a current, independently justified behavior.
- No source comment claims that a deleted workflow runs a tier.
- External tests remain opt-in.
- Structural tests continue to run without pproxy installed.
- The reduced selection logic has direct unit tests.
- The total amount of tier-selection code decreases.

---

# Workstream RF6 — Measure and bound the retained smoke workflows

## Objective

Ensure the two retained workflows remain fast enough for iterative development without prematurely removing broad correctness coverage.

## Baseline measurement

Measure locally on a clean or representative cache state:

```bash
time cargo fmt --all -- --check
time cargo clippy --workspace --all-targets -- -D warnings
time cargo test --workspace --locked

time bash -c '
  rm -rf .venv target/wheels &&
  python3 -m venv .venv &&
  .venv/bin/python -m pip install -q "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47" &&
  (cd crates/eggress-python && ../../.venv/bin/maturin build --release --out ../../target/wheels) &&
  .venv/bin/python -m pip install -q target/wheels/eggress-*.whl &&
  .venv/bin/python -m pip install -q --no-deps ./python-pproxy-compat &&
  .venv/bin/python -m pytest python/tests tests/compat -q
'
```

Use recent GitHub job durations when available. Do not add telemetry or a benchmark workflow.

## Runtime budgets

- Rust smoke target: median under 8 minutes; hard timeout remains 20 minutes.
- Python smoke target: median under 10 minutes; hard timeout remains 20 minutes.
- No ordinary change should start more than two jobs.

## Decision rule

If the full jobs fit the budgets and are reliable, keep them unchanged.

If a job exceeds the budget or has recurring environment-sensitive failures, reduce it using explicit commands rather than dynamic selection infrastructure.

## Permitted Rust reduction

A reduced Rust job may use:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --bins --locked
cargo test --locked -p eggress-runtime --test startup
cargo test --locked -p eggress-runtime --test routing
cargo test --locked -p eggress-runtime --test shutdown
cargo test --locked -p eggress-cli --test cli_exit_codes
```

The exact integration tests must be selected from measured high-value, deterministic coverage. Do not add more than five explicit integration-test commands.

The full workspace suite remains expected locally before merging substantial runtime changes and before release.

## Permitted Python reduction

A reduced Python smoke job should use an explicit short file list, for example:

```bash
python -m pytest \
  python/tests/test_wheel_import_smoke.py \
  python/tests/test_proxy_connection.py \
  tests/compat/test_pproxy_api_contract.py \
  -q
```

Adjust the exact list based on runtime and coverage. It must include:

- clean wheel import;
- one native Rust-backed connection path;
- compatibility API contract validation.

Do not add custom markers, test sharding, generated manifests, or path-to-test mapping code.

## Trigger duplication

Choose one automatic trigger model based on actual repository practice:

### Preferred when main requires pull requests

- run on `pull_request`;
- retain `workflow_dispatch` for manual main verification;
- remove post-merge `push` duplication.

### Preferred when direct pushes to main are normal

- retain `push` to `main`;
- retain PR runs only if PR review is genuinely used;
- accept duplication only if both paths are intentionally required.

Do not remove coverage for direct pushes while direct pushes remain a supported workflow.

## Acceptance criteria

- Runtime measurements are recorded in the implementation PR or commit summary.
- No new workflow is added.
- No matrix is added.
- No test-selection framework is added.
- If tests are reduced, the full local command remains documented.
- The ordinary workflow count remains at most two.
- Normal development does not produce duplicate automatic runs unless repository policy explicitly requires both PR and push validation.

---

# Workstream RF7 — Define and minimize the crates.io publication surface

## Objective

Prevent manual release from becoming a large workspace-wide crate train.

## Required inventory

For every workspace package, record:

- package name;
- current `publish` setting;
- whether it exposes an intentionally supported public Rust API;
- whether another intended public package depends on it;
- whether dependency declarations include a registry version compatible with publication;
- whether users are expected to depend on it directly.

Use:

```bash
cargo metadata --no-deps --format-version 1
rg -n "^publish\s*=|version\.workspace|path\s*=|package\]" Cargo.toml crates/*/Cargo.toml
```

## Classification

Each crate must be one of:

- `public-product` — intentionally supported external API or binary package;
- `required-public-dependency` — must be published because a public product depends on it and crate boundaries are intentionally retained;
- `internal` — implementation detail; should be `publish = false`;
- `packaging-only` — Python/PyO3, benchmarks, fuzzing, or local tooling; should be `publish = false`.

## Decision rule

Prefer the smallest publication closure that matches real user needs.

Do not classify every internal crate as public merely because Cargo requires registry dependencies for a published top-level crate.

If the desired public product cannot be published without exposing a large set of internal crates, choose one of these outcomes explicitly:

1. publish the minimal dependency closure and accept those crates as maintained public packages; or
2. declare crates.io publication blocked and create a separate crate-boundary/consolidation plan.

Do not solve that architectural issue by adding release automation.

## Documentation result

`docs/release/RELEASE_PROCESS.md` must list exact commands in exact order, for example:

```bash
cargo publish --dry-run -p <exact-public-crate>
cargo publish -p <exact-public-crate>
```

No `<crate-name>` placeholders may remain after RF7.

If multiple crates are required, document the exact dependency order. Do not add a release-order generator unless the list is demonstrably too large to maintain manually; if it is that large, reconsider the publication surface.

## Acceptance criteria

- Every workspace crate has an intentional publication classification.
- Internal and packaging-only crates use `publish = false`.
- The public crate list is exact and documented.
- Every intended public crate passes `cargo publish --dry-run` or is explicitly recorded as blocked.
- A blocked dry run is not bypassed with `--allow-dirty`, generated manifests, or GitHub automation.
- The publication surface is no larger than technically required.

---

# Workstream RF8 — Clean GitHub repository settings

## Objective

Remove settings-side remnants of deleted workflows and publishing automation.

## Manual settings checklist

### Branch protection / rulesets

- remove required checks for deleted jobs and workflows;
- remove checks such as old OS matrices, audit, deny, interoperability, strict differential, wheel build, or release gates;
- if retaining required checks, require only an always-running check;
- do not require the path-scoped Python smoke check unless it is changed to run on every pull request;
- avoid requiring administrator bypass ceremonies for a single-maintainer repository unless there is a demonstrated need.

### Actions settings

- default `GITHUB_TOKEN` permission: read-only contents;
- do not grant package write or contents write globally;
- remove unused Actions secrets for release or publishing;
- remove unused `pypi` and `testpypi` environments if they existed only for deleted workflows;
- remove GHCR-specific release assumptions if container publishing is no longer a supported release channel.

### Registry trusted publishing

- remove GitHub Actions trusted-publisher bindings for PyPI/TestPyPI if they are no longer intentionally used;
- keep local registry credentials outside the repository;
- do not document secret values or credential setup beyond the registry’s standard local authentication.

## Repository documentation

Record only the durable result in `docs/CI_STATUS.md`, for example:

- current automatic workflows;
- current required check policy;
- manual release boundary.

Do not add screenshots, account-specific billing notes, or a large repository-settings runbook.

## Acceptance criteria

- A new PR does not show deleted checks as expected or pending.
- Merging an unrelated change is not blocked by a path-scoped check that did not run.
- No GitHub environment exists solely for removed publishing workflows.
- Actions default permissions are read-only.
- No repository secret is required for ordinary CI.

---

# Workstream RF9 — Final consistency and closure

## Objective

Verify that the reductive pass removed process complexity without hiding correctness requirements.

## Required repository checks

```bash
# Exactly the intended workflows
find .github/workflows -maxdepth 1 -type f -print | sort

# No deleted workflow references in active surfaces
rg -n "release\.yml|publish-pypi\.yml|python-wheels\.yml|python-compat\.yml|strict-differential\.yml|pproxy-compat\.yml|shadowsocks-interop\.yml" \
  README.md AGENTS.md docs .skills scripts

# No obsolete policy language
rg -n "Hosted CI.*non-functional|billing issue|trusted publisher configured|GitHub Actions workflow" \
  README.md AGENTS.md docs .skills

# No obsolete checker references
rg -n "check_release_docs" .

# Review current publication flags
rg -n "^publish\s*=" Cargo.toml crates/*/Cargo.toml

# Routine broad verification
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

For Python-facing documentation or packaging changes:

```bash
rm -rf .venv target/wheels
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47" twine
(cd crates/eggress-python && ../../.venv/bin/maturin build --release --out ../../target/wheels)
.venv/bin/python -m pip install target/wheels/eggress-*.whl
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q
.venv/bin/python -m twine check target/wheels/*
```

Run strict compatibility certification only if RF4 or RF5 changes compatibility selection, runners, observations, or report generation.

## Closure metrics

Record in the implementation PR or final commit summary:

- workflows before/after;
- process scripts before/after;
- active release documents before/after;
- lines added/deleted;
- Rust smoke duration before/after;
- Python smoke duration before/after;
- exact crates.io public package list;
- repository settings changed manually.

Do not add a permanent completion document solely to store these metrics.

## Final acceptance criteria

The line of work is complete only when:

1. current release instructions contain no deleted automation path;
2. obsolete release-process documents are deleted or consolidated;
3. historical documentation is no longer part of executable verification;
4. compatibility certification is behavior-specific and manual;
5. CI-specific oracle tiering is materially reduced or proven necessary;
6. the two smoke jobs meet their budgets or have been reduced with explicit test lists;
7. repository settings no longer reference deleted checks or publishing environments;
8. the crates.io publication surface is exact and minimal;
9. routine Rust and Python checks pass;
10. the change is net-negative in process lines and files;
11. no new workflow, matrix, release framework, or evidence platform was introduced.

---

# 6. Suggested commit sequence

Keep implementation commits narrow and independently reviewable.

1. `docs: remove stale automated release guidance`
2. `docs: delete superseded release artifact records`
3. `chore: remove obsolete release document checker`
4. `testkit: decouple pproxy certification from release gates`
5. `testkit: collapse obsolete CI tier selection`
6. `ci: bound smoke suites based on measured runtime` — only if RF6 requires change
7. `release: define minimal crates.io publication surface`
8. `docs: record final lean repository policy`

Do not combine crate publication-flag changes with compatibility-runner changes.

---

# 7. Handoff notes for a smaller implementation model

- Start from the baseline commit stated above or rebase and record the new baseline before editing.
- Do not infer that a file is obsolete from its name. Search its callers first.
- Treat anything under `plans/` and most `PHASE_*_COMPLETION.md` files as historical unless a current policy document links to it.
- Preserve actual test code unless it is unreachable duplication. This is a process-reduction pass, not a coverage-deletion pass.
- Prefer deleting stale documentation to adding “superseded” banners to dozens of files.
- Do not add a replacement workflow for strict compatibility evidence.
- Do not add `cargo-make`, `just`, `xtask`, release-plz, cargo-release, semantic-release, or another release orchestrator.
- Do not create a generated release-order file. If publication order is large, revisit the public crate boundary.
- Do not make the Python smoke workflow a required check while it remains path-scoped.
- Do not claim GitHub settings were changed unless they were actually inspected and changed.
- Do not claim crates.io readiness until exact intended crates pass `cargo publish --dry-run`.
- Do not claim compatibility certification passes unless the behavior-specific manual command was executed after RF4/RF5 changes.
- When uncertain whether to preserve a process artifact, ask: “Does current runtime correctness, current user documentation, or a current registry publication depend on this?” If not, prefer deletion.
