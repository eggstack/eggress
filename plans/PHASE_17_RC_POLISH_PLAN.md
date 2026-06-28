# Phase 17 RC Polish Plan

## Purpose

The Phase 13–17 implementation pass added a real Rust embed API, PyO3 bindings, Python package wrappers, pproxy helper APIs, wheel/release infrastructure, and a release-candidate audit document. The repo is credible as a pre-release candidate, but the latest audit identified several narrow issues that should be polished before tagging `v0.1.0-rc.1`.

This plan is deliberately limited. It should not add new proxy protocols, rewrite Shadowsocks TCP, expand parity claims, or begin GA hardening. The goal is to make the existing RC boundary precise, predictable, and easier to maintain.

---

# Current high-level status

The repo now has:

- `crates/eggress-embed`: Rust in-process service API;
- `crates/eggress-python`: PyO3 extension module over `eggress-embed`;
- `python/eggress`: Python wrapper package;
- Python pproxy translation helpers;
- Python examples and tests;
- PyPI/wheel workflow files;
- release-candidate documentation;
- explicit non-parity for Shadowsocks TCP standard framing and other unsupported pproxy features.

The remaining issues are polish and correctness-boundary issues:

1. Embed runtime/thread ownership is functional but complex.
2. Python object destruction may block; explicit lifecycle guidance should be reinforced.
3. There are two `pyproject.toml` files that may confuse build entrypoints.
4. RC docs are close to overclaiming differential evidence that was not run.
5. Version strings may drift between Rust, PyO3, and Python package surfaces.
6. “No release blockers” must be scoped to pre-release RC, not GA.

---

# Non-goals

Do not implement:

- standard Shadowsocks TCP framing rewrite;
- inbound Shadowsocks listener;
- new protocols;
- new scheduler semantics;
- pproxy `--reuse` pooling;
- mTLS/admin-auth implementation;
- new wheel targets beyond metadata/workflow correction;
- broad runtime refactor;
- unsafe Rust;
- OpenSSL/native-tls.

---

# Workstream 1: Embed runtime ownership audit

## Goal

Make the Rust embed lifecycle easier to reason about and safe enough for Python pre-release use.

## Current concern

`EggressService::start()` and `EggressService::start_blocking()` both create multi-layer execution paths. The async path uses `tokio::task::spawn_blocking`, then starts a separate OS thread running `ServiceSupervisor::run()`. The blocking path also creates one thread that spawns another run thread. This may pass tests, but it is more complex than necessary and makes drop/shutdown behavior harder to reason about.

## Files to inspect

```text
crates/eggress-embed/src/lib.rs
crates/eggress-embed/tests/start_stop.rs
crates/eggress-embed/tests/proxy_traffic.rs
crates/eggress-embed/tests/reload.rs
crates/eggress-embed/tests/metrics_status.rs
python/tests/test_threading.py
python/tests/test_pproxy_concurrency.py
```

## Required audit questions

1. Does `start()` ever leak the temporary config file if readiness fails?
2. Does `start()` return only after the runtime is genuinely accepting traffic?
3. Does `start_blocking()` leave an extra orchestration thread alive after readiness?
4. Does `shutdown()` always join the actual supervisor thread?
5. Does `Drop` block longer than documented?
6. Does dropping a handle inside an async runtime attempt to create a nested Tokio runtime?
7. Does failure during startup cleanly cancel and join all spawned work?
8. Can two independent embedded services run and shut down independently?

## Preferred outcome

If a small simplification is feasible, prefer one owned thread model:

```text
EggressHandle owns exactly one supervisor thread handle + cancellation token + state.
```

The async `start()` can call the same internal blocking start using `spawn_blocking`, but should not introduce an additional long-lived thread/task layer beyond what is needed.

## Acceptable outcome

If the current structure is retained, add explicit comments and tests proving:

- which thread owns `ServiceSupervisor::run()`;
- which object joins it;
- what `Drop` does;
- why nested runtime creation is safe or avoided.

## Tests to add or strengthen

