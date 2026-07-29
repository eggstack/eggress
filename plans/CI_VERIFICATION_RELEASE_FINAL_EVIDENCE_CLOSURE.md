# CI, Verification, and Release — Final Evidence Closure Pass

## Status

**READY FOR IMPLEMENTATION**

## Baseline

- Repository: `eggstack/eggress`
- Baseline branch: `main`
- Baseline commit: `22ff2d95e5025916b32348e54b70d493f5b8afd7`
- Original reductive plan: `plans/CI_VERIFICATION_RELEASE_REDUCTIVE_FOLLOW_UP.md`
- First corrective plan: `plans/CI_VERIFICATION_RELEASE_CORRECTIVE_CLOSURE.md`
- Main corrective implementation: `8d4a7ba095267debb3088a8922b502a1cec0ab9f`
- Workflow-dispatch follow-up: `c645b717d90db0c26512b1fa354b926e2e1055d3`
- Incorrect closure declaration: `22ff2d95e5025916b32348e54b70d493f5b8afd7`

## Purpose

Close the remaining correctness and evidence gaps after the major CI/release simplification landed.

The previous implementation achieved the intended large-scale reduction:

- only two GitHub Actions workflows remain;
- automated package publication and GitHub Release creation remain absent;
- workflow permissions remain read-only;
- Python and Rust publication are documented as manual operator actions;
- the compatibility runner no longer duplicates formatting, Clippy, full workspace tests, dependency audits, release wheel checks, JUnit output, or Markdown evidence generation;
- PyPI repair guidance now uses immutable roll-forward semantics;
- the old CI-tier names were partially converted to certification-profile terminology.

The line of work is not actually closed. The prior closure record marked several criteria complete when the current code or hosted evidence directly contradicts them. This plan is a narrow final pass to correct those specific defects without rebuilding the deleted verification platform.

---

# 1. Confirmed residual defects

Implementation must begin from the current tree rather than from the prior completion claims.

## 1.1 Certification environments are not isolated

Current `scripts/run_pproxy_certification.sh`:

- invokes host `python3` directly;
- imports whichever `pproxy` distribution is visible to that interpreter;
- runs candidate Python tests in the same host environment;
- does not create a dedicated oracle virtual environment;
- does not create a dedicated candidate virtual environment;
- does not install the local PyO3 extension and compatibility package into a clean candidate environment;
- allows stale or unrelated installed packages to influence certification.

The prior closure plan nevertheless checks “Candidate environment is isolated.” That statement is false at the baseline.

## 1.2 Oracle setup does not fail before comparisons

The current generic `run_check` function records setup failure and continues. If pproxy is absent or the version is wrong, later paired and differential stages still execute.

The intended invariant is stronger:

> Oracle and candidate environment construction must succeed before any behavioral comparison begins.

A wrong oracle version is not an ordinary aggregated behavioral failure. It invalidates the test environment and must terminate certification immediately.

## 1.3 Successful per-check logs are retained

The current runner creates `.stdout` and `.stderr` files for every check. Failed output is copied into `failures/`, but successful temporary files are not removed.

This contradicts:

- the script header, which promises failure diagnostics only;
- the reductive requirement to avoid a large success artifact tree;
- the prior checked closure item claiming selective diagnostics.

## 1.4 Profile reduction is incomplete

The current code renamed CI tiers but retained the same three-way model:

- `Structural`;
- `Differential`;
- `Platform`.

It also retained the legacy gate variables:

- `EGRESS_ORACLE`;
- `EGRESS_ORACLE_EXTENDED`;
- `EGRESS_ORACLE_PLATFORM`.

Current documentation still describes Structural as a gated certification profile. This contradicts the explicit objective that structural tests run as ordinary ungated tests and that only behavior-dependent profiles remain.

## 1.5 Python smoke has no successful hosted proof

The recorded final Python workflow run completed with a failed test:

- `test_socks5_relay_multiple_chunks` returned empty relayed data;
- the workflow conclusion was `failure`;
- the prior closure document reclassified this as a harmless pre-existing flake and still checked “Python smoke succeeds.”

A failing smoke job is not successful evidence. The root cause must be corrected or the smoke selection must be changed only after proving that the failing test is outside the intended smoke contract. A mere waiver is not closure.

## 1.6 Trusted-publisher state was explicitly not verified

The previous plan required PyPI/TestPyPI trusted-publisher state to be inspected or recorded as a blocker. The closure commit instead declared the item complete while stating that verification was intentionally not performed.

Manual Twine publication makes trusted publishing unnecessary, but it does not prove that stale trusted-publisher bindings are absent. External state must be checked from the registry project settings or closure must remain blocked.

## 1.7 Trigger duplication remains

Both workflows currently run on:

- pull requests to `main`;
- pushes to `main`;
- manual dispatch.

Because `main` is unprotected and direct pushes are the stated normal path, automatic PR plus post-merge push execution is unnecessary complexity. The final trigger model should be simplified to match actual repository practice.

## 1.8 Closure status is not truthful

`plans/CI_VERIFICATION_RELEASE_CORRECTIVE_CLOSURE.md` is marked `COMPLETE`, with every checkbox selected, despite the contradictions above.

The status and evidence record must be corrected before this line can be closed.

---

# 2. Required final state

This pass is complete only when all of the following are true.

