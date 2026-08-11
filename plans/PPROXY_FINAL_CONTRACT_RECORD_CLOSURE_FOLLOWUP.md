# pproxy Final Contract-Record Closure Follow-up

## Status

**PLANNED**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `65370af606b134fab7ff4282a0182ff793651ba8`
- Compatibility target: `pproxy==2.7.9`
- Parent corrective plan: [`PPROXY_FINAL_CLOSURE_CORRECTIVE_PASS.md`](PPROXY_FINAL_CLOSURE_CORRECTIVE_PASS.md)
- Parent closure roadmap: [`PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md`](PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md)

## Purpose

Close the final contract-record and evidence inconsistencies found after the implementation of the pproxy closure corrective pass.

The proxy runtime, compatibility execution path, regex boundary, Python semantic fixes, and supported protocol surface are already substantially complete. This follow-up is **not** another parity phase and must not reopen protocol/runtime scope. It exists only to make the active compatibility contract, active system-proxy documentation, packaging metadata, test evidence references, and Phase 3 measurement record accurately reflect the code that is already shipped.

When this pass is complete, bounded `pproxy==2.7.9` compatibility is closed. Do not create another pproxy plan for wording polish or historical-document synchronization.

## Confirmed remaining items at baseline

### 1. Phase 3 artifact comparison was not actually attempted

`PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md` now records current release artifact sizes, but the required pre/post comparison was not performed. The current wording says the historical pre-Phase-3 revision was not rebuilt because it **may not** compile with the current toolchain.

That is not concrete evidence of a build limitation. The prior acceptance criterion allowed an exception only after an actual attempt produced a specific historical-build/toolchain/environment failure.

Expected pre-Phase-3 revision:

```text
3c1f12721deb2f25832c81a0303b8e7a6230d37a
```

Current implementation baseline:

```text
65370af606b134fab7ff4282a0182ff793651ba8
```

The implementer must verify ancestry before using these revisions.

### 2. Active system-proxy documentation still mixes library capability with nonexistent CLI capability

The canonical manifest now correctly states that:

- `eggress system-proxy inspect` is the public read-only CLI surface;
- crate-level apply/rollback primitives exist;
- no public `eggress system-proxy apply` or rollback CLI command exists.

However, active documentation still contains user-facing statements such as:

- Eggress provides “explicit dry-run apply” capabilities;
- mutation requires an explicit `--apply` flag;
- “Dry-run apply” is supported;
- rollback/revert is presented as a supported user-facing command/surface;
- the platform support matrix advertises system-proxy apply/revert support in a way that reads as public release capability.

At minimum inspect:

- `docs/system_proxy/README.md`
- `docs/system_proxy/PPROXY_SYSTEM_PROXY_BEHAVIOR.md`
- `docs/release/PLATFORM_SUPPORT_MATRIX.md`
- `docs/system_proxy/MACOS_NETWORKSETUP.md`
- `docs/system_proxy/LINUX_DESKTOP_PROXY.md`
- `docs/system_proxy/WINDOWS_PROXY_SETTINGS.md`
- active architecture/security/root documentation returned by a targeted search for `--apply`, `apply --dry-run`, `system-proxy apply`, and equivalent wording.

Do not delete useful documentation of the `eggress-system-proxy` Rust APIs. Correct the **surface classification**: library/apply-plan capability is not a public CLI command.

### 3. Canonical Python package contract contradicts the actual wheel/package configuration

The canonical manifest entry `python.importable_package` currently says the Eggress distribution does not install top-level `pproxy`, classifies the capability as unsupported, and says callers should use `from eggress import pproxy`.

The actual Maturin packaging configuration includes:

```toml
include = ["eggress/**/*.py", "pproxy/**/*.py", "eggress/py.typed"]
```

and the repository contains maintained top-level `python/pproxy/` compatibility modules plus public-namespace tests.

The practical compatibility matrix already describes “`eggress` wheel plus top-level `pproxy`” as matched. The canonical machine contract is therefore stale and internally inconsistent with both packaging configuration and the maintained human matrix.

