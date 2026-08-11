# pproxy Final Closure Corrective Pass

## Status

**PLANNED**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Review baseline: `cb64363c8f8d565ca678b772eec7fe63c8913432`
- Compatibility target: `pproxy==2.7.9`
- Parent closure roadmap: [`PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md`](PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md)

## Purpose

Correct the small set of contract, help-text, verification-record, and cross-entry-point inconsistencies found after the final pproxy closure implementation landed.

The underlying compatibility/runtime work is substantially complete. This pass must **not** reopen broad parity work. It exists only to make the live contract and closure evidence accurately describe the code that already landed, add the two missing focused regex/resource-bound tests, record the Phase 3 artifact measurement that the completed plan required, and remove one unnecessary exit-code divergence between the two pproxy execution entry points.

After this pass, bounded `pproxy==2.7.9` compatibility should return to the existing closed state. No follow-on roadmap should be created unless a reproducible user-visible defect, supported-path correctness/security defect, explicit scope expansion, or new upstream target warrants reopening.

## Confirmed findings

### 1. `cli.test` canonical manifest text is stale after Phase 3

`docs/parity/pproxy_capability_manifest.toml` still describes `--test` as delegating to:

```text
eggress upstream test -c <config> -t <target>
```

Phase 3 deliberately removed that subprocess/config-file composition. Both pproxy compatibility execution paths now compile translated TOML in memory and call shared Rust functionality through `eggress_cli::run_upstream_test(...)` / `run_upstream_test_with_mode(...)`.

The behavior/tier need not change solely because the implementation became in-process. The canonical text and evidence references must describe the current execution architecture.

### 2. `cli.sys` canonical manifest text advertises a native CLI command that does not exist

The canonical manifest currently states that native `eggress system-proxy inspect` **and `apply`** subcommands remain available and refers to `apply --dry-run` / an explicit `--apply` path.

At the review baseline, the actual native CLI exposes `eggress system-proxy inspect`. The `eggress-system-proxy` crate contains planning/application primitives, but that is not equivalent to a public `eggress system-proxy apply` command.

The live compatibility contract must distinguish crate/library capability from user-visible CLI capability. Do not add an `apply` CLI command merely to make the stale documentation true.

### 3. Standalone `pproxy --help` overstates `-d` and `--log`

The standalone compatibility help currently says, in substance:

```text
-d       Debug/traceback diagnostics (native equivalent)
--log    Log file path (native equivalent: stderr)
```

The canonical contract correctly classifies both as `compatible_with_warning`:

- `-d` selects Eggress tracing diagnostics; it does not reproduce Python traceback behavior.
- `--log <PATH>` is recognized, but Eggress does not write that requested path; logs remain on stderr unless the caller redirects/configures output externally.

Help text must use the same behavioral vocabulary as the active contract. Do not add a logging subsystem in this pass.

### 4. Phase 3 was marked complete without the artifact-size measurement required by its own acceptance criteria

`PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md` correctly records the architectural simplification and production dependency cleanup, but its implementation summary does not record the requested before/after artifact measurements.

This is a closure-evidence gap, not a reason to reopen binary-size optimization. Perform one informational measurement using the same host, target, Rust toolchain, profile, and build settings for both revisions where feasible. Record the result in the existing Phase 3 plan and optionally the parent roadmap closure record. Do not introduce an artifact-size CI gate or a new report file.

### 5. Phase 4 overstates regex-limit verification and lacks an explicit 10,000-entry overflow test

Phase 4 switched `fancy_regex::Regex::new()` to `RegexBuilder::new(...).build()` and states that the backend's built-in backtracking limit is enabled. However:

- the code does not explicitly set a limit value;
- the test named `fancy_regex_backtrack_limit_applied` only proves that an ordinary fancy-regex pattern successfully matches and does not prove limit exhaustion/fail-closed behavior;
- the Phase 4 acceptance criteria called for explicit rule-entry overflow coverage, but the reviewed test set does not contain a focused 10,001-entry test even though the production code enforces `MAX_RULE_ENTRIES = 10_000`.

