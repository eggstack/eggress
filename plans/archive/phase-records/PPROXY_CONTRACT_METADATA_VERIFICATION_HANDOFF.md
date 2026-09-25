# pproxy Contract Metadata and Verification Handoff

## Status

**IMPLEMENTED — OUTCOME 1 (WORKSPACE GATE PASSES)**

## Parent line of work

- [`PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`](PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md)
- [`PPROXY_FINAL_CONTRACT_REPORTING_CLOSURE_PASS.md`](PPROXY_FINAL_CONTRACT_REPORTING_CLOSURE_PASS.md)

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Review baseline: `f157110b87abb25c53bc6d86c36fe4172adc6c50`
- Compatibility reference: checked-in `pproxy==2.7.9` baseline under `compat/pproxy-2.7.9/`
- Scope: final contract metadata reconciliation plus a bounded test-only
  synchronization correction for the existing workspace-test race.
- Implementation commits: `cef6851cc7275aca3cb8f9e27cc3dc8b4f7abff6`,
  `cf448e7` (focused GET fail-closed evidence and closure records), and
  `eeaa411` (test-only UDP observability gauge synchronization).

## Final disposition

- `pac-serving` is `compatible_with_warning` across the canonical manifest,
  Rust tier/diagnostic mappings, and Python reporter. PAC is served through
  the mapped Eggress admin route.
- `verbose-mode` is `compatible_with_warning` across the same surfaces.
  `-v/-vv/-vvv` select Rust tracing defaults (`debug`/`trace`), with explicit
  `RUST_LOG` authoritative; runtime behavior was not changed.
- `cli.get` now records `config = complete`, `runtime = complete`, and
  `cli = complete`; valid `PATH,FILE` remains native admin static content and
  malformed or unreadable values remain fail-closed.
- Outcome 1 applies to the workspace gate. The named observability test was
  corrected without production/runtime changes: it now renders `/metrics`
  before reading the bridged `/-/udp` gauge and polls within a bounded
  readiness window. This removes the assertion race while preserving the
  behavior under test.