This must be corrected based on **installed-wheel behavior**, not source-tree importability alone.

### 4. `cli.test` canonical evidence references a removed pre-Phase-3 helper

The `cli.test` behavior text now correctly describes the in-process shared Rust upstream-test path, but its `tests` list still names:

```text
upstream_test_command_args_preserve_target
```

That helper belonged to the removed subprocess/config-path execution path and no longer exists as current evidence.

Replace the stale evidence reference with maintained tests that actually prove:

- `--test` owns the supplied target;
- both compatibility entry points use the in-process test path;
- the compatibility service is not started for test mode where that is already covered;
- success/failure/target semantics remain stable.

Do not recreate the old helper merely to make the manifest reference valid.

### 5. Standalone unsupported-feature exit-code test does not freeze the exact code

The implementation now returns exit code `5` for unsupported features from both:

```text
pproxy <unsupported args>
eggress pproxy run -- <unsupported args>
```

The `eggress pproxy run` process test asserts `Some(5)`, but the representative standalone `pproxy --daemon` test only asserts a nonzero status.

The behavior is correct; the contract test is incomplete. Tighten the standalone test so both entry points explicitly freeze exit `5` for the same representative unsupported feature.

Do not redesign global CLI exit-code policy.

## Scope constraints

1. Do not change supported HTTP, SOCKS4/4a/5, UDP, TLS, Shadowsocks, Trojan, routing, chaining, reverse, transparent, H2, WS/WSS, raw, or tunnel wire behavior.
2. Do not add SSH, QUIC/H3, SSR, legacy Shadowsocks ciphers/OTA, plugin execution, daemonization, implicit system-proxy mutation, per-client auth reuse, general multi-hop UDP, or any other excluded parity scope.
3. Do not add `eggress system-proxy apply`, rollback, revert, or mutation CLI commands in order to make stale docs true.
4. Do not remove crate-level `eggress-system-proxy` apply/planning APIs merely because they are not exposed on the CLI.
5. Do not create a new compatibility registry, generated report, evidence bundle, dashboard, parity percentage, or documentation generator.
6. Do not add new CI workflows, platform matrices, oracle jobs, size gates, benchmark gates, or release gates.
7. Do not introduce a new packaging mechanism or a second Python distribution. The existing single `eggress` distribution remains authoritative.
8. Do not broaden the canonical manifest validator into a general repository-document synchronization framework. Add only focused assertions needed to prevent recurrence of the concrete packaging/evidence contradictions in this plan.
9. Do not rewrite historical plans/specs unless they are explicitly still presented as current authority. Historical files may retain old state when clearly bannered as historical.
10. Complete this work as one small implementation commit or one tightly related commit series, then stop.

## Required discovery before editing

Before making changes, verify all of the following at current `main`:

1. `65370af606b134fab7ff4282a0182ff793651ba8` is still an ancestor/current baseline and no newer commit has already corrected an item.
2. `3c1f12721deb2f25832c81a0303b8e7a6230d37a` is the immediate code revision before Phase 3's execution-path refactor commit `16abdff2778b83b131979031c134396c2435c45f`.
3. Current `rustc`, `cargo`, host target, and release profile used for artifact measurement.
4. Current `SystemProxyAction` or equivalent CLI enum exposes only the actually supported public commands.
5. Crate-level system-proxy apply/rollback APIs still exist and are library capabilities, so documentation should distinguish rather than erase them.
6. `crates/eggress-python/pyproject.toml` still includes `pproxy/**/*.py` in the wheel/source package.
7. Existing clean-wheel or installed-package tests actually import top-level `pproxy`; identify the best maintained test names to cite in the canonical manifest.
8. Current `python/pproxy/__init__.py` and public namespace tests confirm what symbols are intentionally exported.
9. Current in-process `--test` tests that supersede `upstream_test_command_args_preserve_target`.
10. Current standalone unsupported-feature process test and `eggress pproxy run` unsupported-feature process test.

If discovery contradicts any baseline assumption, record the actual result in this plan's implementation summary and make the smallest correction consistent with observable behavior.

---

