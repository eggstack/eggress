# Phase 32 Plan: Python Packaging, Import Strategy, and Distribution Compatibility

## Purpose

Phase 32 closes the first Python/PyPI compatibility block by making the package distribution, import strategy, wheel behavior, and optional pproxy-compatible namespace story explicit and reliable.

Earlier phases define and implement Python API compatibility surfaces. This phase ensures users can install the package, import it predictably, understand whether it is a drop-in replacement, and rely on wheels across supported platforms.

## Scope

This phase covers:

- PyPI/TestPyPI packaging metadata.
- Wheel build matrix and smoke tests.
- Import namespace strategy: `eggress`, `eggress.pproxy`, and any optional pproxy-compat alias.
- Versioning and feature metadata.
- Python/Rust ABI compatibility policy.
- Type hints and py.typed support.
- Source distribution behavior.
- Documentation for installation and migration.
- Release checklist and CI/workflow alignment.

## Non-goals

Do not publish to production PyPI unless explicitly intended and credentials/trusted publishing are configured.

Do not hijack the `pproxy` package name unless a deliberate separate distribution strategy is approved.

Do not claim full drop-in replacement unless Phases 29-31 evidence supports it.

Do not support unsupported Python versions simply to broaden classifiers.

## Work items

### 32.1 Define package and import strategy

Write an ADR for distribution/import naming.

Suggested path:

```text
docs/adr/ADR_python_import_and_distribution_strategy.md
```

Decisions to make:

- canonical import remains `import eggress`;
- compatibility import lives at `from eggress import pproxy` or `import eggress.pproxy`;
- whether to ship a separate optional package such as `eggress-pproxy`;
- whether to ever provide `import pproxy` compatibility;
- how to avoid accidental collision with upstream pproxy;
- how to communicate partial compatibility.

Recommended default:

- keep `eggress` as the canonical package;
- expose `eggress.pproxy` compatibility helpers;
- defer any top-level `pproxy` shim package until Python API parity is much stronger and legal/naming risks are assessed.

### 32.2 Audit pyproject and maturin metadata

Review and harden Python packaging metadata.

Check:

- package name;
- version source;
- Python requires;
- Rust extension module name;
- package include/exclude rules;
- license metadata;
- README rendering;
- classifiers;
- project URLs;
- optional dependencies;
- type hint marker;
- abi3 policy if used;
- debug/release build behavior.

Ensure metadata does not describe Eggress as full pproxy drop-in unless evidence supports that.

### 32.3 Wheel build matrix and platform policy

Define supported wheel targets.

Potential targets:

- Linux x86_64 manylinux;
- Linux aarch64 manylinux;
- macOS x86_64;
- macOS arm64;
- Windows x86_64;
- source distribution.

Document unsupported or deferred targets.

Tasks:

- review `.github/workflows/python-wheels.yml`;
- ensure maturin version pinned or bounded;
- ensure Rust toolchain version matches workspace policy;
- ensure artifacts are named clearly;
- ensure wheel tests import the package and run basic pproxy helper smoke tests;
- ensure wheels do not accidentally require local Rust toolchain after install.

### 32.4 Local wheel smoke tests

Add a repeatable script/test for wheel installation.

Suggested path:

```text
scripts/test_python_wheel.sh
python/tests/test_wheel_import_smoke.py
```

Smoke tests:

- install built wheel into fresh venv;
- `import eggress`;
- `import eggress.pproxy`;
- print version;
- run `pproxy.check_uri("socks5://127.0.0.1:1080")`;
- verify unsupported URI diagnostic;
- instantiate server object if Phase 30 implemented;
- no network test unless explicitly marked.

### 32.5 Type hints and `py.typed`

If Python APIs are intended for users, add type hints.

Tasks:

- add annotations to Python wrapper modules;
- expose dataclasses/protocols for diagnostics/results;
- include `py.typed` in package data;
- run `python -m compileall`;
- optionally run `mypy` if already in dev dependencies, but do not add heavy type tooling unless desired.

Tests:

- verify `py.typed` is included in wheel;
- verify public dataclasses are importable;
- verify stub/type hints match runtime objects enough for basic use.

### 32.6 Version and capability metadata

Expose version and capability information to Python users.

Potential APIs:

```python
import eggress

eggress.__version__
eggress.version()
eggress.capabilities()
eggress.pproxy.compatibility_version()
eggress.pproxy.supported_features()
```

Requirements:

- version matches package metadata;
- compatibility version states target pproxy version, e.g. `2.7.9`;
- feature list derives from manifest or a generated snapshot where practical;
- no stale hardcoded claims.