1. `scripts/run_pproxy_certification.sh` creates and uses separate oracle and candidate environments under `target/pproxy-certification/`.
2. The oracle environment contains exactly the pinned pproxy distribution required by repository provenance.
3. The candidate environment contains the locally built `eggress` extension and local `eggress-pproxy-compat` package, not a previously installed copy.
4. Oracle and candidate setup failures terminate before behavioral checks begin.
5. Oracle version verification uses distribution metadata and confirms `pproxy==2.7.9`.
6. The runner never installs packages into system Python or a user-level Python environment.
7. The runner never writes compatibility artifacts to `dist/`.
8. Successful checks leave no per-check stdout/stderr files.
9. Failed checks retain concise stdout/stderr diagnostics under `target/pproxy-certification/failures/`.
10. `summary.json` remains the only required success artifact.
11. Summary JSON is generated through a JSON encoder rather than unsafe shell interpolation.
12. Required behavioral failures propagate to a nonzero final exit status.
13. Wrong oracle version, failed candidate import, or failed environment creation exits immediately.
14. Structural tests run without a certification gate.
15. `CertificationProfile` contains only `Differential` and `Platform`, or the profile abstraction is removed entirely if simpler.
16. Structural scenarios use `None`/ordinary test ownership rather than a `Structural` profile.
17. `EGRESS_ORACLE`, `EGRESS_ORACLE_EXTENDED`, and `EGRESS_ORACLE_PLATFORM` are removed from active implementation and current operational documentation.
18. The canonical manual gates become `EGRESS_PPROXY_CERTIFY=1` and `EGRESS_PPROXY_PLATFORM=1`, unless a simpler no-profile interface is implemented.
19. Existing specialized interop environment variables may remain only where they gate a distinct external tool or behavior suite.
20. The Python smoke workflow completes successfully on its final definition.
21. The `test_socks5_relay_multiple_chunks` failure is root-caused and corrected without blanket retries, `xfail`, unconditional sleeps, or test deletion.
22. The targeted flaky test passes repeatedly in a deterministic local stress loop.
23. The full Python smoke suite passes locally in the same environment shape used by GitHub Actions.
24. A hosted Python smoke run on the final commit concludes `success`.
25. A hosted Rust smoke run on the final commit concludes `success`.
26. The workflow trigger model is automatic `push` to `main` plus `workflow_dispatch`; automatic `pull_request` triggers are removed unless branch protection is introduced in the same pass with a documented reason.
27. Exactly two workflow files remain.
28. Neither workflow publishes, uploads release artifacts, runs compatibility certification, or requires release credentials.
29. GitHub Actions default workflow permissions remain read-only.
30. Branch protection/rulesets remain accurately documented.
31. Repository publishing environments and publishing secrets remain absent.
32. PyPI and TestPyPI trusted-publisher bindings are inspected directly and confirmed absent or removed.
33. If registry settings cannot be inspected, this plan remains blocked rather than complete.
34. The prior corrective plan is marked reopened/superseded rather than complete.
35. This plan is marked complete only after all repository, hosted CI, and registry evidence exists.
36. No new workflow matrix, release framework, evidence uploader, generated completion report, or orchestration layer is added.
37. Crates.io publication architecture remains out of scope and truthfully blocked.
38. The final process-code diff is net-negative or near-neutral.

---

# 3. Scope

## In scope

- `scripts/run_pproxy_certification.sh`;
- compatibility-specific helper scripts that currently hard-code `python3` or shared environments;
- `scripts/run_strict_pproxy_api.sh`;
- `scripts/run_strict_pproxy_api.py`;
- `scripts/run_strict_pproxy_interop.sh`;
- `scripts/compat_udp_pproxy.sh`;
- Python strict/differential test invocation plumbing;
- `crates/eggress-testkit/src/oracle/profile.rs`;
- `crates/eggress-testkit/src/oracle/mod.rs`;
- `crates/eggress-testkit/src/oracle/report.rs`;
- `crates/eggress-cli/tests/oracle.rs`;
- tests and serializers affected by profile changes;
- `.github/workflows/ci.yml`;
- `.github/workflows/python-test.yml`;
- root cause and deterministic correction for `test_socks5_relay_multiple_chunks`;
- `docs/CI_STATUS.md`;
- `docs/TESTING.md`;
- `docs/DIFFERENTIAL_TESTING.md`;
- `AGENTS.md`;
- `.skills/testing/skill.md`;
- `plans/CI_VERIFICATION_RELEASE_CORRECTIVE_CLOSURE.md`;
- this final plan;
- GitHub branch/ruleset, Actions permission, environment, and secret inspection;
- PyPI and TestPyPI trusted-publisher inspection.

## Out of scope

Do not:

- add new proxy capabilities;
- weaken compatibility expectations;
- remove a failing behavioral test without proving it is redundant and replacing its coverage;
- label the Python failure “flaky” and stop there;
- add retries around the full Python workflow;
- add `pytest-rerunfailures`;
- add unconditional sleeps as a race workaround;
- mark the failing test `xfail` or skip it on GitHub Actions;
- split the Python suite into a complex matrix;
- restore cross-platform CI;
- add release workflows;
- add package publication credentials to GitHub;
- restructure crates for crates.io publication;
- publish to crates.io, PyPI, or TestPyPI;
- add an `xtask`, task runner, or release manager;
- create a persistent evidence archive;
- upload certification artifacts from CI;
- introduce Docker solely for certification;
- rewrite historical plans unrelated to this closure line;
- change branch protection unless the user explicitly decides to adopt protected-main PR development.

---

# 4. Execution order

Implement in this exact order:

1. FE0 — reopen the prior false closure status;
2. FE1 — define interpreter and environment contracts;
3. FE2 — implement isolated fail-fast certification setup;
4. FE3 — make success output genuinely compact;
5. FE4 — remove the Structural profile and legacy oracle gates;
6. FE5 — root-cause and correct the Python smoke failure;
7. FE6 — simplify workflow triggers and obtain hosted proof;
8. FE7 — verify external repository and registry settings;
9. FE8 — align active documentation;
10. FE9 — run final rejection searches and close status truthfully.

Do not mark any plan complete before FE6 and FE7 are complete.

---

# Workstream FE0 — Reopen the false closure record

## Objective

Prevent future implementers from treating the current `COMPLETE` status as authoritative.

## Required change

At the beginning of implementation, update:

```text
plans/CI_VERIFICATION_RELEASE_CORRECTIVE_CLOSURE.md
```

