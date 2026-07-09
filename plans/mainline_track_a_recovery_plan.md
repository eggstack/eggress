# Mainline Track A Recovery Plan

## Objective

Recover `main` after the Track A pproxy parity branch was merged while required checks were still failing. The immediate target is not to expand parity scope. The immediate target is to make `main` green and honest again while preserving useful Track A scaffolding.

This plan should be treated as the next implementation handoff. It supersedes ad hoc Track A continuation until `main` is stable.

## Current Situation

PR #1 was merged into `main` at commit `07e35d65cfb3aac2d454d448b3317e55a60e148b`. That merge brought in useful Track A assets:

- full pproxy parity roadmap and detailed Track A handoff plans;
- canonical pproxy capability manifest validation scaffolding;
- expanded parity manifest/reporting;
- a `pproxy` compatibility binary target;
- `fancy_regex`-backed compatibility regex support;
- oracle scenario registry and test harness scaffolding;
- additional pproxy binary and differential tests;
- Python feature-report alignment changes.

However, the merged head still had failing workflows:

- `CI` failed;
- `Python Tests` failed;
- `pproxy-compatibility` failed;
- `Shadowsocks Interop` failed.

Within CI, the known failing/cancelled areas were:

- Ubuntu `cargo check --workspace --all-targets` failed;
- Ubuntu `cargo test --workspace` failed;
- Clippy failed with `cargo clippy --workspace --all-targets -- -D warnings`;
- Ubuntu pproxy interoperability failed with `cargo test --test interoperability_pproxy`;
- macOS and Windows checks/tests were cancelled after earlier failures;
- format, deny, and audit passed in the inspected run.

## Recovery Policy

Until this plan is complete, treat `main` as unstable.

Rules for the corrective pass:

1. Do not add new parity features until compile, clippy, workspace tests, and required workflow checks are green.
2. Do not mark any capability as `drop_in` unless the intended surface is executable and covered by real evidence.
3. Do not cite oracle scenarios as differential evidence until the oracle actually runs both pproxy and egress and compares normalized results.
4. Prefer demoting claims over making tests permissive.
5. Prefer narrow fixes over broad lint suppression, broad `ignore`, or broad CI skips.
6. Keep historical parity claims only if explicitly labeled as historical; active status docs must reflect current reality.

## Phase 0: Create Corrective Branch

Create a dedicated branch from current `main`:

```bash
git checkout main
git pull --ff-only
git checkout -b fix/mainline-track-a-recovery
```

All implementation work for this recovery pass should land through a new PR. Do not continue pushing directly to `main` unless repository policy explicitly requires direct commits.

## Phase 1: Capture Exact Failures

### Goal

Identify the first actionable root cause in each failed workflow without guessing.

### Tasks

1. Fetch or re-run the latest failed workflows for the merged commit.
2. Capture the first real compiler/test error from each failed job:
   - CI / Check Ubuntu;
   - CI / Clippy;
   - CI / Test Ubuntu;
   - CI / Interoperability Ubuntu;
   - Python Tests;
   - pproxy-compatibility;
   - Shadowsocks Interop.
3. Store findings in a temporary local note while implementing. Do not commit noisy raw logs unless a concise summary is useful in `docs/` or the PR body.
4. Fix failures in dependency order:
   - compile errors first;
   - clippy warnings second;
   - unit/integration test failures third;
   - external/gated compatibility workflows last.

### Acceptance Criteria

- Each failing workflow has at least one identified root cause.
- No implementation work starts from speculation where logs are available.

## Phase 2: Restore Workspace Compile

### Goal

Make `cargo check --workspace --all-targets` pass on Ubuntu.

### Likely Focus Areas

- `crates/eggress-cli/src/pproxy_main.rs`;
- `crates/eggress-cli/tests/oracle.rs`;
- `crates/eggress-cli/tests/pproxy_binary.rs`;
- `crates/eggress-testkit/src/canonical_manifest.rs`;
- `crates/eggress-testkit/src/oracle/*`;
- `crates/eggress-pproxy-compat/src/regex_compat.rs`;
- workspace dependency placement for `fancy-regex`.

### Tasks

1. Run:

   ```bash
   cargo check --workspace --all-targets
   ```