The pass should either explicitly configure the locked dependency's supported backtracking limit with a tiny local change or accurately document reliance on the locked dependency default. In either case, add a deterministic limit-exhaustion test if the public API/locked crate behavior permits one without a slow/flaky stress test. Add a direct rule-entry overflow test.

### 6. Closure summaries conflate upstream observations with Eggress fail-closed behavior

The Phase 5 verification summary currently says, in substance, that focused pproxy 2.7.9 source/oracle evidence verified:

- `DUMMY` callable identity;
- `UDP_LIMIT = 30`;
- non-`None` `prepare_ciphers` raises;
- plugin-bearing URI raises.

Only the first two are upstream matches. In pproxy 2.7.9, non-`None` `prepare_ciphers` performs real cipher/plugin wrapping, and plugin-bearing URIs participate in plugin behavior. Eggress intentionally raises `UnsupportedPProxyFeature` for these excluded internals.

Later text in the same plan correctly describes this distinction, so this is a closure-summary wording defect rather than an implementation defect. Reconcile the summary/roadmap so upstream-observed behavior and Eggress fail-closed behavior are not presented as the same oracle result.

### 7. The two compatibility entry points use different unsupported-feature exit codes

The shared execution gate is correct, but the wrappers diverge after the gate:

- standalone `pproxy`: unsupported feature -> exit `5` (`EXIT_UNSUPPORTED_FEATURE` semantics);
- `eggress pproxy run`: unsupported feature -> exit `3` (`EXIT_CONFIG_VALIDATION`).

Unknown flags already use exit `2` in both paths. The native CLI already defines `EXIT_UNSUPPORTED_FEATURE = 5` under `pproxy-compat`, so exact unsupported-exit unification appears to be a one-line policy correction rather than architectural work.

Unless discovery finds a documented public reason to preserve exit `3` for `eggress pproxy run`, make both compatibility execution entry points return exit `5` for shared-gate unsupported-feature failures. Preserve exit `3` for actual translated-config validation failures.

## Scope constraints

1. Do not add SSH, QUIC/H3, SSR, legacy Shadowsocks ciphers/OTA, plugin execution, daemonization, per-client auth reuse, implicit system-proxy mutation, general multi-hop UDP, or other excluded parity scope.
2. Do not change supported proxy wire behavior unless a focused test demonstrates a regression caused by this corrective pass.
3. Do not add a new compatibility registry, generated report, certification artifact, dashboard, or parity percentage.
4. Do not add a new CI workflow, matrix, benchmark gate, size gate, fuzz gate, oracle gate, or regex stress job.
5. Do not add a native `system-proxy apply` CLI command merely to satisfy stale documentation.
6. Do not add a new logging/file-output subsystem for `--log`.
7. Do not replace `fancy_regex`, create a regex worker/sandbox, add thread-abandonment timeout machinery, or otherwise broaden the Phase 4 threat model.
8. Keep `tempfile` out of the production `egress-cli` dependency tree; test-only use remains acceptable.
9. Historical documents need not be globally synchronized. Change only active contract/help text and the already-created closure plans whose summaries are factually wrong.
10. Complete this work in one implementation commit or one tightly related commit series. Do not create another planning phase after this file.

## Required discovery before editing

Before changing behavior or contract text, confirm the following at current `main`:

1. `cli.test` current execution path in both `crates/eggress-cli/src/pproxy_main.rs` and `crates/eggress-cli/src/main.rs`.
2. Exact native `SystemProxyAction` variants exposed by the CLI and the distinction between crate-level apply planning and public CLI commands.
3. Current `HELP_TEXT` strings for `-d` and `--log`.
4. Pre-Phase-3 revision to use for artifact comparison. The expected immediately pre-refactor candidate is `3c1f12721deb2f25832c81a0303b8e7a6230d37a`; verify ancestry before using it.
5. Locked `fancy-regex` version from `Cargo.lock`, its `RegexBuilder` API, and whether its default/effective backtrack limit is documented/stable enough to rely upon.
6. Whether the crate exposes a supported way to set the backtrack limit explicitly. Prefer explicit configuration when it is a tiny local change and preserves current compatibility behavior.
7. A deterministic upstream/crate test pattern for triggering backtrack-limit exhaustion without a long-running stress test. Prefer adapting the dependency's own test case or documented example rather than inventing a brittle pathological pattern.
8. Existing process tests for unsupported exit codes across standalone `pproxy` and `eggress pproxy run`.