## Workstream A — Complete the Phase 3 artifact comparison with real evidence

### A1. Attempt the historical build

First verify ancestry:

```bash
git merge-base --is-ancestor \
  3c1f12721deb2f25832c81a0303b8e7a6230d37a \
  16abdff2778b83b131979031c134396c2435c45f
```

Record:

```bash
rustc --version
cargo --version
rustc -vV
```

Use isolated worktrees and isolated `CARGO_TARGET_DIR` values so incremental artifacts do not contaminate the comparison.

Conceptual sequence:

```bash
git worktree add /tmp/eggress-pre-phase3 \
  3c1f12721deb2f25832c81a0303b8e7a6230d37a

git worktree add /tmp/eggress-post-phase3 \
  65370af606b134fab7ff4282a0182ff793651ba8

cd /tmp/eggress-pre-phase3
CARGO_TARGET_DIR=/tmp/eggress-size-pre \
  cargo build -p eggress-cli --release --locked

cd /tmp/eggress-post-phase3
CARGO_TARGET_DIR=/tmp/eggress-size-post \
  cargo build -p eggress-cli --release --locked
```

Measure exact byte sizes for at least:

```text
target/release/eggress
target/release/pproxy
```

Use `stat` or a platform-equivalent exact-byte command; human-readable `ls -lh` may be recorded in addition.

### A2. If the historical build fails

A failure is an acceptable closure result **only after an actual build attempt**.

Record:

- exact revision;
- exact command;
- toolchain/target;
- concise relevant compiler/build error;
- whether the failure is caused by current-toolchain drift, unavailable historical dependency/source, platform incompatibility, or another concrete reason.

Do not patch or modernize the historical revision solely to obtain a comparison.

Then record current exact artifact sizes plus the already-established production dependency result:

```bash
cargo tree -p eggress-cli -i tempfile -e normal
```

The expected current result remains no normal dependency path through `tempfile`.

### A3. Update the Phase 3 record honestly

Update `plans/PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md` in place with either:

- actual same-environment before/after byte measurements; or
- the actual historical-build failure plus current sizes and dependency-tree evidence.

Remove wording based on hypothetical failure such as “may not compile.”

Do not add a size threshold, CI check, benchmark harness, or separate report.

---

## Workstream B — Reconcile active system-proxy documentation by public surface

### B1. Define the terminology once

Use this distinction consistently in active documentation:

**Public CLI surface**

```text
eggress system-proxy inspect
```

is read-only and available.

**Rust library surface**

`eggress-system-proxy` contains apply planning, command construction/execution abstractions, and rollback-state primitives as appropriate to the crate implementation.

**Not a public CLI surface**

```text
eggress system-proxy apply
eggress system-proxy rollback
eggress system-proxy revert
--apply
apply --dry-run
```

must not be presented as commands users can invoke unless discovery finds they now actually exist.

### B2. Correct the active docs

At minimum correct:

#### `docs/system_proxy/README.md`

The overview/design principles must not say Eggress exposes explicit CLI `--apply` or dry-run mutation. The library section may explain crate-level planning/apply primitives, clearly labeled as Rust API capability only.

#### `docs/system_proxy/PPROXY_SYSTEM_PROXY_BEHAVIOR.md`

The Eggress divergence and classification table must distinguish:

- pproxy `--sys`: global mutation;
- Eggress compatibility `--sys`: refused;
- native Eggress CLI: read-only inspect only;
- crate-level apply/rollback primitives: library capability, not a public command.

Remove claims that dry-run apply or rollback are currently supported user-facing CLI features.

#### `docs/release/PLATFORM_SUPPORT_MATRIX.md`

Reconcile the release matrix with the canonical manifest. A row that reads as public `System proxy apply`/`revert` support must not show support when no public CLI command exists.

Preferred minimal treatment:

- rename the row explicitly to `System proxy apply CLI` and mark unsupported on all platforms; or
- remove the public apply/revert rows if they are redundant with the canonical manifest;
- retain library/backend notes only where useful and clearly labeled `Rust library API`, not CLI/release command support.

