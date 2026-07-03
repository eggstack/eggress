#!/usr/bin/env bash
set -euo pipefail

echo "=== Building wheel ==="
cd crates/eggress-python
maturin build --release --out ../../dist

echo "=== Creating test venv ==="
cd ../..
python3 -m venv .venv-wheel-test
source .venv-wheel-test/bin/activate

echo "=== Verifying clean import path (no source tree contamination) ==="
# Use a fresh cwd so any conftest.py or local eggress/ directory cannot shadow the wheel install.
TEST_CWD="$(mktemp -d)"
cd "$TEST_CWD"
echo "Running smoke tests in clean cwd: $TEST_CWD"

echo "=== Installing wheel ==="
pip install "dist/eggress-*.whl"

echo "=== Verifying eggress loads from the installed wheel (not source tree) ==="
python - <<'PY'
import eggress
import eggress._eggress as native
import sys
import os

# Confirm eggress module comes from the site-packages install.
installed = os.path.realpath(eggress.__file__)
assert "site-packages" in installed or "dist-packages" in installed, (
    f"eggress loaded from non-installed path: {installed}"
)
print(f"eggress loaded from: {installed}")

# Confirm native module is bundled (compiled, not source).
assert native.__file__.endswith((".so", ".pyd", ".dylib")), (
    f"native module is not a compiled artifact: {native.__file__}"
)
print(f"native module: {native.__file__}")

# Confirm no source-tree contamination: PYTHONPATH should not include the repo.
for p in sys.path:
    assert "eggress/python" not in p and "/python/" not in p or p == "", (
        f"PYTHONPATH leaks source tree: {p}"
    )
print("PYTHONPATH clean: no source-tree leak")
PY

echo "=== Running wheel import smoke tests ==="
pip install pytest
EGRESS_EXPECT_INSTALLED_WHEEL=1 python -m pytest python/tests/test_wheel_import_smoke.py -v

echo "=== Running full test suite ==="
python -m pytest python/tests -v

echo "=== Cleanup ==="
deactivate
rm -rf "$TEST_CWD"
echo "Wheel test passed!"