- startup failure cleans temp config file;
- readiness timeout cancels runtime;
- repeated start/shutdown loop does not leak threads enough to hang tests;
- dropping handle without explicit shutdown cancels service;
- explicit shutdown is idempotent where exposed through Python;
- async `astart()` + async context manager shuts down under exception.

## Acceptance criteria

- Thread ownership is documented in code and `docs/EMBED_API.md`.
- Tests cover explicit shutdown and drop fallback.
- No extra permanent orchestration thread remains after startup.

---

# Workstream 2: Python lifecycle semantics polish

## Goal

Make Python users strongly prefer explicit lifecycle management and avoid surprising destructor blocking.

## Current concern

`EggressHandle.__exit__()` and `shutdown()` are present, but Rust `Drop` also cancels and may block while joining/waiting. Python garbage collection timing is nondeterministic, so relying on `__del__`/native drop behavior is not a good user contract.

## Files to inspect

```text
python/eggress/service.py
python/README.md
docs/PYTHON_BINDINGS.md
python/examples/*.py
```

## Required changes

1. Ensure docs and examples consistently use:

```python
with EggressService.from_toml(toml).start() as handle:
    ...
```

or:

```python
handle = EggressService.from_toml(toml).start()
try:
    ...
finally:
    handle.shutdown()
```

2. Add a warning in docs that object destruction is a fallback, not the lifecycle API.
3. Ensure `shutdown()` is idempotent at Python level.
4. Ensure context-manager `__exit__` calls shutdown and returns `False`.
5. For `AsyncEggressHandle`, ensure `__aexit__` awaits shutdown and returns `False`.

## Tests

- calling `shutdown()` twice is safe;
- context manager shuts down on exception;
- async context manager shuts down on exception;
- status/metrics after shutdown raise a clear `EggressError` or documented exception.

## Acceptance criteria

- Python lifecycle behavior is deterministic when users follow docs.
- Destruction/drop fallback is documented as best-effort cleanup only.

---

# Workstream 3: Packaging entrypoint consolidation

## Goal

Remove ambiguity around which `pyproject.toml` is authoritative for wheel builds.

## Current concern

There are two project metadata files:

```text
python/pyproject.toml
crates/eggress-python/pyproject.toml
```

Both describe package `eggress`, but they use different relative paths. This can work, but it creates a risk that users, CI, or release automation build from the wrong directory and produce inconsistent artifacts.

## Required decision

Pick one authoritative release entrypoint.

Recommended options:

### Option A: `crates/eggress-python/pyproject.toml` is authoritative

Pros:

- colocates PyO3 crate and maturin metadata;
- common Rust-extension layout;
- direct relation to `Cargo.toml`.

Required:

- keep `python/` as pure Python source tree;
- document commands run from `crates/eggress-python`;
- ensure workflow uses that path consistently;
- make root/python pyproject either absent, minimal dev helper, or clearly non-release.

### Option B: `python/pyproject.toml` is authoritative

Pros:

- Python developers expect package metadata in `python/`;
- examples/tests colocated.

Required:

- ensure maturin can find Rust manifest reliably;
- workflows use `python/` consistently;
- document how Rust crate is included.

## Required changes

1. Update `docs/PYPI_RELEASE.md` with one authoritative build path.
2. Update `.github/workflows/python-wheels.yml` and `scripts/test_wheel.sh` to use the same path.
3. Update `docs/WHEEL_AUDIT.md` and `python/README.md` accordingly.
4. If keeping both pyprojects, add comments explaining one is release-authoritative and the other is local-dev convenience.

## Acceptance criteria

- A maintainer can build a wheel by following exactly one documented command path.
- CI workflow and docs use the same path.
- There is no ambiguity about which metadata is published to PyPI.

---

# Workstream 4: Version-source-of-truth polish

## Goal

Prevent version drift between Rust crate, PyO3 module, Python package, docs, and release tags.

## Current concern

The Python package has a static `__version__ = "0.1.0"`, while the PyO3 module also exposes a Cargo-derived version. Both must remain aligned.

## Required decision

Choose one source of truth.

Recommended:

- PyO3 native module exports `_eggress.__version__` from `CARGO_PKG_VERSION`.
- Python `eggress.__version__` imports that value rather than duplicating a literal.
- `python/pyproject.toml` and `crates/eggress-python/pyproject.toml` versions are still static due packaging metadata, but release checklist verifies alignment.

## Required changes

1. Change `python/eggress/__init__.py` to source version from native module if practical:

```python
from eggress._eggress import __version__ as __version__
```

2. Add a Python test:

```python
def test_version_matches_native_module(): ...
```

3. Add a release checklist item comparing:

- workspace/crate version;
- `crates/eggress-python/Cargo.toml` version;
- pyproject version;
- `eggress.__version__`;
- tag.

## Acceptance criteria

- Runtime Python version surface cannot drift from native module version.
- Release docs include explicit metadata version check.

---

# Workstream 5: RC wording and evidence taxonomy correction

## Goal

Ensure release-candidate docs do not imply unrun gated differential/interop tests are proof.

## Current concern

The release-candidate doc lists some pproxy-compatible features as “verified by differential or runtime tests,” while the same doc says gated differential tests were not run. This wording is close to overclaiming.

## Required docs

Update:

```text
docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md
docs/PHASE_17_TRUE_PPROXY_PARITY_RELEASE_CANDIDATE_COMPLETION.md
docs/PARITY_MATRIX.md
docs/DIFFERENTIAL_TESTING.md
README.md
```

## Required terminology

Use explicit evidence labels:

- `unit-tested`;
- `runtime-tested`;
- `synthetic-tested`;
- `known-good interop-tested`;
- `pproxy differential-tested`;
- `gated, not run`;
- `documented non-parity`.

## Required correction

For any claim currently saying “pproxy-compatible,” require one of:

1. pproxy differential test actually run and recorded;
2. documented as `runtime-tested pproxy-style behavior`, not differential-compatible;
3. documented as `compatible claim pending gated differential`.

## Acceptance criteria

- Release docs no longer treat unrun gated tests as proof.
- RC claim remains accurate: pre-release, common pproxy-style behavior, not exhaustive GA parity.

---

# Workstream 6: Pre-release vs GA release-blocker wording

## Goal

Make “no release blockers” precise.

## Current concern

The docs say no release blockers, but also list hosted CI unavailable, gated differentials unrun, TestPyPI pending, formal pproxy benchmarks deferred, and residual security items. This is acceptable for a pre-release RC but not for GA.

## Required wording

Replace broad language like:

```text
Release blockers: None
```

with:

```text
Pre-release RC blockers: None identified.
GA blockers remain and are listed below.
```

## Required GA blocker list

At minimum:

- hosted CI must run successfully or have a documented release fallback;
- TestPyPI install must be verified;
- at least core pproxy differential tests should run or be explicitly scoped out;
- Shadowsocks UDP interop should hard-fail on failure if claimed standard-compatible;
- formal wheel install tests on supported platforms;
- security residuals triaged for GA.

## Acceptance criteria

- Docs distinguish RC readiness from GA readiness.
- No user can reasonably read the docs as saying exhaustive pproxy parity is complete.

---

# Workstream 7: Workflow and wheel-script path check

## Goal

Verify workflow commands match the authoritative packaging entrypoint and package layout.

## Files

```text
.github/workflows/python-test.yml
.github/workflows/python-wheels.yml
.github/workflows/publish-pypi.yml
scripts/test_wheel.sh
docs/PYPI_RELEASE.md
```

## Required checks

1. `maturin develop` runs from documented working directory.
2. `maturin build --release` runs from documented working directory.
3. `pip install dist/eggress-*.whl` path is correct relative to workflow/script cwd.
4. Tests import the installed wheel, not the source tree accidentally.
5. `python-source` path in pyproject matches actual layout.
6. `py.typed` is included.
7. License and README paths resolve from build cwd.

## Acceptance criteria

- Workflow/script/doc commands agree.
- Wheel build has one canonical path.

---

# Workstream 8: Mypy/typing debt classification

## Goal