## Verification record

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test -p eggress-pproxy-compat`: passed, 329 tests.
- `cargo test -p eggress-cli --test pproxy_binary`: passed, 22 tests.
- `cargo test -p eggress-cli --test pproxy_run_process`: passed, 8 tests.
- Fresh `maturin develop` plus `pytest python/tests tests/compat -q`: passed,
  2215 passed, 114 skipped, 5 existing warnings.
- `cargo test -p eggress-runtime --test observability
  udp_active_gauges_return_to_zero_after_close`: passed in five consecutive
  focused runs after `eeaa411`.
- `cargo test --workspace --locked`: passed, including all workspace tests and
  doc tests; the previously hanging observability test completed successfully.

## Purpose

Close the remaining inconsistencies found after `bd10467` / `f157110b` without reopening proxy runtime, protocol parity, Python lifecycle architecture, binary-size work, or CI design.

The substantive pproxy compatibility fixes have landed. The remaining defects are narrow:

1. the canonical manifest and reporter disagree on the tier for `pac-serving`;
2. the canonical manifest and reporter disagree on the tier for `verbose-mode`;
3. the `cli.get` manifest row describes a working CLI/config/runtime path while still marking those layers `not_applicable`;
4. the previous closure plan was marked complete even though its literal `cargo test --workspace --locked` acceptance gate did not pass because of an apparently unrelated pre-existing runtime observability test failure.

This pass must resolve those contradictions and leave one truthful closure state. It is not another parity phase.

---

## Governing constraints

1. Do not add proxy protocols, transports, routing modes, schedulers, plugins, daemonization, connection pooling, system-proxy mutation, reverse features, or UDP features.
2. Do not redesign the compatibility reporter, capability manifest, CLI hierarchy, Python API, or runtime architecture.
3. Do not create another manifest, evidence registry, certification framework, parity percentage, benchmark gate, or CI workflow.
4. Do not modify unrelated runtime behavior merely to obtain a green workspace count unless the allegedly unrelated failure is proven to be caused by this pproxy change set.
5. Treat current executable behavior and the checked-in `pproxy==2.7.9` baseline as authority. Documentation must follow behavior, not the reverse.
6. Prefer the more conservative supported tier when a user-visible semantic difference remains and evidence does not justify `native_equivalent`.
7. Keep the implementation delta small. Expected production changes are limited to tier/diagnostic mapping if needed; most changes should be manifest/tests/planning records.
8. Preserve current fail-closed behavior for unsupported compatibility inputs.
9. Preserve the current `--pac`, `--get`, `--test`, `-d`, `--sys`, `--reuse`, and `--auth` execution semantics unless a focused test proves a regression.
10. Do not expand ordinary hosted CI. Existing Rust/Python smoke boundaries remain sufficient.

---

# Confirmed residuals

## Residual A — `pac-serving` tier disagreement

At the review baseline:

- `docs/parity/pproxy_capability_manifest.toml` records `cli.pac` as `compatible_with_warning`;
- the same manifest row describes the emitted diagnostic as native-equivalent behavior;
- `crates/eggress-pproxy-compat/src/tier.rs` maps `pac-serving` to `native_equivalent`;
- `crates/eggress-pproxy-compat/src/diagnostics.rs` emits `native_equivalent` for `pac-serving`;
- `python/eggress/pproxy.py` also maps `pac-serving` to `native_equivalent`;
- the practical matrix uses the broader human label `supported_difference`.

This violates the stated rule that the canonical manifest and executable reporter agree.

### Required decision

Inspect the actual observable semantics of `--pac <path>` and choose exactly one machine tier.

Use these rules:

- choose `native_equivalent` only if the practical user outcome is equivalent despite the implementation mechanism;
- choose `compatible_with_warning` if the mapped admin PAC route has a meaningful user-visible semantic caveat compared with pproxy 2.7.9;
- do not use `drop_in`;
- do not reclassify it as unsupported.

Whichever tier is selected must be applied consistently to:

- canonical manifest row `cli.pac`;
- Rust `manifest_tier_for_category("pac-serving")`;
- Rust `StructuredDiagnostic::from()` for `pac-serving`;
- Python `_manifest_tier_for_diagnostic("pac-serving")`;
- focused tests;
- active human docs only where they expose the machine-tier name.

The practical matrix may retain `supported_difference` if that human vocabulary intentionally abstracts over the machine-tier distinction, but its notes must not contradict the selected semantics.

---

## Residual B — `verbose-mode` tier disagreement

At the review baseline:

- canonical manifest row `cli.verbose` is `compatible_with_warning`;
- Rust `tier.rs` maps `verbose-mode` to `native_equivalent`;
- Rust `diagnostics.rs` emits `native_equivalent`;
- Python reporter mapping emits `native_equivalent`.

### Required decision

Inspect what `-v/-vv/-vvv` actually does in both execution and `pproxy check` reporting.

Choose one tier using the same conservative rule:

- `native_equivalent` only if the practical logging outcome matches pproxy closely enough that the mechanism difference is the only material difference;
- `compatible_with_warning` if the user-visible logging/diagnostic contract differs materially or requires an explicit caveat.

Do not conflate `verbose-mode` with `debug-mode`; `-d` remains a separate `compatible_with_warning` capability because Python traceback semantics are not reproduced.

Apply the chosen `verbose-mode` tier consistently across:

- canonical manifest;
- Rust tier mapping;
- Rust structured diagnostic;
- Python reporter mapping;
- focused tests.

Do not change the runtime `default_log_level()` behavior merely to force a tier choice.

---

## Residual C — `cli.get` layer metadata is internally false

The current manifest correctly describes valid `--get PATH,FILE` as supported through Eggress admin static content, but the same row still says:

- `config = "not_applicable"`
- `runtime = "not_applicable"`
- `cli = "not_applicable"`

That contradicts the implemented behavior and the maintained practical matrix.

### Required correction

Inspect the existing capability-manifest layer vocabulary and use the same conventions as comparable working CLI features.

For `cli.get`, record the real state of:

- parser;
- translator;
- generated config;
- runtime serving behavior;
- CLI execution/reporting.

Expected direction, subject to the repository's existing vocabulary:

- parser: complete;
- translator: complete;
- config: complete if the generated static-content configuration is real and consumed;
- runtime: complete if the admin static-content path is actually served by runtime;
- cli: complete if the compatibility CLI accepts and executes the supported form;
- Python: leave `not_applicable` unless a separate Python-facing contract exists for this CLI utility.

Do not change runtime code merely because the manifest metadata is stale.

Add or retain focused evidence proving valid `PATH,FILE` produces usable static content and malformed/unreadable values remain fail-closed.

---

## Residual D — workspace verification gate (resolved)

`PPROXY_FINAL_CONTRACT_REPORTING_CLOSURE_PASS.md` requires:

```text
cargo test --workspace --locked
```

to pass before closure.

The initial metadata verification reproduced a failure in:

```text
eggress-runtime::observability::udp_active_gauges_return_to_zero_after_close
```

The failure was an assertion race: `/-/udp` read the metrics gauge before the
`/metrics` bridge synchronized it. The test-only correction in `eeaa411` now
renders `/metrics` and polls within a bounded readiness window. The full
workspace gate passes, including this test.

This was a closure-policy inconsistency even though the pproxy implementation
itself was correct.

### Required disposition

Do not automatically fix the runtime test.

First prove whether the failure is actually outside this line of work.

Use the smallest useful checks:

1. run the exact failing test on current `main`;
2. inspect the failing test and immediately relevant runtime code;
3. determine whether any file touched by `bd10467`/`f157110b` can plausibly affect the failure;
4. if practical, reproduce the same failure on the pre-pass baseline `f6336674` or otherwise establish from unchanged code/history that the failure predates this pass;
5. rerun the pproxy-owned focused suites to ensure they remain green.

Do not create a new runtime hardening project from this plan.

### Allowed closure outcomes

Exactly one of these outcomes must be recorded.

#### Outcome 1 — workspace gate passes

If the test no longer fails or a harmless test-only correction is clearly appropriate and within scope:

- `cargo test --workspace --locked` passes;
- record the exact result;
- retain the literal workspace-pass acceptance criterion.

#### Outcome 2 — proven unrelated baseline blocker

If the failure is reproducibly pre-existing and unrelated to pproxy compatibility:

- do not modify unrelated runtime behavior merely for closure optics;
- change the final closure record so it does **not** claim the full workspace gate passed;
- explicitly state the exact failing test and that it reproduces outside this pproxy change set;
- state that all compatibility-owned focused suites, formatting, and Clippy gates pass;
- revise the closure acceptance language so the pproxy line may close with a named external blocker only when the blocker is proven pre-existing and unaffected by the pproxy changes.

A named, evidenced external blocker is acceptable. Pretending the literal acceptance gate passed is not.

#### Outcome 3 — failure is caused by this line

If inspection or bisect-level evidence shows the failure was introduced by the pproxy changes:

- fix only the direct regression;
- run the workspace gate again;
- do not broaden the fix beyond the causal change.

---

# Workstream 1 — Establish final machine-tier decisions

Before editing implementation, write down the final decisions for only these warning categories:

| Warning category | Capability | Required final state |
|---|---|---|
| `pac-serving` | `cli.pac` | one tier shared by manifest, Rust reporter, Python reporter |
| `verbose-mode` | `cli.verbose` | one tier shared by manifest, Rust reporter, Python reporter |
| `debug-mode` | `cli.debug` | remains `compatible_with_warning` unless executable behavior has changed |
| `get-static-content` | `cli.get` | remains supported and internally consistent |
| `test-mode` | `cli.test` | remains supported and internally consistent |

Do not reopen unrelated warning categories.

### Workstream 1 acceptance

- one explicit final tier is chosen for PAC;
- one explicit final tier is chosen for verbose mode;
- choices are justified by current behavior rather than old prose;
- no new tier vocabulary is introduced;
- `debug-mode`, `get-static-content`, and `test-mode` are not regressed.

---

# Workstream 2 — Reconcile Rust reporter mappings

Likely files:

- `crates/eggress-pproxy-compat/src/tier.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`

Change only if Workstream 1 chooses a tier different from the current Rust mapping.

### Required invariants

For every touched category:

```text
manifest_tier_for_category(category).as_str()
== StructuredDiagnostic::from(warning).tier
```

Retain or extend the existing table-driven consistency test rather than creating a new framework.

At minimum the table must cover:

- `debug-mode`;
- `verbose-mode`;
- `pac-serving`;
- `get-static-content`;
- `test-mode`.

### Workstream 2 acceptance

- no touched category yields two different tiers inside `eggress pproxy check --json`;
- no runtime translation semantics change;
- no new diagnostic registry abstraction is added.

---

# Workstream 3 — Reconcile Python reporter mapping

Likely file:

- `python/eggress/pproxy.py`

The pure-Python `_manifest_tier_for_diagnostic()` mapping must mirror the final Rust decisions for the touched categories.

Do not change Python networking, lifecycle, proxy classes, or bindings.

### Required focused tests

Use existing Python compatibility/reporter tests where possible. Add the smallest table-driven assertion if needed proving:

- `pac-serving` -> selected tier;
- `verbose-mode` -> selected tier;
- `debug-mode` -> `compatible_with_warning`;
- `get-static-content` -> selected supported tier;
- `test-mode` -> selected supported tier.

### Workstream 3 acceptance

- Python and Rust report the same tier vocabulary for touched categories;
- no Python network behavior changes;
- no new compatibility mapping subsystem is introduced.

---

# Workstream 4 — Correct the canonical manifest

Primary file:

- `docs/parity/pproxy_capability_manifest.toml`

Required changes:

1. `cli.pac`
   - set `tier` to the Workstream 1 decision;
   - make `eggress_behavior`, `diagnostic`, `notes`, and evidence agree with that tier;
   - remove wording that calls the diagnostic native-equivalent if the selected tier is `compatible_with_warning`.

2. `cli.verbose`
   - set `tier` to the Workstream 1 decision;
   - make notes/evidence describe actual runtime/reporting behavior;
   - do not conflate `-v` with `-d`.

3. `cli.get`
   - correct `config`, `runtime`, and `cli` layer fields to reflect implemented static-content behavior;
   - retain valid-value supported behavior and invalid-value fail-closed behavior;
   - keep `get-static-content` as the diagnostic identifier unless current code has changed.

4. Adjacent rows
   - inspect `cli.debug` and `cli.test` only to ensure edits did not introduce contradictions;
   - do not perform another whole-manifest audit.

### Workstream 4 acceptance

- the manifest is internally self-consistent for PAC, verbose, GET, debug, and test;
- layer metadata describes actual code paths;
- every modified supported row points to executable evidence where practical;
- no unrelated capability is reclassified.

---

# Workstream 5 — Reconcile active human documentation

Inspect only active references that restate the affected classifications:

- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/PPROXY_PARITY_SPEC.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `docs/architecture/pproxy-compat.md`
- `AGENTS.md`
- `.skills/rust-proxy-dev/skill.md`

Only edit a file if it actually contradicts the final selected semantics.

The practical matrix may continue using its human-facing `supported_difference` status where appropriate; do not force machine tier names into the human matrix unless that is already its convention.

### Workstream 5 acceptance

- no active doc says PAC is `compatible_with_warning` while simultaneously describing its diagnostic as `native_equivalent`, or vice versa;
- no active doc contradicts the final verbose-mode tier;
- GET is described as an operational admin static-content path, not as non-applicable metadata;
- no aggregate parity percentage or strict 100% claim is introduced.

---

# Workstream 6 — Truthfully verify the workspace gate

This workstream is verification and recordkeeping first, not runtime development.

### Focused compatibility verification

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli --test pproxy_run_process
```

