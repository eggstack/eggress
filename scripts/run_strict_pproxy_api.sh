#!/usr/bin/env bash
# Tier 2 — Paired API oracle comparison runner
#
# Creates clean oracle (pproxy==2.7.9) and candidate (eggress + compat) venvs,
# runs the strict API probes in both, and compares observations.
#
# Usage:
#     ./scripts/run_strict_pproxy_api.sh                    # Full run
#     ./scripts/run_strict_pproxy_api.sh --dry-run          # List records
#     ./scripts/run_strict_pproxy_api.sh --category python_namespace  # Filter
#
# Requires: python3, pip, maturin (for candidate wheel build)
# Exit codes: 0 = all pass, 1 = mismatches, 2 = harness error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

ORACLE_VENV="${ORACLE_VENV:-.venv-oracle-api}"
CANDIDATE_VENV="${CANDIDATE_VENV:-.venv-candidate-api}"
OUTPUT_DIR="${OUTPUT_DIR:-target/strict/paired_observations}"

echo "=== Tier 2: Paired API Oracle Comparison ==="
echo "Oracle venv:    $ORACLE_VENV"
echo "Candidate venv: $CANDIDATE_VENV"
echo "Output dir:     $OUTPUT_DIR"
echo ""

# Check for required tools
if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 not found" >&2
    exit 2
fi

# Setup oracle venv (pproxy 2.7.9)
if [ ! -d "$ORACLE_VENV" ]; then
    echo "Creating oracle venv..."
    python3 -m venv "$ORACLE_VENV"
    "$ORACLE_VENV/bin/pip" install --upgrade pip >/dev/null 2>&1
    "$ORACLE_VENV/bin/pip" install "pproxy==2.7.9" >/dev/null 2>&1
    echo "Oracle venv ready."
else
    echo "Using existing oracle venv."
fi

# Setup candidate venv (eggress + compat)
if [ ! -d "$CANDIDATE_VENV" ]; then
    echo "Creating candidate venv..."
    python3 -m venv "$CANDIDATE_VENV"
    "$CANDIDATE_VENV/bin/pip" install --upgrade pip >/dev/null 2>&1
    "$CANDIDATE_VENV/bin/pip" install maturin pytest pytest-asyncio >/dev/null 2>&1

    echo "Building eggress wheel..."
    maturin build --release --out target/wheels 2>/dev/null

    EGGRESS_WHEEL=$(ls target/wheels/eggress-*.whl 2>/dev/null | head -1)
    if [ -n "$EGGRESS_WHEEL" ]; then
        "$CANDIDATE_VENV/bin/pip" install "$EGGRESS_WHEEL" >/dev/null 2>&1
    else
        echo "ERROR: Failed to build eggress wheel" >&2
        exit 2
    fi

    echo "Building compat wheel..."
    "$CANDIDATE_VENV/bin/pip" wheel --no-deps --wheel-dir target/wheels ./python-pproxy-compat >/dev/null 2>&1
    COMPAT_WHEEL=$(ls target/wheels/eggress_pproxy_compat-*.whl 2>/dev/null | head -1)
    if [ -n "$COMPAT_WHEEL" ]; then
        "$CANDIDATE_VENV/bin/pip" install "$COMPAT_WHEEL" >/dev/null 2>&1
    fi

    echo "Candidate venv ready."
else
    echo "Using existing candidate venv."
fi

# Verify venvs
echo ""
echo "Verifying oracle imports..."
"$ORACLE_VENV/bin/python" -c "import pproxy; print(f'  pproxy version: {getattr(pproxy, \"__version__\", \"unknown\")}')" 2>&1 || {
    echo "ERROR: Oracle venv cannot import pproxy" >&2
    exit 2
}

echo "Verifying candidate imports..."
"$CANDIDATE_VENV/bin/python" -c "import pproxy; print(f'  pproxy version: {getattr(pproxy, \"__version__\", \"unknown\")}')" 2>&1 || {
    echo "ERROR: Candidate venv cannot import pproxy" >&2
    exit 2
}

echo ""

# Run paired comparison
python3 "$SCRIPT_DIR/run_strict_pproxy_api.py" \
    --oracle-venv "$ORACLE_VENV" \
    --candidate-venv "$CANDIDATE_VENV" \
    --output-dir "$OUTPUT_DIR" \
    "$@"
