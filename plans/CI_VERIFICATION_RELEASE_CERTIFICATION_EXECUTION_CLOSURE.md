# CI, Verification, and Release — Certification Execution Closure Pass

## Status

**READY FOR IMPLEMENTATION**

## Baseline

- Repository: `eggstack/eggress`
- Baseline branch: `main`
- Baseline commit: `ab2a73eaff720e27935b1dcedac1a5472df227ae`
- Parent plan: `plans/CI_VERIFICATION_RELEASE_FINAL_EVIDENCE_CLOSURE.md`
- Parent implementation commits:
  - `ace059c94b6fc375d3a358fbb3656b9e5af318e7` — isolated certification setup, profile reduction, and trigger simplification
  - `56071d3ec8734bb0cd54d71de6b5b5bc8cd3a56d` — deterministic SOCKS5 relay half-close correction
  - `ab2a73eaff720e27935b1dcedac1a5472df227ae` — closure record currently marked complete
- Previously verified hosted smoke runs:
  - Rust smoke: GitHub Actions run `30451272521`, success
  - Python 3.12 smoke: GitHub Actions run `30451272360`, success

## Purpose

Close the remaining execution and evidence defects in the manual pproxy behavioral-certification path without changing the already-successful CI and release simplification.

The broad reductive objective has been achieved and must remain intact:

- exactly two small GitHub Actions workflows remain;
- automatic workflow triggers are push-to-`main` plus manual dispatch only;
- both workflows use read-only permissions;
- automated package publication, GitHub Release creation, artifact publication, signing, SBOM generation, and release matrices remain absent;
- Rust and Python publication remain manual operator actions;
- ordinary hosted Rust and Python smoke jobs are green;
- structural oracle scenarios are ordinary ungated tests;
- the certification profile model contains only Differential and Platform;
- the Python multi-chunk SOCKS5 race has been corrected.

The line is not yet fully closed because the manual certification command does not consistently use its isolated oracle interpreter, its first Rust differential check sets the wrong gate, the documented observation layout is not implemented, no successful end-to-end certification run is recorded, and PyPI/TestPyPI trusted-publisher state was inferred from public 404 responses rather than inspected directly.

This is a narrow execution-closure plan. It must not become another CI redesign, release-system project, compatibility roadmap, or evidence platform.

---

# 1. Confirmed residual defects

Implementation must begin from the current tree and verify each statement below before editing.

## 1.1 The first certification check sets the wrong gate

Current `scripts/run_pproxy_certification.sh` invokes the Rust differential test with:

```bash
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 \
  cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
```

Current `crates/eggress-cli/tests/differential_pproxy.rs` requires:

```text
EGRESS_REQUIRE_EXTERNAL_INTEROP=1
```

The test's `require_external_interop()` function panics when that variable is absent. The top-level certification command therefore does not satisfy the test binary's own prerequisite contract.

The corrective implementation must use the gate required by the test being executed. Do not introduce another alias merely to conceal the mismatch.

## 1.2 The Rust differential test bypasses the isolated oracle interpreter

The certification runner creates and verifies:

```text
target/pproxy-certification/oracle-venv/bin/python
```

It exports:

```text
EGRESS_ORACLE_PYTHON
```

However, `crates/eggress-cli/tests/differential_pproxy.rs` directly executes `python3` for:

- Python availability checks;
- pproxy import checks;
- pproxy process startup;
- authenticated pproxy process startup.

That means the first certification check can:

- ignore the pinned oracle environment;
- use an unrelated system pproxy installation;
- fail when system Python lacks pproxy even though the oracle environment is valid;
- execute a pproxy version other than `2.7.9`;
- produce results that do not belong to the verified oracle environment.

The reusable differential harness in `eggress-testkit` already recognizes `EGRESS_PYTHON_BIN`, but the certification runner exports a different variable and the direct test bypasses the reusable helper.

A single canonical interpreter contract is required.

## 1.3 Interpreter ownership is inconsistent across helpers

Current certification-path scripts and Rust harnesses use a mixture of:

- `EGRESS_ORACLE_PYTHON`;
- `EGRESS_CANDIDATE_PYTHON`;
- `EGRESS_PYTHON_BIN`;
- `ORACLE_VENV`;
- `CANDIDATE_VENV`;
- host `python3`;
- venv-local `bin/python`;
- venv-local `bin/pip`;
- host `maturin` in standalone helper paths.

The top-level runner must remain the owner of environment creation. Every certification subprocess must receive an explicit interpreter or venv path from that owner.

Host `python3` is acceptable only for the initial preflight and `python3 -m venv` bootstrap. Once the oracle and candidate environments exist, behavioral execution must not silently fall back to host Python.

## 1.4 The paired observation layout is flat despite a split contract

The top-level runner exports:

```text
EGRESS_ORACLE_OBSERVATIONS_DIR=.../observations/oracle
EGRESS_CANDIDATE_OBSERVATIONS_DIR=.../observations/candidate
```

The paired API helper is instead called with one flat `OUTPUT_DIR`. The strict Python differential invocation then passes the same directory as both:

```text
--oracle-observations-dir
--candidate-observations-dir
```

The implementation therefore does not match the declared canonical layout:

```text
observations/
  oracle/
  candidate/
```

A filename suffix convention inside one directory can be valid, but the repository must choose one contract. Because the current plan and environment variables already define split ownership, this corrective pass should implement split directories rather than weaken the contract again.

## 1.5 The paired API helper can recreate environments and build wheel artifacts

`scripts/run_strict_pproxy_api.sh` supports standalone use by creating venvs and building wheels under `target/wheels` when its configured environments do not exist.

That standalone behavior is not inherently wrong. It is wrong if the certification path can accidentally fall into it after the top-level runner has already established isolated environments.