If Python mapping or Rust reporter APIs changed, build the extension fresh and run the existing focused Python compatibility suite:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

Do not add a new matrix.

### Workspace verification

Run:

```bash
cargo test --workspace --locked
```

If it fails only at the known observability test, execute the Residual D disposition procedure and record Outcome 1, 2, or 3.

### Workstream 6 acceptance

- formatting passes;
- Clippy passes;
- focused pproxy Rust suites pass;
- Python compatibility suite passes when affected by the mapping changes;
- workspace result is recorded exactly;
- an unrelated baseline failure is never described as a passing workspace run;
- unrelated runtime code is not modified without causal evidence.

---

# Workstream 7 — Close planning records without new ceremony

Primary records:

- `plans/PPROXY_FINAL_CONTRACT_REPORTING_CLOSURE_PASS.md`
- `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`
- this file

### Required record updates when implementation lands

1. Mark this plan `IMPLEMENTED` only after all handoff criteria below are satisfied.
2. Record the exact implementation commit SHA(s).
3. Record the selected final PAC and verbose tiers.
4. Record the corrected `cli.get` layer metadata.
5. Record the exact workspace verification result and Outcome 1, 2, or 3.
6. If Outcome 2 applies, explicitly state that closure is scoped to the pproxy compatibility line and that the named runtime test remains an external/pre-existing repository issue.
7. Remove or amend any prior sentence that says the literal workspace gate passed when it did not; record Outcome 1 when the gate passes.
8. Do not create another closure report or evidence file.