### 32.7 Source distribution check

Ensure sdist works or is explicitly unsupported.

Tasks:

- build sdist with maturin;
- inspect included files;
- install sdist in clean environment with Rust toolchain;
- verify import smoke;
- document Rust requirement for sdist builds;
- ensure generated docs/fixtures needed by Python tests are included or excluded intentionally.

### 32.8 Import collision and shim safety tests

Add tests for import behavior.

Tests:

- `import eggress` works;
- `import eggress.pproxy` works;
- `from eggress import pproxy` works;
- `import pproxy` is not shadowed unless a deliberate shim package is installed;
- installing Eggress alongside upstream pproxy does not break either import;
- optional pproxy oracle tests can import real pproxy and Eggress simultaneously.

This is critical: compatibility testing must not accidentally replace the oracle import.

### 32.9 Documentation: installation and migration

Create/update:

```text
docs/python/INSTALLATION.md
docs/python/PACKAGING.md
docs/python/IMPORT_STRATEGY.md
docs/python/MIGRATION_FROM_PPROXY.md
README.md
```

Docs should state:

- package name;
- install commands;
- supported platforms;
- what `eggress.pproxy` means;
- what is not drop-in yet;
- how to test compatibility;
- how to use with upstream pproxy installed;
- how to build from source;
- how to enable gated oracle tests.

### 32.10 Release workflow and CI alignment

Review Python workflows.

Tasks:

- ensure `python-test.yml` runs import and helper tests;
- ensure `python-wheels.yml` builds wheel matrix;
- ensure `publish-pypi.yml` uses trusted publishing or documented token path;
- ensure local verification commands exist because hosted CI may be unavailable due to billing;
- ensure release checklist does not say CI passed unless observed.

Add:

```text
docs/python/RELEASE_CHECKLIST.md
```

Checklist should include:

- manifest validation;
- Python tests;
- wheel smoke;
- sdist smoke;
- pproxy oracle gated tests;
- README metadata rendering;
- version bump;
- changelog entry;
- TestPyPI dry run;
- production PyPI manual approval.

### 32.11 Security and supply-chain review

Python packages introduce supply-chain concerns.

Tasks:

- verify no secrets in package data;
- ensure wheel build does not include target/debug artifacts;
- ensure generated config/test fixture files do not contain real credentials;
- verify long descriptions do not overclaim compatibility;
- verify dependency list is minimal;
- run `pip-audit` if available or document as optional;
- update `docs/SECURITY_REVIEW.md` with Python packaging notes.

### 32.12 Manifest and evidence updates

Add/refine manifest entries:

```text
python_package_import_eggress
python_package_import_eggress_pproxy
python_package_import_no_pproxy_shadow
python_wheel_linux_x86_64
python_wheel_linux_aarch64
python_wheel_macos_x86_64
python_wheel_macos_arm64
python_wheel_windows_x86_64
python_sdist_build
python_py_typed
python_version_metadata
python_capability_metadata
```

Evidence should be per-platform where possible. Do not mark a wheel target supported until it has a build/test path.

## Validation commands

Local packaging:

```bash
python -m pip install -U pip maturin build twine
maturin build --release
python -m venv /tmp/eggress-wheel-smoke
/tmp/eggress-wheel-smoke/bin/python -m pip install target/wheels/*.whl
/tmp/eggress-wheel-smoke/bin/python -m pytest python/tests/test_wheel_import_smoke.py -q
```

Python tests:

```bash
maturin develop
python -m pytest python/tests -q
python -m compileall python/eggress
```

Oracle coexistence:

```bash
python -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/compat/test_pproxy_api_oracle.py -q
```

Metadata:

```bash
python -m twine check target/wheels/*.whl target/wheels/*.tar.gz
cargo test -p eggress-testkit manifest
```

## Acceptance criteria

Phase 32 is complete when:

- Import/distribution strategy ADR exists.
- `eggress` and `eggress.pproxy` import behavior is tested.
- Eggress does not shadow upstream `pproxy` unless deliberately installed as a separate shim.
- Wheel smoke tests exist and pass locally.
- Supported wheel targets are documented.
- `py.typed` and type hints are present if public Python APIs are stable enough.
- Version and pproxy compatibility metadata are exposed.
- Source distribution behavior is tested or explicitly documented as deferred.
- Python release checklist exists.
- Manifest/evidence docs classify packaging targets by actual evidence.

## Handoff notes

The packaging phase is where wording matters. A package can be useful without being a full drop-in replacement. Keep the default import path honest: `eggress.pproxy` can be a compatibility layer, while top-level `pproxy` replacement should wait until real API parity evidence exists.