Change its status from:

```text
COMPLETE
```

to:

```text
REOPENED — FINAL EVIDENCE CLOSURE REQUIRED
```

Add a short note linking to:

```text
plans/CI_VERIFICATION_RELEASE_FINAL_EVIDENCE_CLOSURE.md
```

The note must state that closure was reopened because:

- certification environments were not isolated;
- successful logs were retained;
- the Structural profile and old gates remained;
- the only final Python hosted proof run failed;
- trusted-publisher state was not inspected.

Do not rewrite the old implementation-results section yet. Preserve it as historical evidence of what was claimed at that time.

## Acceptance criteria

- No current plan claims this line is complete while final work is in progress.
- The new plan is the only active closure authority.
- Historical run IDs and prior claims remain visible for auditability but are clearly labeled insufficient.

---

# Workstream FE1 — Define explicit interpreter and environment contracts

## Objective

Make every compatibility subprocess run through an explicit interpreter belonging to either the oracle or candidate environment.

## Canonical directories

Use:

```text
target/pproxy-certification/
  oracle-venv/
  candidate-venv/
  observations/
    oracle/
    candidate/
  failures/
  tmp/
  summary.json
```

The certification runner may create additional temporary files only under `tmp/`. Successful completion must remove `tmp/`.

## Canonical interpreter variables

The top-level runner should export explicit absolute paths:

```bash
EGRESS_ORACLE_PYTHON="$CERT_DIR/oracle-venv/bin/python"
EGRESS_CANDIDATE_PYTHON="$CERT_DIR/candidate-venv/bin/python"
```

On Windows support is not required for this shell script. On macOS/Linux, use the venv layout above.

Helper scripts must not silently fall back to host `python3` when these variables are set.

Preferred helper contract:

```bash
: "${EGRESS_ORACLE_PYTHON:?EGRESS_ORACLE_PYTHON is required}"
: "${EGRESS_CANDIDATE_PYTHON:?EGRESS_CANDIDATE_PYTHON is required}"
```

For helper scripts that are also useful standalone, support explicit flags:

```text
--oracle-python PATH
--candidate-python PATH
```

Environment variables may supply defaults, but an explicit flag wins.

## Required call-site inventory

Before editing, run:

```bash
rg -n \
  "python3|python -m|pip|pproxy|run_strict_pproxy_api|run_strict_pproxy_interop|compat_udp_pproxy" \
  scripts python/tests tests/compat crates/eggress-cli/tests
```

Classify each Python invocation as:

- oracle;
- candidate;
- build/setup;
- ordinary non-certification tooling.

Only certification-path invocations need interpreter plumbing.

## Acceptance criteria

- Every certification Python process has an explicit environment owner.
- Oracle code never imports the local compatibility package.
- Candidate code never resolves canonical pproxy from the oracle venv.
- No certification helper chooses an interpreter based on ambient PATH after explicit variables are supplied.
- Standalone helper behavior is documented and tested.

---

# Workstream FE2 — Implement isolated, fail-fast certification setup

## Objective

Create clean oracle and candidate environments before any comparison begins.

## Primary file

- `scripts/run_pproxy_certification.sh`

## Step 1 — Preflight

Check only host tools that cannot reasonably live in the virtual environments:

```text
git
cargo
rustc
python3
```

Require Python 3.11 or 3.12 because canonical pproxy support is documented for those versions.

Use a concise check equivalent to:

```bash
python3 - <<'PY'
import sys
if sys.version_info[:2] not in {(3, 11), (3, 12)}:
    raise SystemExit(
        f"pproxy certification requires Python 3.11 or 3.12; got {sys.version.split()[0]}"
    )
PY
```

Do not install a different interpreter automatically.

A missing host tool or unsupported Python version must terminate immediately before the check runner is initialized.

## Step 2 — Clean only the certification directory

```bash
rm -rf target/pproxy-certification
mkdir -p \
  target/pproxy-certification/observations/oracle \
  target/pproxy-certification/observations/candidate \
  target/pproxy-certification/failures \
  target/pproxy-certification/tmp
```

Do not remove `.venv`, `dist/`, unrelated `target/` subdirectories, or user environments.

## Step 3 — Create oracle environment

```bash
python3 -m venv "$CERT_DIR/oracle-venv"
"$EGRESS_ORACLE_PYTHON" -m pip install --upgrade pip
"$EGRESS_ORACLE_PYTHON" -m pip install -r tests/compat/requirements-oracle.txt
```

Use the repository’s canonical requirements/provenance file. If the existing file is not named exactly as above, use its current canonical path rather than creating duplicate provenance.

The requirements source must pin:

```text
pproxy==2.7.9
```

Verify using package metadata rather than module attributes:

```bash
"$EGRESS_ORACLE_PYTHON" - <<'PY'
from importlib.metadata import version
actual = version("pproxy")
expected = "2.7.9"
if actual != expected:
    raise SystemExit(f"expected pproxy=={expected}, got {actual}")
import pproxy
print(actual)
PY
```

A failure here must terminate the entire script immediately.

## Step 4 — Create candidate environment

```bash
python3 -m venv "$CERT_DIR/candidate-venv"
"$EGRESS_CANDIDATE_PYTHON" -m pip install --upgrade pip
"$EGRESS_CANDIDATE_PYTHON" -m pip install \
  "maturin>=1.0,<2.0" \
  pytest \
  "pytest-asyncio>=0.23,<1" \
  "cryptography>=42,<47"
```

Install the native extension into the candidate environment:

```bash
VIRTUAL_ENV="$CERT_DIR/candidate-venv" \
PATH="$CERT_DIR/candidate-venv/bin:$PATH" \
  "$CERT_DIR/candidate-venv/bin/maturin" develop \
  --manifest-path crates/eggress-python/Cargo.toml
```

Install the local compatibility package without producing a retained release wheel:

```bash
"$EGRESS_CANDIDATE_PYTHON" -m pip install --no-deps ./python-pproxy-compat
```

Verify package origin:

```bash
"$EGRESS_CANDIDATE_PYTHON" - <<'PY'
from pathlib import Path
import eggress
import pproxy
root = Path.cwd().resolve()
print(Path(eggress.__file__).resolve())
print(Path(pproxy.__file__).resolve())
PY
```

Add a stronger assertion appropriate to the installed layout so stale global packages cannot pass. Do not require imports to resolve directly from the source tree if `maturin develop` correctly installs into the candidate venv.

A candidate build, installation, or import failure must terminate before paired comparison begins.

## Step 5 — Keep setup outside aggregated behavioral checks

Implement dedicated fatal helpers:

```bash
fatal_step "create oracle environment" command ...
fatal_step "verify oracle version" command ...
fatal_step "create candidate environment" command ...
fatal_step "verify candidate imports" command ...
```

`fatal_step` must exit immediately on failure.

The ordinary `run_check` aggregator starts only after both environments are valid.

## Step 6 — Pass interpreters to helpers

All retained paired/differential runners must receive:

```bash
export EGRESS_ORACLE_PYTHON
export EGRESS_CANDIDATE_PYTHON
export EGRESS_ORACLE_OBSERVATIONS_DIR="$CERT_DIR/observations/oracle"
export EGRESS_CANDIDATE_OBSERVATIONS_DIR="$CERT_DIR/observations/candidate"
```

Do not symlink old `target/strict/paired_observations` output into certification. Change the paired runner to write directly to the canonical certification directories.

## Step 7 — Distinguish ordinary tests from certification checks

Remove from the certification script any check that does not actually compare or validate a pproxy compatibility claim.

Re-evaluate these current gates:

```text
strict_manifest_tests
runtime_examples
runtime_failure_cleanup
resource_leak_check
```

Retain one only if the implementation adds a concise comment mapping it to:

- a named pproxy compatibility manifest capability;
- paired oracle/candidate behavior;
- an observable process lifecycle contract required for pproxy compatibility.

Otherwise, leave it in ordinary local/CI testing and remove it from certification.

Expected core certification categories:

1. paired public Python API observations;
2. strict comparator over separate oracle and candidate observations;
3. Rust differential proxy cases against canonical pproxy;
4. required TCP interoperability;
5. required UDP interoperability for supported UDP claims;
6. directly mapped cipher/plugin compatibility probes;
7. directly mapped process-lifecycle compatibility probes.

The final gate count should be driven by behavior ownership, not by preserving the current count of twelve.

## Acceptance criteria

- Two fresh venvs are created on every run.
- `pproxy==2.7.9` exists only in the oracle environment unless a candidate dependency independently requires it, which should be treated as an error for the compatibility package.
- The candidate environment contains the current local extension and compatibility package.
- Host/site/user packages cannot satisfy certification imports.
- Setup failures stop before comparisons.
- Observation directories are distinct.
- No symlink to legacy observation output is required.
- No package is installed globally or with `--user`.
- No release wheel is written to `dist/`.

---

# Workstream FE3 — Make success output genuinely compact

## Objective

Retain detailed diagnostics only for failed behavioral checks.

## Required `run_check` behavior

For each check:

1. create temporary stdout/stderr paths under `target/pproxy-certification/tmp/`;
2. execute the command;
3. on success:
   - update the in-memory summary record;
   - delete both temporary files;
4. on required failure:
   - move non-empty stdout/stderr into `failures/`;
   - update the summary record;
   - continue to other independent behavioral checks if aggregation is useful;
5. on a technically inapplicable optional platform check:
   - record `skip` with a structured reason;
   - delete temporary files unless they contain a diagnostic needed to explain the skip.

After summary generation, remove `tmp/` on both pass and fail.

## Safe summary generation

Do not construct JSON by concatenating arbitrary check names or messages in shell.

Preferred implementation:

- write tab-separated or NUL-delimited internal records to a temporary file;
- invoke the candidate or host Python interpreter with a small inline script using `json.dump`;
- serialize commit, versions, paths, check results, durations, and skip reasons through Python’s JSON encoder.

Required summary shape:

```json
{
  "schema_version": 2,
  "commit": "<full sha>",
  "profile": "differential",
  "oracle": {
    "distribution": "pproxy",
    "version": "2.7.9",
    "python": "3.12.x",
    "interpreter": "target/pproxy-certification/oracle-venv/bin/python"
  },
  "candidate": {
    "python": "3.12.x",
    "interpreter": "target/pproxy-certification/candidate-venv/bin/python"
  },
  "result": "pass",
  "passed": 0,
  "failed": 0,
  "skipped": 0,
  "elapsed_ms": 0,
  "checks": []
}
```

Use repository-relative interpreter paths in the JSON rather than machine-specific absolute paths.

## Required filesystem assertions

Add a shell-level or Python test that asserts after a successful synthetic run:

```text
target/pproxy-certification/summary.json exists
failures/ is empty
tmp/ does not exist
no check_*.stdout exists
no check_*.stderr exists
no Markdown file exists
no JUnit XML exists
```

For a synthetic failed check, assert:

```text
summary.json exists
result == fail
failed >= 1
failures/<check>.stderr or stdout exists
tmp/ does not exist
```

## Acceptance criteria

- Successful checks leave no logs.
- Failed checks retain only relevant diagnostics.
- Summary JSON is always valid JSON even if a check name or message contains punctuation.
- Summary commit equals `git rev-parse HEAD`.
- Summary result agrees with process exit status.
- No Markdown, JUnit, hash index, or artifact bundle returns.

---

# Workstream FE4 — Remove Structural profile and legacy gates

## Objective

Complete the semantic reduction rather than merely renaming the old CI tiers.

## Target files

