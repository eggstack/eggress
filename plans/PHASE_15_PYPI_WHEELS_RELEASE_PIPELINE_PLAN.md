# Phase 15 Detailed Plan: PyPI Packaging, Wheels, and Release Pipeline

## Purpose

Phase 15 turns the local Python bindings from Phase 14 into a distributable Python package. The goal is for typical Python users to install Eggress from PyPI without having Rust installed, then embed a Rust-powered proxy service from Python.

This phase is packaging, wheel building, release workflow, and installation validation. Do not expand the Python API substantially in this phase except where required for packaging quality.

---

# Prerequisites

Required from Phase 14:

- `crates/eggress-python` exists and builds with maturin;
- `python/` package imports locally;
- Python tests start/stop service and proxy local traffic;
- type hints and Python README exist;
- Rust/Python error mapping is implemented;
- no import-time side effects.

If Phase 14 is incomplete, finish bindings before packaging.

---

# Non-goals

Do not implement:

- new Python APIs beyond packaging fixes;
- new proxy protocols;
- pproxy helper API expansion beyond existing bindings;
- standard Shadowsocks TCP rework;
- automatic OS proxy installation;
- hosted CI billing remediation outside documentation/workflow setup.

---

# Workstream 1: Finalize Python package metadata

## Target files

```text
python/pyproject.toml
python/README.md
python/LICENSE or license reference
python/eggress/__init__.py
```

## Required metadata

- package name: `eggress` or final reserved PyPI name;
- version: aligned with Rust crate version unless policy says otherwise;
- license;
- authors/maintainers;
- Python version requirement;
- classifiers;
- project URLs;
- README content type;
- included type information via `py.typed`;
- maturin build backend.

## Version policy

Recommended lockstep:

- Rust workspace version == Python package version;
- `eggress.__version__` exposes the same version;
- release tags use `vX.Y.Z`;
- PyPI package changelog references GitHub release.

## Acceptance criteria

- `python -m build` or `maturin build` reads metadata cleanly.
- Package metadata renders acceptably on TestPyPI.

---

# Workstream 2: Wheel build configuration

## Goal

Build wheels for common platforms without requiring end users to install Rust.

## Initial wheel matrix

Target:

- Linux x86_64 manylinux;
- Linux aarch64 manylinux;
- macOS x86_64;
- macOS arm64;
- Windows x86_64.

Optional later:

- musllinux x86_64;
- musllinux aarch64;
- Windows arm64.

## Dependency policy

Verify wheels do not introduce prohibited production dependencies:

- no OpenSSL/native-tls;
- no production `aws-lc-sys` if policy remains ring/rustls-only;
- no unexpected dynamic library dependencies;
- no bundled secrets/certs except normal root store behavior;
- no platform-specific native build tools required at install time.

## Acceptance criteria

- `maturin build --release` produces a local wheel.
- Built wheel installs in a clean venv without Rust.

---

# Workstream 3: Local wheel install tests

## Goal

Test the wheel artifact, not only editable development installs.

## Required scripts

Add scripts or documented commands:

```bash
python -m venv .venv-wheel-test
. .venv-wheel-test/bin/activate
pip install dist/eggress-*.whl
python -m pytest python/tests
```

## Test scenarios

- import package;
- start service;
- proxy local traffic;
- metrics/status;
- reload;
- shutdown;
- error redaction.

## Acceptance criteria

- Wheel test succeeds in a clean environment.

---

# Workstream 4: GitHub Actions release workflows

## Goal

Create workflows now, while honestly documenting that hosted CI may remain unavailable until billing/status is fixed.

## Workflows

Suggested files:

```text
.github/workflows/python-wheels.yml
.github/workflows/python-test.yml
.github/workflows/publish-pypi.yml
```

## `python-test.yml`

Trigger:

- pull request;
- push to main;
- manual dispatch.

Jobs:

- install Rust stable;
- install Python versions, e.g. 3.9–3.13;
- install maturin;
- `maturin develop`;
- `python -m pytest python/tests`.

## `python-wheels.yml`

Trigger:

- tag `v*`;
- manual dispatch.

Use maturin-action or equivalent.

Build wheels for target matrix and upload artifacts.

## `publish-pypi.yml`

Trigger:

- manual dispatch;
- release publish;
- tag with approval if configured.

Use trusted publishing if possible. Prefer TestPyPI first.

## CI limitation

Update `docs/CI_STATUS.md`:

- workflow files exist;
- hosted runs may be blocked by GitHub Actions billing/status;
- local release commands remain source of truth until workflow runs are visible.