---

# Explicit handoff criteria

The implementation model may hand this work back as **complete** only when every item below is satisfied.

## A. Scope discipline

- [x] No new proxy/runtime feature was added.
- [x] No protocol, Python lifecycle, binary-size, release, or CI scope was opened.
- [x] No unrelated runtime file was changed unless direct causal evidence required it.
- [x] No new manifest, registry, workflow, or evidence framework was created.

## B. PAC contract

- [x] One final machine tier is chosen for `pac-serving`.
- [x] Canonical manifest `cli.pac` uses that tier.
- [x] Rust `tier.rs` uses that tier.
- [x] Rust `StructuredDiagnostic` uses that tier.
- [x] Python reporter mapping uses that tier.
- [x] Focused tests assert the shared tier.
- [x] Human docs do not contradict the final decision.

## C. Verbose contract

- [x] One final machine tier is chosen for `verbose-mode`.
- [x] Canonical manifest `cli.verbose` uses that tier.
- [x] Rust tier/diagnostic mappings use that tier.
- [x] Python reporter mapping uses that tier.
- [x] `debug-mode` remains independently `compatible_with_warning`.
- [x] Runtime `-v/-vv/-vvv` behavior is unchanged unless a focused test proved a bug.

## D. GET metadata

- [x] `cli.get` no longer marks operational config/runtime/CLI layers `not_applicable` when they are actually exercised.
- [x] Valid `PATH,FILE` remains supported through admin static content.
- [x] Invalid or unreadable `PATH,FILE` remains fail-closed.
- [x] Manifest evidence points to focused executable tests.
- [x] Practical matrix and manifest no longer contradict one another about whether the path is operational.