If any of these discoveries contradict the findings above, record the actual state in this plan's implementation summary and make the smallest correction consistent with observable behavior.

## Workstream A — Reconcile the canonical `cli.test` contract

Update `docs/parity/pproxy_capability_manifest.toml` for `cli.test` so it describes the actual post-Phase-3 path.

Expected substance:

```text
The target remains owned by --test. Compatibility execution compiles the
translated configuration in process and calls the shared Rust upstream-test
implementation. The proxy service is not started for test mode.
```

Do not describe a temporary config path or sibling executable.

Review the corresponding practical-matrix row and active architecture documentation. Change them only if they still imply subprocess execution.

### Tests

Add or tighten one small canonical-manifest assertion that prevents the active `cli.test` entry from reintroducing known stale subprocess wording. Prefer a semantic assertion over a large snapshot, for example verifying that the behavior/notes mention shared/in-process/native testing and do not claim delegation through `-c <config>` / a sibling command.

Do not create a generated docs test suite.

## Workstream B — Correct `cli.sys` public-surface claims

Update the canonical `cli.sys` entry so it says exactly what users can invoke today.

Expected outcome at the review baseline:

- pproxy compatibility `--sys` remains unsupported/fatal before startup;
- native `eggress system-proxy inspect` is the available read-only user-facing alternative;
- crate-level system-proxy apply/planning primitives must not be advertised as a CLI subcommand unless an actual CLI action exists at implementation time.

Search the maintained practical matrix, active architecture docs, and pproxy standalone help for the same stale `apply` claim. Correct active user-facing occurrences only.

Do not add an apply command in this pass.

### Tests

Add one targeted contract test proving the canonical `cli.sys` text does not advertise a nonexistent native apply subcommand. Avoid assertions against every historical document.

## Workstream C — Align standalone compatibility help with the canonical tiers

Edit `crates/eggress-cli/src/pproxy_main.rs::HELP_TEXT`.

Required semantics:

- `-d` must not say `native equivalent`. It should state that it enables Eggress debug diagnostics/tracing and differs from Python traceback semantics, or use a concise `compatible with warning` phrasing.
- `--log <PATH>` must not call stderr a native equivalent of writing a file. It should state that the path is recognized for compatibility but file output is not reproduced and logs remain on stderr, or similarly concise language.

Keep help short. The canonical manifest remains the detailed explanation.

### Tests

Update `pproxy_binary` help tests so they protect the corrected semantics, not merely flag presence. Minimum assertions:

- help includes `-d` and does not pair it with `native equivalent`/equivalent traceback wording;
- help includes `--log` and clearly indicates stderr/no-file-output behavior;
- help remains successful and existing option inventory stays intact.

## Workstream D — Close Phase 3 artifact measurement honestly

Perform one informational before/after measurement.

### Preferred comparison

Verify that `3c1f12721deb2f25832c81a0303b8e7a6230d37a` is the immediate pre-Phase-3 code baseline. Build that revision and current implementation revision under the same environment.

Use an isolated worktree or equivalent so results are not contaminated by incremental artifacts:

```bash
rustc --version
cargo --version
rustc -vV

# Conceptual layout; use safe local paths appropriate to the environment.
git worktree add /tmp/eggress-pre-phase3 3c1f12721deb2f25832c81a0303b8e7a6230d37a

CARGO_TARGET_DIR=/tmp/eggress-size-pre \
  cargo build -p eggress-cli --release --locked

CARGO_TARGET_DIR=/tmp/eggress-size-post \
  cargo build -p eggress-cli --release --locked
```

Measure at minimum:

```text
release/eggress
release/pproxy
```

Record exact bytes where practical (`stat`) in addition to human-readable sizes.

