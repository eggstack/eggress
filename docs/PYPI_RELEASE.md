# Python Release Procedure

Build, test, and publish the `eggress` Python packages locally. Python
publication is separate from crates.io and must not be coupled to the Rust
release workflow.

## Packages

| Package | Source | Purpose |
|---------|--------|---------|
| `eggress` | `crates/eggress-python/` | Core Python bindings (PyO3 + native extension) |
| `eggress-pproxy-compat` | `python-pproxy-compat/` | Separate `import pproxy` compatibility layer |

The canonical `eggress` wheel never aliases `pproxy` through `sys.modules`.

## Prerequisites

- Rust stable toolchain
- Python >= 3.9 with `pip`
- `maturin` (`pip install "maturin>=1.0,<2.0"`)
- `twine` (`pip install twine`)
- PyPI account with API token (local `~/.pypirc` or `TWINE_USERNAME`/`TWINE_PASSWORD`)

## Build

```bash
# Build the core wheel
(cd crates/eggress-python && maturin build --release --out ../../dist)

# Build the compat wheel
python3 -m pip wheel --no-deps --wheel-dir dist ./python-pproxy-compat

# Build source distribution
(cd crates/eggress-python && maturin sdist --out ../../dist)
```

## Test in a clean venv

```bash
python3 -m venv .venv-release-test
.venv-release-test/bin/pip install dist/eggress-*.whl
.venv-release-test/bin/pip install dist/eggress_pproxy_compat-*.whl
.venv-release-test/bin/pip install pytest pytest-asyncio
.venv-release-test/bin/python -m pytest python/tests tests/compat -q
.venv-release-test/bin/python -c "import eggress; print(eggress.__version__)"
.venv-release-test/bin/python -c "import pproxy; print('pproxy import OK')"
deactivate
rm -rf .venv-release-test
```

## Check metadata

```bash
twine check dist/*
```

## Upload

```bash
# Optional: TestPyPI first
twine upload --repository testpypi dist/*

# Production
twine upload dist/*
```

If a published version is broken, yank it via the PyPI web interface or
`twine upload --repository pypi --replace dist/*`, then bump the patch
version and publish a corrected release. Crates.io versions are immutable;
the same roll-forward policy applies here.

## Version alignment

Python package version is aligned with the Rust workspace version. See
`crates/eggress-python/pyproject.toml` for the authoritative source.

## Supply chain checks

Before upload, verify wheel contents:

```bash
python -m zipfile -l dist/eggress-*.whl
```

Confirm that `eggress/_eggress.*.so` (or `.dylib`/`.pyd`), `eggress/__init__.py`,
`eggress/py.typed`, `METADATA`, and `RECORD` are present. Confirm that no
`.env`, keys, certs, or test-only configs are included.
