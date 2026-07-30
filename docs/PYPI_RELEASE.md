# Python Release Procedure

Build, test, and publish the `eggress` Python package locally. Python
publication is separate from crates.io and must not be coupled to the Rust
release workflow.

## Packages

| Package | Source | Purpose |
|---------|--------|---------|
| `eggress` | `crates/eggress-python/` | Core Python bindings (PyO3 + native extension) |

## Prerequisites

- Rust stable toolchain
- Python >= 3.9 with `pip`
- `maturin` (`pip install "maturin>=1.0,<2.0"`)
- `twine` (`pip install twine`)
- PyPI account with API token (local `~/.pypirc` or `TWINE_USERNAME`/`TWINE_PASSWORD`)

## Build

```bash
rm -rf dist
mkdir -p dist

# Build the core wheel
(cd crates/eggress-python && maturin build --release --out ../../dist)

# Build source distribution
(cd crates/eggress-python && maturin sdist --out ../../dist)
```

## Test in a clean venv

```bash
python3 -m venv /tmp/test-eggress
/tmp/test-eggress/bin/python -m pip install dist/eggress-*.whl
/tmp/test-eggress/bin/python -c "import eggress; print(eggress.__version__)"
/tmp/test-eggress/bin/python -m pytest python/tests -q
rm -rf /tmp/test-eggress
```

## Publish

```bash
# Upload to TestPyPI first
twine upload --repository testpypi dist/*

# Upload to PyPI
twine upload dist/*
```

## Post-publish verification

```bash
python3 -m venv /tmp/verify-eggress
/tmp/verify-eggress/bin/python -m pip install eggress
/tmp/verify-eggress/bin/python -c "import eggress; print(eggress.__version__)"
rm -rf /tmp/verify-eggress
```