Certification mode needs an explicit fail-closed option such as:

```text
--use-existing-environments
```

or:

```text
--no-bootstrap
```

When that option is active, the helper must:

- reject missing oracle or candidate interpreters;
- reject missing imports;
- avoid creating venvs;
- avoid installing packages;
- avoid building wheels;
- avoid writing to `target/wheels` or `dist/`;
- run the comparison using the interpreters supplied by the top-level runner.

Standalone bootstrap behavior may remain for direct developer use if it is clearly separated and tested.

## 1.6 No successful full certification run is recorded

The current closure record contains:

- successful hosted Rust smoke evidence;
- successful hosted Python smoke evidence;
- synthetic failure-injection exercises;
- local unit/workspace test claims.

It does not contain a successful invocation of:

```bash
./scripts/run_pproxy_certification.sh
```

ending with:

```text
CERTIFICATION PASSED
```

and a validated `summary.json` generated by the final script.

This omission is material because the incorrect gate and hard-coded system interpreter are present specifically in the full certification path. Green ordinary CI does not prove that manual certification works.

## 1.7 Public registry 404 responses do not prove the absence of pending publishers

The current closure record treats public JSON endpoint 404 responses for absent PyPI and TestPyPI projects as proof that trusted-publisher bindings cannot exist.

PyPI supports pending trusted publishers for projects that do not yet exist. A pending publisher can be configured from account-level publishing settings and later create a project on first authorized publication.

Therefore:

- project absence is useful registry-state information;
- project absence is not sufficient trusted-publisher evidence;
- account-level pending publishers must be inspected directly;
- project-level publishers must also be inspected if a project exists by closure time.

The same direct inspection requirement applies independently to PyPI and TestPyPI.

## 1.8 Closure status and acceptance matrix disagree

The parent plan is marked `COMPLETE`, while its final acceptance matrix remains unchecked and the execution defects above are still present.

The current status must be reopened before implementation begins. Historical claims and run IDs should remain visible, but they must be labeled as partial evidence rather than final proof of certification closure.

---

# 2. Required final state

This pass is complete only when every applicable statement below is true.

1. `scripts/run_pproxy_certification.sh` sets the exact gate required by `differential_pproxy.rs`.
2. The Rust differential test never invokes a bare `python3` during certification execution.
3. Every pproxy process used by certification is launched through the verified oracle interpreter.
4. The verified oracle interpreter points inside `target/pproxy-certification/oracle-venv/`.
5. The candidate Python suites run only through the verified candidate interpreter.
6. The candidate interpreter points inside `target/pproxy-certification/candidate-venv/`.
7. The oracle environment contains exactly `pproxy==2.7.9`, verified through distribution metadata.
8. The candidate environment imports the locally installed `eggress` and compatibility package.
9. A missing interpreter, missing import, or wrong oracle version fails before behavioral checks begin.
10. The canonical Rust helper checks `EGRESS_ORACLE_PYTHON` first during certification.
11. `EGRESS_PYTHON_BIN` is either removed from the certification path or retained only as a documented backward-compatible fallback.
12. Direct test binaries use the shared interpreter resolver rather than duplicating `Command::new("python3")`.
13. The first Rust differential check executes at least one ignored test and cannot report success from a zero-test invocation.
14. Paired oracle records are written only under `observations/oracle/`.
15. Paired candidate records are written only under `observations/candidate/`.
16. The comparator receives distinct oracle and candidate directories.
17. Missing oracle records fail.
18. Missing candidate records fail.
19. Duplicate or cross-contaminated records fail with a concise diagnostic.
20. Certification mode in `run_strict_pproxy_api.sh` does not create environments or build wheels.
21. Certification mode does not write to `target/wheels` or `dist/`.
22. Standalone helper behavior remains usable or is deliberately simplified and documented.
23. `run_strict_pproxy_interop.sh` uses the supplied oracle interpreter.
24. `compat_udp_pproxy.sh` uses the supplied oracle interpreter.
25. No certification helper silently falls back to host Python after environment setup.
26. A clean end-to-end invocation of `run_pproxy_certification.sh` exits zero.
27. The successful end-to-end invocation reports at least one executed check and zero required failures.
28. `summary.json` parses successfully and reports `result: pass`.
29. `summary.json` records the correct commit, oracle version, and interpreter ownership.
30. Successful certification leaves no per-check stdout/stderr logs or `tmp/` directory.
31. Failed certification retains only relevant diagnostics under `failures/`.
32. No Markdown, JUnit, release wheel, evidence bundle, or workflow artifact is required.
33. The two existing hosted smoke workflows remain unchanged unless a narrowly necessary path trigger adjustment is required.
34. Rust smoke passes on the final implementation commit.
35. Python smoke passes on the final implementation commit when the changed paths trigger it.
36. Exactly two workflow files remain.
37. Workflow permissions remain read-only.
38. No release or certification workflow is added.
39. PyPI account-level pending publishers are inspected directly.
40. TestPyPI account-level pending publishers are inspected directly.
41. Existing project-level publishers are inspected if either project exists.
42. Any stale publisher referencing `eggstack/eggress` or an old release workflow is removed.
43. If authenticated registry settings cannot be inspected, this plan remains `BLOCKED — REGISTRY SETTINGS ACCESS REQUIRED`.
44. Public 404 responses are not presented as sufficient trusted-publisher proof.
45. The parent plan is marked reopened or superseded by this plan while implementation is in progress.
46. This plan is marked complete only after full certification and registry verification are both proven.
47. The final acceptance matrix is actually checked when closure is declared.
48. Crates.io publication boundaries remain unchanged and out of scope.
49. Manual release ownership remains unchanged.
50. The implementation remains deletion-first and avoids new orchestration abstractions.

