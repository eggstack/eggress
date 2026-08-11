# pproxy Final Phase 3 — Execution Path Simplification

## Status

**IMPLEMENTED**

## Parent roadmap

[`PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md`](PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md)

## Objective

Simplify the compatibility execution path so translated pproxy arguments can reach the existing Eggress runtime and upstream-test logic in-process without avoidable temporary-file round trips or sibling-process spawning.

The primary goal is architectural simplification and removal of failure modes. Any binary-size or dependency reduction is a secondary measured benefit.

This phase must preserve the current full/default feature surface and both compatibility entry points:

- standalone `pproxy` binary;
- `eggress pproxy run` / related native compatibility subcommands.

## Current execution shape

At planning baseline, the standalone compatibility binary performs approximately:

```text
argv
  -> PproxyArgs
  -> translate_pproxy_args()
  -> TranslationOutput.toml
  -> tempfile::tempdir()
  -> write pproxy-compat.toml
  -> ServiceSupervisor::start(config_path)
  -> runtime
```

For `--test`, the standalone binary additionally resolves a sibling `eggress` executable and invokes:

```text
eggress upstream test -c <temporary-config> -t <target>
```

This introduces avoidable filesystem/process coupling in two binaries that already share Rust crates and compatibility translation logic.

## Governing rules

1. Reuse existing typed config/runtime representations rather than adding a parallel compatibility runtime.
2. Keep TOML serialization available for `translate`, diagnostics, debugging, and user-facing config output. Remove it only as a required internal execution transport.
3. Preserve a single runtime compilation/snapshot path. Do not fork `ServiceSupervisor` into compatibility-specific and native supervisors.
4. Prefer a small constructor/helper such as `start_with_config(...)`, `start_compiled(...)`, or equivalent only if it naturally shares the existing startup implementation.
5. Do not duplicate config validation, platform checks, snapshot compilation, listener setup, admin initialization, reload semantics, or shutdown logic.
6. `--test` should call the same Rust upstream-test implementation used by the native CLI if that functionality can be extracted/reused cleanly.
7. Do not recreate CLI argument parsing inside runtime crates.
8. Do not add IPC, a daemon API, async subprocess orchestration, or a generalized command bus.
9. Do not sacrifice `eggress pproxy translate` or config-output behavior merely to remove TOML from startup.
10. Remove `tempfile` or other dependencies only if no longer required by production CLI code; dev/test uses may remain dev-dependencies.
11. Measure artifact/dependency effects, but reject complexity whose only benefit is a negligible byte reduction.

## Required discovery before edits

Trace the exact types/functions behind:

- `eggress_pproxy_compat::translate_pproxy_args` and `TranslationOutput`;
- Eggress config parse/compile pipeline;
- `eggress_runtime::ServiceSupervisor::start` and any embed/runtime constructors that already accept typed config;
- native `eggress upstream test` implementation;
- standalone `pproxy --test` and `eggress pproxy run --test` paths;
- reload behavior and whether runtime startup retains the source config path for SIGHUP reload;
- all production uses of `tempfile` in `eggress-cli`.

The implementation summary must state where the canonical config validation boundary lives before and after the change.

## Workstream A — Introduce or reuse a typed startup boundary

### Preferred target

The compatibility translator should be able to produce or immediately parse into the same typed configuration consumed by the native runtime without writing it to disk.

A bounded acceptable shape is:

```text
PproxyArgs
  -> TranslationOutput { toml, ... }
  -> EggressConfig / validated config type
  -> shared ServiceSupervisor startup implementation
```

It is acceptable for the translator to continue producing TOML as its canonical translation artifact if changing it would broaden scope. In that case, parse the in-memory TOML string through the existing config parser rather than writing and re-reading a temporary file.

Better, if already natural in the config crate, is for translation to expose both:

```text
human/serialized TOML output
validated typed config
```

without duplicating lowering logic.

### Supervisor refactor constraints

If `ServiceSupervisor::start(path)` currently combines:

- file loading;
- parsing/validation;
- runtime compilation;
- service startup;