## E. Reporter consistency

- [x] `manifest_tier_for_category()` and `StructuredDiagnostic::from()` agree for `debug-mode`, `verbose-mode`, `pac-serving`, `get-static-content`, and `test-mode`.
- [x] Python reporter mapping agrees with Rust for those categories.
- [x] `eggress pproxy check --json` cannot emit contradictory feature/diagnostic tiers for the touched categories.
- [x] No new reporter abstraction was introduced.

## F. Verification

- [x] `cargo fmt --all -- --check` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] `cargo test -p eggress-pproxy-compat` passes.
- [x] `cargo test -p eggress-cli --test pproxy_binary` passes.
- [x] `cargo test -p eggress-cli --test pproxy_run_process` passes.
- [x] Relevant Python compatibility tests pass against a fresh extension if Rust/Python reporter mappings changed.
- [x] `cargo test --workspace --locked` is attempted and its exact result is recorded.
- [x] The workspace run is green and Outcome 1 is recorded.
- [x] No response or planning record claims a green workspace suite without a
  completed green run.

## G. Closure records

- [x] This file is marked `IMPLEMENTED` and records implementation SHA(s).
- [x] The final contract/reporting plan is corrected to reflect the actual workspace result.
- [x] The parent roadmap records this final follow-up and its exact closure status.
- [x] If an unrelated external blocker remains, it is named once and not converted into a new pproxy phase.
- [x] No additional follow-up plan is created for wording-only cleanup.

---

# Expected touchpoints

Likely production/test files:

- `crates/eggress-pproxy-compat/src/tier.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- existing focused tests in `crates/eggress-pproxy-compat`
- `python/eggress/pproxy.py`
- existing Python pproxy reporter/contract tests

Likely contract/docs files:

- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` only if wording is contradictory
- directly affected active pproxy docs only if needed
- `plans/PPROXY_FINAL_CONTRACT_REPORTING_CLOSURE_PASS.md`
- `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`
- this file

Expected non-touchpoints:

- protocol crates;
- reverse/UDP implementation;
- server/runtime internals, unless Residual D is proven causal;
- Cargo feature topology;
- GitHub workflow files;
- release configuration.

---

# Handoff instructions for a smaller implementation model

1. Start from `f157110b87abb25c53bc6d86c36fe4172adc6c50` or newer `main` and re-check the four residuals before editing.
2. Do not infer that PAC or verbose must use one particular tier solely from old docs. Inspect behavior and choose one tier, conservatively.
3. Make the Rust/Python/manifest tier decision once, then propagate it; do not independently choose tiers in each layer.
4. Treat the `cli.get` issue as metadata unless executable tests fail.
5. Do not touch `-d`, `--test`, `--sys`, `--reuse`, or networking behavior unless a focused regression appears.
6. For a workspace blocker, prove causality before changing runtime code. A
   harmless test-only synchronization correction is acceptable when it removes
   a confirmed test race without changing runtime behavior.
7. Prefer adding one table row to existing consistency tests over creating new test infrastructure.
8. Run focused checks first. Run the broad workspace gate once after the narrow changes settle.
9. Update planning records last, from the actual observed results.
10. Hand back only when every checked handoff criterion is satisfied and the
    workspace result is recorded under the applicable outcome.

## Terminal condition

After this pass, do not continue the pproxy corrective line for metadata or wording polish. A future plan is justified only by a newly demonstrated functional regression or a new product requirement outside this completed scope.