---

# 3. Scope

## In scope

- `scripts/run_pproxy_certification.sh`;
- `scripts/run_strict_pproxy_api.sh`;
- `scripts/run_strict_pproxy_api.py`;
- `scripts/run_strict_pproxy_interop.sh`;
- `scripts/compat_udp_pproxy.sh`;
- `crates/eggress-cli/tests/differential_pproxy.rs`;
- `crates/eggress-testkit/src/differential.rs`;
- narrow tests for interpreter resolution and pproxy process launch;
- strict Python differential observation-loading plumbing;
- `python/tests/strict/conftest.py` or equivalent option parsing when needed;
- active documentation describing certification execution;
- `docs/DIFFERENTIAL_TESTING.md`;
- `docs/TESTING.md`;
- `docs/CI_STATUS.md` only if its current description needs correction;
- `.skills/testing/skill.md` only if it names obsolete interpreter or observation contracts;
- `plans/CI_VERIFICATION_RELEASE_FINAL_EVIDENCE_CLOSURE.md` status correction;
- this plan's final status and evidence record;
- authenticated PyPI account publishing settings;
- authenticated TestPyPI account publishing settings.

## Out of scope

Do not:

- add proxy protocols or features;
- change pproxy parity expectations;
- remove differential or interoperability tests;
- add a GitHub Actions certification job;
- add a workflow matrix;
- restore pull-request CI triggers;
- add release workflows;
- publish to PyPI, TestPyPI, crates.io, or GitHub Releases;
- add PyPI API tokens or OIDC permissions to GitHub;
- add `id-token: write` to any workflow;
- add package-signing or attestation machinery;
- introduce `xtask`, `just`, `cargo-make`, release-plz, cargo-release, or semantic-release;
- create a permanent evidence archive;
- upload certification output as a workflow artifact;
- add Docker solely for certification;
- restructure the Rust crate publication graph;
- modify the already-correct SOCKS5 relay fix unless a new regression is demonstrated;
- alter branch protection policy;
- treat public package-index responses as authenticated settings evidence;
- mark closure complete when registry access is unavailable.

---

# 4. Execution order

Implement in this exact order:

1. CX0 — reopen the parent closure status;
2. CX1 — establish one canonical oracle-interpreter contract;
3. CX2 — correct the Rust differential gate and process launch paths;
4. CX3 — make helper scripts consume existing environments explicitly;
5. CX4 — implement separate observation ownership;
6. CX5 — add focused contract tests and failure injections;
7. CX6 — execute and validate the full certification command;
8. CX7 — verify authenticated PyPI and TestPyPI publisher settings;
9. CX8 — align documentation and close status truthfully.

Do not begin CX6 until CX1 through CX5 pass locally. Do not mark any plan complete before CX7 is resolved.

---

# Workstream CX0 — Reopen the current false closure

## Objective

Make the repository's planning state truthful before additional implementation lands.

## Required change

Update:

```text
plans/CI_VERIFICATION_RELEASE_FINAL_EVIDENCE_CLOSURE.md
```

Change:

```text
COMPLETE
```

to:

```text
REOPENED — CERTIFICATION EXECUTION CLOSURE REQUIRED
```

Add a concise note linking to:

```text
plans/CI_VERIFICATION_RELEASE_CERTIFICATION_EXECUTION_CLOSURE.md
```

The note must identify exactly these blockers:

- wrong environment gate in the first certification check;
- direct `python3` usage bypassing the pinned oracle interpreter;
- flat observation directory despite split-directory claims;
- absence of a successful full certification run;
- registry publisher state inferred rather than authenticated.

Do not delete the historical hosted run IDs or previous verification record. Label them as valid ordinary smoke evidence but insufficient certification evidence.

## Acceptance criteria

- No active plan claims complete certification closure while implementation is in progress.
- The new plan is identified as the active closure authority.
- Historical evidence remains auditable.
- No unrelated historical plan is rewritten.

---

# Workstream CX1 — Establish one oracle-interpreter contract

## Objective

Make the verified oracle interpreter the only source of pproxy execution during certification.

## Canonical variables

The top-level certification runner owns and exports:

```bash
export EGRESS_ORACLE_PYTHON="$CERT_DIR/oracle-venv/bin/python"
export EGRESS_CANDIDATE_PYTHON="$CERT_DIR/candidate-venv/bin/python"
```

For compatibility with the existing reusable differential helper, either:

### Preferred option

Update `eggress-testkit` to recognize `EGRESS_ORACLE_PYTHON` as the canonical variable and retain `EGRESS_PYTHON_BIN` only as a fallback for standalone legacy use.

Resolution order:

1. `EGRESS_ORACLE_PYTHON`;
2. `EGRESS_PYTHON_BIN`;
3. interpreter discovery only when not running required certification.

### Acceptable transitional option

Export both variables from the top-level runner:

```bash
export EGRESS_ORACLE_PYTHON="$ORACLE_PYTHON"
export EGRESS_PYTHON_BIN="$ORACLE_PYTHON"
```

and still remove direct `python3` use from the test binary.

The preferred option is clearer because it distinguishes the oracle interpreter from a generic Python override.

## Required Rust API

Centralize interpreter resolution in one public helper, for example:

```rust
pub const ORACLE_PYTHON_VAR: &str = "EGRESS_ORACLE_PYTHON";
pub const LEGACY_PYTHON_VAR: &str = "EGRESS_PYTHON_BIN";

pub fn find_oracle_python(require_explicit: bool) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(ORACLE_PYTHON_VAR) {
        return validate_oracle_python(PathBuf::from(path));
    }

    if let Some(path) = std::env::var_os(LEGACY_PYTHON_VAR) {
        return validate_oracle_python(PathBuf::from(path));
    }

    if require_explicit {
        return Err(format!(
            "required certification needs {} to point to pproxy=={}",
            ORACLE_PYTHON_VAR,
            PINNED_PPROXY_VERSION,
        ));
    }

    discover_standalone_python()
}
```

Exact naming may differ. Preserve these semantics.

## Interpreter validation

Validation must prove:

- the path exists;
- the command launches;
- `importlib.metadata.version("pproxy")` returns `2.7.9`;
- the failure diagnostic includes the attempted path;
- no module `__version__` attribute is required.

Suggested command:

```rust
Command::new(&python)
    .args([
        "-c",
        "from importlib.metadata import version; assert version('pproxy') == '2.7.9'",
    ])
```

Use structured error handling rather than `unwrap()` in the shared helper.

## Required tests

Add focused unit or integration tests covering:

1. canonical variable wins over legacy variable;
2. legacy variable works outside strict certification;
3. strict certification rejects missing explicit interpreter;
4. nonexistent interpreter path fails clearly;
5. interpreter without pproxy fails clearly;
6. interpreter with wrong pproxy version fails clearly;
7. interpreter with `pproxy==2.7.9` passes;
8. paths containing spaces remain valid.

Avoid tests that mutate global environment variables concurrently. Use one of:

- a serial test guard already available in the repository;
- a helper taking an explicit environment map;
- process-isolated integration tests.

Do not introduce a new dependency solely for environment-variable serialization unless no existing mechanism is adequate.

## Acceptance criteria

- There is one shared interpreter resolver.
- Certification can require explicit resolution without fallback.
- The resolver verifies the distribution version.
- Direct test code no longer duplicates interpreter discovery.
- Unit/integration tests cover success and failure cases.

---

# Workstream CX2 — Correct the Rust differential gate and process launch

## Objective

Make the first certification check execute the intended tests against the pinned oracle environment.

## Top-level command correction

Change the certification invocation to pass the gate required by the test:

```bash
run_check "pproxy_differential" required env \
    EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
    EGRESS_ORACLE_PYTHON="$ORACLE_PYTHON" \
    EGRESS_PYTHON_BIN="$ORACLE_PYTHON" \
    cargo test -p eggress-cli \
      --test differential_pproxy \
      -- --ignored --test-threads=1
```

If CX1 removes the legacy variable from the certification path, omit `EGRESS_PYTHON_BIN` after all consumers are updated.

Do not set `EGRESS_REQUIRE_PPROXY_DIFFERENTIAL` unless an actual current consumer requires it. Remove that incorrect variable from the script and current documentation if it has no real consumer.

## Test binary correction

In `crates/eggress-cli/tests/differential_pproxy.rs`, replace all direct uses of:

```rust
Command::new("python3")
```

with the shared resolver.

The following functions must use the same resolved path:

- availability validation;
- pproxy import/version validation;
- `start_pproxy_server`;
- `start_pproxy_server_with_auth`;
- any additional pproxy subprocess introduced later.

Resolve once per test or once per process-start helper. Do not discover a different interpreter between prerequisite checks and process startup.

## Prevent zero-test success

The certification runner must not report this check as passing when the test filter executes zero ignored tests.

Acceptable implementations:

### Option A — parse the cargo result

Retain temporary stdout until the check result is classified. Fail the check if the final result indicates zero tests were run.

### Option B — execute an explicit named set

Run known required tests by exact name and record each as a separate check or one controlled command.

### Option C — add a deterministic harness sentinel

Expose a test-list assertion in the test binary and require a minimum scenario count before execution.

Prefer the least complex option. Do not create a manifest generator.

## Failure-injection checks

Prove:

1. wrong gate omitted: test fails before behavior;
2. correct gate present: prerequisite proceeds;
3. explicit oracle path points to wrong version: test fails;
4. system Python has no pproxy but oracle venv is correct: test still passes its prerequisites;
5. system Python has a different pproxy version but oracle venv is correct: oracle venv wins;
6. test invocation cannot pass with zero executed tests.

## Acceptance criteria

- The top-level script sets `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`.
- The incorrect gate is absent from the certification path.
- Every pproxy subprocess in this test uses the pinned interpreter.
- At least one intended ignored test executes.
- Failure diagnostics identify gate or interpreter problems precisely.

---

# Workstream CX3 — Make helper scripts consume existing environments explicitly

## Objective

Prevent nested helpers from silently rebuilding or replacing the environments established by the top-level certification runner.

## `run_strict_pproxy_api.sh` interface

Add explicit certification-mode arguments. A recommended interface is:

```text
--oracle-python PATH
--candidate-python PATH
--oracle-output-dir PATH
--candidate-output-dir PATH
--no-bootstrap
--closure-required
```

Equivalent names are acceptable. Preserve these rules:

- explicit flags override environment variables;
- `--no-bootstrap` requires both interpreters to exist;
- `--no-bootstrap` rejects missing imports;
- `--no-bootstrap` never runs `python3 -m venv`;
- `--no-bootstrap` never invokes `pip install`;
- `--no-bootstrap` never invokes `maturin build`;
- `--no-bootstrap` never writes `target/wheels`;
- `--closure-required` treats missing observations and mismatches as fatal.

The top-level certification command must always use no-bootstrap mode.

## Standalone behavior

A direct developer may still run the helper without no-bootstrap mode. If retained:

- use directories outside `dist/`;
- clean stale wheel outputs before selecting a wheel;
- verify the installed distribution came from the current source tree;
- document that standalone bootstrap is convenience tooling, not release packaging.

