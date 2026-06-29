#!/usr/bin/env bash
set -euo pipefail

echo "=== Checking prerequisites ==="

if ! python3 --version >/dev/null 2>&1; then
    echo "ERROR: python3 not found"
    exit 1
fi

if ! python3 -c "import pproxy" >/dev/null 2>&1; then
    echo "ERROR: pproxy not installed (pip install pproxy)"
    exit 1
fi

echo "=== Running standalone UDP differential tests ==="
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test \
    -p eggress-cli \
    --test differential_pproxy \
    -- differential_standalone_udp \
    --ignored \
    --nocapture

echo ""
echo "=== All standalone UDP differential tests passed ==="
