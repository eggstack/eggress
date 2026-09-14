# Final Release and Repository Cleanup Closure

## Status

**READY FOR IMPLEMENTATION**

## Baseline

Written against `main` at:

```text
95380162b33195a67410db243455864a056975a7
```

This plan is the final bounded cleanup/release pass after the listener-free SSH `OutboundConnector` correction and its CI closure. It is not a new feature phase.

Relevant completed work:

- `plans/OUTBOUND_CONNECTOR_SSH_FEATURE_BOUNDARY_CORRECTIVE_PASS.md`
- `plans/OUTBOUND_CONNECTOR_SSH_CLOSURE_PASS.md`
- implementation commit `4f6ec746adf4a76df7ed6e534053bd8e387da999`
- CI/runtime-closure commit `51551160f739a2e4e40b0bcb237a52912e699d8e`
- plan-closure head `95380162b33195a67410db243455864a056975a7`

At this baseline the repository is functionally in good shape: there are no open GitHub issues, no obvious indexed `TODO` markers, the ordinary Rust CI is green, the SSH facade regression is enforced in CI, the CLI already exposes `version` and `update`, and the current release infrastructure already publishes Python and prebuilt CLI artifacts from version tags. The remaining work is primarily release completion, repository truth cleanup, and removal of stale planning ambiguity.

---

# Objective

Leave Eggress in a state where:

1. the SSH/embed correction is present in a public release rather than only on `main`;
2. all published package/version surfaces are coherent;
3. the 27-crate publish graph is verified and publishable in dependency order;
4. release docs, CI docs, install docs, and user-facing compatibility statements describe the current system rather than the pre-fix `1.0.6` state;
5. historical planning files no longer advertise already-completed or superseded work as `READY FOR IMPLEMENTATION`;
6. dependency/feature cleanup is evidence-driven and does not turn into an unrelated modernization campaign;
7. no obsolete release workflow, stale helper, or misleading documentation remains authoritative;
8. the downstream handoff to Eggpool is explicit once the fixed Eggress release is available.

This pass should make the repository easy for another agent or maintainer to understand without rediscovering which plans are historical and which work is genuinely pending.

---

# Current facts that constrain the pass

## Version/release state

The workspace is currently `1.0.6`:

- root `[workspace.package].version = "1.0.6"`;
- internal workspace crates use exact `=1.0.6` pins;
- `crates/eggress-python/pyproject.toml` is `1.0.6`;
- `python-pproxy-compat/pyproject.toml` is `1.0.6` and pins `eggress==1.0.6`;
- the latest public GitHub release is `v1.0.6`.

`v1.0.6` predates the fixed embed SSH facade. A downstream consumer must not be told that `1.0.6` contains the fix.

The correction is patch-level in nature. Unless a maintainer has already reserved a different version, the expected next version is `1.0.7`. Do not hard-code `1.0.7` into source until the implementer has confirmed that no newer version has been published or reserved since this plan was written.

## Release architecture

The current release model is intentionally operator-driven:

- crates.io publication is manual;
- `.github/workflows/publish-python.yml` publishes the Python wheel/sdist on a `v*` tag;
- `.github/workflows/release-binaries.yml` builds the five canonical CLI target archives and creates/updates the GitHub Release on the same tag;
- ordinary CI remains a small smoke/verification signal and must not be expanded into a release engine;
- `scripts/release-preflight.sh` enforces version/tag alignment;
- `scripts/publish-remaining.sh` contains the dependency-first publication order for all 27 publishable `eggress-*` crates.

Do not replace this model with a new automated crates.io workflow during cleanup.

## SSH/embed state

The corrected `eggress-embed` feature boundary is:

```toml
pproxy-compat = ["dep:eggress-pproxy-compat"]
ssh = [
    "dep:eggress-transport-ssh",
    "eggress-runtime/ssh",
    "eggress-pproxy-compat?/ssh",
]
```

Required invariants:

- `ssh` alone must not activate `eggress-pproxy-compat`;
- native/TOML SSH must retain verified host-key policy;
- explicit pproxy SSH compatibility may retain compatibility policy at that constructor boundary;
- direct mode must not allocate SSH state;
- invalid SSH authentication remains fail-closed and credential-redacted;
- the private-key URI form such as `ssh://user::/path/to/key@host:22` must continue parsing correctly;
- the OpenSSH embed regression must remain an actual runtime test, not a compile-only assertion.

---

# Non-goals

Do **not** use this closure pass to:

- add new proxy protocols;
- add Eggpool-specific features, types, adapters, or public APIs;
- split `eggress-embed` or create an `eggress-outbound` crate merely for tidiness;
- redesign the runtime, routing, relay, or transport architecture;
- raise MSRV or change Rust edition without an independent requirement;
- upgrade dependencies solely because newer versions exist;
- remove compatibility features solely to reduce dependency count;
- add a Cartesian feature matrix to CI;
- make expensive external interoperability suites mandatory on every push;
- automate crates.io publishing;
- add mandatory SBOM/signing/container machinery;
- tune Eggress release profiles to optimize a downstream consumer's final binary;
- rewrite historical plans. Preserve their contents and add concise status/closure annotations instead;
- perform speculative micro-optimizations without measurements;
- expand the OpenSSH fixture into shared test infrastructure unless a third consumer or duplicated maintenance change makes that extraction clearly worthwhile.

If an apparently necessary cleanup requires one of these expansions, stop and create a narrowly scoped follow-up plan rather than broadening this one silently.

---

# Workstream 0 — Reconfirm the baseline before editing

## Goal

Avoid executing a stale closure plan against a moving repository.

## Steps

1. Fetch `main` and the latest GitHub release/tag.
2. Confirm whether the workspace is still `1.0.6`.
3. Confirm whether a post-`1.0.6` release already exists.
4. Confirm that the SSH implementation and closure commits remain in ancestry.
5. Check open issues/PRs for release blockers that appeared after this plan was written.
6. Re-read the current versions of:
   - `docs/CI_STATUS.md`
   - `docs/TESTING.md`
   - `docs/release/RELEASE_PROCESS.md`
   - `.github/workflows/ci.yml`
   - `.github/workflows/publish-python.yml`
   - `.github/workflows/release-binaries.yml`
   - `scripts/release-preflight.sh`
   - `scripts/publish-remaining.sh`

## Decision gate

If a newer Eggress release already contains the SSH fix, skip the publication-specific version bump and treat the remaining tasks as post-release repository cleanup. Do not publish a redundant patch release merely because this plan expected one.

---

# Workstream 1 — Reconcile the planning directory with repository reality

## Goal

Make `plans/` usable as a handoff surface again. Old plans must not tell an agent to reimplement work that already exists or resurrect superseded workflow architecture.

## Method

Inventory every `plans/*.md` file and classify its current state into exactly one of:

- **COMPLETE — VERIFIED**: the intended behavior exists and can be tied to current source/tests/workflows;
- **SUPERSEDED**: later architecture or policy intentionally replaced the plan before/as it landed;
- **READY FOR IMPLEMENTATION**: genuine unfinished work that is still desired and still architecturally valid;
- **DEFERRED / OPTIONAL**: useful future work with no current closure requirement.

Do not infer completion from file age alone. Check the current code, workflow, docs, tests, or release assets that correspond to each plan.

## High-priority stale plans to reconcile

At minimum inspect these files because their current `READY FOR IMPLEMENTATION` wording is known to conflict with later repository state:

- `CLI_CLEANUP_AND_BINARY_DELIVERY_ROADMAP.md`
- `CLI_COMMAND_SURFACE_AND_RUNTIME_BOUNDARY_CLEANUP.md`
- `CLI_BINARY_RELEASE_AND_INSTALLER.md`
- `CLI_VERSION_AND_SELF_UPDATE.md`
- `CLI_DISTRIBUTION_DOCUMENTATION_AND_RELEASE_POLICY.md`
- `ARCHITECTURE_CONVERGENCE_FINAL_CORRECTIVE_CLOSURE.md`
- `CI_VERIFICATION_RELEASE_FINAL_EVIDENCE_CLOSURE.md`
- `CI_VERIFICATION_RELEASE_CERTIFICATION_EXECUTION_CLOSURE.md`