- `crates/eggress-testkit/src/oracle/profile.rs`;
- `crates/eggress-testkit/src/oracle/report.rs`;
- `crates/eggress-testkit/src/oracle/mod.rs`;
- `crates/eggress-cli/tests/oracle.rs`;
- direct tests and fixtures;
- current testing documentation.

## Required data model

Preferred enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificationProfile {
    Differential,
    Platform,
}
```

Structural scenarios are ordinary tests and therefore have:

```rust
certification_profile: None
```

Preferred classification function:

```rust
pub fn certification_profile(
    scenario: &OracleScenario,
) -> Option<CertificationProfile> {
    if scenario.platform.requires_root || scenario.platform.required_os.is_some() {
        return Some(CertificationProfile::Platform);
    }

    if scenario.requires_external_oracle() {
        return Some(CertificationProfile::Differential);
    }

    None
}
```

Use an existing explicit scenario field if available. Do not infer external-oracle ownership solely from duration or an `ext.` identifier when a clearer property can be represented.

## Required deletions

Remove:

```text
CertificationProfile::Structural
STRUCTURAL_GATE
EGRESS_ORACLE
EGRESS_ORACLE_EXTENDED
EGRESS_ORACLE_PLATFORM
all_profile_configs() entries for Structural
summary text calling Structural a profile
profile tests asserting Structural gating
```

Replace active manual gates with:

```text
EGRESS_PPROXY_CERTIFY=1
EGRESS_PPROXY_PLATFORM=1
```

Rules:

- ordinary structural/schema/unit tests run with no environment variable;
- differential oracle tests require `EGRESS_PPROXY_CERTIFY=1`;
- platform-specific tests require both certification and platform selection, or a single clearly documented platform gate;
- specialized external-tool gates such as Shadowsocks may remain separate;
- no alias for old variables is needed unless an actual external consumer is identified.

## Integration test updates

Change `crates/eggress-cli/tests/oracle.rs` from:

```text
EGRESS_ORACLE=1
```

to the new canonical certification gate.

Update:

- module documentation;
- panic messages;
- constants;
- test names;
- examples;
- report round-trip fixtures;
- JSON golden data if checked in.

## Required tests

1. Structural scenario classification returns `None`.
2. Differential scenarios return `Some(Differential)`.
3. Root/OS-specific scenarios return `Some(Platform)`.
4. Every scenario appears once in classification output.
5. No structural test checks an environment variable.
6. Differential tests remain ignored or explicitly gated when external pproxy is required.
7. Serialization emits only `differential` or `platform` when a profile is present.
8. Old serialized `ci_tier` does not reappear.
9. If backward reading of `ci_tier` was already supported and externally useful, preserve only a deserialization alias; do not emit it.

## Rejection searches

```bash
! rg -n \
  "CertificationProfile::Structural|STRUCTURAL_GATE|EGRESS_ORACLE_EXTENDED|EGRESS_ORACLE_PLATFORM" \
  crates scripts docs AGENTS.md .skills

! rg -n \
  "EGRESS_ORACLE=1|gate: `EGRESS_ORACLE|3-profile|Structural.*profile" \
  crates scripts docs AGENTS.md .skills
```

Historical plan text may retain old names when clearly describing the former state.

## Acceptance criteria

- There are at most two certification profiles.
- Structural tests are ungated in code, not merely described as ungated.
- Old gate variables are absent from active code and current guidance.
- Profile abstraction is smaller than the current implementation.
- No workflow is added to run the profiles automatically.

---

# Workstream FE5 — Root-cause and correct the Python smoke failure

## Objective

Turn Python smoke from a merely executable workflow into a reliable passing signal.

## Known symptom

The final recorded hosted run failed in:

```text
test_socks5_relay_multiple_chunks
```

Observed symptom:

```text
expected relayed bytes; received empty bytes
```

Do not assume this is only a test problem. Empty relay output may indicate a real shutdown, half-close, scheduling, buffering, or task-lifecycle defect.

## Step 1 — Locate and reproduce

Use the hosted job log to identify the exact file and assertion.

Run the exact test locally in the same environment shape as CI:

```bash
rm -rf .venv
git clean -fX python/eggress
python3.12 -m venv .venv
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install \
  "maturin>=1.0,<2.0" \
  pytest \
  "pytest-asyncio>=0.23,<1" \
  "cryptography>=42,<47"
(
  cd crates/eggress-python
  ../../.venv/bin/maturin develop
)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest <exact-file>::test_socks5_relay_multiple_chunks -vv -s
```

Then stress it without adding a retry plugin:

```bash
for i in $(seq 1 25); do
  echo "iteration $i"
  .venv/bin/python -m pytest \
    <exact-file>::test_socks5_relay_multiple_chunks \
    -q || exit 1
done
```

If the failure does not reproduce, inspect the hosted logs for timing and runner-specific behavior and add temporary local diagnostics. Do not commit noisy diagnostics after root cause is known.

## Step 2 — Classify the defect

Determine whether the failure is caused by:

- production relay shutdown before buffered data flushes;
- candidate server task cancellation before client read completion;
- incorrect half-close propagation;
- test server exiting before all chunks are written;
- client calling shutdown and assuming immediate relay completion;
- race in port readiness or listener startup;
- test cleanup terminating the runtime prematurely;
- Python binding lifecycle mismatch;
- a genuinely nondeterministic assertion unrelated to product behavior.

Record the classification in the implementation PR summary.

## Step 3 — Correct deterministically

### If production code is wrong

Fix the relay/lifecycle implementation so that:

- all accepted bytes are forwarded before shutdown completion;
- half-close semantics are explicit;
- cancellation does not discard buffered payload;
- task ownership waits for the appropriate relay direction;
- errors propagate rather than silently producing empty output.

Add or strengthen a Rust-level regression test as close to the faulty subsystem as practical.

### If the test harness is wrong

Replace race-prone timing with deterministic synchronization:

- listener-ready channel/event;
- explicit “all chunks written” signal;
- bounded wait for relay completion;
- joining the producer/server task before teardown;
- exact byte-count read rather than timing-dependent `read_to_end` when appropriate.

Do not solve with:

```text
sleep(...)
pytest reruns
xfail
skip on CI
catch-and-ignore empty result
increased global timeout only
```

A small bounded timeout remains appropriate around deterministic events to prevent hangs.

## Step 4 — Prove stability

Required local evidence:

```bash
# Targeted repetition
for i in $(seq 1 25); do
  .venv/bin/python -m pytest \
    <exact-file>::test_socks5_relay_multiple_chunks -q || exit 1
