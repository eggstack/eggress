# pproxy Final Contract and Reporting Closure Pass

## Status

**IMPLEMENTED — METADATA HANDOFF COMPLETE (OUTCOME 2)**

## Parent roadmap

[`PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`](PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md)

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Review baseline: `f6336674353aa63d868c8247ffafb9eb5a3eca4e`
- Compatibility reference: checked-in `pproxy==2.7.9` baseline under `compat/pproxy-2.7.9/`
- Scope: final contract/reporting consistency only, plus one bounded `--test` entry-point parity correction if confirmed by focused test.
- Implementation commit: `bd10467` (`fix: close pproxy contract reporting gaps`)
- Metadata reconciliation follow-up: `cef6851cc7275aca3cb8f9e27cc3dc8b4f7abff6`;
  verification/closure records: `cf448e7`.

## Purpose

Close the remaining pproxy compatibility line without reopening protocol parity, runtime architecture, Python lifecycle work, binary-size optimization, or CI design.

The post-closure corrective implementation at `f6336674` fixed the substantive runtime defects previously identified: `-d` now changes default diagnostics, `--sys` fails closed, both execution entry points share a blocker gate, Python compatibility/native lifecycle claims are separated, structural Python methods are classified, and the fresh Rust/Python verification suite was green.

A final review found a smaller defect class: **the machine-readable compatibility contract and reporter still contain stale or internally contradictory classifications for already-implemented CLI behavior.** There is also a narrow `--test` asymmetry between the standalone `pproxy` wrapper and `eggress pproxy run` that must be verified and, if confirmed, corrected.

This pass should be the terminal pass for this line of work.

The metadata follow-up selected `compatible_with_warning` for both
`pac-serving` and `verbose-mode`, corrected the operational `cli.get` layer
metadata to `complete` for config/runtime/CLI, and kept all execution
semantics unchanged. Its workspace gate is closed under Outcome 2: the sole
remaining failure is the pre-existing
`eggress-runtime::observability::udp_active_gauges_return_to_zero_after_close`
test, reproduced at both current `main` and pre-pass `f6336674`.

## Governing constraints

1. Do not add proxy protocols, transports, plugins, schedulers, routing modes, daemonization, implicit system-proxy mutation, connection pooling, or Python network engines.
2. Do not redesign the translator, reporter, capability manifest, or CLI command hierarchy.
3. Do not create a second manifest, evidence registry, compatibility database, schema generator, or documentation framework.
4. Do not add routine CI jobs, matrices, binary-size gates, oracle-install jobs, or release automation.
5. Treat `compat/pproxy-2.7.9/cli-baseline.json`, current parser behavior, current translator behavior, and focused executable tests as behavioral authority. Stale prose must not override executable behavior.
6. Prefer correcting classification/message data over changing runtime behavior when runtime behavior already matches the pinned upstream contract.
7. Any runtime code change must be justified by a focused failing test and limited to the specific confirmed mismatch.
8. Preserve the current default/full Cargo feature topology and current lean-profile decisions.
9. Preserve the current distinction between execution and inspection: `eggress pproxy check` reports only; it must not create temp config, mutate system state, or start services.
10. Update this plan in place when implementation lands. Do not create another closure plan unless a genuinely new functional defect class is discovered.

---

## Confirmed residuals

### 1. `--pac` machine manifest is stale

Pinned upstream CLI baseline:

- `--pac` is an option with a required `PAC` value (`http PAC path`).

Current Eggress parser:

- consumes exactly one value for `--pac`;
- stores it as `pac=<value>` in the compatibility option state.

Current translator:

- consumes `pac=<value>`;
- enables PAC serving;
- preserves/maps the supplied path into `pac_path`;
- emits a `pac-serving` warning.

Current canonical manifest still claims that Eggress recognizes `--pac` without consuming its required path and treats it as a boolean. That statement is false and contradicts current parser/translator behavior.

### 2. `--get` machine manifest and diagnostic semantics are stale

Pinned upstream CLI baseline describes `--get GETS` as `http custom {path,file}`. It is not a generic URL-fetch option.

Current Eggress parser consumes one `--get` value.

