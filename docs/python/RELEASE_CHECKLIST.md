# Release Checklist

Pre-release checklist for the `eggress` Python package.

## 1. Manifest validation

```bash
cargo test -p eggress-testkit manifest
```

Verifies `tests/compat/pproxy_manifest.toml` is internally consistent and all
evidence levels match the `egress_status` claims.

## 2. Python tests pass

```bash
python -m pytest python/tests -q
```

All non-gated tests must pass.

## 3. Wheel smoke test

```bash
./scripts/test_wheel.sh
```

Builds a wheel, installs it in a clean venv, runs pytest, and verifies the
native module loads.

## 4. Source distribution smoke test

```bash
cd crates/eggress-python
maturin sdist --out ../../dist
```

Verify the sdist builds without error. Manual inspection:

```bash
tar tzf dist/eggress-*.tar.gz | head -20
```

Check that `crates/`, `python/`, and `pyproject.toml` are included.

## 5. pproxy oracle gated tests (if applicable)

```bash
EGRESS_REQUIRE_PPROXY_ORACLE=1 python -m pytest python/tests/test_pproxy_oracle.py -v
```

Run only if the pproxy oracle test harness is relevant to the release. These
tests verify Python API behavior against a frozen pproxy 2.7.9 snapshot.

## 6. README metadata renders correctly

Verify the package README renders correctly on PyPI:

```bash
# Check the long description source
python -c "
import tomllib
with open('crates/eggress-python/pyproject.toml', 'rb') as f:
    cfg = tomllib.load(f)
print('readme:', cfg['project']['readme'])
"
```

Confirm the `readme` path exists and is valid. If using a PyPI-rendered
README, verify markdown syntax manually.

## 7. Version bump in pyproject.toml

Update the version in `crates/eggress-python/pyproject.toml`:

```toml
[project]
version = "X.Y.Z"
```

The version is the single source of truth for the Python package. The native
module reads `CARGO_PKG_VERSION` at build time.

## 8. Changelog entry

When `CHANGELOG.md` exists, add an entry for the release version covering:

- New features
- Bug fixes
- Breaking changes
- Deprecations

## 9. TestPyPI dry run

```bash
cd crates/eggress-python
maturin build --release --out ../../dist
maturin upload --repository testpypi ../../dist/*
```

Test install from TestPyPI:

```bash
python3 -m venv .venv-testpypi
source .venv-testpypi/bin/activate
pip install --index-url https://test.pypi.org/simple --extra-index-url https://pypi.org/simple eggress==X.Y.Z
python -c "import eggress; print(eggress.__version__)"
python -m pytest python/tests -v
deactivate
rm -rf .venv-testpypi
```

## 10. Production PyPI manual approval

Requires explicit human approval. No automated publish.

```bash
maturin upload dist/*
```

## 11. Tag and GitHub release

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

Create a GitHub release with:

- Release notes summarizing changes
- Wheel artifacts attached (from `dist/`)
- Link to PyPI package page

## Post-release verification

```bash
python3 -m venv .venv-prod-test
source .venv-prod-test/bin/activate
pip install eggress==X.Y.Z
python -c "import eggress; print(eggress.__version__)"
python -m pytest python/tests -v
deactivate
rm -rf .venv-prod-test
```
