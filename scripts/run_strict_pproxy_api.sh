#!/usr/bin/env bash
# Tier 2 — Paired API oracle comparison runner
#
# Creates clean oracle (pproxy==2.7.9) and candidate (eggress + compat) venvs,
# runs the strict API probes in both, and compares observations.
#
# Usage:
#     ./scripts/run_strict_pproxy_api.sh                    # Full run (standalone)
#     ./scripts/run_strict_pproxy_api.sh --no-bootstrap     # Certification mode
#     ./scripts/run_strict_pproxy_api.sh --dry-run          # List records
#     ./scripts/run_strict_pproxy_api.sh --category python_namespace  # Filter
#
# Certification mode (--no-bootstrap):
#     Requires --oracle-python and --candidate-python flags.
#     Skips venv creation, package installation, and wheel building.
#     Fails closed if interpreters or imports are missing.
#
# Requires: python3, pip, maturin (for candidate wheel build in standalone mode)
# Exit codes: 0 = all pass, 1 = mismatches, 2 = harness error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# ── Parse flags ────────────────────────────────────────────────────

NO_BOOTSTRAP=false
ORACLE_PYTHON_FLAG=""
CANDIDATE_PYTHON_FLAG=""
PassthroughArgs=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-bootstrap)
            NO_BOOTSTRAP=true
            shift
            ;;
        --oracle-python)
            ORACLE_PYTHON_FLAG="$2"
            shift 2
            ;;
        --candidate-python)
            CANDIDATE_PYTHON_FLAG="$2"
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
    # Certification mode: require explicit interpreters
    if [ -z "$ORACLE_PYTHON_FLAG" ]; then
        echo "ERROR: --no-bootstrap requires --oracle-python PATH" >&2
        exit 2
    fi
    if [ -z "$CANDIDATE_PYTHON_FLAG" ]; then
        echo "ERROR: --no-bootstrap requires --candidate-python PATH" >&2
        exit 2
    fi
    if [ ! -x "$ORACLE_PYTHON_FLAG" ]; then
        echo "ERROR: oracle interpreter not found or not executable: $ORACLE_PYTHON_FLAG" >&2
        exit 2
    fi
    if [ ! -x "$CANDIDATE_PYTHON_FLAG" ]; then
        echo "ERROR: candidate interpreter not found or not executable: $CANDIDATE_PYTHON_FLAG" >&2
        exit 2
    fi
    ORACLE_PYTHON="$ORACLE_PYTHON_FLAG"
    CANDIDATE_PYTHON="$CANDIDATE_PYTHON_FLAG"
else
    # Standalone mode: use env vars or defaults
    ORACLE_VENV="${ORACLE_VENV:-.venv-oracle-api}"
    CANDIDATE_VENV="${CANDIDATE_VENV:-.venv-candidate-api}"
    ORACLE_PYTHON="$ORACLE_VENV/bin/python"
    CANDIDATE_PYTHON="$CANDIDATE_VENV/bin/python"
fi

OUTPUT_DIR="${OUTPUT_DIR:-target/strict/paired_observations}"

echo "=== Tier 2: Paired API Oracle Comparison ==="
echo "Mode:           $([ "$NO_BOOTSTRAP" = true ] && echo 'certification (no-bootstrap)' || echo 'standalone')"
echo "Oracle python:  $ORACLE_PYTHON"
echo "Candidate python: $CANDIDATE_PYTHON"
echo "Output dir:     $OUTPUT_DIR"
echo ""

# ── Standalone: create venvs if needed ────────────────────────────

if [ "$NO_BOOTSTRAP" = false ]; then
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
        maturin build --release --out target/wheels -m crates/eggress-python/Cargo.toml 2>/dev/null

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
fi

# ── Verify interpreters ───────────────────────────────────────────

echo "Verifying oracle imports..."
"$ORACLE_PYTHON" -c "import pproxy; print(f'  pproxy version: {getattr(pproxy, \"__version__\", \"unknown\")}')" 2>&1 || {
    echo "ERROR: Oracle interpreter cannot import pproxy" >&2
    exit 2
}

echo "Verifying candidate imports..."
"$CANDIDATE_PYTHON" -c "import pproxy; print(f'  pproxy version: {getattr(pproxy, \"__version__\", \"unknown\")}')" 2>&1 || {
    echo "ERROR: Candidate interpreter cannot import pproxy" >&2
    exit 2
}

echo ""

# ── Run paired comparison through the candidate interpreter ────────

"$CANDIDATE_PYTHON" "$SCRIPT_DIR/run_strict_pproxy_api.py" \
    --oracle-venv "$(dirname "$ORACLE_PYTHON")/.." \
    --candidate-venv "$(dirname "$CANDIDATE_PYTHON")/.." \
    --output-dir "$OUTPUT_DIR" \
    "${PassthroughArgs[@]}"