Examples of current evidence that must influence classification:

- `eggress version` and `eggress update` already exist in the CLI surface;
- the repository already has tag-triggered Python and binary release workflows;
- `v1.0.6` already shipped multi-platform CLI archives/checksums/installers;
- `docs/CI_STATUS.md` explicitly supersedes older verification designs that treated every check as mandatory;
- the current release architecture intentionally has four workflows rather than one monolithic certification workflow.

Therefore, do not implement an old plan merely because its header says `READY FOR IMPLEMENTATION`.

## Editing rule

For a historical plan that is now complete or superseded:

1. change the status near the top;
2. append a short dated closure/supersession note;
3. point to the source/workflow/commit/document that proves the disposition;
4. leave the original body intact as historical context.

Do not rewrite dozens of pages to make old predictions read as if they were written today.

## Completion evidence

Record counts in this plan's final closure record:

```text
plans reviewed: N
complete/verified: N
superseded: N
deferred/optional: N
genuinely ready: N
```

A successful cleanup should leave **zero misleading `READY FOR IMPLEMENTATION` plans**. If genuine ready work remains, list it explicitly in the closure record and explain why it belongs outside this final pass.

---

# Workstream 2 — Repository truth/documentation cleanup

## Goal

Make current source-of-truth documents agree with current behavior and with the release that will contain the SSH correction.

## Audit targets

Search at least:

- `README.md`
- `docs/INSTALLATION.md`
- `docs/CI_STATUS.md`
- `docs/TESTING.md`
- `docs/release/RELEASE_PROCESS.md`
- `docs/EMBED_API.md`
- `architecture/`
- `.skills/`
- `AGENTS.md`
- crate-level README files
- installer/help text where release assumptions are user-visible.

## Required cleanup

### 2.1 Replace temporary `1.0.6` SSH warnings once a fixed version is selected

Current documentation correctly warns that the published `1.0.6` predates the SSH facade correction. Do not remove that warning before a fixed release exists.

For the release candidate, prefer durable wording such as:

```text
The listener-free SSH OutboundConnector correction is available beginning with vX.Y.Z.
```

rather than wording that becomes stale immediately after release.

### 2.2 Keep verification policy coherent

`docs/CI_STATUS.md` is the source of truth for routine verification. `docs/TESTING.md` already documents the required OpenSSH embed gate. `docs/release/RELEASE_PROCESS.md` intentionally says specialized checks are selected when relevant.

For this release, the SSH runtime regression **is relevant** and must be run. Do not solve this by turning every specialized test into a permanent release-wide mandatory command list. If the release guide needs clarification, add a concise example/pointer rather than duplicating `docs/TESTING.md` wholesale.

### 2.3 Remove obsolete workflow statements

Ensure no authoritative document claims any of the following if they are no longer true:

- there is only one hosted workflow;
- GitHub Releases are never automated;
- binary archives/installers do not exist;
- self-update is future work;
- version reporting is future work;
- SSH-only embed necessarily activates pproxy compatibility;
- `1.0.6` contains the SSH facade correction.

### 2.4 Do not create a changelog subsystem just for this release

There is no current root `CHANGELOG.md`/`RELEASE_NOTES.md`. Do not introduce a permanent changelog process unless maintainers explicitly want one. GitHub release text/commit history is sufficient for this cleanup release.

---

# Workstream 3 — Dependency and feature hygiene audit

## Goal

Perform one evidence-driven dependency/feature pass before publication without turning closure into a dependency modernization project.

## Commands

Run at minimum:

```bash
cargo metadata --locked --no-deps > /tmp/eggress-metadata.json
cargo tree --workspace --locked -d
cargo tree -p eggress-embed --locked --no-default-features --features ssh -e features
cargo tree -p eggress-embed --locked --no-default-features --features pproxy-compat -e features
cargo tree -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat -e features
```

If already installed in the development environment, an unused-dependency tool may be used as supporting evidence. Do **not** add a new project dependency or CI job solely to run it.

