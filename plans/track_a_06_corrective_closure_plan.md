# Track A.06: Corrective Closure Plan

## Objective

Bring the Track A implementation branch from promising but non-merge-ready to a defensible, mergeable pproxy parity foundation. The branch currently contains meaningful progress: canonical manifest validation, a `pproxy` binary target, a `fancy_regex` compatibility backend, expanded HTTP/SOCKS capability granularity, and a scenario-driven oracle registry. However, it is not safe to merge until CI passes and the new compatibility evidence is real rather than partially declarative.

This corrective pass must address all remaining items surfaced after the Track A implementation attempt:

- failing CI checks;
- incomplete oracle semantics;
- config-schema drift in oracle scenarios;
- incorrect `pproxy --test` execution path;
- no-argument pproxy default mismatch;
- documentation overclaiming;
- manifest/report consistency after capability expansion;
- pproxy binary behavior and tests that accept weak error/startup outcomes;
- `fancy_regex` integration boundaries and resource-safety documentation;
- Python/drop-in status reporting drift.

## Current Branch State

Branch: `plans/full-pproxy-parity-track-a`

PR: `#1`

Latest inspected head: `6742286dfee1ed7b4019a66d7a6191285ac1823b`

Observed state:

- branch is open and mergeable at the GitHub metadata level;
- branch is ahead of `main` and implementation work is not yet on `main`;
- GitHub checks are failing for CI, Python Tests, pproxy-compatibility, and Shadowsocks Interop;
- format, deny, and audit passed in the inspected run;
- Ubuntu `cargo check`, workspace tests, clippy, and pproxy interoperability failed or were cancelled after failure;
- the oracle registry exists but does not yet run both pproxy and egress for comparison;
- docs still preserve strong "final parity certification" language despite the new branch being failing and the oracle being incomplete.

## Guiding Principle

Do not mark any capability `drop_in` unless it is actually executable through the intended surface and has evidence appropriate to that surface.

For this corrective pass, prefer conservative compatibility tiers over optimistic claims. It is acceptable for the branch to end with fewer `drop_in` entries if the evidence becomes trustworthy. It is not acceptable for the branch to claim full/final parity while the certification path is incomplete or failing.

## Workstream 1: CI Red/Green Triage

### Goals

Make the branch compile and pass the normal ungated test suite before expanding any more compatibility claims.

### Required Tasks

1. Re-run or inspect the failing CI jobs in this order:
   - Ubuntu `cargo check --workspace --all-targets`;
   - `cargo clippy --workspace --all-targets -- -D warnings`;
   - Ubuntu `cargo test --workspace`;
   - `cargo test --test interoperability_pproxy`;
   - Python Tests;
   - pproxy-compatibility;
   - Shadowsocks Interop.
2. Treat `cargo check` and clippy failures as first-class blockers. Do not proceed to runtime diagnosis until compile/lint failures are fixed.
3. Capture the exact first compiler error per job in a temporary local notes file or PR comment, then fix from the first root cause outward.
4. Ensure `RUSTFLAGS=-D warnings` remains honored; do not hide warnings with broad `allow` attributes unless they are narrowly justified.
5. Keep newly ignored tests out of the normal suite unless they are genuinely gated external-dependency tests.

### Likely Areas To Inspect

- `crates/eggress-cli/src/pproxy_main.rs` for binary-target compile issues and incorrect dependency wiring.
- `crates/eggress-cli/tests/oracle.rs` for unused imports, dead paths, and incorrect helper signatures under `-D warnings`.
- `crates/eggress-testkit/src/oracle/*` for serde derives, unused fields, and schema mismatch.
- `crates/eggress-pproxy-compat/src/regex_compat.rs` for `fancy_regex` error types, unused helper paths, and test-only dependencies.
- workspace `Cargo.toml` / `Cargo.lock` for `fancy-regex` dependency placement.

### Acceptance Criteria

- `cargo fmt --all -- --check` passes.
- `cargo check --workspace --all-targets` passes on Ubuntu.
- `cargo clippy --workspace --all-targets -- -D warnings` passes on Ubuntu.
- `cargo test --workspace` passes on Ubuntu, excluding explicitly ignored/gated external tests.
- No new broad lint suppressions are introduced to force green.

