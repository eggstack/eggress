#!/usr/bin/env bash
set -euo pipefail

echo "=== Building wheel ==="
cd crates/eggress-python
maturin build --release --out ../../dist

echo "=== Creating test venv ==="
cd ../..
python3 -m venv .venv-wheel-test
source .venv-wheel-test/bin/activate

echo "=== Installing wheel ==="
pip install dist/eggress-*.whl

echo "=== Running tests ==="
pip install pytest
python -m pytest python/tests -v

echo "=== Cleanup ==="
deactivate
echo "Wheel test passed!"