2. Fix missing imports, incorrect visibility, unused dependencies under all-targets, incorrect feature gates, and integration-test compile errors.
3. Verify binary targets compile:

   ```bash
   cargo check -p eggress-cli --bin eggress
   cargo check -p eggress-cli --bin pproxy
   ```

4. Verify test targets compile:

   ```bash
   cargo test --workspace --no-run
   ```

### Acceptance Criteria

- `cargo check --workspace --all-targets` passes locally and in CI.
- `cargo test --workspace --no-run` passes.
- No broad `#[allow(...)]` is added outside narrow, justified test-only cases.

## Phase 3: Restore Clippy Cleanliness

### Goal

Make `cargo clippy --workspace --all-targets -- -D warnings` pass without weakening lint standards.

### Tasks

1. Run:

   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   ```

2. Fix clippy findings directly.
3. Pay special attention to:
   - needless clones in oracle scenario setup;
   - `unwrap`/`expect` in production paths;
   - large test helper functions that trigger complexity warnings;
   - format-string and path handling warnings in `pproxy_main.rs`;
   - `fancy_regex` result handling.
4. If a lint suppression is needed, scope it to the smallest item and explain why in a comment.

### Acceptance Criteria

- Clippy passes in CI with `-D warnings`.
- No broad crate/module-level `allow(clippy::all)` is introduced for production code.

## Phase 4: Restore Workspace Tests

### Goal

Make `cargo test --workspace` pass without hiding broken Track A behavior behind permissive assertions.

### Tasks

1. Run:

   ```bash
   cargo test --workspace
   ```

2. Separate failures into:
   - deterministic unit-test failures;
   - integration-test fixture failures;
   - port binding / readiness races;
   - schema-validation failures;
   - expected external dependency failures that should be ignored/gated.
3. Fix deterministic failures first.
4. Replace arbitrary sleeps in new tests with readiness polling wherever possible.
5. Avoid fixed ports in normal tests. Use ephemeral ports or translation-only assertions.
6. Any external dependency test must be `#[ignore]` or environment-gated by default.

### Acceptance Criteria

- `cargo test --workspace` passes on Linux.
- macOS and Windows tests are not expected to be cancelled by earlier Linux failures.
- New tests do not silently pass on startup failure unless the failure is explicitly the scenario under test.

## Phase 5: Correct `pproxy` Binary Semantics Enough For Mainline Stability

### Goal

Fix known semantic defects that can cause tests, docs, or users to observe broken compatibility behavior.

### Known Defects

1. `pproxy --test` likely invokes the `pproxy` wrapper via `std::env::current_exe()` instead of invoking the real egress upstream-test implementation.
2. No-argument `pproxy` behavior was tested as an error, but pproxy compatibility expects default mixed listener behavior.
3. Some binary tests accept `startup OR error`, which can hide broken startup paths.

### Tasks

1. Fix `pproxy --test` by using one of:
   - in-process shared upstream test implementation;
   - explicit sibling `eggress` binary resolution;
   - a hidden internal subcommand only if cleanly separated from user-facing pproxy CLI.
2. Add tests for:
   - successful `pproxy --test` path;
   - failed `pproxy --test` path;
   - exit status propagation;
   - no recursive wrapper invocation.
3. Implement or conservatively classify no-argument behavior:
   - preferred: translate no args to mixed HTTP/SOCKS listener on `:8080` with direct routing;
   - if runtime implementation is deferred, docs/manifest must not mark no-arg behavior `drop_in`.
4. Rewrite binary tests so successful-start scenarios require successful readiness, not merely non-crash or any stderr.
5. Keep unsupported-feature diagnostics precise and stable.

### Acceptance Criteria

- The `pproxy` binary does not recursively invoke itself for `--test`.
- No-argument behavior is either actually compatible or explicitly demoted in manifest/docs.
- Binary tests fail on unexpected startup errors.

## Phase 6: Correct Oracle Harness Status

### Goal

Prevent the oracle harness from providing false confidence while preserving it as useful scaffolding.

### Current Problem

The oracle registry exists, but the runner has been observed to exercise pproxy only rather than running both pproxy and egress and comparing results. That means it is not yet valid differential evidence.

### Tasks

