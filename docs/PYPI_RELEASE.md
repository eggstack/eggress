# PyPI Release Procedure

This document covers building, testing, and publishing the `eggress` Python package to PyPI.

## Prerequisites

- Rust toolchain (stable)
- Python >= 3.9
- `maturin` (`pip install maturin`)
- PyPI account with API token (or trusted publisher configured)
- TestPyPI account (recommended for dry runs)

## Version Policy

- Python package version is aligned with the Rust workspace version
- Current version: see `crates/eggress-python/pyproject.toml` (authoritative)
- Release tags use the format `vX.Y.Z`
- `python/pyproject.toml` is a local-dev convenience only; builds for PyPI use `crates/eggress-python/pyproject.toml`

## Local Build and Test

### Build the wheel

```bash
cd crates/eggress-python
maturin build --release --out ../../dist
```

### Test in a clean venv

```bash
python3 -m venv .venv-wheel-test
source .venv-wheel-test/bin/activate
pip install dist/eggress-*.whl
pip install pytest
python -m pytest python/tests -v
deactivate
rm -rf .venv-wheel-test
```

Or use the helper script:

```bash
./scripts/test_wheel.sh
```

### Verify wheel contents

```bash
python -m zipfile -l dist/*.whl
```

Check that:
- `eggress/_eggress.*.so` (or `.dylib`/`.pyd`) is present
- `eggress/__init__.py` is present
- `eggress/py.typed` is present
- `eggress/config.py` and `eggress/service.py` are present
- No `.env`, keys, certs, or test-only configs are included
- `METADATA` and `RECORD` are present

## TestPyPI Release

TestPyPI is recommended for validating the upload/install pipeline before production.

**Status: Not yet published.** The package name `eggress` must be reserved on
TestPyPI before the first pre-release upload. This is a pre-release RC task,
not a GA requirement.

```bash
# Build
cd crates/eggress-python
maturin build --release --out ../../dist

# Upload to TestPyPI
maturin upload --repository testpypi ../../dist/*

# Test install from TestPyPI
python3 -m venv .venv-testpypi
source .venv-testpypi/bin/activate
pip install --index-url https://test.pypi.org/simple --extra-index-url https://pypi.org/simple eggress==0.1.0
python -c "import eggress; print(eggress.__version__)"
python -m pytest python/tests -v
deactivate
rm -rf .venv-testpypi
```

> **Note:** `--extra-index-url https://pypi.org/simple` is needed because TestPyPI may not have all
> transitive dependencies (e.g., pytest). This falls back to production PyPI for missing packages.

## Production PyPI Release

### 1. Update version

```bash
# Update version in python/pyproject.toml
# Update __version__ in python/eggress/__init__.py
# Update version in crates/eggress-python/Cargo.toml (optional, for reference)
```

### 2. Build release artifacts

```bash
cd crates/eggress-python
maturin build --release --out ../../dist
maturin sdist --out ../../dist
```

### 3. Run wheel tests

```bash
cd ../..
./scripts/test_wheel.sh
```

### 4. Upload to PyPI

```bash
maturin upload ../../dist/*
```

Or use the GitHub Actions workflow:
- Trigger the `publish-pypi.yml` workflow with `repository: pypi`

### 5. Verify production install

```bash
python3 -m venv .venv-prod-test
source .venv-prod-test/bin/activate
pip install eggress==0.1.0
python -c "import eggress; print(eggress.__version__)"
python -m pytest python/tests -v
deactivate
rm -rf .venv-prod-test
```

### 6. Create GitHub release

```bash
git tag v0.1.0
git push origin v0.1.0
```

Create a GitHub release with:
- Release notes
- Wheel artifacts attached
- Link to PyPI package

## Rollback / Yank

If a published release has issues:

```bash
# Yank the release (keeps it installable but marks as broken)
pip install yank  # if needed
twine upload --repository pypi --replace dist/*
```

Or use the PyPI web interface to yank the release.

To publish a fixed version, bump the patch version (e.g., 0.1.0 -> 0.1.1) and follow the release process again.

## Known Limitations

- Hosted GitHub Actions CI is non-functional due to billing issues
- Local verification is the source of truth until CI resumes
- Wheel builds for Linux aarch64 require cross-compilation or CI with native arm64 runner
- The Rust crate `eggress-python` has `publish = false` (intentional — it is not published to crates.io)
- Windows arm64 wheels are not currently built

## Supply Chain Checks

Before any release, run:

```bash
cargo deny check
cargo audit
python -m zipfile -l dist/*.whl
```

See `docs/DEPENDENCY_POLICY.md` for the full dependency policy.
