#!/usr/bin/env bash
set -euo pipefail

# ── Parse flags ────────────────────────────────────────────────────

ORACLE_PYTHON=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --oracle-python)
            ORACLE_PYTHON="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

# ── Resolve interpreter ────────────────────────────────────────────

if [ -z "$ORACLE_PYTHON" ]; then
    ORACLE_PYTHON="${EGRESS_ORACLE_PYTHON:-python3}"
fi

echo "=== Checking prerequisites ==="

if ! "$ORACLE_PYTHON" --version >/dev/null 2>&1; then
    echo "ERROR: oracle interpreter not found: $ORACLE_PYTHON"
    exit 1
fi

if ! "$ORACLE_PYTHON" -c "import pproxy" >/dev/null 2>&1; then
    echo "ERROR: pproxy not installed in oracle interpreter: $ORACLE_PYTHON"
    exit 1
fi

echo "=== Running standalone UDP differential tests ==="
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 EGRESS_ORACLE_PYTHON="$ORACLE_PYTHON" cargo test \
    -p eggress-cli \
    --test differential_pproxy \
    -- differential_standalone_udp \
    --ignored \
    --nocapture

echo ""
echo "=== All standalone UDP differential tests passed ==="