## Workstream 2: Correct the Oracle Harness Semantics

### Problem

The new oracle registry is useful, but the runner currently starts and exercises pproxy only. It does not start egress, exercise egress, normalize both outputs, and compare them. That means the oracle report cannot support parity claims yet.

### Required Tasks

1. Split oracle execution into explicit phases:
   - allocate ports;
   - start fixtures;
   - start pproxy side;
   - wait for pproxy readiness;
   - exercise pproxy side;
   - stop pproxy side;
   - start egress side;
   - wait for egress readiness;
   - exercise egress side;
   - stop egress side;
   - normalize outputs;
   - compare according to scenario equivalence target;
   - emit scenario result.
2. Add a process abstraction for both systems:
   - `OracleProcess::Pproxy`;
   - `OracleProcess::Eggress`;
   - captured stdout/stderr;
   - exit status;
   - readiness status;
   - cleanup/kill behavior.
3. Replace fixed sleeps with readiness polling:
   - TCP port connect polling for listeners;
   - optional startup-line parsing only as a fallback;
   - hard deadline per scenario.
4. Make scenario failures fail the gated oracle test. Do not allow `Pass || Skipped` as a normal assertion for scenarios that have no unmet platform/dependency requirement.
5. Preserve skip semantics only for declared requirements, such as root, IPv6, external pproxy package, platform, or legacy feature flags.
6. Generate a JSON report with both sides represented:
   - `pproxy.stdout`;
   - `pproxy.stderr`;
   - `pproxy.exit_status`;
   - `egress.stdout`;
   - `egress.stderr`;
   - `egress.exit_status`;
   - normalized comparison artifacts;
   - pass/fail/skip status;
   - failure reason.
7. Add a minimal Markdown report path later only after JSON is reliable.

### Acceptance Criteria

- The oracle harness executes both pproxy and egress for at least HTTP CONNECT, SOCKS5 CONNECT, and one chain scenario.
- A scenario with egress-only failure fails the oracle test.
- A scenario with pproxy-only failure fails unless explicitly classified as skipped due to unmet dependency.
- The JSON report contains both pproxy and egress results.
- The manifest does not cite oracle scenario IDs as differential evidence until those scenarios actually compare both sides.

## Workstream 3: Fix Oracle Scenario Config Schema Drift

### Problem

The scenario registry currently contains TOML snippets that appear inconsistent with the repository's established egress config model. Examples include singular `[listener]`, `[[upstream]]`, and fields such as `bind = "socks5://..."`. Existing docs and runtime appear to use plural listener/upstream sections and URI/group-based configuration.

### Required Tasks

1. Identify the canonical runtime config schema from `eggress-config` and current working integration tests.
2. Replace all oracle `egress_toml` snippets with schema-valid configs.
3. Add a test that parses every oracle `egress_toml` snippet through the real config loader/compiler without starting the network.
4. Add placeholder substitution validation:
   - all placeholders in a scenario must be known;
   - no unreplaced `{PORT}`, `{PORT2}`, `{ECHO_PORT}`, `{UPSTREAM_PORT}`, or similar tokens may remain before parse.
5. Use the pproxy translator where appropriate rather than hand-written TOML when the scenario is specifically testing CLI translation. Separate two scenario types:
   - translator-derived egress config;
   - hand-written native egress config.
6. For hand-written configs, include a field in `OracleScenario` indicating why the config is native rather than translator-derived.

### Acceptance Criteria

- Every oracle scenario TOML snippet parses through the real config path.
- Every scenario declares whether it is translator-derived or native-config-derived.
- No scenario's egress side is skipped because its TOML is schema-invalid.
- Config schema drift is caught by normal ungated unit tests, not only by gated oracle tests.

## Workstream 4: Correct `pproxy --test`

### Problem

The new `pproxy` compatibility binary writes translated config and then invokes `std::env::current_exe()` with `upstream test -c ...`. In the `pproxy` binary this resolves to the `pproxy` wrapper, not the `eggress` binary. Unless the wrapper supports `upstream test`, this can recurse into the wrong command shape or fail incorrectly.

