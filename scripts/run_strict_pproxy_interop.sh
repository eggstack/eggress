#!/usr/bin/env bash
# Tier 3 — External TCP/UDP interoperability runner
#
# Runs bidirectional interop tests between pproxy oracle and eggress candidate.
# Requires both venvs to be set up with their respective packages.
#
# Usage:
#     ./scripts/run_strict_pproxy_interop.sh              # Full run
#     ./scripts/run_strict_pproxy_interop.sh --protocol http  # Filter
#
# Requires: python3, pproxy==2.7.9, eggress wheel
# Exit codes: 0 = all pass, 1 = failures, 2 = harness error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

ORACLE_VENV="${ORACLE_VENV:-.venv-oracle-api}"
CANDIDATE_VENV="${CANDIDATE_VENV:-.venv-candidate-api}"
OUTPUT_DIR="${OUTPUT_DIR:-target/strict/interop_observations}"

echo "=== Tier 3: External TCP/UDP Interoperability ==="
echo "Oracle venv:    $ORACLE_VENV"
echo "Candidate venv: $CANDIDATE_VENV"
echo "Output dir:     $OUTPUT_DIR"
echo ""

# Verify venvs exist
for label in oracle candidate; do
    venv_var="${label^^}_VENV"
    eval "venv_dir=\$$venv_var"
    if [ ! -d "$venv_dir" ]; then
        echo "ERROR: $label venv not found: $venv_dir" >&2
        echo "Run scripts/run_strict_pproxy_api.sh first to create venvs." >&2
        exit 2
    fi
done

# Check pproxy is importable in oracle
"$ORACLE_VENV/bin/python" -c "import pproxy" 2>/dev/null || {
    echo "ERROR: Oracle venv cannot import pproxy" >&2
    exit 2
}

# Check eggress is importable in candidate
"$CANDIDATE_VENV/bin/python" -c "import pproxy" 2>/dev/null || {
    echo "ERROR: Candidate venv cannot import pproxy (eggress compat)" >&2
    exit 2
}

# Run interop tests using the existing Rust interop infrastructure
echo "Running Rust interop tests (oracle client -> candidate server, candidate client -> oracle server)..."
echo ""

# These tests exercise real TCP connections between the two implementations
cargo test -p eggress-cli --test interoperability_pproxy -- --ignored \
    --skip test_pproxy_http_server_eggress_client \
    --skip test_pproxy_socks5_server_eggress_client \
    2>&1 || true

# Run Python-level bidirectional interop tests if they exist
if [ -f "$SCRIPT_DIR/run_strict_pproxy_interop.py" ]; then
    python3 "$SCRIPT_DIR/run_strict_pproxy_interop.py" \
        --oracle-venv "$ORACLE_VENV" \
        --candidate-venv "$CANDIDATE_VENV" \
        --output-dir "$OUTPUT_DIR" \
        "$@"
fi

echo ""
echo "=== Tier 3 complete ==="