A simpler acceptable alternative is to remove automatic wheel bootstrap and require explicit interpreter paths for all uses. Choose this only if all documented standalone workflows are updated.

## Harness driver interpreter

The orchestration script `run_strict_pproxy_api.py` does not itself represent oracle or candidate behavior, but certification should still avoid an unexplained host fallback.

Preferred invocation:

```bash
"$CANDIDATE_PYTHON" "$SCRIPT_DIR/run_strict_pproxy_api.py" ...
```

This ensures its Python dependencies belong to the candidate execution environment. The driver must continue launching oracle probes with `--oracle-python` and candidate probes with `--candidate-python`.

## Interop helper scripts

Update:

```text
scripts/run_strict_pproxy_interop.sh
scripts/compat_udp_pproxy.sh
```

Each must accept or derive the explicit oracle interpreter and launch pproxy with:

```bash
"$EGRESS_ORACLE_PYTHON" -m pproxy ...
```

They must not use:

```bash
python3 -m pproxy
python -m pproxy
pproxy
```

when invoked by certification.

## Bash validation

Run:

```bash
bash -n scripts/run_pproxy_certification.sh
bash -n scripts/run_strict_pproxy_api.sh
bash -n scripts/run_strict_pproxy_interop.sh
bash -n scripts/compat_udp_pproxy.sh
```

Add focused shell-level dry-run checks where practical. Do not add a general shell-testing framework.

## Acceptance criteria

- Certification owns environment creation exactly once.
- Nested helpers consume, rather than replace, those environments.
- No-bootstrap mode fails closed.
- No certification path writes wheel or release artifact directories.
- All pproxy processes use the explicit oracle interpreter.
- All candidate Python probes use the explicit candidate interpreter.

---

# Workstream CX4 — Implement separate observation ownership

## Objective

Make oracle and candidate records physically and logically distinct.

## Canonical directories

Use:

```text
target/pproxy-certification/observations/oracle/
target/pproxy-certification/observations/candidate/
```

The top-level runner should define:

```bash
ORACLE_OBS_DIR="$OBS_DIR/oracle"
CANDIDATE_OBS_DIR="$OBS_DIR/candidate"
mkdir -p "$ORACLE_OBS_DIR" "$CANDIDATE_OBS_DIR"
```

## Paired runner changes

Update the shell and Python paired runners to accept two output directories.

Recommended Python interface:

```text
--oracle-output-dir PATH
--candidate-output-dir PATH
```

Each oracle probe writes only into the oracle directory. Each candidate probe writes only into the candidate directory.

Do not rely on `_oracle.json` and `_candidate.json` suffixes to establish ownership when the directories already encode it. Retaining descriptive suffixes is acceptable but not required.

## Strict comparator invocation

Invoke strict tests with distinct paths:

```bash
"$CANDIDATE_PYTHON" -m pytest python/tests/strict -q \
  --oracle-observations-dir "$ORACLE_OBS_DIR" \
  --candidate-observations-dir "$CANDIDATE_OBS_DIR" \
  --tb=short
```

## Pre-comparison validation

Before running the comparator, validate:

- oracle directory exists;
- candidate directory exists;
- oracle record count is greater than zero;
- candidate record count is greater than zero;
- record ID sets match where one-to-one comparison is required;
- no record appears in both directories through a symlink or copied cross-role path;
- JSON files parse successfully.

Use Python for this validation rather than fragile shell glob counting.

Example validation sketch:

```python
from pathlib import Path
import json

oracle = Path(oracle_dir)
candidate = Path(candidate_dir)

oracle_records = {p.stem: json.loads(p.read_text()) for p in oracle.glob("*.json")}
candidate_records = {p.stem: json.loads(p.read_text()) for p in candidate.glob("*.json")}

if not oracle_records:
    raise SystemExit("no oracle observations produced")
if not candidate_records:
    raise SystemExit("no candidate observations produced")
```

Adapt ID extraction to the repository's actual filenames and schema.

## Summary metadata

Add observation counts to `summary.json`:

```json
{
  "observations": {
    "oracle": 0,
    "candidate": 0,
    "compared": 0
  }
}
```

Use real counts. This makes a zero-observation false pass visible without retaining a large artifact report.

## Failure-injection checks

Prove:

1. missing oracle directory fails;
2. empty oracle directory fails;
3. missing candidate directory fails;
4. empty candidate directory fails;
5. malformed JSON fails;
6. mismatched record IDs fail;
7. valid paired directories proceed;
8. summary counts match actual files.

## Acceptance criteria

- Oracle and candidate output paths are distinct.
- The comparator receives distinct paths.
- Empty or mismatched observations cannot pass.
- Summary counts prove nonzero paired execution.
- No symlink compatibility shim remains necessary.

---

# Workstream CX5 — Add focused contract tests and rejection searches

## Objective

Prevent recurrence without adding broad CI machinery.

## Rust-focused tests

Run and, where needed, add tests for:

```bash
cargo test -p eggress-testkit differential
cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
```

The second command requires the explicit oracle interpreter and correct gate.

Test the interpreter resolver independently from network behavior wherever possible.

## Python-focused tests

Run strict observation-loader tests covering separate directories and malformed/missing records.

Suggested targeted commands should use the candidate environment shape:

```bash
"$CANDIDATE_PYTHON" -m pytest python/tests/strict -q
```

Use the actual required options or fixture setup.

## Required rejection searches

Run from repository root:

```bash
rg -n 'EGRESS_REQUIRE_PPROXY_DIFFERENTIAL' \
  scripts crates docs .skills plans/CI_VERIFICATION_RELEASE_CERTIFICATION_EXECUTION_CLOSURE.md
```

Expected active implementation result: no certification-path consumer.