## Acceptance criteria

- Workflow definitions are present and syntactically plausible.
- Docs do not claim hosted releases work until verified.

---

# Workstream 5: TestPyPI release dry run

## Goal

Verify package upload/install path before production PyPI.

## Commands

```bash
maturin build --release
maturin upload --repository testpypi dist/*
python -m venv .venv-testpypi
. .venv-testpypi/bin/activate
pip install --index-url https://test.pypi.org/simple --extra-index-url https://pypi.org/simple eggress==X.Y.Z
python -c "import eggress; print(egress.__version__)"
python -m pytest python/tests
```

## Requirements

- do not upload to production PyPI until TestPyPI install is verified;
- record exact version used;
- ensure wheels are used rather than local source build where possible.

## Acceptance criteria

- TestPyPI package installs and passes smoke tests, or blocker is documented.

---

# Workstream 6: Production PyPI release procedure

## Goal

Document a controlled production release path.

## Required doc

Create:

```text
docs/PYPI_RELEASE.md
```

Required sections:

- prerequisites;
- version bump process;
- changelog/release note process;
- local build commands;
- wheel test commands;
- TestPyPI upload;
- TestPyPI install verification;
- production PyPI upload;
- post-release verification;
- rollback/yank policy;
- known CI limitations.

## Acceptance criteria

- Maintainer can follow the document without guessing.

---

# Workstream 7: Platform-specific validation

## Goal

Catch common packaging/runtime errors.

## Linux

- manylinux wheel imports;
- service starts on loopback;
- no unexpected dynamic dependency errors.

## macOS

- arm64 wheel imports on Apple Silicon;
- x86_64 wheel if runner available;
- service binds loopback;
- no codesign surprises for local use.

## Windows

- wheel imports;
- service starts on `127.0.0.1:0`;
- tests avoid Unix-only assumptions;
- process/thread shutdown works.

## Acceptance criteria

- Platform support table exists in docs.
- Unsupported platforms are explicit.

---

# Workstream 8: Supply-chain and artifact audit

## Goal

Ensure release artifacts do not undermine dependency/security policy.

## Checks

- `cargo deny check`;
- `cargo audit`;
- inspect wheel contents;
- verify `py.typed` included;
- verify no `.env`, keys, certs, or test-only configs included;
- verify license files included;
- verify README included;
- verify native extension name/import path correct.

## Suggested commands

```bash
python -m zipfile -l dist/*.whl
pip install check-wheel-contents
check-wheel-contents dist/*.whl
```

## Acceptance criteria

- Wheel artifacts pass content sanity checks.

---

# Workstream 9: Docs update

## Required docs

Create/update:

```text
docs/PYPI_RELEASE.md
docs/PYTHON_BINDINGS.md
docs/CI_STATUS.md
docs/RELEASE_READINESS.md
python/README.md
README.md
AGENTS.md
```

## Required content

- install from PyPI;
- install from wheel;
- install from source;
- local development build;
- TestPyPI process;
- platform support;
- known limitations;
- security/dependency policy;
- troubleshooting.

---

# Recommended commit sequence

1. Finalize pyproject/package metadata.
2. Add wheel build/test scripts or documented commands.
3. Add GitHub Actions workflows.
4. Add TestPyPI/production release docs.
5. Add platform validation docs.
6. Add artifact audit checks.
7. Add completion record.

---

# Required verification

Rust/Python dev:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
maturin develop
python -m pytest python/tests
```

Wheel build/install:

```bash
maturin build --release
python -m venv .venv-wheel-test
. .venv-wheel-test/bin/activate
pip install dist/eggress-*.whl
python -m pytest python/tests
```

Supply chain:

```bash
cargo deny check
cargo audit
python -m zipfile -l dist/*.whl
check-wheel-contents dist/*.whl
```

---

# Definition of done

Phase 15 is complete only when:

1. Python package metadata is complete.
2. Local release wheel builds.
3. Built wheel installs in a clean venv without Rust.
4. Wheel-installed package can start/stop/proxy local traffic.
5. TestPyPI process is documented and preferably exercised.
6. Production PyPI release procedure is documented.
7. Release workflows exist, even if hosted CI remains unavailable.
8. CI status docs honestly state workflow availability.
9. Wheel contents are audited.
10. Platform support matrix exists.

## Completion record

Add:

```text
docs/PHASE_15_PYPI_WHEELS_RELEASE_COMPLETION.md
```

Include wheel targets, test results, TestPyPI status, workflow status, and blockers for Phase 16.