Current translator treats the value as `PATH,FILE`, validates an absolute safe path, reads the file, and adds static admin content. Invalid or unreadable values become unsupported blockers.

However:

- the manifest describes upstream `--get` as fetching URLs through the configured proxy;
- the manifest describes Eggress as unsupported and recommends `curl --proxy`;
- the translator warning category is named `get-url`, which no longer describes the implemented behavior;
- `manifest_tier_for_category("get-url")` currently returns `native_equivalent`;
- `StructuredDiagnostic::from()` currently emits an `unsupported` tier and a `curl --proxy` suggestion for the same warning category.

As a result, `eggress pproxy check --json` can report contradictory tier data for the same invocation.

### 3. `--test` machine manifest is stale

Pinned upstream CLI baseline:

- `--test` consumes a required URL/target value and tests it through remote proxies before exiting.

Current parser consumes exactly one value and stores `test=<value>`.

Current standalone `pproxy` execution path detects `test=<value>`, invokes the native upstream-test command, and passes the target with `-t <value>`.

The canonical manifest still says the value is not consumed and the translator treats `--test` like a boolean. That is false.

### 4. `eggress pproxy run -- --test <target>` appears to drop the target

At the review baseline, `handle_pproxy_run()` detects that a `test=<value>` option exists and invokes:

```text
eggress upstream test -c <generated-config>
```

but, unlike the standalone `pproxy` wrapper, it does not appear to append:

```text
-t <target>
```

This is a narrow behavioral mismatch between two compatibility execution entry points.

Do not assume this finding from source inspection alone. Add a focused process/command-construction test first. If the test proves the target is dropped, pass the exact parsed target through using the same bounded logic already present in the standalone wrapper.

Do not redesign upstream testing or subprocess execution.

### 5. Bare `-d` is not represented correctly by `pproxy check`

Runtime execution is correct after `f6336674`: `PproxyArgs::default_log_level()` uses `debug=true` to select a debug-level default tracing filter.

The machine manifest classifies `-d` as `compatible_with_warning`, reflecting that Eggress provides debug-level native diagnostics rather than Python traceback semantics.

However, translation currently emits `verbose-mode` only when `verbose_level > 0`; a bare `-d` does not emit a reporting diagnostic. Since `eggress pproxy check` derives aggregate tier/features from translation warnings and unsupported entries, a bare `-d` can be reported as `drop_in` even though the canonical contract says `compatible_with_warning`.

This is a reporter-contract defect, not a runtime logging defect.

### 6. Reporter tier mappings are not internally self-consistent

At least one exact mismatch is confirmed:

- `tier.rs`: `get-url` -> `native_equivalent`
- `diagnostics.rs`: `get-url` -> `unsupported`

Related rows such as `verbose-mode`, `pac-serving`, and `test-mode` must be checked against the canonical machine manifest during this pass. Do not assume they are correct merely because the Rust mapping functions agree with each other.

The objective is one coherent classification per behavior, not a new classification system.

### 7. Final implementation bookkeeping is incomplete

`PPROXY_POST_CLOSURE_CORRECTIVE_PASS.md` contains the final test result and implementation summary but does not explicitly record `f6336674353aa63d868c8247ffafb9eb5a3eca4e` as its implementation commit.

The parent roadmap refers readers to that plan for the implementation range. Record the exact commit so the closure chain is self-contained.

---

## Workstream A — Establish the final CLI semantic table

Before editing code or docs, write down the final intended semantics for only these options:

| Option | Upstream 2.7.9 | Current Eggress target behavior | Expected reporting class |
|---|---|---|---|
| `--pac <path>` | Serve/use supplied PAC path | Consume supplied path; map to Eggress PAC serving path | supported with an explicit implementation-difference tier if needed |
| `--get <path,file>` | Serve custom HTTP path from file | Consume `PATH,FILE`; validate/read file; serve as static admin content | supported with warning/different mechanism; invalid value is unsupported |
| `--test <target>` | Test target through configured remotes and exit | Consume target; invoke native upstream-test path with that exact target | native equivalent or compatible-with-warning based on existing observable tests |
| `-d` | Enable exception traceback/debug diagnostics | Select debug-level default tracing; no Python traceback equivalence | `compatible_with_warning` |