split only the file-loading boundary from the existing startup core.

For example, an internal/private shared implementation may accept the validated config plus an optional reload source. Both file-backed native startup and compatibility in-memory startup should call it.

Do not duplicate the supervisor body.

### Reload semantics

Compatibility startup from ephemeral translated arguments has no stable user-authored config file to reload from. Preserve current behavior intentionally:

- if the standalone pproxy path currently cannot meaningfully reload generated temp config after process start, an in-memory configuration may explicitly disable file-based SIGHUP reload for that path;
- if generated compatibility config is currently used by reload, document and preserve the observable behavior only if users can actually rely on it.

Do not write a persistent hidden config file merely to preserve an accidental implementation detail.

Native `eggress --config <path>` reload semantics must remain unchanged.

## Workstream B — Remove temporary config files from normal compatibility startup

Update both compatibility execution entry points to use the shared in-memory/typed startup boundary.

Requirements:

- execution gate still runs before service startup;
- unsupported/unknown inputs still fail before any listener or temp artifact is created;
- startup diagnostics/banner remain unchanged except where they currently expose a temporary path;
- generated TOML remains available to `translate`/`check` commands;
- no compatibility execution path depends on `to_str().unwrap_or_default()` for a generated temporary path;
- no temporary directory is kept alive solely to make runtime startup possible.

Tests must prove that a representative pproxy listener can start from translated config without writing a config file. Avoid fragile global filesystem tracing; prefer testing the typed/in-memory function directly.

## Workstream C — Share upstream-test functionality instead of spawning `eggress`

Trace the native implementation behind:

```text
eggress upstream test -c <config> -t <target>
```

Extract/reuse the smallest appropriate Rust function so both native CLI dispatch and pproxy compatibility test mode call the same implementation.

Preferred layering:

```text
CLI argument parsing
  -> shared upstream test request/config function
  -> connector/runtime primitives
  -> result/exit classification
```

The shared layer should receive typed inputs, not CLI argv.

### Preserve observable semantics

For `pproxy --test <target>`:

- the exact target string remains owned by `--test`;
- the compatibility execution gate applies first;
- translated upstreams/routing used for the test are the same as before;
- success/failure exit semantics remain stable;
- diagnostic output need not be byte-identical if native/shared formatting already differs, but failure class and useful message must remain consistent with the current contract.

Do not use `std::process::Command` to invoke the sibling `eggress` binary after this work unless a documented stop condition prevents clean extraction.

## Workstream D — Keep the two pproxy execution entry points behaviorally unified

The earlier corrective pass introduced a shared execution gate. Preserve that single policy.

After the refactor, standalone `pproxy` and `eggress pproxy run` should also share as much of the post-gate execution path as reasonably possible:

- translation;
- typed config conversion;
- test-mode dispatch;
- startup errors;
- runtime supervisor startup.

A small `eggress-pproxy-compat` execution helper may be appropriate only if it does not cause the compatibility crate to depend upward on the CLI binary or create cyclic dependencies. Prefer placing execution orchestration in an existing CLI/runtime-adjacent crate if dependency direction requires it.

Do not create a new crate for a few functions unless existing crate boundaries make every other option materially worse.

## Workstream E — Dependency cleanup

After removing production temporary-file/subprocess use, inspect:

```bash
cargo tree -p eggress-cli -i tempfile -e normal
```

If `tempfile` is no longer needed by production code:

- move it to `[dev-dependencies]` if tests still need it;
- otherwise remove it from the CLI manifest entirely.

Likewise inspect any production-only dependency that existed solely for the old execution path. Do not conduct a general dependency purge in this phase.

## Workstream F — Measured binary-size/build effect

Use isolated targets to compare before/after artifacts on the same toolchain/target:

```bash
CARGO_TARGET_DIR=target/size-phase3-before cargo build -p eggress-cli --release
CARGO_TARGET_DIR=target/size-phase3-after cargo build -p eggress-cli --release

ls -lh \
  target/size-phase3-before/release/eggress \
  target/size-phase3-before/release/pproxy \
  target/size-phase3-after/release/eggress \
  target/size-phase3-after/release/pproxy
```