## Review questions

For each suspicious direct dependency or duplicate version, ask:

1. Is it directly used by the crate that declares it?
2. Is the duplicate caused by unavoidable transitive compatibility?
3. Does removal alter a public feature or supported protocol?
4. Does changing it materially reduce compile/binary/maintenance burden?
5. Is there a focused test that proves removal is safe?

Remove only dependencies/features with clear evidence of dead or redundant ownership.

Do not chase `cargo tree -d` to zero. Duplicate transitive versions are not automatically defects.

## Feature invariants

Reconfirm:

- default `full` composition remains intentional;
- SSH, QUIC, `pproxy-legacy`, legacy crypto, and daemon-only behavior remain explicitly gated where documented;
- `eggress-embed` `ssh` alone does not activate `eggress-pproxy-compat`;
- combined `ssh,pproxy-compat` activates both and remains buildable;
- no test-only insecure QUIC combination leaks into ordinary product gates.

Any feature change discovered here must be justified by a concrete maintenance/footprint/correctness benefit and receive focused tests. Otherwise record the finding and leave the graph alone.

---

# Workstream 4 — Validate the 27-crate publication graph

## Goal

Ensure the next version can actually be published from crates.io bottom-up without discovering an ordering or package-metadata defect midway through an irreversible release.

The current helper states that it publishes 27 crates and includes `eggress-relay` before `eggress-core`. Preserve that ordering requirement.

## Required checks

1. Compare workspace members against the crate names in `scripts/publish-remaining.sh`.
2. Confirm every publishable internal crate appears exactly once.
3. Confirm non-publishable root/bench packages are not accidentally included.
4. Confirm every internal dependency edge points to the same workspace version and an appropriate path/version declaration for package publication.
5. Confirm `eggress-relay` remains before `eggress-core` and all other dependency ordering remains topologically valid.
6. Confirm `eggress-admin` remains ahead of `eggress-runtime` because the optional dependency still has to resolve at package time.
7. Confirm the helper still never passes `--no-verify`.
8. Confirm the helper's long crates.io cooldown behavior is intentional; do not optimize it away without current rate-limit evidence.

## Dry-run gate

Before any real publication:

```bash
EGGRESS_PUBLISH_DELAY_SECONDS=0 scripts/publish-remaining.sh --dry-run
```

If the helper cannot safely use zero delay in dry-run mode, use the narrowest supported override that avoids needless sleeping while retaining package verification.

A dry-run failure is a packaging defect. Fix the manifest/package contents before publication; do not bypass Cargo verification.

---

# Workstream 5 — Prepare the patch release

## Goal

Create a coherent release commit containing the already-completed SSH correction, the compatibility parser correction, plan/document cleanup, and no unrelated feature churn.

## Version selection

Reconfirm latest public version first. If it is still `1.0.6`, use `1.0.7` unless maintainers have intentionally chosen another version.

## Version surfaces

Update all lockstep version surfaces required by `scripts/release-preflight.sh` and the release guide:

- root `[package]` version for `eggress-bench` where applicable;
- root `[workspace.package].version`;
- every internal exact `=X.Y.Z` pin under `[workspace.dependencies]`;
- `crates/eggress-python/pyproject.toml`;
- `python-pproxy-compat/pyproject.toml` project version;
- `python-pproxy-compat` exact `eggress==X.Y.Z` dependency;
- `Cargo.lock`;
- any generated or user-visible version constant that does not inherit from workspace metadata.

Then run:

```bash
scripts/release-preflight.sh --check-versions-only
cargo metadata --locked --no-deps >/dev/null
```

Do not manually bump each crate manifest if it already uses `version.workspace = true`.

## Release message scope

The release notes/description should emphasize:

- fixed listener-free SSH `OutboundConnector` state ownership;
- native/TOML SSH keeps verified host-key behavior;
- explicit pproxy SSH compatibility keeps compatibility semantics at that boundary;
- `ssh` no longer force-enables `eggress-pproxy-compat`;
- pproxy SSH private-key URI parsing correctly preserves `/path/to/key` inside credentials;
- required CI regression now exercises real OpenSSH byte traversal, fail-closed/redacted auth failure, and native untrusted-host rejection;
- any cleanup/dependency changes actually made in this pass.