Use the checked-in upstream CLI baseline and current source/tests to settle wording. Do not run a broad differential project.

### Classification rule

Do not promote a row to `drop_in` simply because it works.

Use the existing tier vocabulary consistently:

- `drop_in`: same user-visible contract without material caveat;
- `compatible_with_warning`: usable behavior with a user-visible semantic caveat;
- `native_equivalent`: same practical outcome through a different native mechanism;
- `intentional_non_parity` / `unsupported`: behavior is intentionally unavailable.

If `--pac`, `--get`, or `--test` has a meaningful observable difference, prefer the conservative supported tier rather than overclaiming equivalence.

### Acceptance for Workstream A

- one explicit semantic decision exists for each of the four options;
- decisions are supported by baseline/source/tests;
- no unrelated capability is reclassified.

---

## Workstream B — Correct canonical manifest rows

Primary file:

- `docs/parity/pproxy_capability_manifest.toml`

Correct the existing rows in place. Do not add replacement rows.

### `cli.pac`

Required corrections:

- state that `--pac` consumes one required path;
- state that Eggress consumes and maps that supplied path;
- remove all claims that it is treated as a boolean or that the path is not consumed;
- point evidence to parser/translator tests that actually prove value ownership and path lowering;
- select the tier from Workstream A.

### `cli.get`

Required corrections:

- correct upstream behavior to the pinned baseline's `http custom {path,file}` meaning;
- describe Eggress `PATH,FILE` static-content behavior accurately;
- remove generic URL-fetch / `curl --proxy` wording unless there is a separate capability that genuinely represents URL fetching;
- reflect that valid values are supported and malformed/unreadable values fail closed;
- use focused test names instead of `docs_only` evidence if tests are added/available;
- select the tier from Workstream A.

### `cli.test`

Required corrections:

- state that the URL/target is consumed;
- state that standalone compatibility execution passes the target to native upstream testing;
- after Workstream D, state the same for `eggress pproxy run` if the source-inspection defect is confirmed/fixed;
- remove boolean/non-consuming wording;
- select the tier from Workstream A.

### `cli.debug`

Keep the post-closure runtime description, but ensure its tests and tier match the reporter behavior after Workstream E.

### Bounded adjacent audit

While editing the manifest, inspect only adjacent rows whose warning categories are touched by this pass (`verbose`, `pac`, `get`, `test`, `debug`). Correct contradictions found there, but do not perform another whole-manifest parity audit.

### Acceptance for Workstream B

- the machine manifest does not contradict current parser/translator behavior for PAC/get/test/debug;
- no row says a value is unconsumed when the parser consumes it;
- `cli.get` no longer describes the wrong upstream feature;
- all modified rows cite executable evidence where practical.

---

## Workstream C — Unify warning-category and structured-diagnostic tiers

Primary files:

