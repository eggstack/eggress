#!/usr/bin/env bash
# Tier 1: Run local performance baseline
# Usage: scripts/perf/run_local_baseline.sh
set -euo pipefail

echo "=== Tier 0: Criterion Benchmarks ==="
echo "Building benchmarks..."
cargo bench --workspace --no-run 2>/dev/null
echo "Benchmarks compiled successfully."
echo ""
echo "Run 'cargo bench --workspace' to execute benchmarks."
echo "Results will be stored in target/criterion/"
echo ""

echo "=== Tier 1: Performance Smoke Tests ==="
cargo test -p eggress-runtime --test performance_smoke -- --nocapture 2>&1 | head -50
echo ""

echo "=== Python Binding Overhead ==="
if command -v python3 &>/dev/null && python3 -c "import eggress" 2>/dev/null; then
    python3 -m pytest python/tests/test_performance_smoke.py -v --tb=short 2>&1 | head -30
else
    echo "Python bindings not installed. Run 'maturin develop' first."
fi
echo ""

echo "=== Done ==="
echo "For soak tests: EGRESS_REQUIRE_SOAK=1 scripts/perf/run_soak.sh"
echo "For pproxy comparison: EGRESS_REQUIRE_PPROXY_PERF=1 scripts/perf/run_pproxy_comparison.sh"