done

# Related module/file
.venv/bin/python -m pytest <exact-file> -q

# Full smoke selection
.venv/bin/python -m pytest python/tests tests/compat -q
```

If the fix changes Rust networking behavior, also run the closest Rust package and integration tests.

## Step 5 — Keep workflow simple

Do not add workflow retries, conditional reruns, test splitting, or flake suppression. The current single Python job remains the preferred shape.

The compatibility package may be installed directly:

```bash
pip install --no-deps ./python-pproxy-compat
```

Replace the temporary wheel step only if direct installation is verified reliable in GitHub Actions. This cleanup is optional and must not block the central smoke fix.

## Acceptance criteria

- Root cause is identified, not merely guessed.
- Targeted test passes 25 consecutive local runs.
- Related tests pass.
- Full Python smoke suite passes locally.
- No retry plugin or `xfail` is added.
- No unconditional sleep is used as the sole synchronization mechanism.
- Hosted Python smoke concludes `success` on the final workflow definition.

---

# Workstream FE6 — Simplify workflow triggers and obtain final hosted proof

## Objective

Remove automatic duplication while preserving a simple post-push smoke signal and manual verification capability.

## Final trigger model

Because the repository’s documented normal path is direct push to an unprotected `main`, use:

```yaml
on:
  push:
    branches: [main]
  workflow_dispatch:
```

Apply this to both workflows.

For Python smoke, retain the current `paths` filter under `push`.

Remove `pull_request` triggers from both workflows.

This yields:

- one automatic verification after a main update;
- manual on-demand verification for branches or release preparation;
- no PR-plus-post-merge duplicate automatic runs;
- no requirement to invent branch-protection policy for a small repository.

If implementation discovers that protected PR-only development has been adopted after this plan was written, stop and update the plan/PR summary before selecting a different model. Do not retain both trigger paths by inertia.

## Workflow invariants

Confirm both workflows still have:

```yaml
permissions:
  contents: read
```

Confirm absence of:

- matrices;
- artifact upload;
- package permissions;
- release environments;
- certification scripts;
- dependency audits;
- tag triggers;
- release commands.

## Required hosted proof

After the smoke defect is fixed and trigger changes are final, push the implementation commit to a branch and use `workflow_dispatch` for pre-merge proof if needed.

After merge/push to `main`, record final successful runs for the same final commit or its direct merge commit:

- `Rust smoke`: `success`;
- `Python 3.12 smoke`: `success`.

Record:

- run URL;
- commit SHA;
- conclusion;
- duration.

Store these only in the implementation PR description or final plan status section. Do not add a new evidence file.

## Runtime budgets

- Rust target: under 8 minutes; hard timeout 20 minutes.
- Python target: under 10 minutes; hard timeout 20 minutes.

A single final successful run is sufficient. Use existing recent successful runs only for context; do not generate repeated hosted runs solely for statistics.

## Acceptance criteria

- Exactly two workflows remain.
- Automatic triggers are push-to-main only.
- Manual dispatch remains available.
- Python paths remain scoped.
- Both final hosted runs pass.
- No release or certification activity occurs in CI.
- No automatic duplicate PR/post-merge model remains.

---

# Workstream FE7 — Verify repository and registry settings

## Objective

Complete the external-state evidence that repository files cannot prove.

## GitHub repository settings

Re-check at implementation completion, even though prior results reported:

- unprotected `main`;
- no required checks;
- read-only default Actions permissions;
- zero environments;
- zero repository Actions secrets.

Use GitHub settings or `gh api` with sufficient permissions.

Required checks:

```bash
gh api repos/eggstack/eggress/branches/main/protection
# Expected: 404 while main remains intentionally unprotected.

gh api repos/eggstack/eggress/actions/permissions
# Expected: default_workflow_permissions == "read".

gh api repos/eggstack/eggress/environments
# Expected: total_count == 0 unless a retained non-publishing environment has a documented reason.