### Required Tasks

1. Replace `current_exe()` dispatch with one of the following:
   - call the upstream test implementation in-process through a shared library/helper;
   - resolve the sibling `eggress` binary path robustly;
   - add a hidden internal mode to the `pproxy` binary only if it is clearly separated from user-facing pproxy CLI.
2. Prefer in-process library invocation. It avoids binary-path ambiguity and works better in tests and packaged installs.
3. Add tests for:
   - `pproxy --test` with a reachable direct upstream;
   - `pproxy --test` with a refused upstream;
   - exit status propagation;
   - stderr/stdout shape;
   - temp config cleanup.
4. Ensure the Python console-script path for Track B can reuse the same test implementation rather than shelling out.

### Acceptance Criteria

- `pproxy --test` does not recursively execute the pproxy wrapper.
- Success and failure exit codes are deterministic.
- The behavior is documented as `drop_in`, `native_equivalent`, or `compatible_with_warning` in the manifest based on actual pproxy comparison.

## Workstream 5: Implement pproxy No-Argument Defaults

### Problem

The new binary test currently expects no-argument `pproxy` to error. That is not pproxy-compatible. Real pproxy defaults to a mixed HTTP/SOCKS listener on port 8080 with direct routing.

### Required Tasks

1. Change `PproxyArgs::parse(&[])` or the `pproxy` binary wrapper to synthesize default arguments equivalent to:
   - listener: mixed HTTP/SOCKS4/SOCKS5 on `:8080`;
   - upstream: direct.
2. Confirm exact pproxy default bind host and protocol set through the existing compatibility docs/oracle baseline.
3. Update tests that currently expect no-args error.
4. Add tests for:
   - parse default shape;
   - translation default TOML;
   - startup banner for no-args mode;
   - actual listener readiness in a gated or short runtime test using port override if needed.
5. Avoid hard-binding port 8080 in normal CI when another process may already use it. For unit tests, validate translation. For runtime tests, use a test-only override or ephemeral bind path.
6. Make manifest status for no-arg default honest until runtime-tested.

### Acceptance Criteria

- `pproxy` with no args starts or translates to the default mixed listener behavior.
- The old test expecting no-args error is removed or inverted.
- Default behavior is covered by parser, translator, and at least one runtime-level test.

## Workstream 6: Strengthen pproxy Binary Tests

### Problem

Some new tests accept weak outcomes such as `stderr contains listen OR error`, which can hide broken startup behavior. For drop-in CLI work, tests should distinguish successful startup from expected diagnostics.

### Required Tasks

1. Replace broad assertions with outcome-specific assertions:
   - successful startup should show listener/remote banner and bind readiness;
   - unsupported features should produce structured warnings/errors and appropriate exit behavior;
   - runtime bind failures should be explicitly expected only when a test intentionally uses a contested port.
2. Use ephemeral ports in startup tests instead of fixed `:8080` wherever possible.
3. Add a small helper that waits until the process either binds or exits, then captures output.
4. Ensure tests do not depend on arbitrary `sleep(2000ms)` as proof of startup.
5. Verify `--help` and `--version` remain fast and side-effect free.
6. Verify all known pproxy flags are parsed as known flags rather than unknown flags.

### Acceptance Criteria

- pproxy binary tests fail if the service errors before startup in a scenario expected to run.
- unsupported-feature tests assert diagnostic codes/messages, not merely the presence of the word `error`.
- fixed ports are eliminated from normal tests unless the test intentionally validates default port translation only.

## Workstream 7: Reconcile Documentation and Release Claims

### Problem

Documentation still contains strong "final parity certification complete" language after the new branch has failing checks and an incomplete oracle. This undermines the purpose of the canonical manifest work.

### Required Tasks

1. Replace status wording in README and release docs with conservative language until certification is genuinely complete.
2. Suggested status:

   `Status: Track A parity hardening in progress. Strong common HTTP/SOCKS/TCP compatibility is implemented, but full pproxy parity and Python drop-in replacement certification remain gated on the canonical manifest, differential oracle, Python API, and CI passing.`

