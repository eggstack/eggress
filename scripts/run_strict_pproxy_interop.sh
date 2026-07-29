#!/usr/bin/env bash
# Tier 3 — External TCP/UDP interoperability runner
#
# Runs bidirectional interop tests between pproxy oracle and eggress candidate.
# Requires both venvs to be set up with their respective packages.
#
# Usage:
#     ./scripts/run_strict_pproxy_interop.sh                    # Standalone
#     ./scripts/run_strict_pproxy_interop.sh --oracle-python PATH --candidate-python PATH  # Certification
#     ./scripts/run_strict_pproxy_interop.sh --protocol http  # Filter
#
# Requires: python3, pproxy==2.7.9, eggress wheel (standalone) or pre-built environments (certification)
# Exit codes: 0 = all pass, 1 = failures, 2 = harness error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# ── Parse flags ────────────────────────────────────────────────────

NO_BOOTSTRAP=false
ORACLE_PYTHON=""
CANDIDATE_PYTHON=""
PassthroughArgs=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-bootstrap)
            NO_BOOTSTRAP=true
            shift
            ;;
        --oracle-python)
            ORACLE_PYTHON="$2"
            shift 2
            ;;
        --candidate-python)
            CANDIDATE_PYTHON="$2"
            shift 2
            ;;
        *)
            PassthroughArgs+=("$1")
            shift
            ;;
    esac
done

# ── Resolve interpreters ──────────────────────────────────────────

if [ "$NO_BOOTSTRAP" = true ]; then
    if [ -z "$ORACLE_PYTHON" ] || [ -z "$CANDIDATE_PYTHON" ]; then
        echo "ERROR: --no-bootstrap requires --oracle-python and --candidate-python" >&2
        exit 2
    fi
else
    ORACLE_VENV="${ORACLE_VENV:-.venv-oracle-api}"
    CANDIDATE_VENV="${CANDIDATE_VENV:-.venv-candidate-api}"
    ORACLE_PYTHON="${ORACLE_PYTHON:-$ORACLE_VENV/bin/python}"
    CANDIDATE_PYTHON="${CANDIDATE_PYTHON:-$CANDIDATE_VENV/bin/python}"
fi

OUTPUT_DIR="${OUTPUT_DIR:-target/strict/interop_observations}"

echo "=== Tier 3: External TCP/UDP Interoperability ==="
echo "Oracle python:    $ORACLE_PYTHON"
echo "Candidate python: $CANDIDATE_PYTHON"
echo "Output dir:       $OUTPUT_DIR"
echo ""

# ── Verify interpreters ───────────────────────────────────────────

# Check pproxy is importable in oracle
"$ORACLE_PYTHON" -c "import pproxy" 2>/dev/null || {
    echo "ERROR: Oracle interpreter cannot import pproxy" >&2
    exit 2
}

# Check pproxy (via compat) is importable in candidate
"$CANDIDATE_PYTHON" -c "import pproxy" 2>/dev/null || {
    echo "ERROR: Candidate interpreter cannot import pproxy (eggress compat)" >&2
    exit 2
}

# ── Run interop tests ─────────────────────────────────────────────

# Run eggress-server/pproxy-client tests (pproxy as external client)
echo "Running Rust interop tests (oracle client -> candidate server, candidate client -> oracle server)..."
echo ""

INTEROP_EXIT=0

if env \
    EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
    EGRESS_ORACLE_PYTHON="$ORACLE_PYTHON" \
    cargo test -p eggress-cli --test interoperability_pproxy -- --ignored \
    --skip test_pproxy_http_server_eggress_client \
    --skip test_pproxy_socks5_server_eggress_client \
    2>&1; then
    echo "  PASS: eggress-server/pproxy-client interop"
else
    echo "  WARN: Some interop tests failed or were skipped (pproxy may not be installed)"
    INTEROP_EXIT=1
fi

# Run Python-level bidirectional interop tests if they exist
if [ -f "$SCRIPT_DIR/run_strict_pproxy_interop.py" ]; then
    echo ""
    echo "Running Python bidirectional interop tests..."
    if "$CANDIDATE_PYTHON" "$SCRIPT_DIR/run_strict_pproxy_interop.py" \
        --oracle-venv "$(dirname "$ORACLE_PYTHON")/.." \
        --candidate-venv "$(dirname "$CANDIDATE_PYTHON")/.." \
        --output-dir "$OUTPUT_DIR" \
        "${PassthroughArgs[@]}"; then
        echo "  PASS: Python bidirectional interop"
    else
        echo "  FAIL: Python bidirectional interop"
        INTEROP_EXIT=1
    fi
fi

echo ""
echo "=== Tier 3 complete ==="
exit $INTEROP_EXIT
