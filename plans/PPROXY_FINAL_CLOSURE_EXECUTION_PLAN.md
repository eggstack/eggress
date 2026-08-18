# pproxy Final Closure Execution Plan

## Status

Ready for implementation.

This is the final bounded closure pass for the current `pproxy==2.7.9` compatibility line of work. It is intentionally not a new roadmap. Do not reopen completed protocol phases, add new protocol families, expand the verification framework, or introduce a broad feature-matrix CI system unless a concrete failing acceptance test proves that additional implementation is required.

## Baseline

Plan baseline:

- repository: `eggstack/eggress`
- branch: `main`
- baseline commit: `d149e4dc544c78d7934f416e3d2762c211bef665`
- baseline commit message: `Merge remote-tracking branch 'origin/ci/lean-manual-release'`

Frozen upstream compatibility oracle:

- package: `pproxy==2.7.9`
- repository: `https://github.com/qwj/python-proxy`
- exact upstream commit: `09d4752f17ed6787e1a073c93980eec019887ee3`

Previous corrective-plan baseline:

- `plans/PPROXY_FINAL_CORRECTIVE_CLOSURE_PASS.md`
- planning commit: `cdd2a4d34e33044f84b0e2217bc668d3f829d666`

The implementation after that plan materially improved reverse/backward compatibility. In particular, `crates/eggress-runtime/tests/reverse_interop.rs` now contains real-oracle test scenarios for both compatibility directions, HTTP-jump traversal, and forced reconnect. This plan therefore treats reverse implementation as provisionally complete and focuses first on executing/recording that evidence, not rewriting the adapter again.

## Objective

Close the remaining implementation-policy, compatibility-claim, MSRV, and CI/release inconsistencies so that the repository can make one defensible final statement:

> Eggress is a strong behavior-oriented replacement for `pproxy==2.7.9` across the supported/default feature set, with all remaining feature-gated behavior, supported differences, and intentional exclusions explicitly documented.

Completion requires six bounded work packages:

1. execute and record the existing real-pproxy reverse/backward oracle evidence;
2. restore `pproxy-legacy` as a true opt-in rather than a default CLI feature;
3. reconcile the active manifest, compatibility matrix, README, and migration documentation with actual remaining boundaries;
4. make Rust 1.85 the explicit workspace MSRV decision unless a concrete blocker requires reconsideration;
5. repair the latest CI/PyPI policy merge regression, including the stale non-existent Python package path;
6. mark the prior strict/corrective closure documents complete only after the above work is actually verified.

## Non-goals / scope guardrails

The executor must not:

- add new pproxy protocol families;
- implement macOS PF original-destination recovery;
- add the four intentionally unavailable legacy cipher primitives solely to improve a parity count;
- generalize the six bounded pproxy SSR plugins into a new plugin framework;
- add SSR UDP support in this closure pass;
- add QUIC UDP-association support in this closure pass;
- redesign the native Eggress reverse protocol;
- replace the existing compatibility reverse adapter if its oracle tests pass;
- introduce another certification framework or evidence database;
- add a combinatorial feature-matrix CI workflow;
- automate crates.io publication;
- remove the existing PyPI multi-platform workflow merely to make documentation internally consistent;
- create a new standalone `python-pproxy-compat` package unless the repository already contains and intentionally publishes such a distribution at execution time;
- broaden ordinary CI beyond the small Rust/Python smoke boundary already intended for this project.

If a required command fails because of an unrelated pre-existing issue, record that separately. Do not expand this closure pass into a general cleanup program.

---

# Work Package 1 — Execute and record real pproxy reverse/backward evidence

## Current state

The earlier evidence problem was that reverse tests proved process startup, port reachability, or Eggress-to-Eggress roundtrips rather than real pproxy payload interoperability.

The current `crates/eggress-runtime/tests/reverse_interop.rs` has since been materially corrected. It now contains ignored/gated real-oracle scenarios covering:

- pproxy backward worker -> Eggress compatibility listener with payload relay;
- Eggress compatibility backward worker -> pproxy endpoint with payload relay;
- Eggress backward worker -> pproxy through one local HTTP CONNECT jump;
- forced disconnect/restart followed by reconnect and a second successful payload relay.

The tests resolve a canonical pproxy Python interpreter, launch `$python -m pproxy`, allocate explicit loopback ports, use local fixtures, and retain cancellation/process handles.

