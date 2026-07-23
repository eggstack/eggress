#!/usr/bin/env bash
set -euo pipefail

PASS=0
FAIL=0
RESULTS=()

run_gate() {
    local name="$1"
    shift
    echo "=== GATE: $name ==="
    local start=$(date +%s)
    if "$@"; then
        local end=$(date +%s)
        local elapsed=$((end - start))
        RESULTS+=("PASS: $name (${elapsed}s)")
        PASS=$((PASS + 1))
    else
        local end=$(date +%s)
        local elapsed=$((end - start))
        RESULTS+=("FAIL: $name (${elapsed}s)")
        FAIL=$((FAIL + 1))
    fi
    echo ""
}

echo "=== MILESTONES A-C CLOSURE AUDIT ==="
echo "Started at $(date)"
echo ""

run_gate "cargo fmt" cargo fmt --all -- --check
run_gate "cargo check" cargo check --workspace --all-targets
run_gate "cargo clippy" cargo clippy --workspace --all-targets -- -D warnings
run_gate "cargo test" cargo test --workspace
run_gate "release-docs check" python3 scripts/check_release_docs.py

echo "=== AUDIT SUMMARY ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo ""
for r in "${RESULTS[@]}"; do
    echo "  $r"
done
echo ""

if [ "$FAIL" -gt 0 ]; then
    echo "AUDIT FAILED: $FAIL gate(s) failed"
    exit 1
else
    echo "AUDIT PASSED: all $PASS gates passed"
    exit 0
fi