Also record:

```bash
cargo tree -p eggress-cli -i tempfile -e normal
```

at current head, confirming `tempfile` is absent from the production dependency path.

### If the historical revision cannot build

Do not spend substantial engineering effort making old code compile. Instead:

1. record the concrete incompatibility/toolchain/environment reason;
2. record current `eggress`/`pproxy` artifact sizes using the current standard release profile;
3. record the production dependency-tree result showing `tempfile` removal;
4. change the Phase 3 closure wording from “criterion met” to an explicit informational-measurement exception rather than inventing a before/after number.

### Documentation updates

Update `PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md` in place with:

- toolchain/target;
- baseline/current revisions used;
- exact measured sizes or the concrete measurement limitation;
- dependency-tree observation;
- explicit statement that no binary-size threshold or CI gate was added.

If the parent roadmap says all criteria were met, make that statement consistent with the evidence actually obtained.

Do not create a separate size report.

## Workstream E — Make the regex/resource-bound evidence real

### E1. Verify the locked dependency behavior

Read the locked `fancy-regex` version and its builder implementation/docs.

Determine:

- whether `RegexBuilder::new(...).build()` uses a finite default backtrack limit;
- the exact default for the locked version, if documented;
- whether `.backtrack_limit(...)` or an equivalent supported API exists.

Do not rely on memory or a plan comment for the value.

### E2. Prefer explicit configuration if low-complexity

If the locked API provides a simple supported setter, define one compatibility constant near the existing resource bounds, e.g. conceptually:

```rust
const MAX_FANCY_BACKTRACKS: usize = <verified value>;
```

and construct fancy regexes using that explicit value in both fallback compilation and `compile_fancy()`.

The exact value should normally preserve the dependency's current effective default rather than arbitrarily tightening compatibility. The purpose is to make the bound explicit/stable and testable, not to redesign regex policy.

If no clean supported setter exists, leave runtime behavior unchanged and correct Phase 4 documentation to state that Eggress relies on the locked dependency's finite default behavior, with the exact limitation documented. Do not add custom cancellation machinery.

### E3. Add a deterministic exhaustion test

If the crate exposes deterministic backtrack-limit failure behavior, add a focused test that:

1. compiles through the fancy backend;
2. uses a dependency-documented/upstream-tested pathological case that exceeds the configured/effective limit quickly;
3. proves `CompatRegex::is_match` returns `Err` rather than hanging or silently returning a result;
4. verifies the error classification/message is recognizably a backtrack-limit failure without depending on unstable full-string formatting.

The test must complete quickly in debug CI. Do not use enormous inputs or wall-clock timing assertions.

If a deterministic fast exhaustion case cannot be expressed through the public API without brittle dependency-internal assumptions, do **not** add a slow stress test. Instead rename/remove the misleading existing test and replace it with a test that accurately proves only what can be guaranteed (for example, explicit builder configuration if introspectable plus ordinary fancy matching). Record the limitation in Phase 4.

### E4. Add the missing rule-count overflow test

Construct a temporary rule file with exactly `MAX_RULE_ENTRIES + 1` valid, cheap patterns.

Verify:

- exactly 10,000 entries are accepted;
- loading stops at the configured maximum;
- an error-level diagnostic is present for the overflow line;
- the diagnostic mentions the maximum/excess condition;
- the test does not create a permanent fixture or large checked-in data file.

If writing 10,001 lines makes the unit test materially slow, refactor only the smallest internal helper needed to test the counter boundary cheaply. Do not create a new parser abstraction solely for this test.

### E5. Correct Phase 4 closure wording

Update `PPROXY_FINAL_PHASE_4_REGEX_AND_VERIFICATION_BOUNDARY.md` so its implementation summary says exactly what is enforced and tested.

Do not claim a separately verified hard limit if the implementation only relies on an opaque dependency default.

## Workstream F — Correct Phase 5 and roadmap closure summaries

Update the existing closure documents in place; do not create another completion report.

### Required semantic distinction

The summary must separate:

**Upstream pproxy 2.7.9 facts:**