gh api repos/eggstack/eggress/actions/secrets
# Expected: no repository release/publishing secrets.
```

Also inspect repository variables if available for stale release names.

Record names/counts only. Never print secret values.

## PyPI trusted publishers

Inspect the project settings for both published Python projects when they exist:

```text
eggress
eggress-pproxy-compat
```

Inspect both:

- PyPI;
- TestPyPI, if projects exist there.

For each project, confirm one of:

- no trusted publishers configured;
- stale GitHub publisher removed;
- project does not exist;
- access unavailable.

Because release policy is local `twine upload`, the desired state is no GitHub Actions trusted-publisher binding.

If access is unavailable:

- record `BLOCKED — REGISTRY SETTINGS ACCESS REQUIRED`;
- do not mark this plan complete;
- do not infer absence from workflow YAML.

Do not store tokens or account screenshots in the repository.

## Required concise evidence table

Record in PR/final plan status:

| Surface | Expected final state |
|---|---|
| Main protection | unprotected/no rules, unless user changes policy |
| Required checks | none |
| Actions default token | read-only |
| Environments | zero |
| Repository publishing secrets | zero |
| Repository publishing variables | zero or documented non-secret exception |
| PyPI `eggress` trusted publisher | absent/removed/project absent |
| PyPI compat trusted publisher | absent/removed/project absent |
| TestPyPI trusted publishers | absent/removed/projects absent |

## Acceptance criteria

- Repository settings are rechecked after workflow changes.
- PyPI/TestPyPI state is checked directly.
- No stale GitHub publisher remains.
- No release credential is available to ordinary CI.
- Unknown external state blocks completion rather than being waived.

---

# Workstream FE8 — Align active documentation

## Objective

Make active guidance match the final implementation and remove the remaining false closure language.

## Target files

- `docs/CI_STATUS.md`;
- `docs/TESTING.md`;
- `docs/DIFFERENTIAL_TESTING.md`;
- `AGENTS.md`;
- `.skills/testing/skill.md`;
- `plans/CI_VERIFICATION_RELEASE_CORRECTIVE_CLOSURE.md`;
- this plan.

## Required documentation state

### CI policy

State that:

- Rust smoke runs automatically on pushes to `main` and manually by dispatch;
- Python smoke runs automatically on Python-relevant pushes to `main` and manually by dispatch;
- pull requests do not automatically trigger duplicate smoke runs;
- neither workflow publishes or certifies releases;
- both workflows use read-only permissions.

### Testing policy

State that:

- structural/schema/unit tests are ordinary ungated tests;
- pproxy differential certification uses `EGRESS_PPROXY_CERTIFY=1`;
- platform-only certification uses `EGRESS_PPROXY_PLATFORM=1`;
- specialized external tool gates remain separate;
- certification creates isolated oracle and candidate environments;
- success output is only `summary.json`;
- failed checks retain diagnostics.

Remove active references to:

```text
EGRESS_ORACLE
EGRESS_ORACLE_EXTENDED
EGRESS_ORACLE_PLATFORM
Structural certification profile
3-profile model
ci.rs renamed to profile.rs
CI tier filtering
```

Update stale text that still calls the report filter a CI tier.

### Closure plan status

At final completion:

- mark `plans/CI_VERIFICATION_RELEASE_CORRECTIVE_CLOSURE.md` as:

```text
SUPERSEDED — FINAL EVIDENCE CLOSED BY CI_VERIFICATION_RELEASE_FINAL_EVIDENCE_CLOSURE.md
```

- mark this plan `COMPLETE`;
- record final implementation SHA;
- record successful Rust and Python run IDs/durations;
- record the external settings evidence table;
- leave unchecked any item that is not proven.

Do not retain the earlier false all-checked matrix as the current final state. It may remain under a clearly labeled historical section, or be corrected to reflect actual completion through this plan.

## Acceptance criteria

- Active docs match code and workflow triggers.
- No old oracle gate appears in active guidance.
- No Structural certification profile is described.
- No failed workflow is represented as successful.
- No unverified external state is represented as verified.
- Only one plan is active for this line of work.

---

# Workstream FE9 — Final verification and rejection pass

## Objective

Prove closure with direct checks rather than another evidence framework.

## 9.1 Repository rejection searches

```bash
# Exactly two workflows
find .github/workflows -maxdepth 1 -type f -print | sort

# No PR trigger duplication
! rg -n "pull_request:" .github/workflows/ci.yml .github/workflows/python-test.yml

# No publishing/release automation
! rg -n \
  "cargo publish|twine upload|maturin upload|gh release|docker push|cosign|id-token: write|packages: write|contents: write" \
  .github/workflows

# No old oracle gates in active surfaces
! rg -n \
  "EGRESS_ORACLE_EXTENDED|EGRESS_ORACLE_PLATFORM|EGRESS_ORACLE=1|CertificationProfile::Structural|STRUCTURAL_GATE" \
  crates scripts docs AGENTS.md .skills

# No false profile terminology
! rg -n \
  "3-profile|Structural.*certification profile|CI tier filtering|ci_tier" \
  crates scripts docs AGENTS.md .skills

# No broad repository gates inside certification
! rg -n \
  "cargo fmt|cargo check --workspace|cargo clippy|cargo test --workspace|cargo deny|cargo audit|cargo publish|twine|GitHub Release" \
  scripts/run_pproxy_certification.sh

# No release artifact output from certification
! rg -n \
  "dist/|maturin build|maturin sdist|pip wheel|CLOSURE_AUDIT_REPORT|junit|Markdown report" \
  scripts/run_pproxy_certification.sh
```

Allow `maturin develop` for candidate setup.

## 9.2 Shell and unit validation

```bash
bash -n scripts/run_pproxy_certification.sh
bash -n scripts/run_strict_pproxy_api.sh
bash -n scripts/run_strict_pproxy_interop.sh
bash -n scripts/compat_udp_pproxy.sh

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## 9.3 Profile validation

Run targeted testkit/profile tests and prove:

- Structural scenarios are unprofiled;
- Differential and Platform are the only profile values;
- old gate variables do not affect test selection;
- new gate variables do;
- serialization round trips.

## 9.4 Certification validation

Run:

```bash
./scripts/run_pproxy_certification.sh
python3 -m json.tool target/pproxy-certification/summary.json >/dev/null
```

On success, assert:

```bash
test -f target/pproxy-certification/summary.json
test ! -d target/pproxy-certification/tmp
! find target/pproxy-certification -maxdepth 1 \
  \( -name '*.stdout' -o -name '*.stderr' -o -name '*.md' -o -name '*.xml' \) \
  -print -quit | grep -q .
test -z "$(find target/pproxy-certification/failures -type f -print -quit)"
```

Inspect summary fields:

- schema version;
- full commit SHA;
- oracle version `2.7.9`;
- separate interpreter paths;
- pass result;
- zero required failures.

## 9.5 Failure injection

Provide automated tests or a documented one-time implementation proof for:

1. unsupported host Python version fails before venv creation;
2. wrong pinned oracle version fails before any behavioral check;
3. candidate import failure fails before comparisons;
4. missing paired observation fails certification;
5. forced comparator mismatch produces nonzero exit and failure diagnostics;
6. successful synthetic check leaves no log files;
7. failed synthetic check retains only its diagnostic files;
8. malformed summary input cannot create invalid JSON.