```bash
rg -n 'Command::new\("python3"\)|Command::new\("python"\)' \
  crates/eggress-cli/tests/differential_pproxy.rs \
  crates/eggress-testkit/src/differential.rs
```

Expected result: no direct hard-coded interpreter in certification-owned pproxy launch code.

```bash
rg -n 'python3 -m pproxy|python -m pproxy|(^|[[:space:]])pproxy([[:space:]]|$)' \
  scripts/run_pproxy_certification.sh \
  scripts/run_strict_pproxy_api.sh \
  scripts/run_strict_pproxy_interop.sh \
  scripts/compat_udp_pproxy.sh
```

Expected result: no certification execution using an implicit interpreter.

```bash
rg -n 'target/wheels|(^|/)dist/' \
  scripts/run_pproxy_certification.sh \
  scripts/run_strict_pproxy_api.sh \
  scripts/run_strict_pproxy_interop.sh \
  scripts/compat_udp_pproxy.sh
```

Expected result: no certification-mode artifact construction. Standalone-only code must be clearly guarded if retained.

```bash
rg -n -- '--oracle-observations-dir.*\$OBS_DIR|--candidate-observations-dir.*\$OBS_DIR' \
  scripts python
```

Expected result: no invocation passing one root as both roles.

## General repository checks

Before the full certification run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

For Python-facing changes:

```bash
python3 -m venv .venv-verification
.venv-verification/bin/python -m pip install --upgrade pip
.venv-verification/bin/python -m pip install \
  "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
VIRTUAL_ENV="$PWD/.venv-verification" \
PATH="$PWD/.venv-verification/bin:$PATH" \
  .venv-verification/bin/maturin develop \
  --manifest-path crates/eggress-python/Cargo.toml
.venv-verification/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv-verification/bin/python -m pytest python/tests tests/compat -q
rm -rf .venv-verification
```

Do not upload or publish anything.

## Acceptance criteria

- Rejection searches are clean or documented with narrow standalone exceptions.
- Rust workspace checks pass.
- Python smoke suite passes when Python code changes.
- Interpreter and observation contract tests cover negative cases.
- No new broad test framework is introduced.

---

# Workstream CX6 — Execute and validate full behavioral certification

## Objective

Produce the missing proof that the actual manual certification command works end to end.

## Clean-run preparation

Before execution:

```bash
rm -rf target/pproxy-certification
rm -rf .venv-oracle-api .venv-candidate-api
rm -rf target/strict/paired_observations target/wheels
```

Do not delete unrelated developer outputs unnecessarily.

Record:

```bash
git rev-parse HEAD
python3 --version
rustc --version
cargo --version
uname -a
```

Record concise values in the plan's final implementation section. Do not create a separate evidence document.

## Required command

Run exactly:

```bash
./scripts/run_pproxy_certification.sh
```

Do not pre-populate system Python with pproxy to make the run pass. The test must succeed because the script-created oracle environment is correctly wired.

## Required success assertions

After exit zero, run:

```bash
python3 - <<'PY'
import json
from pathlib import Path

root = Path("target/pproxy-certification")
summary_path = root / "summary.json"
summary = json.loads(summary_path.read_text())

assert summary["result"] == "pass", summary
assert summary["failed"] == 0, summary
assert summary["passed"] > 0, summary
assert summary["oracle"]["version"] == "2.7.9", summary
assert summary["observations"]["oracle"] > 0, summary
assert summary["observations"]["candidate"] > 0, summary
assert summary["observations"]["compared"] > 0, summary
assert not (root / "tmp").exists()

unexpected_logs = list(root.glob("*.stdout")) + list(root.glob("*.stderr"))
assert not unexpected_logs, unexpected_logs
PY
```

Adapt keys only if the implementation uses an equally explicit schema.

## System-interpreter isolation proof

Run one controlled proof showing the certification path does not depend on system pproxy.

Acceptable method:

1. use a shell environment whose host `python3` can create venvs but cannot import pproxy;
2. confirm `python3 -c 'import pproxy'` fails;
3. run the full certification command;
4. confirm certification succeeds using its oracle venv.

Do not uninstall packages from a shared system environment merely to perform this proof. Use an isolated shell, clean VM, disposable environment, or interpreter that lacks pproxy.

## Wrong-system-version proof

When practical, prove that a different system pproxy version cannot influence the run. Use a disposable environment only.

The required invariant is:

```text
summary oracle interpreter == target/.../oracle-venv/bin/python
summary oracle version == 2.7.9
```

## Failure proof

Perform at least these controlled negative cases without committing the injected changes:

1. alter the expected oracle version check to an impossible value: setup exits before behavioral checks;
2. point the explicit oracle interpreter to a Python without pproxy: first Rust differential test fails clearly;
3. remove candidate observation files after generation: strict comparison fails;
4. force one comparator mismatch: final exit is nonzero and diagnostics remain;
5. verify successful check logs are absent after a clean pass.

Restore the tree and verify:

```bash
git status --short
```

Only intended implementation files may remain changed before commit.

## Evidence record

Update this plan with:

- final implementation SHA;
- certification date;
- command exit status;
- total duration;
- passed/failed/skipped check counts;
- oracle/candidate/compared observation counts;
- exact oracle version;
- summary schema version;
- concise failure-injection outcomes.

Do not commit generated `target/` output.

## Acceptance criteria

- Full certification exits zero from a clean state.
- System pproxy is not required.
- Summary proves nonzero behavioral execution.
- Summary proves oracle version and interpreter ownership.
- Failure cases fail closed.
- Generated outputs remain untracked.

---

# Workstream CX7 — Verify PyPI and TestPyPI publisher settings directly

## Objective