Do not expand the matrix with a large second dimension solely to document internals.

### B3. Bounded phrase search

Search active documentation for:

```text
system-proxy apply
apply --dry-run
--apply
explicit dry-run apply
Dry-run apply
System proxy revert
```

For each hit:

- active user-facing/reference/release/architecture/security doc -> correct if misleading;
- crate API documentation -> retain if accurate but make API/CLI distinction clear where needed;
- historical plan/spec with a clear historical banner -> leave alone.

Stop after this targeted search. Do not synchronize every historical parity file.

### B4. Focused regression assertion

Extend the existing canonical contract test or add one small test that prevents the **active canonical/public** documentation path from re-advertising `eggress system-proxy apply` as a current CLI command.

Do not snapshot all system-proxy documentation.

---

## Workstream C — Correct the canonical top-level Python package contract

### C1. Establish installed-wheel behavior

The source tree alone is insufficient evidence. Build/install a fresh wheel or use the repository's maintained clean-wheel harness.

Required observation in a clean environment:

```python
import pproxy
```

must succeed if the packaging contract is to claim top-level compatibility.

Also verify representative public symbols already targeted by the bounded compatibility layer, such as the maintained `Server`, `Connection`, `Rule`, `DIRECT`, or whatever the current public namespace tests explicitly require.

Do not add strict parity requirements for every private upstream symbol as part of this pass.

### C2. Correct `python.importable_package`

If clean-wheel `import pproxy` succeeds as expected, update `docs/parity/pproxy_capability_manifest.toml` so `python.importable_package` reflects actual packaged behavior.

Expected direction:

- `eggress_behavior`: the single `eggress` distribution installs a bundled top-level `pproxy` compatibility namespace alongside `eggress`;
- `python = "complete"` for namespace importability;
- `tier`: `drop_in` if the capability is strictly the ability to `import pproxy` unchanged; otherwise use the existing supported-difference tier only if an observable caveat applies to import itself;
- evidence: `integration` or `differential` based on the actual maintained clean-wheel test;
- `tests`: real maintained installed-package/public-namespace tests;
- notes: top-level namespace ownership is intentional and bundled in the same distribution; importability does not imply every private pproxy internal is implemented.

Remove stale text referring to “until Phase 4” or claiming the namespace is intentionally absent.

### C3. Reconcile the maintained practical matrix

The practical matrix already describes the top-level package as matched. Confirm it agrees exactly with the final manifest wording. Change only if needed.

### C4. Add a focused packaging-contract assertion

Add one narrow regression check that prevents the canonical manifest from drifting back to `python.importable_package = unsupported` while the package actually ships the top-level namespace.

Preferred approaches, in order:

1. an installed-wheel test that imports `pproxy` and asserts the manifest entry is not unsupported/refused;
2. an existing testkit manifest test that checks the entry against the maintained packaging configuration plus an existing clean-wheel import test;
3. a simple targeted assertion against `crates/eggress-python/pyproject.toml` only if the clean-wheel test already independently proves installation/import behavior.

Do not build a generic TOML-to-manifest synchronization engine.

---

## Workstream D — Replace stale `cli.test` evidence with live in-process evidence

Inspect current tests around:

- standalone `pproxy --test`;
- `eggress pproxy run -- ... --test`;
- shared `eggress_cli::run_upstream_test` / `run_upstream_test_with_mode`;
- exact target preservation.

Update the canonical `cli.test.tests` array so every named test actually exists at current `main` and proves current behavior.

Remove:

```text
upstream_test_command_args_preserve_target
```

unless discovery finds a current test with that exact name and current semantics.

Prefer existing process/integration test names. Add at most one focused test if an acceptance property is currently untested.

Do not recreate subprocess/config-path helpers.

---

## Workstream E — Freeze exit code 5 on both compatibility entry points

Tighten the representative standalone unsupported-feature process test to assert:

```text
exit code == 5
```

Use the same representative excluded feature as the `eggress pproxy run` test where practical, e.g. `--daemon`.

Required contract after this pass:

```text
unsupported compatibility feature -> 5
unknown option                 -> 2
actual config validation error -> 3
runtime failure                -> 1
success                        -> 0
```

This pass only needs to strengthen the standalone `5` assertion. Do not redesign unrelated exit handling.

If the standalone and nested tests currently use different excluded features, either align them or document why both still prove the same shared-gate contract.

---

## Workstream F — Update closure records in place and stop

After implementation and verification:

1. update this file from `PLANNED` to `IMPLEMENTED`;
2. record implementation commit SHA(s);
3. update `PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md` with actual artifact-attempt evidence;
4. update `PPROXY_FINAL_CLOSURE_CORRECTIVE_PASS.md` if its “all acceptance criteria met” summary would otherwise remain false or incomplete;
5. update the parent roadmap only if a statement there materially contradicts the final evidence;
6. do not create another completion/certification/evidence file.

The implementation summary in this file must state:

- historical Phase 3 build result and exact artifact evidence;
- final public-vs-library system-proxy documentation decision;
- final `python.importable_package` tier/evidence and clean-wheel test used;
- replacement `cli.test` evidence names;
- exact unsupported exit-code tests;
- focused and broad verification results.

---

## Verification sequence

### 1. Focused CLI/manifest tests

Run the directly affected Rust tests, adapting exact filters to current names:

```bash
cargo test -p eggress-testkit canonical_manifest
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli --test pproxy_run_process
cargo test -p eggress-cli --test cli_exit_codes
```

### 2. Canonical manifest validation

```bash
python3 scripts/validate_pproxy_parity_manifest.py \
  docs/parity/pproxy_capability_manifest.toml
```

Use the script's actual supported invocation if it accepts no positional path.

### 3. Fresh Python package verification

Use the repository's normal Maturin development or wheel workflow in a clean virtual environment.

Minimum required assertions:

```bash
python -c 'import pproxy; print(pproxy.__file__)'
python -m pytest python/tests/test_pproxy_public_namespace.py -q
```

Also run the maintained clean-wheel smoke test that proves `pproxy` is included in the installed distribution. If its current name/location differs, use the actual existing test.

If changing only manifest/docs/tests and no Python implementation, do not expand into a new cross-version Python matrix.

### 4. Phase 3 artifact attempt

Run Workstream A's isolated pre/post build attempt and record the actual result.

### 5. Bounded stale-phrase search

Search active docs/code for the known stale phrases from Workstream B plus:

```text
upstream_test_command_args_preserve_target
it does not install top-level pproxy
Top-level import is intentionally absent
```

Classify results rather than blindly editing historical records.

### 6. Broad Rust gate

Because this pass touches canonical tests and CLI process tests, finish with:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

### 7. CI topology check

Confirm `.github/workflows/` is unchanged by this pass unless an unrelated concurrent commit changed it. Do not add jobs.

---

## Explicit acceptance criteria

This follow-up is complete only when **all** criteria below are satisfied.

### Phase 3 artifact evidence

- ancestry of the pre-Phase-3 revision is verified;
- an actual build of the pre-Phase-3 revision is attempted under the recorded current toolchain/target/profile;
- if both revisions build, exact byte sizes for `eggress` and `pproxy` are recorded for both under equivalent conditions;
- if the historical revision fails to build, the exact attempted command and concrete failure reason are recorded instead of hypothetical wording;
- current artifact sizes remain recorded;
- current `cargo tree -p eggress-cli -i tempfile -e normal` shows no production dependency path through `tempfile`;
- no historical code is patched merely to obtain a measurement;
- no size threshold, benchmark gate, or CI gate is added;
- `PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md` accurately represents the evidence actually obtained.

### System-proxy documentation

- no active user-facing document claims that `eggress system-proxy apply`, rollback, revert, `--apply`, or `apply --dry-run` is a current public CLI command unless discovery proves otherwise;
- `docs/system_proxy/README.md` distinguishes read-only CLI inspection from crate-level apply/rollback APIs;
- `docs/system_proxy/PPROXY_SYSTEM_PROXY_BEHAVIOR.md` no longer classifies dry-run apply/rollback as supported public CLI features;
- `docs/release/PLATFORM_SUPPORT_MATRIX.md` agrees with the canonical manifest about public system-proxy apply/rollback availability;
- useful crate-level apply/planning documentation remains available and is clearly labeled as Rust library capability;
- no new mutation CLI is implemented;
- a bounded search of the known stale phrases finds no false active public-CLI claim.