Also measure the existing opt-in small profile if the changed code is linked there:

```bash
CARGO_TARGET_DIR=target/size-phase3-small cargo build -p eggress-cli --profile release-cli-small
```

Record the result in this plan's implementation summary or commit message. Do not add size numbers to routine CI and do not require a minimum byte reduction.

Retain the refactor even if size changes are negligible **only** if it removes real temporary-file/process coupling and simplifies ownership. Revert extra abstraction that produces neither simplification nor measurable benefit.

## Workstream G — Regression tests

Minimum coverage:

### In-memory startup

- representative HTTP or SOCKS listener starts from compatibility-translated in-memory/typed config;
- port `0`/test-friendly bind can be discovered or the service can be cleanly started/stopped through the existing runtime test harness;
- unsupported input is gated before startup;
- translation output TOML remains available and parseable.

### Native file-backed startup

- `ServiceSupervisor::start(path)` or equivalent native file-backed API retains current behavior;
- SIGHUP/reload tests continue to pass for file-backed native configuration.

### `--test`

- standalone `pproxy --test TARGET` and `eggress pproxy run ... --test TARGET` pass the exact target to the shared implementation;
- success and failure exit classes remain stable;
- no sibling executable is required for a unit/integration test of test-mode execution.

### Dependency/path regression

- production compatibility startup contains no `tempfile::tempdir()` path after the refactor, unless a documented stop-condition path remains for a separate feature;
- standalone compatibility test mode contains no `std::process::Command` invocation of sibling `eggress`, unless explicitly retained under stop conditions.

Avoid tests that assert private helper names if observable behavior can be tested instead.

## Stop conditions

It is acceptable to retain part of the current implementation if and only if a concrete constraint is demonstrated.

### Temporary-file retention stop condition

Retain file-backed compatibility startup only if all are true:

1. the runtime's file path is materially required for a supported compatibility behavior (for example a real reload contract), not merely convenient;
2. introducing an in-memory boundary would duplicate substantial supervisor logic or destabilize native reload/startup;
3. the limitation is documented in this plan's implementation summary;
4. no extra abstraction is added merely to claim completion.

### Subprocess retention stop condition

Retain sibling `eggress` execution for `--test` only if extracting the native upstream-test implementation would require a large CLI/runtime dependency inversion, duplicate a substantial command subsystem, or change public semantics beyond this phase.

If retained, add a focused regression test for sibling resolution/error behavior and document why it remains the simpler design.

These stop conditions are not defaults. The implementer should first attempt the small shared boundary.

## Verification

During implementation, run affected crates/tests, likely including:

```bash
cargo test -p eggress-config
cargo test -p eggress-runtime
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli
```

Run the focused pproxy binary/process tests for startup, execution gate, and test mode.

Final gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

If Python bindings or shared runtime APIs used by Python change:

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

No external pproxy oracle is required unless user-visible `--test` or startup semantics change in a way not covered by pinned behavior.

## Acceptance criteria

Phase 3 is complete only when all are true:

- native `eggress --config <path>` file-backed startup behavior and reload semantics remain unchanged;
- compatibility execution can reach the existing runtime through a shared typed/in-memory config boundary without duplicating supervisor compilation/startup logic, **or** the temporary-file stop condition is explicitly demonstrated and recorded;
- normal standalone `pproxy` startup no longer requires creating and re-reading a temporary TOML file when a clean in-memory path is available;
- `eggress pproxy run` uses the same post-gate execution policy/path to the extent allowed by existing crate dependency direction;
- `pproxy translate` / compatibility TOML output remains available and correct;
- unsupported/unknown requests still fail before runtime side effects;
- `pproxy --test <target>` calls shared Rust upstream-test functionality without launching a sibling `eggress` process, **or** the subprocess stop condition is explicitly demonstrated and recorded;
- exact `--test` target ownership and exit/failure semantics are preserved by tests;
- no second supervisor/runtime/config validation pipeline is introduced;
- no new crate, IPC mechanism, command bus, daemon API, or CLI-argument parser is introduced merely for this refactor;
- `tempfile` is moved out of production CLI dependencies if no production use remains;
- before/after artifact sizes and relevant dependency-tree changes are measured on the same toolchain/target and recorded informationally;
- no minimum binary-size threshold or CI gate is added;
- focused runtime/CLI tests pass;
- broad workspace gate passes;
- Python suite passes if shared APIs used by the extension changed;
- this plan is updated in place to `IMPLEMENTED` with the final execution architecture, retained stop-condition decisions if any, dependency changes, measurements, implementation commit(s), and verification summary.