Replace inference with authenticated registry evidence.

## Why direct inspection is required

PyPI supports both:

- trusted publishers attached to existing projects;
- pending trusted publishers configured at account level before a project exists.

Therefore, a public project JSON endpoint returning 404 does not prove that no pending publisher exists.

## PyPI inspection

Using the account intended to own publication:

1. sign in to PyPI;
2. open account publishing settings;
3. inspect pending publishers;
4. check for project names:
   - `eggress`;
   - `eggress-pproxy-compat`;
5. check for repository identity:
   - owner: `eggstack`;
   - repository: `eggress`;
6. check for old workflow filenames such as:
   - `release.yml`;
   - `publish.yml`;
   - `python-publish.yml`;
   - any deleted release workflow;
7. if either project exists, inspect that project's Publishing page for normal trusted publishers;
8. remove stale bindings because the intended path is manual Twine publication;
9. do not create a new publisher.

## TestPyPI inspection

Repeat independently on TestPyPI. Do not assume the production account state applies to TestPyPI.

## What to record

Record only:

- registry name;
- account-level pending publisher count matching these project/repository names;
- project existence state;
- project-level matching publisher count when applicable;
- action taken: none required or removed;
- verification date.

Do not record:

- passwords;
- API tokens;
- recovery codes;
- session cookies;
- complete account inventory unrelated to Eggress;
- screenshots containing sensitive account data.

## Inaccessible settings

If authenticated settings cannot be accessed, set this plan status to:

```text
BLOCKED — REGISTRY SETTINGS ACCESS REQUIRED
```

Repository implementation may still be committed, but the line must not be declared complete.

Do not substitute:

- public JSON endpoint responses;
- workflow-file absence;
- GitHub environment absence;
- GitHub secret absence;
- personal recollection.

## Acceptance criteria

- PyPI pending publishers inspected directly.
- TestPyPI pending publishers inspected directly.
- Existing project publishers inspected where applicable.
- Stale Eggress publishers removed or confirmed absent.
- No sensitive values committed.
- Inaccessibility remains an explicit blocker.

---

# Workstream CX8 — Align documentation and close truthfully

## Objective

Make active guidance describe the execution model that actually passed.

## Documentation updates

Review and update only where necessary:

```text
docs/DIFFERENTIAL_TESTING.md
docs/TESTING.md
docs/CI_STATUS.md
.skills/testing/skill.md
AGENTS.md
```

Current guidance must state:

- ordinary hosted CI remains two smoke workflows;
- certification remains manual and specialist-only;
- `run_pproxy_certification.sh` creates isolated oracle and candidate environments;
- the oracle is pinned to `pproxy==2.7.9`;
- certification-owned pproxy processes use `EGRESS_ORACLE_PYTHON`;
- candidate Python probes use `EGRESS_CANDIDATE_PYTHON`;
- structural tests remain ungated;
- `EGRESS_PPROXY_CERTIFY` and `EGRESS_PPROXY_PLATFORM` are the profile gates;
- `EGRESS_REQUIRE_EXTERNAL_INTEROP` remains the gate for `differential_pproxy.rs`;
- oracle and candidate observations use separate directories;
- certification writes one compact summary plus failure diagnostics;
- certification is not a release gate or CI job;
- publication remains manual.

Do not expose the internal implementation variables as requirements for ordinary developers unless they are invoking lower-level helpers directly.

## Parent-plan status

After CX1 through CX7 are complete, change the parent plan status to:

```text
SUPERSEDED — CERTIFICATION EXECUTION CLOSED BY `plans/CI_VERIFICATION_RELEASE_CERTIFICATION_EXECUTION_CLOSURE.md`
```

Before CX7 is complete, keep it reopened.

## This-plan status

Permitted terminal states:

```text
COMPLETE
```

or:

```text
BLOCKED — REGISTRY SETTINGS ACCESS REQUIRED
```

Do not use `COMPLETE` unless:

- repository checks pass;
- hosted smoke checks pass on the final code commit;
- full manual certification passes;
- summary validation passes;
- PyPI settings are inspected;
- TestPyPI settings are inspected;
- the final matrix below is checked.

## Final implementation record

Record:

- final code SHA;
- final status SHA;
- hosted Rust smoke run ID and result;
- hosted Python smoke run ID and result, when triggered;
- manual certification commit and duration;
- summary check and observation counts;
- registry verification date and non-sensitive result;
- any remaining out-of-scope publication blocker.

## Acceptance criteria

- Active docs agree.
- No document claims public 404 proves publisher absence.
- No document instructs certification to use system pproxy.
- Parent and child statuses are internally consistent.
- Historical evidence is preserved but clearly scoped.
- No separate completion report is added.

---

# 5. Final acceptance matrix

Do not mark these boxes until the corresponding evidence exists.

## Gate and interpreter contract

- [ ] First certification check uses `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`.
- [ ] Incorrect certification gate removed.
- [ ] Canonical oracle interpreter variable defined.
- [ ] Shared Rust interpreter resolver implemented.
- [ ] Direct `python3` launches removed from `differential_pproxy.rs`.
- [ ] pproxy version is validated through distribution metadata.
- [ ] Missing explicit interpreter fails in required certification mode.
- [ ] Wrong oracle version fails before behavior.
- [ ] System pproxy absence does not break certification.
- [ ] Zero-test execution cannot pass.

## Helper ownership

- [ ] Top-level runner creates environments exactly once.
- [ ] Paired API helper supports no-bootstrap certification mode.
- [ ] No-bootstrap mode rejects missing environments.
- [ ] No-bootstrap mode performs no installs or wheel builds.
- [ ] API driver runs through an explicit interpreter.
- [ ] TCP interop launches pproxy through the oracle interpreter.
- [ ] UDP interop launches pproxy through the oracle interpreter.
- [ ] Candidate probes use the candidate interpreter.
- [ ] Certification writes neither `target/wheels` nor `dist/`.