- `crates/eggress-pproxy-compat/src/tier.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- `crates/eggress-pproxy-compat/src/translate.rs` if a warning category/message must be renamed

### Required invariant

For every warning category touched by this pass, the following must agree:

1. translator warning meaning;
2. `manifest_tier_for_category()` result;
3. `StructuredDiagnostic::from()` tier;
4. canonical manifest capability tier/description;
5. `eggress pproxy check` human/JSON output.

### `--get` category cleanup

The current category name `get-url` is semantically misleading because the implemented behavior is static `PATH,FILE` content.

Preferred bounded correction:

- rename the warning category to a truthful stable name such as `get-static-content` if no externally documented compatibility guarantee depends on `get-url`;
- map it to the selected supported tier from Workstream A;
- change the structured diagnostic message/suggestion to describe the native static-content mapping;
- remove the stale `curl --proxy` suggestion.

If changing the diagnostic category would break an explicitly documented stable identifier, retain `get-url` as a legacy identifier but correct its tier/message/suggestion and add a comment explaining the legacy name. Do not create aliases or a migration subsystem solely for this.

### Small consistency test

Add one table-driven unit test covering the warning categories touched by this pass. For each category, assert that the tier returned by `manifest_tier_for_category()` matches the tier emitted by `StructuredDiagnostic::from()`.

At minimum include:

- debug diagnostic category introduced/used in Workstream E;
- `verbose-mode`;
- `pac-serving`;
- get static-content category;
- `test-mode`.

Do not parse the TOML manifest from Rust unit tests solely to enforce documentation. The test should prevent internal reporter self-contradiction without coupling production code to docs files.

### Acceptance for Workstream C

- no touched warning category produces two different tiers inside `pproxy check --json`;
- `--get` no longer reports a supported static-content translation as unsupported merely because of stale diagnostic code;
- no new tier vocabulary or reporter subsystem is added.

---

## Workstream D — Align `--test` execution target across both entry points

Primary files:

- `crates/eggress-cli/src/pproxy_main.rs`
- `crates/eggress-cli/src/main.rs`
- `crates/eggress-cli/tests/pproxy_binary.rs`
- `crates/eggress-cli/tests/pproxy_run_process.rs`

### Test first

Add a focused test proving whether the target value from:

```text
--test https://example.invalid/health
```

is forwarded to the native upstream-test command by each execution path.

The standalone wrapper already contains target-forwarding logic; preserve it unless the test exposes a defect.

For `eggress pproxy run`, if the test confirms target loss, make the smallest correction:

1. extract the existing `test=<value>` value from `pproxy_args.known_unsupported` using the same semantics as standalone;
2. append `-t <value>` to the spawned `eggress upstream test` command;
3. preserve generated-config forwarding;
4. preserve subprocess exit propagation;
5. do not add a shared subprocess framework.

If process-level testing is difficult because invoking the real upstream tester would perform network access, extract only a small pure command-argument builder or testable target-extraction helper. Do not introduce mocking infrastructure or a generalized command abstraction.

### Acceptance for Workstream D

- both execution entry points consume the same `--test` target;
- neither silently substitutes a default target;
- `eggress pproxy check` remains non-executing;
- no network-dependent CI test is required.

---

## Workstream E — Make bare `-d` visible to compatibility reporting

Primary files:

- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-pproxy-compat/src/tier.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- focused compatibility tests

### Required outcome

A bare compatibility invocation containing `-d` must not be reported as `drop_in` if the canonical manifest classifies it `compatible_with_warning`.

Preferred bounded implementation:

1. when `args.debug` is true, emit a dedicated informational compatibility warning category such as `debug-mode`;
2. message should state the exact implemented difference: debug-level default tracing is enabled, but Python traceback semantics are not reproduced;
3. map `debug-mode` to `compatible_with_warning` in `tier.rs`;
4. map the same category to `feature_id = "debug"` and `tier = "compatible_with_warning"` in `StructuredDiagnostic::from()`;
5. do not reuse `verbose-mode` if doing so would incorrectly identify `-d` as `-v` or preserve the wrong tier.

`-d` execution must continue to use `PproxyArgs::default_log_level()` exactly as implemented at `f6336674`.

### Required tests

At minimum:

- translation of `-d` emits the dedicated debug diagnostic;
- aggregate tier for bare `-d` is `compatible_with_warning`;
- JSON structured diagnostic uses feature id `debug` and the same tier;
- `-d -vvv` still selects `trace` at runtime while reporting the debug caveat accurately;
- `--daemon` remains independently unsupported/fatal;
- explicit `RUST_LOG` precedence behavior remains unchanged.

### Acceptance for Workstream E

- runtime and reporter agree on what `-d` does;
- `pproxy check --json -- -d ...` cannot report `drop_in` merely because no `-v` flag is present;
- no logging framework changes are introduced.

---

## Workstream F — Reconcile active human docs only where required

After code/tier decisions are final, inspect current active references for these specific semantics:

- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/PPROXY_PARITY_SPEC.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `docs/architecture/pproxy-compat.md`
- `AGENTS.md` / `.skills/rust-proxy-dev/skill.md` only if they restate the affected flags

Correct only statements about:

- PAC path ownership/translation;
- `--get PATH,FILE` behavior;
- `--test <target>` ownership and native upstream-test delegation;
- `-d` reporter tier if the active doc exposes it.

Historical plans do not need wholesale rewriting. If a historical implementation summary states a now-known false final fact, add a short correction note rather than rewriting the original rationale.

Do not add a new documentation source of truth. The maintained matrix plus machine manifest remain authoritative.

### Acceptance for Workstream F

- active human docs agree with code and the machine manifest for the affected options;
- no new parity percentage or broad “100% drop-in” claim is introduced;
- historical files remain historical.

---

## Workstream G — Close implementation bookkeeping

Primary files:

- `plans/PPROXY_POST_CLOSURE_CORRECTIVE_PASS.md`
- `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`
- this plan

### Required corrections

1. Record the exact prior post-closure implementation commit:

```text
f6336674353aa63d868c8247ffafb9eb5a3eca4e
```

in `PPROXY_POST_CLOSURE_CORRECTIVE_PASS.md` where its implementation commit/range belongs.

2. Update the parent roadmap to register this final contract/reporting closure pass while it is pending.

3. When this pass lands:

- mark this file `IMPLEMENTED`;
- record the exact implementation commit(s);
- record focused verification results;
- return the parent roadmap to a completed status only after all acceptance criteria below are satisfied.

Do not create a separate closure/evidence report.

---

## Expected production-code touchpoints

The likely production-code delta should remain small:

- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-pproxy-compat/src/tier.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- `crates/eggress-cli/src/main.rs` only if `--test` target loss is confirmed

Likely tests:

- `crates/eggress-pproxy-compat` unit tests
- `crates/eggress-cli/tests/pproxy_binary.rs`
- `crates/eggress-cli/tests/pproxy_run_process.rs`
- existing compatibility contract tests if they already cover manifest/reporter output

Likely docs/plans:

- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- directly affected migration/spec/inventory docs
- `plans/PPROXY_POST_CLOSURE_CORRECTIVE_PASS.md`
- `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`
- this file

Do not touch protocol/runtime crates unless a focused test proves a direct dependency of one of the listed corrections.

---

## Required verification

### Focused Rust tests

Run the smallest relevant suites first:

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli --test pproxy_run_process
```