1. If time allows, implement minimal two-sided oracle execution for:
   - HTTP CONNECT;
   - SOCKS5 CONNECT;
   - one simple direct routing scenario.
2. If two-sided oracle execution cannot be completed in this recovery pass, then:
   - keep oracle tests ignored/gated;
   - mark them explicitly as scaffold/pre-certification;
   - remove or demote any manifest/report evidence that cites oracle scenarios as completed differential evidence.
3. Add a validation rule or test that prevents oracle scenario IDs from being cited as `differential` unless the two-sided runner flag/capability is enabled.
4. Ensure oracle scenarios that are intended to run egress use valid config snippets.

### Acceptance Criteria

Either:

- the oracle compares both pproxy and egress for the minimal core set and fails on one-sided breakage;

or:

- the oracle is clearly labeled scaffold/gated, and no active parity claim depends on it.

## Phase 7: Validate Oracle Config Schema

### Goal

Prevent schema-invalid scenario TOML from living unnoticed in `main`.

### Tasks

1. Identify the canonical egress config schema from the config crate and existing passing integration tests.
2. Add a test that parses every oracle scenario `egress_toml` snippet through the real config loader/compiler after placeholder substitution.
3. Correct invalid snippets, especially any using non-canonical structures such as singular `[listener]` or ambiguous `[[upstream]] bind` fields if those are not accepted by the real schema.
4. If translator-derived scenarios are intended, generate config through the translator instead of maintaining duplicate hand-written TOML.
5. Mark every scenario as one of:
   - translator-derived;
   - native-config-derived;
   - scaffold-only.

### Acceptance Criteria

- Scenario TOML parse validation runs in normal tests.
- Invalid oracle configs cannot silently remain in `main`.
- Scaffold-only scenarios are not cited as passing runtime evidence.

## Phase 8: Fix pproxy Compatibility Workflow

### Goal

Make the `pproxy-compatibility` workflow either pass or fail only on explicitly documented, non-required gated conditions.

### Tasks

1. Inspect the failed workflow logs.
2. Identify whether the failure is caused by:
   - compile failure already covered by earlier phases;
   - manifest/report mismatch;
   - pproxy external package behavior;
   - new binary behavior;
   - oracle scaffold overclaiming.
3. Fix the workflow so required checks verify real supported behavior.
4. Do not weaken the workflow by turning meaningful failures into unconditional skips.
5. Add environment-gated mode for external pproxy package tests if the workflow relies on network/package installs.

### Acceptance Criteria

- `pproxy-compatibility` passes on the corrective branch.
- The workflow remains meaningful and catches regression in pproxy compatibility surfaces.

## Phase 9: Fix Python Tests and Feature Reporting

### Goal

Restore Python test health and prevent Python-facing feature overclaims.

### Tasks

1. Run the same Python commands used by CI.
2. Build the extension exactly as CI does, likely through `maturin` or the project’s existing Python workflow.
3. Fix import/package failures first.
4. Audit Python feature reporting after the manifest expansion:
   - do not list parser-only features as runtime-supported;
   - do not list protocol-crate-only features as supported through Python;
   - ensure Trojan status matches runtime reality;
   - ensure WebSocket/raw/H2 status remains honest if runtime/config refuse them.
5. If top-level `pproxy` import is not implemented, do not claim Python drop-in library parity yet.
6. Add or adjust Python tests for `supported_features()` / manifest-derived status.

### Acceptance Criteria

- Python Tests workflow passes.
- Python-facing docs and feature-report APIs agree with the canonical manifest.
- No top-level `pproxy` import claim exists unless it is actually implemented and tested.

## Phase 10: Fix Shadowsocks Interop Workflow

### Goal

Classify and resolve the Shadowsocks Interop failure without overclaiming compatibility.

### Tasks

1. Inspect the failed job logs.
2. Classify failure as:
   - Track A regression;
   - known pproxy/Shadowsocks limitation;
   - environment dependency issue;
   - stale test expectation;
   - transient network/package issue.
3. If regression, fix implementation.
4. If known limitation, move the test to an explicit ignored/gated path and document why.
5. If environment issue, make the workflow deterministic or soften only the environment-dependent part.
6. Reconcile manifest tiers for Shadowsocks entries with actual interop evidence.