- `DUMMY` is a callable identity helper;
- `UDP_LIMIT == 30`;
- `prepare_ciphers(None, ...) -> (None, None)`;
- non-`None` `prepare_ciphers` performs actual upstream cipher/plugin wrapping;
- plugin metadata participates in upstream plugin behavior.

**Eggress bounded compatibility decisions:**

- `DUMMY` and `UDP_LIMIT` match upstream exactly;
- `prepare_ciphers(None, ...)` preserves the upstream sentinel;
- non-`None` `prepare_ciphers` fails explicitly with `UnsupportedPProxyFeature` because private cipher/plugin stream wrapping is outside the bounded compatibility target;
- plugin-bearing compatibility URIs fail explicitly rather than discarding metadata or pretending to execute plugins.

The closure summary must not state that upstream itself raises for these excluded operations.

### Files

At minimum inspect/update:

- `plans/PPROXY_FINAL_PHASE_5_DIFFERENTIAL_CLOSURE.md`;
- `plans/PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md`.

Update Phase 3/4 status summaries as required by Workstreams D/E. Do not rewrite the body of historical older plans.

## Workstream G — Unify unsupported-feature exit status across compatibility entry points

Inspect whether any public/current documentation intentionally distinguishes the exit status of:

```text
pproxy <args with unsupported feature>
eggress pproxy run -- <args with same unsupported feature>
```

If no deliberate reason exists, use the already-defined `EXIT_UNSUPPORTED_FEATURE` (`5`) in `handle_pproxy_run()` for shared-gate unsupported-feature failures.

Preserve:

- unknown option -> exit `2`;
- malformed/invalid translated config -> exit `3`;
- runtime failure -> exit `1`;
- unsupported compatibility request -> exit `5`.

Do not globally redesign CLI exit codes.

### Tests

Add/adjust process tests that invoke both entry points with the same representative unsupported option, preferably an existing excluded flag such as `--daemon` or `--auth`, and assert both return `5`.

Keep existing unknown-flag assertions at `2` if already explicit. Add an actual config-validation failure assertion at `3` only if there is already a stable cheap fixture/path; do not invent complex failure setup merely to test every constant.

## Workstream H — Bounded active-document search

Before final verification, perform a targeted search for the exact stale phrases corrected in this pass:

```text
native equivalent: stderr
Debug/traceback diagnostics (native equivalent)
eggress upstream test -c
system-proxy apply
apply --dry-run
prepare_ciphers non-None raises
```

Classify hits:

- active canonical/help/architecture document -> correct if false;
- implementation plan being updated by this pass -> correct summary if false;
- historical document explicitly bannered as historical -> leave alone unless its banner is insufficient and the stale line is likely to be mistaken for active guidance.

Stop after these known phrases. Do not conduct another repository-wide parity rewrite.

## Verification sequence

### Focused Rust

Run the directly affected tests first:

```bash
cargo test -p eggress-pproxy-compat regex
cargo test -p eggress-pproxy-compat rule
cargo test -p eggress-testkit canonical_manifest
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli --test pproxy_run_process
```

Use actual module/test filters if repository names differ.

### Canonical manifest

```bash
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
```

Use the script's actual supported invocation if it accepts no path argument.

### Broad Rust gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

### Python

This corrective pass should not require Python production changes. If Python files are not changed, the full Python suite is optional because the prior Phase 5 build already covered the Python semantic closure and this pass changes only Rust CLI/manifest/docs/tests.

If any Python compatibility code is touched unexpectedly, require a fresh extension build and:

```bash
.venv/bin/python -m pytest python/tests tests/compat -q
```

### Dependency/artifact evidence

Run the Phase 3 measurement from Workstream D. Do not turn it into a permanent gate.

## Acceptance criteria

This corrective pass is complete only when **all** of the following are true:

### Canonical contract

- `cli.test` no longer claims compatibility execution launches `eggress upstream test -c <config> -t <target>`; it accurately describes the current shared in-process Rust path.
- `cli.sys` no longer advertises a native `system-proxy apply` CLI command unless such a command actually exists at implementation time.
- the practical matrix and active architecture docs do not contradict the corrected `cli.test` / `cli.sys` manifest entries.
- canonical-manifest targeted tests protect both corrections without snapshotting unrelated documentation.