### Canonical top-level Python package contract

- a fresh installed wheel/environment successfully executes `import pproxy`;
- representative maintained public namespace tests pass from the installed package;
- `python.importable_package` no longer says top-level `pproxy` is absent or intentionally unsupported when it is actually packaged;
- its final tier/layer/evidence values are justified by observable installed-package behavior;
- its `tests` list names maintained tests that actually exist;
- the canonical manifest and practical matrix agree on top-level `pproxy` package availability;
- a focused regression assertion prevents the manifest from returning to an unsupported/refused claim while the package still installs `pproxy`;
- the test does not impose strict parity on private pproxy internals outside the bounded product contract;
- no second Python distribution/package architecture is introduced.

### `cli.test` evidence

- `cli.test` retains the correct in-process shared Rust behavior description;
- `upstream_test_command_args_preserve_target` or any other removed subprocess-era helper is absent from active canonical evidence unless it genuinely exists again for current semantics;
- every test named in `cli.test.tests` exists at current `main`;
- named tests collectively cover exact target ownership and the current in-process test-mode path;
- no temporary-config or sibling-process execution path is reintroduced.

### Exit-code contract

- standalone `pproxy` explicitly returns and tests exit `5` for the representative unsupported feature;
- `eggress pproxy run` explicitly returns and tests exit `5` for the same shared-gate unsupported class;
- unknown-option exit `2` tests remain green;
- actual config-validation failures remain distinct from unsupported-feature failures;
- no global exit-code redesign occurs.

### Closure-record integrity

- `PPROXY_FINAL_CLOSURE_CORRECTIVE_PASS.md` no longer claims all criteria were met if the Phase 3 historical-build requirement was not actually attempted;
- after this implementation it may state completion only with the actual build-attempt evidence recorded;
- this plan is updated in place to `IMPLEMENTED` with implementation SHA(s), exact verification results, and all retained limitations;
- no new closure report, evidence bundle, registry, dashboard, or parity percentage is created.

### Regression/scope control

- focused canonical-manifest tests pass;
- CLI process tests pass;
- canonical manifest validation passes with no hard errors;
- clean-wheel/top-level `pproxy` import verification passes;
- `cargo fmt --all -- --check` passes;
- `cargo clippy --workspace --all-targets -- -D warnings` passes;
- `cargo test --workspace --locked` passes;
- no supported proxy data-plane behavior is intentionally changed;
- no excluded pproxy protocol/plugin/process scope is added;
- no new runtime dependency is added for this pass;
- hosted CI topology is unchanged;
- crates.io/PyPI release policy is unchanged.

## Implementation order

Execute in this order to minimize churn:

1. Attempt and record the Phase 3 historical/current artifact build comparison.
2. Correct active system-proxy documentation and release matrix wording.
3. Build/install the Python wheel and correct `python.importable_package` from observed behavior.
4. Add the focused packaging-contract assertion.
5. Replace stale `cli.test` evidence names with current tests.
6. Tighten standalone unsupported exit-code assertion to exact `5`.
7. Run bounded stale-phrase searches and fix only active false claims.
8. Run focused tests, manifest validator, clean-wheel verification, then the broad Rust gate.
9. Update this plan and the prior corrective/Phase 3 records in place.
10. Stop.

## Closure rule

After the acceptance criteria above are satisfied, the `pproxy==2.7.9` line is closed.

Do **not** create another pproxy parity/corrective plan for residual wording, aggregate parity percentages, private API completeness, or historical-document synchronization.

Reopen only for one of:

- a reproducible user-visible compatibility defect within the supported bounded claim;
- a correctness/security defect in a currently supported proxy path;
- an explicit project decision to expand scope;
- a decision to target a different upstream pproxy version.
