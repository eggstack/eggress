#!/usr/bin/env bash
# Tier 3: Compare eggress vs pproxy performance
# Usage: EGRESS_REQUIRE_PPROXY_PERF=1 scripts/perf/run_pproxy_comparison.sh
set -euo pipefail

if [ "${EGRESS_REQUIRE_PPROXY_PERF:-}" != "1" ]; then
    echo "ERROR: Set EGRESS_REQUIRE_PPROXY_PERF=1 to run pproxy comparisons."
    echo "This requires pproxy==2.7.9 installed."
    exit 1
fi

# Check pproxy is available
if ! python3 -c "import pproxy" 2>/dev/null; then
    echo "ERROR: pproxy not installed. Run 'pip install pproxy==2.7.9'."
    exit 1
fi

echo "=== pproxy Performance Comparison ==="
echo "Environment: $(uname -s) $(uname -m) Rust $(rustc --version)"
echo ""

echo "--- HTTP CONNECT Relay ---"
echo "eggress: running performance smoke test..."
cargo test -p eggress-runtime --test performance_smoke -- performance_tcp_relay_smoke --nocapture 2>&1 | tail -5
echo ""
echo "pproxy: requires manual setup with pproxy CLI."
echo "Compare by running both against the same TCP echo server."
echo ""

echo "--- SOCKS5 Relay ---"
echo "eggress: running SOCKS5 benchmark..."
cargo bench --package eggress-bench -- tcp_relay 2>&1 | tail -5
echo ""
echo "pproxy: requires manual setup."
echo ""

echo "--- Comparison Notes ---"
echo "- Record exact environment (OS, CPU, Rust version) for reproducibility."
echo "- pproxy comparison is context, not acceptance gate."
echo "- Do not claim superiority without reproducible numbers."
echo "- See docs/performance/README.md for interpretation guidelines."