Add or run focused assertions for:

- `--pac` consumes its value and produces the expected PAC path/config;
- valid `--get PATH,FILE` produces static content and a supported reporter tier;
- invalid/unreadable `--get` fails as unsupported;
- `--test <target>` value ownership is preserved;
- both execution paths forward the exact test target;
- bare `-d` produces `compatible_with_warning` reporter output;
- tier mapping and structured diagnostic tier agree for the touched warning categories.

### Broad Rust gate

Because production Rust code is expected to change, finish with:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Do not add a new CI workflow to run these commands.

### Python verification

No Python behavior is expected to change. Do not perform Python code changes merely for this pass.

If the repository's existing smoke workflow runs automatically because Rust crates changed, let it provide normal coverage. A fresh local Python extension rebuild is not mandatory unless the modified Rust compatibility APIs are exposed through Python or an existing Python contract test fails.

If Python-visible reporter APIs do change, use the existing fresh-extension command from the parent roadmap and run `python/tests tests/compat`; do not add a new environment matrix.

---

## Explicit non-goals

This pass does **not** authorize:

- another pproxy parity roadmap;
- strict/full 2.7.9 private-internal parity;
- new protocols or transports;
- implementing `--daemon`, `--auth`, or pproxy-style implicit `--sys` mutation;
- changing `--reuse` behavior;
- expanding reverse/UDP support;
- Python server/runtime rewrites;
- changing Cargo feature topology or binary-size profiles;
- adding CI workflows or platform matrices;
- parsing the documentation manifest at runtime;
- generating code from the manifest;
- a universal diagnostic registry rewrite;
- a generalized subprocess abstraction for one `--test` fix;
- broad cleanup of historical plan prose.

---

## Acceptance criteria