Do not commit broken production configuration to prove failure injection.

## 9.6 Python smoke validation

```bash
for i in $(seq 1 25); do
  .venv/bin/python -m pytest \
    <exact-file>::test_socks5_relay_multiple_chunks -q || exit 1
done

.venv/bin/python -m pytest python/tests tests/compat -q
```

## 9.7 Hosted evidence

Final commit must have:

- successful Rust smoke;
- successful Python smoke;
- no release jobs;
- durations within the existing timeout and target budgets.

## 9.8 External evidence

Confirm the FE7 table is complete. Unknown PyPI/TestPyPI trusted-publisher state blocks final status.

---

# 5. Final acceptance matrix

## Certification isolation

- [ ] Oracle venv is created fresh.
- [ ] Candidate venv is created fresh.
- [ ] Oracle uses exactly `pproxy==2.7.9`.
- [ ] Candidate uses current local `eggress` build.
- [ ] Candidate uses current local compatibility package.
- [ ] Oracle and candidate interpreters are explicit.
- [ ] Observation directories are separate.
- [ ] No host/user package can satisfy certification accidentally.
- [ ] No global Python installation is modified.
- [ ] No `dist/` artifact is created.

## Fail-fast behavior

- [ ] Preflight failure exits immediately.
- [ ] Wrong oracle version exits before comparisons.
- [ ] Candidate build/import failure exits before comparisons.
- [ ] Behavioral failures return nonzero.
- [ ] Missing observations fail.
- [ ] Comparator mismatches fail.

## Compact evidence

- [ ] `summary.json` is the only required success artifact.
- [ ] Successful logs are deleted.
- [ ] Failed diagnostics are retained selectively.
- [ ] Temporary files are removed.
- [ ] JSON is encoder-generated and valid.
- [ ] No Markdown/JUnit output is produced.

## Profile model

- [ ] Structural profile removed.
- [ ] Structural tests are ungated.
- [ ] Only Differential and Platform remain, or profile abstraction is further simplified.
- [ ] Old oracle gate variables removed.
- [ ] New behavior-oriented gates documented.
- [ ] Profile tests pass.
- [ ] Current docs no longer describe a three-profile model.

## Python smoke

- [ ] Exact failure root cause documented.
- [ ] Deterministic correction implemented.
- [ ] No retry plugin, `xfail`, skip, or sleep-only workaround.
- [ ] Targeted test passes 25 consecutive runs.
- [ ] Related test module passes.
- [ ] Full Python smoke suite passes locally.
- [ ] Hosted Python smoke passes.

## Hosted CI

- [ ] Exactly two workflows remain.
- [ ] Automatic triggers are push-to-main only.
- [ ] Manual dispatch remains.
- [ ] No automatic PR duplication remains.
- [ ] Rust smoke passes on final commit.
- [ ] Python smoke passes on final commit.
- [ ] Both workflows remain read-only.
- [ ] No publishing/release/certification job exists.

## External settings

- [ ] Main protection state verified.
- [ ] Required checks verified.
- [ ] Actions default token verified read-only.
- [ ] Environments verified absent.
- [ ] Publishing secrets verified absent.
- [ ] Publishing variables verified absent or documented.
- [ ] PyPI trusted publishers verified absent/removed.
- [ ] TestPyPI trusted publishers verified absent/removed.
- [ ] No sensitive value recorded.

## Documentation and status

- [ ] Prior false closure status reopened.
- [ ] Active docs match new gates and triggers.
- [ ] Failed historical Python run is not presented as passing evidence.
- [ ] Unverified trusted-publisher state is not presented as complete.
- [ ] Prior corrective plan marked superseded only after closure.
- [ ] This plan records final SHA and passing run IDs.
- [ ] This plan marked complete only when every applicable item is proven.
- [ ] Crates.io publication remains out of scope.

---

# 6. Suggested implementation commits

Use small commits that allow a smaller model to reason about one ownership boundary at a time.

1. `plans: reopen CI and release closure status`
2. `testkit: isolate pproxy oracle and candidate environments`
3. `testkit: retain certification logs only on failure`
4. `testkit: remove structural certification profile and legacy gates`
5. `test: make multi-chunk SOCKS5 relay deterministic`
6. `ci: use push and manual smoke triggers only`
7. `docs: align final verification and certification guidance`
8. `plans: close final CI and release evidence pass`

The exact number may be reduced when two adjacent edits are inseparable, but do not combine unrelated proxy features or release architecture.

---

# 7. Handoff guidance for a smaller implementation model

- Read the current certification script, profile module, Python smoke workflow, failing hosted job log, and this plan before editing.
- Do not trust the checked boxes in the prior corrective plan; verify every criterion from code or external state.
- Build isolation first. Later behavioral results are not trustworthy until oracle and candidate environments are separate.
- Treat environment setup failures differently from behavioral mismatches: setup fails immediately; independent behavioral checks may aggregate.
- Use `importlib.metadata.version("pproxy")`, not a possibly absent module `__version__` attribute.
- Thread explicit interpreter paths through helper scripts. Do not rely on PATH ordering.
- Do not use the oracle interpreter for candidate tests.
- Do not install the local compatibility package into the oracle environment.
- Delete success logs. The existence of a summary entry is enough evidence for a passing check.
- Remove the Structural profile rather than renaming it again.
- Do not preserve old gate variables unless a real external consumer is identified.
- Root-cause the Python failure. Empty relayed data may expose a real runtime race.
- Do not add retries or `xfail` to obtain a green badge.
- Keep workflows small. The final intended model is push-to-main plus manual dispatch.
- PyPI trusted-publisher state must be checked in PyPI settings; workflow absence is not evidence of registry absence.
- Do not reveal tokens or secret values.
- Do not change crate publication boundaries in this pass.
- Do not create a completion report separate from the plans.
- Mark completion last.