3. Move historical Phase 51 language into a clearly historical release note if it must remain.
4. Ensure README, `docs/parity/PPROXY_PARITY_REPORT.md`, `docs/PARITY_MATRIX.md`, and release notes agree on capability count and certification status.
5. Add a doc consistency test or script check that rejects active docs containing forbidden unqualified phrases before certification:
   - `final parity certification complete`;
   - `full pproxy parity` without qualifier;
   - `drop-in replacement` without supported surface qualifier.
6. Keep the plan documents as plans; they should not be interpreted as completed implementation evidence.

### Acceptance Criteria

- No active README/status page claims final/full parity while CI is failing or oracle is incomplete.
- Generated parity report and README use the same capability count.
- Historical docs are clearly marked historical if they contain older claims.

## Workstream 8: Manifest and Evidence Tightening

### Problem

The manifest was expanded from 109 to 139 capabilities, but expansion alone does not prove stronger parity. The manifest validator is a good foundation, but the content must be evidence-consistent.

### Required Tasks

1. Audit every newly split HTTP/SOCKS capability.
2. For each entry, verify:
   - actual parser support;
   - translator support;
   - config compiler support;
   - runtime support;
   - CLI support;
   - Python support if applicable;
   - evidence class;
   - test IDs that actually exist.
3. Demote any new split capability to `compatible_with_warning`, `native_equivalent`, or `unsupported` if evidence is not adequate.
4. Add a manifest validation rule that every test ID referenced in the manifest exists in a known registry:
   - Rust unit/integration test names where feasible;
   - oracle scenario IDs;
   - Python test names;
   - documented external/gated test IDs.
5. Add a validation rule that oracle scenario IDs cannot be cited as `differential` until the two-sided oracle runner is implemented.
6. Add a validation rule that `drop_in` protocol/routing capabilities require at least integration evidence, preferably differential evidence, unless explicitly exempted.
7. Re-run generated report after content correction.

### Acceptance Criteria

- Manifest references no nonexistent scenario/test IDs.
- No newly split capability overclaims `drop_in` based only on registry presence.
- `PPROXY_PARITY_REPORT.md` is regenerated from corrected manifest.
- `tests/compat/pproxy_manifest.toml` is either clearly legacy or generated/projection-only.

## Workstream 9: `fancy_regex` Compatibility Boundaries

### Problem

The `fancy_regex` backend now exists, but compatibility and safety boundaries need to be finished. Python `re` parity should not be overclaimed, and catastrophic backtracking/resource behavior must be bounded as much as practical.

### Required Tasks

1. Confirm `fancy_regex` supports the intended constructs used in tests:
   - lookahead;
   - lookbehind;
   - backreferences.
2. Add tests for known unsupported Python `re` features or semantic mismatches, and ensure diagnostics are explicit.
3. Add docs that state:
   - native egress rules use fast Rust `regex`/structured matching;
   - pproxy compatibility rules use dual backend with `fancy_regex` fallback;
   - this is broader than Rust `regex` but not exact Python `re` parity;
   - unsupported constructs produce diagnostics.
4. Evaluate match-time resource controls:
   - if no hard timeout is available, document the limitation;
   - keep pattern length and rule-count caps;
   - consider an optional future isolated evaluator for hostile regex sets.
5. Ensure `-b` and `--rulefile` use the compatibility compiler consistently.
6. Ensure rulefile query suffix handling, such as `?rules_file`, actually routes into the loader rather than only preserving the string.

### Acceptance Criteria

- `fancy_regex` path is used where pproxy compatibility requires it.
- Unsupported Python regex semantics are documented and tested.
- Rulefile and block-pattern diagnostics are stable and manifest-backed.
- No docs claim exact Python `re` compatibility.

## Workstream 10: Python Drop-in Status Alignment

### Problem

The broader roadmap includes Python library drop-in replacement for pproxy. Track A does not need to complete all of Track B, but it must stop Python status drift and prevent parser-only/runtime-only claims from leaking into Python feature reporting.

### Required Tasks