Do not claim a breaking API redesign. No Eggpool-specific API was introduced.

---

# Workstream 6 — Release-candidate qualification

## Goal

Run one proportionate but complete release gate on the exact commit intended for publication.

## 6.1 Ordinary Rust gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## 6.2 Optional/product feature compile gate

```bash
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon \
  --bins
```

## 6.3 Embed feature boundaries

```bash
cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat
```

Inspect the feature trees as well; compile success alone does not prove `ssh` isolation.

## 6.4 Required SSH runtime gate

Because this release's principal correction is the embed SSH runtime boundary, run the real fixture in required mode:

```bash
EGRESS_REQUIRE_OPENSSH_TESTS=1 \
  cargo test -p eggress-embed --locked --no-default-features \
  --features ssh,pproxy-compat --test ssh -- --nocapture
```

Expected: all focused embed SSH regressions pass with an actual local OpenSSH daemon. A skipped fixture is **not** acceptable release evidence for this version.

Also run the lower-level transport fixture when OpenSSH is available:

```bash
cargo test -p eggress-transport-ssh --locked --test openssh -- --nocapture
```

## 6.5 Compatibility/parser gate

The SSH private-key pproxy URI parser changed as part of the fix. Run the focused compatibility crate tests and, if practical, the relevant pproxy differential/oracle slice:

```bash
cargo test -p eggress-pproxy-compat --locked
```

Use the external pproxy oracle only for the URI/behavior surface affected; do not run every historical certification suite merely for ceremony.

## 6.6 Python surface