The remaining closure task is therefore evidence execution and claim reconciliation, not another speculative implementation rewrite.

## Required implementation/execution

### 1. Provision the exact oracle interpreter

Use an isolated environment containing exactly the frozen package target.

Example:

```bash
python3 -m venv .venv-pproxy-279
.venv-pproxy-279/bin/python -m pip install --upgrade pip
.venv-pproxy-279/bin/python -m pip install 'pproxy==2.7.9'
```

Verify the installed version before using it:

```bash
.venv-pproxy-279/bin/python - <<'PY'
import importlib.metadata
import pproxy
print(importlib.metadata.version('pproxy'))
print(pproxy.__file__)
PY
```

Do not substitute an arbitrary `pproxy` executable from `PATH`.

### 2. Run the focused real-oracle reverse suite

Use the test's canonical environment-variable contract:

```bash
EGRESS_PPROXY_PYTHON="$PWD/.venv-pproxy-279/bin/python" \
EGRESS_REQUIRE_REVERSE_INTEROP=1 \
cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
```

The executor must confirm that the following named scenarios execute rather than merely compile:

- `gated_pproxy_backward_worker_to_eggress_listener_payload`
- `gated_eggress_backward_worker_to_pproxy_listener_payload`
- `gated_eggress_backward_worker_pproxy_http_jump_payload`
- `gated_eggress_backward_worker_pproxy_disconnect_reconnect`

If names change before implementation, map the replacement tests one-for-one to the same four behaviors.

### 3. Require payload evidence, not counters

For both directions and the HTTP-jump scenario, success means the deterministic binary payload reaches the local echo target and returns byte-for-byte unchanged.

For reconnect, success means:

1. payload A succeeds;
2. the active pproxy listener/control side is deliberately terminated or restarted;
3. the Eggress worker reconnects within its bounded retry policy;
4. payload B succeeds after reconnect;
5. no runaway duplicate worker remains after test completion.

A reconnect metric/counter without post-reconnect payload success is insufficient.

### 4. Do not rewrite passing compatibility code

If all four real-oracle scenarios pass, do not refactor `compat_pproxy.rs` for style or symmetry. The goal is closure.

Only modify the adapter if a real-oracle failure demonstrates a concrete wire/protocol defect. Any such change must be the smallest fix that makes the failing oracle scenario pass while preserving native reverse behavior.

### 5. Record the evidence in the active sources of truth

After the real-oracle run passes, update only the active compatibility evidence locations needed to reflect the executed proof. At minimum inspect:

- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/COMPATIBILITY_EVIDENCE.md`
- `docs/architecture/pproxy-compat.md`

The manifest may describe reverse/backward as a supported difference even after interop succeeds; real pproxy interoperability does not require pretending Eggress's native reverse implementation is byte-identical in all compositions.

Do not promote unsupported reverse-TLS composition to supported merely because raw/SOCKS5 backward interop passes.

## Acceptance criteria — reverse/backward

- [ ] Oracle interpreter reports `pproxy==2.7.9`.
- [ ] Tests launch that interpreter with `-m pproxy`; no arbitrary PATH executable is used.
- [ ] No reverse test scans guessed port ranges.
- [ ] pproxy -> Eggress payload-level compatibility test passes.
- [ ] Eggress -> pproxy payload-level compatibility test passes.
- [ ] Both directions assert byte-for-byte payload equality.
- [ ] One real pproxy HTTP CONNECT jump topology passes end-to-end.
- [ ] Forced disconnect/reconnect is followed by a second successful payload relay.
- [ ] Child processes are terminated/waited and Eggress tasks are cancelled/joined deterministically.
- [ ] Native Eggress reverse tests remain passing.
- [ ] Active compatibility documentation records the executed external evidence without overstating unsupported reverse compositions.

---

# Work Package 2 — Restore `pproxy-legacy` to an explicit opt-in feature

## Current state

At the baseline, `crates/eggress-cli/Cargo.toml` contains:

```toml
[features]
default = ["full"]
full = ["common", "extended", "operations", "reverse", "pproxy-compat", "pproxy-legacy"]
```

This makes the bounded SSR/plugin compatibility path part of the normal default CLI even though the maintained compatibility documentation describes it as opt-in.

Lower layers are already shaped correctly:

- `eggress-runtime` default `full` does not include `pproxy-legacy`;
- `eggress-server` default `full` does not include `pproxy-legacy`;
- `eggress-protocol-shadowsocks` keeps the legacy pproxy plugin path separately gated.

Do not redesign those layers. Fix the accidental CLI promotion.

## Required implementation

### 1. Change only the default CLI aggregation

Expected shape:

```toml
default = ["full"]
full = ["common", "extended", "operations", "reverse", "pproxy-compat"]
pproxy-legacy = ["eggress-runtime/pproxy-legacy"]
```

The feature itself remains available.

### 2. Preserve all other optional tails

The following must remain explicit opt-ins:

- `pproxy-legacy`
- `legacy-crypto`
- `ssh`
- `quic`
- `pproxy-daemon`

Do not make one optional tail pull another unless that dependency is technically required. In particular, SSR/plugin framing and legacy stream-cipher support remain separate concepts.

### 3. Verify feature propagation

Search the workspace before and after the edit:

```bash
rg -n 'default\s*=|full\s*=|pproxy-legacy|legacy-crypto|pproxy-daemon|ssh\s*=|quic\s*=' Cargo.toml crates
```

The default `eggress-cli` feature graph must not activate `pproxy-legacy` indirectly through another default feature.

When useful, inspect with:

```bash
cargo tree -p eggress-cli -e features
```

### 4. Preserve clear feature-off behavior

An SSR URI that reaches runtime without `pproxy-legacy` must fail closed with a structured, understandable feature-required/unsupported diagnostic. It must not silently downgrade to plain Shadowsocks or bypass plugin framing.

Use existing diagnostic/error infrastructure. Do not add a second feature-discovery subsystem.

Add or update focused tests that prove:

- default CLI/runtime does not enable the SSR/plugin compatibility implementation;
- explicit `--features pproxy-legacy` enables it;
- feature-off failure is deterministic and user-readable.

### 5. Focused compile verification

Run:

```bash
cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-cli
cargo check -p eggress-cli --features pproxy-legacy
cargo check -p eggress-cli --features legacy-crypto
cargo check -p eggress-cli --features ssh
cargo check -p eggress-cli --features quic
```

These are local/focused checks. Do not create a permanent six-way CI matrix.

## Acceptance criteria — feature boundary

- [ ] `eggress-cli` default `full` no longer contains `pproxy-legacy`.
- [ ] `pproxy-legacy` remains available explicitly.
- [ ] No lower-layer default feature indirectly re-enables `pproxy-legacy`.
- [ ] Default SSR/plugin use fails closed with a clear feature-required diagnostic.
- [ ] `--features pproxy-legacy` restores bounded SSR/plugin behavior.
- [ ] `legacy-crypto`, `ssh`, `quic`, and `pproxy-daemon` remain opt-in.
- [ ] All six focused compile commands above pass.
- [ ] No feature-matrix CI workflow is added.

---

# Work Package 3 — Reconcile compatibility claims with actual boundaries

## Source-of-truth hierarchy

Use this order when resolving contradictions:

1. frozen `pproxy==2.7.9` source at `09d4752...`;
2. actual Eggress code and executed external interoperability evidence;
3. `docs/parity/pproxy_capability_manifest.toml` as the active detailed inventory;
4. `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` as the maintained human-readable summary;
5. README/migration/architecture documentation as user-facing summaries;
6. historical plan/report files as provenance only.

Do not let an old phase completion document override current code.

## Required boundaries to represent explicitly

At minimum, reconcile all of the following.

### SSR/plugin compatibility

Accurate boundary:

- bounded TCP SSR framing is available with `pproxy-legacy`;
- exactly the six pproxy 2.7.9 built-in plugin names are in scope;
- SSR UDP is not supported by this bounded path;
- arbitrary/external/custom SSR plugins are not supported;
- legacy stream-cipher encryption is a separate `legacy-crypto` feature.

After Work Package 2, documentation must state that `pproxy-legacy` is actually opt-in in the default CLI.

### QUIC/H3

Accurate boundary:

- QUIC/H3 behavior is optional behind `quic`;
- supported listener/client behavior may remain classified as a supported difference;
- pproxy UDP association / UDP-over-QUIC composition remains unsupported.

Do not collapse “QUIC supported” into an implication that every pproxy QUIC composition is supported.

### Reverse/backward

Accurate boundary:

- raw/SOCKS5 `+in` compatibility has real pproxy payload evidence once Work Package 1 passes;
- Eggress native reverse remains a distinct stronger/native protocol;
- reverse/backward TLS composition remains unsupported wherever the active translator/runtime rejects it;
- QUIC reverse behavior remains separately feature-qualified.

### SSH

Accurate boundary:

- SSH is upstream-only and opt-in;
- host-key verification behavior is deliberately permissive/warning-bearing to match pproxy's `known_hosts=None` behavior;
- do not present this compatibility choice as the native security default.

### Daemon behavior

Accurate boundary:

- daemon compatibility is opt-in via `pproxy-daemon`;
- it is Linux/platform-qualified;
- feature-off/non-supported-platform behavior fails closed.

### Legacy ciphers

Accurate boundary:

- maintained RustCrypto subset is opt-in through `legacy-crypto`;
- OTA compatibility remains legacy behavior rather than a native secure default;
- `cast5-cfb`, `idea-cfb`, `rc2-cfb`, and `seed-cfb` remain explicit intentional exclusions.

### macOS PF

Accurate boundary:

- original-destination PF recovery remains intentional non-parity;
- no new unsafe `/dev/pf` implementation is required for this closure.

## Files to reconcile

Inspect and update only where necessary:

- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `README.md`
- `docs/PPROXY_PARITY_SPEC.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/COMPATIBILITY_EVIDENCE.md`
- `docs/release/MIGRATION_FROM_PPROXY_FINAL.md`
- `docs/architecture/pproxy-compat.md`
- relevant SSH/QUIC/Shadowsocks/reverse documentation referenced by the matrix

Do not rewrite every historical phase plan. Historical plans should only receive a short status/supersession note if their current “active” wording creates a real contradiction.

## Required final claim shape

Do not introduce a numerical percentage or “100% parity” claim.

The final public wording should communicate that Eggress provides broad behavior-oriented pproxy 2.7.9 replacement compatibility while explicitly listing feature/platform boundaries.

The final matrix summary must not imply that the only differences are PF plus four cipher names. It must mention, either inline or through an immediately adjacent explicit boundary list:

- opt-in SSR/plugin behavior plus SSR UDP/external-plugin exclusions;
- opt-in QUIC/H3 plus unsupported UDP association composition;
- reverse/backward TLS limitation;
- opt-in SSH and permissive host-key compatibility behavior;
- opt-in Linux daemon behavior;
- opt-in legacy crypto and four unavailable names;
- macOS PF intentional exclusion.

## Acceptance criteria — claims/docs

- [ ] Every material pproxy 2.7.9 incompatibility or supported difference is represented in the active manifest.
- [ ] Manifest status/evidence fields match executed tests, not intended tests.
- [ ] SSR UDP is not implied supported.
- [ ] External/custom SSR plugins are not implied supported.
- [ ] QUIC/H3 UDP association is not implied supported.
- [ ] Reverse/backward TLS composition is documented consistently with actual translator/runtime behavior.
- [ ] macOS PF remains explicit intentional non-parity.
- [ ] Four unavailable cipher names remain explicit.
- [ ] SSH, QUIC/H3, legacy crypto, SSR/plugin, and daemon behavior are clearly feature/platform qualified.
- [ ] Default feature documentation reflects removal of `pproxy-legacy` from CLI `full`.
- [ ] README, matrix, migration docs, architecture docs, and manifest do not contradict each other.
- [ ] No “100% parity” or unsupported aggregate percentage is introduced.

---

# Work Package 4 — Make Rust 1.85 the explicit MSRV decision

## Current state

The workspace declares:

```toml
[workspace.package]
rust-version = "1.85"
```

The optional SSH stack uses `russh = 0.62.6`, and the workspace-wide MSRV increased while adding that maintained SSH implementation.

This is a reasonable 2026 product choice, but it currently reads as an incidental dependency side effect rather than a deliberate compatibility policy.

## Required decision for this closure

Preferred/default execution path: **retain workspace MSRV 1.85 and document it explicitly**.

Do not reopen dependency pinning or split-toolchain architecture merely to recover the old Rust 1.75 floor unless a concrete supported-user requirement or build failure demonstrates that 1.85 is unacceptable.

## Required implementation

### 1. Keep the root declaration

Retain:

```toml
rust-version = "1.85"
```

### 2. Remove stale lower-MSRV claims

Search active repository content:

```bash
rg -n '1\.75|1\.85|MSRV|minimum supported Rust|rust-version' \
  README.md AGENTS.md docs .skills .agents .opencode Cargo.toml crates