The implementation criteria below were satisfied by `bd10467`, with the
metadata reconciliation completed by `cef6851cc7275aca3cb8f9e27cc3dc8b4f7abff6`
and its focused GET evidence/closure records completed by `cf448e7`.
Changed-surface Rust verification and the fresh-extension Python smoke suite
are green. The broad workspace gate was attempted with
`cargo test --workspace --locked` and remains blocked by the unrelated existing
`eggress-runtime::observability::udp_active_gauges_return_to_zero_after_close`
failure at line 1036: unchanged runtime code reports zero active associations
before the test assertion and the panic leaves its supervisor task running.
The same failure reproduces at `f6336674`, and no runtime file is changed by
this line. This is Outcome 2 and is outside this compatibility contract/
reporting scope.

This line of work is complete only when every item below is true.

### PAC / GET / TEST contract

- `--pac` is documented as value-taking everywhere active and its supplied path is represented accurately in the machine manifest;
- `--get` is documented according to the pinned `PATH,FILE`/custom-content contract, not as URL fetching;
- valid `--get` translation is classified as supported at the selected conservative tier;
- malformed/unreadable `--get` remains fail-closed and is reported unsupported;
- `--test` is documented as value-taking and the supplied target is preserved;
- standalone `pproxy` and `eggress pproxy run` use the same `--test` target semantics.

### Reporter consistency

- for every warning category touched by this pass, `manifest_tier_for_category()` and `StructuredDiagnostic::from()` emit the same tier;
- `eggress pproxy check --json` does not contain a feature tier that contradicts the diagnostic tier for the same warning;
- the get/static-content diagnostic no longer recommends unrelated `curl --proxy` behavior;
- bare `-d` is reported at the same `compatible_with_warning` tier recorded in the canonical contract;
- `-d` remains operationally independent from `-v` and `--daemon`.

### Canonical authority

- `docs/parity/pproxy_capability_manifest.toml` agrees with current executable behavior for `pac`, `get`, `test`, and `debug`;
- the maintained practical matrix does not contradict the manifest for those rows;
- active migration/spec/inventory docs contain no stale boolean/non-consuming descriptions for `--pac` or `--test`;
- no broad aggregate parity claim is added.

### Bookkeeping

- `PPROXY_POST_CLOSURE_CORRECTIVE_PASS.md` explicitly records `f6336674353aa63d868c8247ffafb9eb5a3eca4e` as its implementation commit;
- this plan is marked `IMPLEMENTED` with exact implementation commit(s) and verification result;
- the parent roadmap registers this pass and returns to completed status only after the pass is green;
- no additional closure/evidence document is created.

### Verification

- focused `eggress-pproxy-compat` tests pass;
- focused standalone `pproxy` tests pass;
- focused `eggress pproxy run` process tests pass;
- `cargo fmt --all -- --check` passes;
- `cargo clippy --workspace --all-targets -- -D warnings` passes;
- `cargo test --workspace --locked` is attempted and recorded as Outcome 2;
  the sole remaining failure is the named pre-existing runtime observability
  test, not a pproxy compatibility failure;
- no new routine CI job or matrix is introduced.

---

## Handoff guidance for a smaller implementation model

1. Start from `f6336674353aa63d868c8247ffafb9eb5a3eca4e` or newer `main`; do not assume this plan's source snippets are newer than the repo.
2. Verify each residual in current code before editing. If a residual has already been fixed by a newer commit, mark that item satisfied and do not rework it.
3. First write focused tests for reporter tier agreement, bare `-d`, and `--test` target forwarding.
4. Do not change runtime PAC/get/test behavior unless a test proves current behavior is wrong. Most of this pass should be contract/reporting correction.
5. For `--get`, trust the checked-in 2.7.9 CLI baseline: it is custom `PATH,FILE` HTTP content, not URL fetching.
6. Keep one canonical tier per warning category. Do not let `tier.rs` and `diagnostics.rs` disagree.
7. Use a dedicated `debug-mode` category rather than mislabeling `-d` as `verbose-mode` if a reporting warning is needed.
8. If `eggress pproxy run -- --test <target>` drops the target, copy the minimal target-forwarding logic already used by the standalone wrapper; do not invent shared command infrastructure.
9. Update docs only after code/tests settle the final classification.
10. Finish by recording the exact implementation commit and marking this plan/parent roadmap correctly. Do not create another follow-up pass for ordinary wording polish.