Because the Python distributions move version in lockstep, perform the documented Python release smoke/build test even if no binding logic changed:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q
```

## 6.7 Dependency/security gate

Release preparation is an explicit trigger for:

```bash
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0009
```

Do not silently add new advisory ignores. If a new advisory appears, classify whether it is reachable/relevant before deciding whether release is blocked.

## 6.8 Fuzz compile smoke

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Do not require long fuzz campaigns unless this pass changes parser/hardening logic beyond the already-tested SSH URI correction.

## 6.9 CI confirmation

Push the release-preparation commit and require the normal hosted workflows relevant to changed paths to pass. In particular, confirm the Rust CI job **executes** the OpenSSH embed regression rather than skipping it.

Do not proceed to publication from a commit whose CI is red.

---

# Workstream 7 — Publish the release through the existing channels

## Goal

Move the fixed architecture from `main` into supported public artifacts without changing the release model.

## 7.1 crates.io

Use the existing dependency-first helper or equivalent explicit commands:

```bash
scripts/publish-remaining.sh
```

Publication is irreversible. Do not use `--allow-dirty` or `--no-verify` to force through a packaging problem.

After the top-level crate is visible, verify from a clean install root:

```bash
cargo install eggress-cli --version <version> --locked --root /tmp/eggress-release-check
/tmp/eggress-release-check/bin/eggress version
/tmp/eggress-release-check/bin/eggress --version
/tmp/eggress-release-check/bin/pproxy --version
/tmp/eggress-release-check/bin/pproxy --help
```

For the embed fix specifically, also verify a minimal temporary consumer resolves the published `eggress-embed` release with:

```toml
[dependencies]
eggress-embed = { version = "=<version>", default-features = false, features = ["ssh"] }
```

and separately with `features = ["ssh", "pproxy-compat"]`.

This published-consumer check is valuable because the cleanup is partly about the dependency/feature boundary, not merely the workspace build.

## 7.2 Tag

After crates.io publication and version coherence are verified:

```bash
git tag -a v<version> -m "Release v<version>"
git push origin v<version>
```

Do not move/reuse an existing tag.

## 7.3 PyPI

The tag triggers `.github/workflows/publish-python.yml`.

Verify:

- workflow success;
- the expected five abi3 wheels plus sdist were built/smoke-tested;
- PyPI exposes the exact tagged version;
- a clean environment can `pip install eggress==<version>` and import it.

The opt-in `eggress-pproxy-compat` distribution remains manually published according to current policy; do not silently add it to the automated workflow. If the release intends to publish that distribution, follow its documented manual channel and verify its exact `eggress==<version>` pin.

## 7.4 Prebuilt CLI/GitHub Release

The same tag triggers `.github/workflows/release-binaries.yml`.

Verify:

- all five canonical target archives exist;
- all five SHA-256 sidecars exist;
- `install.sh` and `install.ps1` are attached;
- the workflow smoke-tested both `eggress` and `pproxy` binaries;
- the GitHub Release points at the immutable intended tag/commit;
- installers resolve the new release rather than `1.0.6`.

Do not add extra target platforms in this closure unless an existing advertised platform is missing.

---

# Workstream 8 — Post-release documentation and planning closure

## Goal

Immediately remove temporary wording that was correct only while the fix was unreleased and leave durable evidence of what shipped.

## Required updates

1. Replace `1.0.6 predates this correction` language with the exact minimum fixed release where appropriate.
2. Verify installation documentation points to the new latest release without hard-coded stale URLs.
3. Verify Rust embed examples use a compatible version requirement and do not imply `1.0.6` contains the fix.
4. Mark this plan `COMPLETE — VERIFIED <date>` only after public publication checks pass.
5. Append a closure record containing:
   - release version and tag;
   - release commit SHA;
   - CI run/result for release commit;
   - crates.io publication confirmation;
   - PyPI publication confirmation;
   - GitHub Release/binary workflow confirmation;
   - OpenSSH regression result and test count;
   - feature-tree isolation result;
   - publish-helper dry-run result;
   - dependency/security audit result;
   - plan lifecycle inventory counts;
   - any dependencies/features removed, or an explicit statement that none were justified;
   - any intentionally deferred work.

Do not mark the plan complete merely because a version bump commit exists.

---

# Workstream 9 — Downstream Eggpool handoff

## Goal

Make the cross-repository benefit actionable without contaminating Eggress with downstream-specific code.

Once a fixed Eggress release is publicly available, record the exact minimum version for downstream consumers.

The subsequent Eggpool work should be performed in the Eggpool repository, not here:

1. upgrade Eggpool from Eggress `1.0.6` to the fixed release;
2. use `eggress-embed::outbound::OutboundConnector` as the listener-free SSH ownership boundary;
3. remove Eggpool's temporary `eggress-ssh-fallback` implementation path;
4. remove direct Eggress implementation-crate dependencies that existed solely to reconstruct parser/config/compiler/executor/session-cache state;
5. retain any independent Eggpool test-only TLS/root fixture seam only if it remains necessary for Eggpool's own tests;
6. rerun Eggpool's proxy matrix, SSH runtime tests, failover/failure-isolation tests, feature checks, and footprint comparison;
7. confirm the simplification reduces maintenance ownership even if binary-size reduction is modest.

Do not add an Eggpool feature, adapter, or compatibility type back into Eggress to make that migration easier.

---

# Workstream 10 — Final repository hygiene pass

## Goal

Catch small closure defects without opening another refactor cycle.

After the release-specific work, inspect for:

- tracked build artifacts or temporary files;
- obsolete workflow files no longer referenced by policy docs;
- dead release helpers superseded by current scripts;
- stale install commands;
- broken internal documentation links;
- package metadata that references removed files;
- duplicated authoritative policy statements that materially disagree;
- `README`/crate README claims that no longer match feature defaults;
- ignored tests whose comments no longer explain why they are ignored.

Removal rule: delete an obsolete helper/document only when the current replacement is unambiguous and no active workflow/script references it. Historical plan files should generally be retained with status annotations rather than deleted.

Run the broad Rust/Python gates again only if this hygiene work touches executable or package behavior. Pure plan/document status edits do not justify a second full multi-minute qualification run.

---

# Implementation order

Use this order to minimize irreversible release risk:

1. Reconfirm repository/release baseline.
2. Inventory and classify all plan files; identify genuinely pending work before changing code.
3. Audit documentation truth and release-policy references.
4. Audit dependency/feature graph and the 27-crate publish order.
5. Make only justified cleanup changes discovered by steps 2–4.
6. Select the release version.
7. Update all lockstep version surfaces and package metadata.
8. Update durable release-facing documentation for the selected version.
9. Run version coherence and package dry-runs.
10. Run the complete release-candidate qualification, including required OpenSSH runtime coverage.
11. Push the release commit and require green CI.
12. Publish crates.io packages dependency-first.
13. Verify clean crates.io installation and minimal published embed consumers.
14. Create/push the immutable version tag.
15. Verify PyPI workflow/publication.
16. Verify binary workflow/GitHub Release assets/installers.
17. Perform post-release documentation/status closure.
18. Append evidence to this plan and mark it complete.
19. Hand the released Eggress version to the Eggpool cleanup work.

Do not tag before crates.io publication is known-good: tag-triggered Python/binary release automation is a real production action.

---

# Failure/rollback policy

Crates.io versions and published tags are effectively immutable release boundaries.

Before any publication, normal Git rollback/fixup is acceptable.

After any crate version has been published:

- do not overwrite or retag it;
- do not force-push a release tag;
- yank only when appropriate;
- correct the defect on `main`;
- increment the patch version again;
- rerun the relevant verification;
- roll forward with a new release.

If Python or binary tag workflows fail after the tag exists, fix the workflow/artifact issue without moving the tag if the tag's source commit itself is correct. Follow current release-process guidance for rerun/manual-dispatch behavior.

---

# Required completion criteria

This plan is complete only when all applicable conditions below are true:

1. The fixed `OutboundConnector` SSH implementation is contained in a public Eggress release newer than `1.0.6`, unless Workstream 0 determines that such a release already existed before implementation began.
2. Workspace, internal exact pins, Python package, compatibility package, lockfile, and release tag versions agree.
3. `scripts/release-preflight.sh --check-versions-only` passes on the release commit.
4. All 27 publishable crates are accounted for in a valid dependency-first publication graph.
5. `scripts/publish-remaining.sh --dry-run` (or equivalent per-crate Cargo dry runs) succeeds before irreversible publication.
6. fmt, Clippy, workspace tests, optional CLI compile gate, embed feature slices, fuzz compile smoke, and release-triggered dependency/security audits pass.
7. The required embed OpenSSH runtime regression executes rather than skips and proves byte traversal, fail-closed/redacted invalid authentication, and native untrusted-host rejection.
8. `ssh` alone still does not activate `eggress-pproxy-compat`.
9. The SSH private-key pproxy URI regression remains covered.
10. crates.io exposes the new top-level CLI and embed crates and a clean install/consumer resolves them.
11. PyPI exposes the new `eggress` version and its tag-triggered workflow succeeds.
12. The GitHub Release exposes all canonical CLI archives, checksum sidecars, and installers and the binary workflow succeeds.
13. No authoritative documentation still incorrectly says `1.0.6` contains the SSH fix or that the fix remains unreleased.
14. Historical planning files have been classified so no misleading `READY FOR IMPLEMENTATION` status remains.
15. Current CI/release documentation agrees on the four-workflow/operator-driven model.
16. Dependency/feature cleanup was either completed with evidence or explicitly closed with a finding that no further removal was justified.
17. No Eggpool-specific API or new architecture was added to Eggress.
18. This plan contains a final closure record with release and verification evidence.
19. The fixed release version is explicitly handed off for the downstream Eggpool fallback-removal pass.

---

# Handoff guidance for the implementation agent

This is a closure task, not permission to search indefinitely for improvements.

Prefer evidence over aesthetic cleanup. If a dependency is not demonstrably dead, leave it. If an old plan describes architecture that current policy intentionally replaced, mark it superseded rather than reimplementing it. If documentation differs because one file is historical and another is explicitly authoritative, update the historical status rather than destabilizing the current system.

The highest-value outcome is a clean public release and a repository whose source-of-truth documents and planning state agree with reality.

Once those conditions are satisfied, stop. Further feature work belongs in a new plan with a new justification.