1. Audit Python `eggress.pproxy` feature reporting after manifest changes.
2. Ensure feature lists do not call parser-only or protocol-crate-only features supported.
3. If a structured feature report exists, align it with the canonical manifest layers.
4. If only a flat list exists, restrict it to runtime-supported Python-visible features or rename it to avoid implying support.
5. Ensure Python package docs distinguish:
   - native `eggress` package;
   - compatibility `eggress.pproxy` helpers;
   - future top-level `pproxy` shim / console script;
   - actual pproxy API drop-in status.
6. Add Python tests verifying:
   - feature reporting excludes unsupported WS/WSS/raw/H2 if runtime-refused;
   - Trojan status matches manifest;
   - `fancy_regex`/rulefile support is reported accurately if exposed.

### Acceptance Criteria

- Python feature reporting agrees with the canonical manifest.
- No top-level `pproxy` import/package claim is made unless implemented.
- Python docs do not imply full drop-in library parity before Track B completion.

## Workstream 11: Shadowsocks and Interop Failure Handling

### Problem

Shadowsocks Interop failed in the inspected checks. Existing docs already note some Shadowsocks TCP interop limitations. Track A must distinguish expected/gated known limitations from new regressions.

### Required Tasks

1. Inspect Shadowsocks Interop failure logs.
2. Classify failures as one of:
   - expected known limitation already documented;
   - new regression introduced by Track A;
   - environment dependency issue;
   - stale CI expectation.
3. If expected, adjust the CI gating or test expectation so known failures are explicitly ignored/skipped and documented.
4. If regression, fix before merge.
5. Ensure manifest entries for Shadowsocks do not overclaim pproxy-compatible behavior where only standard AEAD interop exists.

### Acceptance Criteria

- Shadowsocks Interop job is green or skipped for explicitly documented reasons.
- No failing known-limitation test remains in required CI.
- Manifest tier/evidence reflects actual interop status.

## Workstream 12: PR Hygiene and Merge Path

### Required Tasks

1. Keep the corrective work on `plans/full-pproxy-parity-track-a` unless a fresh branch is needed.
2. Update PR #1 body after corrective implementation to reflect that it is no longer only a plan PR; it now contains Track A implementation plus corrective closure.
3. Consider splitting if the PR becomes too large:
   - plan docs only;
   - canonical manifest + docs correction;
   - regex compatibility;
   - pproxy binary;
   - oracle harness.
4. If keeping one PR, ensure commit messages identify Track A workstream numbers.
5. Add a final PR comment with:
   - checks run;
   - known gaps still deferred;
   - exact compatibility language to use after merge.

### Acceptance Criteria

- PR title/body accurately describe implementation, not just plan files.
- Required checks are green or intentionally non-required/gated.
- The branch can be merged without misrepresenting parity status.

## Suggested Execution Order

1. Fix compile/clippy/test failures first.
2. Correct docs overclaiming immediately so every subsequent commit is honest.
3. Fix no-arg defaults and `--test` recursion.
4. Correct oracle config schema and add parse validation tests.
5. Make the oracle two-sided for a small subset of scenarios.
6. Tighten manifest evidence for newly split capabilities.
7. Expand two-sided oracle coverage only after the minimal harness is correct.
8. Align Python feature reporting.
9. Revisit Shadowsocks/interop gates.
10. Regenerate parity report and update PR body.

## Final Verification Checklist

Run these before marking the branch ready:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
python3.11 -m pytest python/tests
```

Then run gated checks where dependencies are available:

```bash
python3.11 -m pip install "pproxy==2.7.9"
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle -- --ignored
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored
```

If any gated check remains failing, document the failure in the manifest/report and do not cite it as passing evidence.

## Exit Criteria

Track A corrective closure is complete when:

- CI is green for required checks;
- `pproxy` binary no-arg/default behavior is compatible or explicitly classified with evidence;
- `pproxy --test` invokes the correct implementation path;
- oracle scenarios parse valid egress configs;
- the oracle harness compares both pproxy and egress for at least the core HTTP/SOCKS scenarios;
- docs no longer claim final/full parity ahead of evidence;
- manifest/report/test IDs are internally consistent;
- Python feature reporting does not overclaim unsupported runtime surfaces;
- PR #1 accurately describes the implementation and remaining deferred work.