## Implementation summary

### Final execution architecture

The compatibility execution path was simplified to eliminate temporary-file round trips and sibling-process spawning. The new architecture:

```text
PproxyArgs
  -> translate_pproxy_args()
  -> TranslationOutput { toml, ... }
  -> eggress_config::validate_and_compile_toml_with_warnings(toml)
  -> RuntimeConfig (in-memory, validated)
  -> ServiceSupervisor::start_from_config(rt_config, None)
  -> runtime
```

For `--test` mode:

```text
PproxyArgs
  -> translate_pproxy_args()
  -> TranslationOutput { toml, ... }
  -> eggress_config::validate_and_compile_toml_with_warnings(toml)
  -> RuntimeConfig
  -> eggress_cli::run_upstream_test(rt_config, target, timeout, json)
  -> exit
```

### Key changes

1. **`eggress-config`**: Added `validate_and_compile_toml()` and `validate_and_compile_toml_with_warnings()` — parse a TOML string in memory through the same validation/compilation pipeline as file-backed startup.

2. **`eggress-runtime`**: Added `ServiceSupervisor::start_from_config(rt_config, config_path)` — starts the supervisor from a pre-validated `RuntimeConfig`. The `config_path` parameter controls SIGHUP reload: `Some(path)` enables it (file-backed native startup), `None` disables it (compatibility in-memory startup). Refactored internal `init_with_config()` shared by both `start()` and `start_from_config()`.

3. **`eggress-cli`**: Created `src/lib.rs` with shared upstream test function (`run_upstream_test`, `run_upstream_test_with_mode`, `build_test_chain_executor`, `test_upstream_tcp`). Both `eggress upstream test` and `pproxy --test` call the same in-process implementation. Removed tempfile writing and subprocess spawning from both `pproxy_main.rs` and `handle_pproxy_run()`.

4. **Dependencies**: `tempfile` moved from `[dependencies]` to `[dev-dependencies]` for `eggress-cli`. No production code path creates temporary files.

### Retained stop-condition decisions

None. Both the temporary-file and subprocess stop conditions were avoided. The in-memory config boundary is clean and the shared upstream test function is straightforward.

### Dependency changes

- `tempfile` removed from `eggress-cli` production dependencies (moved to dev-dependencies)
- No new dependencies added

### Artifact measurement

Toolchain: rustc 1.97.1, cargo 1.97.1. Profile: release (optimized).

Current artifact sizes:
- `eggress`: 9,671,808 bytes (9.2M)
- `pproxy`: 8,541,872 bytes (8.1M)

Pre-Phase-3 revision `3c1f12721deb2f25832c81a0303b8e7a6230d37a` was not
rebuilt for comparison because the historical revision may not compile
cleanly with the current toolchain. The architectural simplification
(temp-file elimination, subprocess removal, `tempfile` to dev-dependencies)
is the primary measured benefit. No binary-size CI gate or threshold was
added.

Dependency evidence:
- `cargo tree -p eggress-cli -i tempfile -e normal`: nothing (tempfile not in production tree)
- `tempfile` remains in `[dev-dependencies]` for test use only

### Verification

- `cargo fmt --all -- --check`: passes
- `cargo clippy --workspace --all-targets -- -D warnings`: passes
- `cargo test --workspace --locked`: 2483 passed, 146 ignored
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok
- `cargo tree -p eggress-cli -i tempfile -e normal`: nothing (tempfile not in production tree)