Make the Python type-checking status honest and actionable.

## Current status

The release-candidate doc says `mypy` has 20 expected false-positive `_inner` attribute errors because PyO3 native types are invisible to mypy.

## Required decision

Choose one:

### Option A: Accept as known pre-release typing debt

- Document exact error class.
- Exclude those checks from required release commands.
- Create a future task for `.pyi` stubs or Protocol wrappers.

### Option B: Fix now

- Add `.pyi` stubs for native `_eggress` module.
- Or add type aliases/casts in Python wrappers.
- Make `mypy python/eggress` pass.

## Preferred outcome

For RC polish, either is acceptable. For GA, type checking should pass or have a narrow documented ignore configuration.

## Acceptance criteria

- Docs and verification table do not list mypy as both expected-failing and “passed.”
- Future action is explicit if not fixed.

---

# Workstream 9: TestPyPI status and package-name reservation

## Goal

Clarify whether `eggress` is actually available and usable on TestPyPI/PyPI.

## Tasks

1. Check whether package name `eggress` is available on PyPI/TestPyPI before release.
2. If not available, document fallback names.
3. Run TestPyPI upload if credentials/environment are available.
4. If not available, mark TestPyPI as pending, not complete.

## Docs

Update:

```text
docs/PYPI_RELEASE.md
docs/WHEEL_AUDIT.md
docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md
```

## Acceptance criteria

- Package publication status is explicit.
- RC can be tagged even if TestPyPI publish is pending, but docs say so.

---

# Workstream 10: Completion record

## Required doc

Add:

```text
docs/PHASE_17_RC_POLISH_COMPLETION.md
```

Required sections:

- commit list;
- embed lifecycle decision;
- Python lifecycle decision;
- authoritative packaging entrypoint;
- version source of truth;
- RC vs GA blocker wording changes;
- mypy/typing status;
- TestPyPI/package-name status;
- local verification commands;
- remaining blockers before `v0.1.0-rc.1`;
- remaining blockers before GA.

---

# Recommended commit sequence

1. Embed lifecycle audit comments/tests or simplification.
2. Python lifecycle docs/tests/idempotency polish.
3. Packaging entrypoint consolidation and workflow/script updates.
4. Version source-of-truth update and test.
5. RC evidence wording correction.
6. RC-vs-GA blocker wording correction.
7. Mypy/typing status correction or stubs.
8. TestPyPI/package-name status docs.
9. Completion record.

---

# Required verification

Rust:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p eggress-embed
cargo test -p eggress-python
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Python local:

```bash
maturin develop
python -m pytest python/tests
python -m ruff check python
```

Typing, depending on chosen policy:

```bash
python -m mypy python/eggress
```

or document exact known false positives and exclude mypy from RC pass criteria.

Wheel path:

```bash
maturin build --release
scripts/test_wheel.sh
```

Use the canonical working directory selected in Workstream 3.

Optional/gated:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python -m pytest python/tests/test_pproxy_differential.py
```

If not run, docs must keep them marked unverified.

---

# Definition of done

The RC polish pass is complete only when:

1. Embed runtime ownership is documented and tested, or simplified.
2. Python lifecycle docs/tests prefer explicit shutdown/context managers.
3. Python shutdown is idempotent at the wrapper layer.
4. One packaging build entrypoint is authoritative.
5. Workflow/script/docs use the same packaging path.
6. Python runtime version surface is derived from the native module or release docs enforce version alignment.
7. RC docs do not count unrun gated tests as proof.
8. Docs distinguish pre-release RC blockers from GA blockers.
9. Mypy/typing status is honest and actionable.
10. TestPyPI/package-name status is explicit.
11. Local Rust/Python checks pass according to updated criteria.
12. `docs/PHASE_17_RC_POLISH_COMPLETION.md` exists.

## Expected final status

After this pass, the repo should be suitable for a conservative pre-release tag such as:

```text
v0.1.0-rc.1
```

It should still not claim GA-level exhaustive pproxy parity until gated differential/interop testing, TestPyPI/wheel validation, and GA security items are completed.