```

Update stale active references to the former floor.

Historical plan prose may remain if clearly historical and not presented as current policy.

### 3. Document the rationale once

Place the active policy in one user/developer-facing location such as README or `AGENTS.md`, then reference it rather than duplicating long rationale everywhere.

Required content:

- supported workspace MSRV: Rust 1.85;
- this applies to the workspace even if optional SSH is disabled;
- the floor accommodates the maintained SSH dependency stack and current workspace dependency set;
- no separate lower-MSRV “lean” product is promised.

### 4. Keep verification small

If the existing CI already uses stable Rust newer than 1.85, do not add a new MSRV matrix for this pass.

A focused local check under Rust 1.85 is sufficient if that toolchain is available:

```bash
cargo +1.85 check -p eggress-cli
```

If unavailable in the executor environment, ensure the declaration/docs are coherent and record that the exact-toolchain check was not run. Do not add infrastructure solely to manufacture the check.

## Acceptance criteria — MSRV

- [ ] Root workspace still declares `rust-version = "1.85"`.
- [ ] Active docs no longer claim Rust 1.75 as supported.
- [ ] Rust 1.85 is explicitly described as the workspace MSRV.
- [ ] Optional SSH is documented as one reason for the modern floor without implying SSH is enabled by default.
- [ ] No obsolete `russh` pin is introduced solely to lower MSRV.
- [ ] No new MSRV CI matrix is added.

---

# Work Package 5 — Repair CI/PyPI policy and current Python-smoke merge regression

## Intended release boundary

The maintained project policy for this repository is:

- crates.io publication remains manual/operator-driven;
- GitHub Actions must not publish Rust crates or create a Rust release cadence;
- Python/PyPI uses CI because the project needs multi-platform native wheels;
- the existing PyPI workflow may build platform wheels, validate them, smoke-test them, and publish through the configured PyPI/TestPyPI path;
- GitHub Release creation, container publication, SBOM/signature expansion, and unrelated release machinery remain out of scope unless separately requested.

Do not “simplify” by deleting the PyPI workflow. Its cross-platform wheel construction is the reason Python publishing remains in CI.

## Current merge inconsistency

At the baseline:

- `.github/workflows/publish-python.yml` still exists and is configured for multi-platform wheel/sdist build and PyPI/TestPyPI publication;
- `docs/CI_STATUS.md` and `docs/release/RELEASE_PROCESS.md` now claim there are no publishing workflows and that Python publication is manual;
- `.github/workflows/python-test.yml` now references `python-pproxy-compat/**` and runs `pip install --no-deps ./python-pproxy-compat`, but that directory does not exist on `main`;
- the actual Python source tree already contains both `python/eggress` and `python/pproxy`, and `python/pyproject.toml` includes the `pproxy/**/*.py` namespace in the Eggress wheel.

This is a merge-resolution regression. Fix the workflow/docs to match the repository's actual packaging model; do not recreate a stale package directory just to satisfy the broken workflow.

## Required implementation

### 1. Fix `python-test.yml` stale package references

Inspect the authoritative Python build entry point:

- `crates/eggress-python/pyproject.toml`
- `python/pyproject.toml` for local-development packaging metadata

The current source layout ships the top-level `pproxy` namespace as part of the Eggress Python distribution.

Unless the repository has intentionally reintroduced a separate compatibility distribution by implementation time:

- remove `python-pproxy-compat/**` from workflow path filters;
- remove `pip install --no-deps ./python-pproxy-compat`;
- install the built Eggress wheel only;
- run tests that import both `eggress` and top-level `pproxy` from that installed wheel.

Do not solve the stale path by adding an empty compatibility package.

### 2. Keep the Python smoke workflow small

The smoke job should remain one bounded Linux/Python job for ordinary Python-facing changes.

It should verify at minimum:

```bash
python -c 'import eggress, pproxy; print(eggress.__file__); print(pproxy.__file__)'
python -m pytest python/tests tests/compat -q
```

Use the venv interpreter explicitly, as the workflow already does.

Do not duplicate the five-platform release wheel matrix in ordinary smoke CI.

### 3. Preserve the existing PyPI release workflow

Review `.github/workflows/publish-python.yml` for consistency with the current package layout and version files, but do not redesign it unless a concrete stale path or package assumption is found.

The desired release workflow continues to own the expensive multi-platform build/smoke path.

Required policy:

- tag `v*` may trigger production PyPI publication after validation;
- manual dispatch may target TestPyPI/PyPI according to the existing guarded input contract;
- OIDC trusted publishing remains acceptable;
- wheel/sdist validation and installed-artifact smoke tests remain release-only checks;
- crates.io remains entirely outside this workflow.

If the workflow currently assumes a removed separate compatibility distribution, correct it to the current single Eggress distribution that includes the `pproxy` import namespace. Do not add a second PyPI distribution without an explicit product requirement.

### 4. Correct documentation to match the actual split

Update at minimum:

- `docs/CI_STATUS.md`
- `docs/TESTING.md`
- `docs/release/RELEASE_PROCESS.md`
- `AGENTS.md` if it describes release boundaries
- relevant testing skill documentation if it repeats the stale package path

The docs should state clearly:

- ordinary Rust/Python CI remains small;
- crates.io publication is manual;
- PyPI publication/build remains in the dedicated release workflow because native wheels are built for multiple platforms;
- release workflow execution is not an ordinary merge gate;
- pproxy compatibility certification remains focused/on-demand rather than required on every commit.

### 5. Do not reintroduce verification overkill

Do not add:

- release artifact checks to ordinary Rust CI;
- broad cross-platform matrices to every PR;
- mandatory oracle tests on every push;
- crates.io publishing tokens/workflows;
- automatic GitHub Release generation;
- duplicate Python build jobs that test the same artifact boundary.

## Acceptance criteria — CI/PyPI

- [ ] `.github/workflows/python-test.yml` contains no path/install reference to a non-existent `python-pproxy-compat` directory.
- [ ] Python smoke installs the actual built Eggress distribution and can import both `eggress` and top-level `pproxy`.
- [ ] `python/tests` and `tests/compat` run from the installed-wheel environment.
- [ ] `.github/workflows/publish-python.yml` remains present unless an explicit subsequent product decision supersedes this plan.
- [ ] PyPI release workflow still builds the required native wheel targets rather than moving that responsibility to a maintainer laptop.
- [ ] crates.io publication remains manual.
- [ ] CI/release docs no longer say “no publishing workflow” while the PyPI workflow exists.
- [ ] Ordinary CI remains small; no broad matrix/certification expansion is introduced.

---

# Work Package 6 — Final status and closure bookkeeping

This package must be done last.

## Required updates

After Work Packages 1–5 pass:

### 1. Update this plan

Change:

```text
Status: Ready for implementation
```

to a short completion statement containing:

- implementation commit SHA(s);
- reverse oracle command executed and pass/fail result;
- focused feature checks executed;
- ordinary Rust/Python verification summary;
- any intentionally unexecuted optional/platform check.

Do not paste enormous logs.

### 2. Update the previous corrective plan

`plans/PPROXY_FINAL_CORRECTIVE_CLOSURE_PASS.md` may receive a short header/status note that it has been completed/superseded by this execution plan.

Do not rewrite its historical findings.

### 3. Reconcile strict Phase 10 status

Only mark Phase 10/final strict closure complete after:

- the four real reverse oracle scenarios pass;
- default `pproxy-legacy` leakage is removed;
- active compatibility claims are reconciled;
- MSRV is documented;
- CI/PyPI docs/workflow package paths are coherent.

### 4. Preserve qualified public language

“Closed” means the tracked pproxy replacement line has no known unrecorded gap under its stated boundaries. It does not mean Eggress implements every optional pproxy behavior identically.

Do not replace qualified compatibility wording with “full parity,” “100% parity,” or similar aggregate marketing language.

## Acceptance criteria — final bookkeeping

- [ ] This plan is marked complete only after implementation/evidence exists.
- [ ] Previous corrective plan is clearly historical/completed/superseded rather than still active.
- [ ] Phase 10 status matches actual evidence.
- [ ] README/matrix remain qualified and boundary-aware.
- [ ] No new roadmap or follow-up plan is required for the issues covered here.

---

# Verification sequence

The executor should use the following order so failures remain attributable and the pass stays small.

## A. Formatting and targeted feature checks

```bash
cargo fmt --all -- --check
cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-cli
cargo check -p eggress-cli --features pproxy-legacy
cargo check -p eggress-cli --features legacy-crypto
cargo check -p eggress-cli --features ssh
cargo check -p eggress-cli --features quic
```

## B. Focused pproxy/reverse tests

Run the normal reverse compatibility tests first:

```bash
cargo test -p eggress-runtime --test reverse_interop
```

Then run the exact real-oracle gate:

```bash
EGRESS_PPROXY_PYTHON="$PWD/.venv-pproxy-279/bin/python" \
EGRESS_REQUIRE_REVERSE_INTEROP=1 \
cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
```

Run any focused SSR feature-on/off tests added by Work Package 2.

## C. Python installed-wheel smoke

Use a clean venv and the authoritative native build entry point. Example:

```bash
python3 -m venv .venv-close
.venv-close/bin/python -m pip install --upgrade pip 'maturin>=1.0,<2.0' pytest 'pytest-asyncio>=0.23,<1' 'cryptography>=42,<47'
(cd crates/eggress-python && ../../.venv-close/bin/maturin build --release --out ../../target/closure-wheels)
.venv-close/bin/python -m pip install target/closure-wheels/eggress-*.whl
.venv-close/bin/python -c 'import eggress, pproxy; print(eggress.__file__); print(pproxy.__file__)'
.venv-close/bin/python -m pytest python/tests tests/compat -q
```

Do not install a non-existent compatibility package path.

## D. Broad local regression

After focused failures are resolved:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Do not add additional broad gates merely for ceremony.

## E. Documentation consistency searches

At minimum:

```bash
rg -n 'python-pproxy-compat|no publishing workflow|publish.*manual|PyPI|crates\.io' \
  .github README.md AGENTS.md docs .skills

rg -n 'pproxy-legacy|SSR UDP|external plugin|UDP association|backward.*TLS|macOS PF|cast5-cfb|idea-cfb|rc2-cfb|seed-cfb' \
  README.md docs

rg -n '1\.75|1\.85|MSRV|rust-version' README.md AGENTS.md docs .skills Cargo.toml crates
```

Review matches semantically; do not mechanically replace historical provenance text.

---

# Final global acceptance criteria

The entire pproxy closure line is complete only when all statements below are true.

1. The frozen oracle remains exactly `pproxy==2.7.9` / `09d4752f17ed6787e1a073c93980eec019887ee3`.
2. Real pproxy reverse/backward payload interop passes in both directions.
3. One real pproxy HTTP-jump reverse topology passes.
4. Forced reverse disconnect is followed by reconnect and a successful second payload relay.
5. `pproxy-legacy` is not enabled by the default CLI `full` feature.
6. `pproxy-legacy`, `legacy-crypto`, `ssh`, `quic`, and `pproxy-daemon` remain explicit optional compatibility tails.
7. SSR feature-off behavior fails closed; feature-on behavior remains functional.
8. The active manifest records every material supported difference/exclusion and uses evidence that actually exists.
9. SSR UDP/external plugins, QUIC UDP association, and backward-TLS limitations are not hidden by summary language.
10. macOS PF and the four unavailable legacy cipher names remain explicit exclusions.
11. Workspace MSRV Rust 1.85 is documented as an intentional supported floor.
12. Python smoke workflow references only real repository package paths.
13. The installed Eggress wheel exposes both `eggress` and top-level `pproxy` namespaces as intended.
14. PyPI remains the CI-managed multi-platform Python release path.
15. crates.io publication remains manual/operator-driven.
16. Ordinary CI remains deliberately small and does not absorb compatibility certification or release-matrix work.
17. README, matrix, manifest, migration docs, architecture docs, CI docs, and release docs agree on the active product boundary.
18. No unqualified “100% parity” claim is introduced.
19. Phase 10 and the prior corrective plan are only marked complete after the above evidence exists.
20. No further roadmap is created for this line of work unless a later real-world defect or upstream-scope decision introduces a new requirement.

## Expected end state

After this pass, the pproxy replacement effort should be treated as closed for the frozen 2.7.9 target. Future work should be ordinary bug fixing, maintenance, performance/security improvements, or explicitly requested scope changes—not recurring parity-planning cycles.