### Acceptance Criteria

- Shadowsocks Interop workflow passes or is explicitly gated with documented rationale.
- Manifest does not claim stronger Shadowsocks pproxy parity than tests prove.

## Phase 11: Documentation Honesty Pass

### Goal

Ensure `main` no longer overstates parity while recovery is still underway.

### Tasks

1. Search active docs for unqualified final/full parity claims:

   ```bash
   rg -n "final parity|full pproxy parity|drop-in replacement|certification complete" README.md docs crates python tests plans
   ```

2. Active docs should say something equivalent to:

   > Track A pproxy parity hardening is in progress. Common HTTP/SOCKS/TCP compatibility has substantial implementation and test coverage, but full pproxy parity and Python drop-in replacement certification remain gated on green CI, two-sided oracle evidence, and Track B Python API completion.

3. Keep historical release docs only if clearly labeled historical or superseded.
4. Ensure README, parity report, compatibility evidence, and parity matrix agree on:
   - current status;
   - capability count;
   - canonical manifest path;
   - known unsupported/deferred surfaces.
5. If generated reports are checked in, regenerate them after manifest corrections.

### Acceptance Criteria

- No active doc page claims Track A is complete if CI or oracle remains incomplete.
- README and parity report do not contradict each other.
- Historical docs are marked historical/superseded where needed.

## Phase 12: Manifest Evidence Audit

### Goal

Make the expanded manifest trustworthy.

### Tasks

1. Audit all newly split capabilities added during Track A.
2. For each capability, verify:
   - parser support;
   - translator support;
   - config support;
   - runtime support;
   - CLI support;
   - Python support, if claimed;
   - evidence class;
   - actual test/scenario IDs.
3. Demote entries whose evidence is incomplete.
4. Add validator checks for:
   - nonexistent test/scenario IDs;
   - oracle scenario IDs cited before two-sided oracle support;
   - `drop_in` status without required layers;
   - protocol-crate-only/runtime-refused contradictions.
5. Re-run manifest/report generation.

### Acceptance Criteria

- Manifest passes validation.
- Manifest references no nonexistent evidence IDs.
- Expanded capability count is accompanied by accurate tiers, not inflated support claims.

## Phase 13: Final Verification

Run the required local equivalents:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run project-specific validation:

```bash
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

Run Python tests with the same interpreter/build path as CI:

```bash
python3.11 -m pytest python/tests
```

Run gated tests only after installing external dependencies:

```bash
python3.11 -m pip install "pproxy==2.7.9"
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle -- --ignored
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test interoperability_pproxy -- --ignored
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored
```

If any gated check fails, update manifest/report accordingly and do not cite it as passing evidence.

## PR Requirements For Recovery Branch

The corrective PR should include:

- concise summary of failures fixed;
- list of commands run locally;
- status of all GitHub workflows;
- explicit note whether oracle is now two-sided or still scaffold-only;
- explicit note whether no-arg pproxy behavior is implemented or demoted;
- explicit note whether `pproxy --test` is fixed;
- explicit note of any remaining Track A follow-up that is deferred after mainline recovery.

## Exit Criteria

This recovery line is complete when:

1. `main` can be made green through the corrective PR.
2. Required CI, Python Tests, pproxy-compatibility, and Shadowsocks Interop are green or intentionally reclassified as non-required/gated with documentation.
3. `cargo check --workspace --all-targets`, clippy, and workspace tests pass.
4. Active docs no longer claim final/full pproxy parity ahead of evidence.
5. The parity manifest/report is internally consistent.
6. Oracle evidence is either genuinely two-sided or not used as completed differential evidence.
7. Python feature reporting is layer-aware and does not overclaim runtime support.
8. `pproxy` binary behavior is stable enough for mainline: no recursive `--test`, no permissive startup tests, and honest no-arg default status.

## Deferred After Recovery

Only after `main` is green should the project continue with broader Track A/Track B work:

- expand two-sided oracle coverage beyond the minimal core set;
- complete no-arg pproxy runtime parity if initially demoted;
- expand CLI exactness for daemon/reuse/get/system proxy behavior;
- finish Python top-level `pproxy` import/drop-in API for Track B;
- add long-tail protocol parity work for Track C.
