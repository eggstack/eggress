#!/usr/bin/env bash
# Tier 2: Run soak tests
# Usage: EGRESS_REQUIRE_SOAK=1 scripts/perf/run_soak.sh
set -euo pipefail

if [ "${EGRESS_REQUIRE_SOAK:-}" != "1" ]; then
    echo "ERROR: Set EGRESS_REQUIRE_SOAK=1 to run soak tests."
    echo "These tests run for 30-120 seconds and are not part of normal development."
    exit 1
fi

echo "=== Tier 2: Reverse Proxy Soak Test ==="
echo "Running reverse proxy soak (30-120s)..."
EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1 --nocapture 2>&1 | tail -20
echo ""

echo "=== Tier 2: Load Tests ==="
echo "Running 100 concurrent TCP session load test..."
cargo test -p eggress-runtime --test load -- --ignored --nocapture 2>&1 | tail -20
echo ""

echo "=== Soak Tests Complete ==="