## Observation contract

- [ ] Oracle records use `observations/oracle/`.
- [ ] Candidate records use `observations/candidate/`.
- [ ] Comparator receives distinct directories.
- [ ] Empty oracle output fails.
- [ ] Empty candidate output fails.
- [ ] Malformed record fails.
- [ ] Mismatched record IDs fail.
- [ ] Summary includes nonzero oracle count.
- [ ] Summary includes nonzero candidate count.
- [ ] Summary includes nonzero compared count.

## Repository verification

- [ ] Bash syntax checks pass.
- [ ] Interpreter resolver tests pass.
- [ ] Observation loader tests pass.
- [ ] Rust differential tests pass using explicit oracle interpreter.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] Clippy passes with warnings denied.
- [ ] Workspace tests pass locked.
- [ ] Python smoke suite passes when affected.
- [ ] Rejection searches are clean.
- [ ] Exactly two workflows remain.
- [ ] Workflow permissions remain read-only.
- [ ] No release or certification workflow added.

## End-to-end certification

- [ ] Clean certification run exits zero.
- [ ] Full command ends with `CERTIFICATION PASSED`.
- [ ] Summary JSON parses.
- [ ] Summary result is `pass`.
- [ ] Summary failed count is zero.
- [ ] Summary passed count is nonzero.
- [ ] Summary oracle version is `2.7.9`.
- [ ] Summary identifies oracle interpreter ownership.
- [ ] Summary observation counts are nonzero.
- [ ] Successful logs are absent.
- [ ] Temporary directory is absent.
- [ ] Failure diagnostics are retained only for failed checks.
- [ ] Failure injections fail closed.

## Hosted smoke evidence

- [ ] Rust smoke passes on final implementation commit.
- [ ] Python smoke passes on final implementation commit when triggered.
- [ ] Run IDs and conclusions are recorded.
- [ ] Ordinary smoke evidence is not substituted for certification evidence.

## Registry evidence

- [ ] PyPI account pending publishers inspected.
- [ ] TestPyPI account pending publishers inspected.
- [ ] PyPI project publishers inspected if project exists.
- [ ] TestPyPI project publishers inspected if project exists.
- [ ] Stale Eggress publishers removed or confirmed absent.
- [ ] Public 404 responses are not treated as sufficient proof.
- [ ] No sensitive value recorded.
- [ ] Inaccessible settings remain a blocker.

## Documentation and closure

- [ ] Parent plan reopened during implementation.
- [ ] Active docs describe the final interpreter contract.
- [ ] Active docs describe split observation ownership.
- [ ] Active docs preserve small hosted CI and manual releases.
- [ ] Parent plan given truthful terminal status.
- [ ] This plan records final code SHA.
- [ ] This plan records full certification result.
- [ ] This plan records registry verification result.
- [ ] Every applicable matrix item is checked before `COMPLETE`.
- [ ] Crates.io publication remains out of scope.

---

# 6. Suggested implementation commits

Keep commits narrow enough for a smaller model to validate ownership boundaries independently.

1. `plans: reopen certification execution closure`
   - reopen parent status;
   - link this plan;
   - no code changes.

2. `testkit: centralize pinned pproxy interpreter resolution`
   - shared resolver;
   - exact version validation;
   - focused tests.

3. `test: route differential pproxy processes through oracle interpreter`
   - correct gate;
   - remove direct `python3` usage;
   - prevent zero-test success.

4. `testkit: make certification helpers consume existing environments`
   - no-bootstrap mode;
   - explicit interpreter flags;
   - no wheel artifacts in certification.

5. `testkit: separate oracle and candidate observations`
   - split output directories;
   - pre-comparison validation;
   - summary counts.

6. `docs: align certification execution contract`
   - current docs only;
   - no broad historical rewrites.

7. `plans: close certification execution with verified evidence`
   - only after full certification and registry inspection;
   - record final SHA, run IDs, summary counts, and registry state.

If authenticated registry settings remain inaccessible, use:

```text
plans: record registry-settings closure blocker
```

and leave this plan blocked rather than complete.

---

# 7. Handoff guidance for a smaller implementation model

- Read this plan, the parent plan, the current certification script, the direct Rust differential test, and the paired API helper before editing.
- Do not trust the current `COMPLETE` label; verify behavior from code and execution.
- Fix the wrong gate first, but do not stop there. The explicit interpreter is the deeper correctness issue.
- Use one shared Rust interpreter resolver. Do not copy environment lookup into each test helper.
- During required certification, do not fall back to system Python.
- Preserve standalone developer convenience only when it does not weaken certification mode.
- Do not install pproxy into system Python to make tests green.
- Do not add pproxy to GitHub Actions.
- Do not add a certification workflow.
- Do not reintroduce pull-request CI triggers.
- Do not create release artifacts from the certification path.
- Keep oracle and candidate observations in separate directories.
- Fail on empty observations; a comparator with no inputs is not evidence.
- Ensure the first cargo check runs real ignored tests, not zero tests.
- Run the complete certification command from a clean state before claiming closure.
- Validate `summary.json` programmatically.
- Public package-index 404 responses do not prove that pending publishers are absent.
- Inspect PyPI and TestPyPI account-level publishing settings directly.
- Never commit credentials, tokens, cookies, or sensitive screenshots.
- If registry access is unavailable, leave a truthful blocker.
- Keep the two-workflow, read-only, manual-release architecture unchanged.
- Do not touch crates.io publication architecture in this pass.
- Mark completion last.