### Help text

- standalone `pproxy --help` does not label `-d` as a native-equivalent Python traceback implementation.
- standalone `pproxy --help` does not describe stderr as native-equivalent `--log <PATH>` file output.
- help still lists all current compatibility options and succeeds normally.

### Phase 3 evidence

- a before/after Phase 3 artifact comparison is recorded using the same toolchain/target/profile **or** a concrete historical-build limitation is recorded with current artifact sizes and dependency-tree evidence instead.
- `PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md` no longer claims an unrecorded measurement criterion was satisfied.
- current production `eggress-cli` dependency tree does not contain `tempfile` through normal dependencies.
- no artifact-size CI gate or new report file is added.

### Regex/resource bounds

- the effective fancy-regex backtracking bound for the locked dependency is verified from the actual dependency/API rather than assumed from prior prose.
- if a simple supported setter exists, the compatibility code explicitly sets a stable limit that preserves current practical behavior; otherwise the retained default reliance is documented accurately.
- the misleading `fancy_regex_backtrack_limit_applied` test is replaced/tightened so its name and assertion prove the behavior it claims.
- a deterministic backtrack-limit exhaustion test exists if the locked public API supports one without brittle/slow stress behavior; otherwise the plan records why such a test is not a reliable unit-test contract.
- a focused `MAX_RULE_ENTRIES + 1` test proves the 10,000-rule cap and overflow diagnostic.
- no regex sandbox, worker process, timeout thread scheme, alternate engine, or new subsystem is introduced.

### Closure records

- Phase 5/roadmap summaries clearly distinguish upstream pproxy behavior from Eggress's intentionally unsupported/fail-closed cipher/plugin behavior.
- no closure record states that pproxy 2.7.9 itself raises on non-`None` `prepare_ciphers` or plugin-bearing URI solely because Eggress does.
- Phase 3 and Phase 4 closure summaries accurately reflect the final measurement/test evidence after this pass.
- no new certification report, evidence bundle, registry, or parity percentage is created.

### Exit semantics

- absent a documented compatibility reason to preserve divergence, standalone `pproxy` and `eggress pproxy run` both return exit `5` for the same shared-gate unsupported-feature request.
- unknown-option behavior remains exit `2`.
- actual config-validation failures remain distinct from unsupported-feature failures.
- process tests protect the unified unsupported exit status.

### Regression and scope control

- focused affected tests pass.
- canonical manifest validation passes.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace --locked` passes.
- no supported HTTP/SOCKS/UDP/TLS/Shadowsocks/Trojan/chaining behavior is intentionally changed.
- no excluded pproxy protocol/plugin/process scope is implemented.
- hosted CI topology remains unchanged.
- this plan is updated in place to `IMPLEMENTED` with implementation commit SHA(s), exact verification performed, measured artifact evidence/limitation, regex-limit decision, and any deliberately retained discrepancy.

## Implementation-order recommendation

Use this order to minimize rework:

1. Correct canonical `cli.test` and `cli.sys` text plus their targeted contract tests.
2. Correct standalone help wording and help tests.
3. Unify unsupported exit status and process tests.
4. Verify/configure the fancy-regex limit and add the rule-count overflow test.
5. Perform the Phase 3 artifact measurement.
6. Reconcile Phase 3/4/5 and roadmap closure summaries with the evidence now actually present.
7. Run the focused gates, manifest validator, then the full Rust gate.
8. Update this plan to `IMPLEMENTED` and stop.

## Closure rule

When the acceptance criteria above are satisfied, do **not** create another pproxy parity/corrective plan merely for residual wording polish. The bounded `pproxy==2.7.9` line returns to closed status.

Reopen only for:

- a reproducible user-visible defect within the supported bounded compatibility claim;
- a correctness/security defect in a supported protocol/path;
- an explicit product-scope expansion decision;
- or a decision to target a different upstream pproxy version.
