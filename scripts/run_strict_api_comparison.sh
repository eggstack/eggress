#!/usr/bin/env bash
#
# run_strict_api_comparison.sh - Paired oracle/candidate API comparison runner
#
# Runs probe scripts against both oracle (pproxy==2.7.9) and candidate
# (eggress-pproxy-compat) and emits normalized JSON observations, then
# compares them dimension-by-dimension.
#
# Usage:
#   ./scripts/run_strict_api_comparison.sh [OPTIONS]
#
# Options:
#   --oracle-venv PATH      Path to oracle virtualenv (default: .venv-oracle)
#   --candidate-venv PATH   Path to candidate virtualenv (default: .venv-candidate)
#   --output-dir PATH       Output directory for observations (default: target/strict-comparison)
#   --records-filter REGEX  Only probe records whose id matches this regex
#   --manifest PATH         Path to strict manifest TOML (default: docs/parity/pproxy_2_7_9_strict_manifest.toml)
#   --help                  Show this help message
#
# Exit codes:
#   0 - All probed records match
#   1 - At least one mismatch found
#   2 - Harness error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ORACLE_VENV="${REPO_ROOT}/.venv-oracle"
CANDIDATE_VENV="${REPO_ROOT}/.venv-candidate"
OUTPUT_DIR="${REPO_ROOT}/target/strict-comparison"
RECORDS_FILTER=""
MANIFEST="${REPO_ROOT}/docs/parity/pproxy_2_7_9_strict_manifest.toml"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --oracle-venv) ORACLE_VENV="$2"; shift 2 ;;
        --candidate-venv) CANDIDATE_VENV="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --records-filter) RECORDS_FILTER="$2"; shift 2 ;;
        --manifest) MANIFEST="$2"; shift 2 ;;
        --help)
            head -30 "$0" | tail -25
            exit 0
            ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
done

PROBE_API="${SCRIPT_DIR}/strict_api_probe.py"
PROBE_SIG="${SCRIPT_DIR}/strict_signature_probe.py"
PROBE_CLASS="${SCRIPT_DIR}/strict_class_probe.py"
COMPARE="${SCRIPT_DIR}/compare_observations.py"

mkdir -p "$OUTPUT_DIR"

# --- Parse manifest to extract probe list ---
# We use Python's tomllib (3.11+) or tomli fallback to parse the manifest.
PROBES_JSON=$(python3 -c "
import sys, json, re

try:
    import tomllib
except ImportError:
    import tomli as tomllib

with open('${MANIFEST}', 'rb') as f:
    manifest = tomllib.load(f)

records = manifest.get('record', [])
filter_re = re.compile('${RECORDS_FILTER}') if '${RECORDS_FILTER}' else None

probes = []
for rec in records:
    # Filter by status: only probe records that have a meaningful comparator
    # and are not 'not_applicable' or 'intentional_non_parity'
    if rec.get('status') in ('not_applicable', 'intentional_non_parity'):
        continue
    if filter_re and not filter_re.search(rec.get('id', '')):
        continue

    module = rec.get('module', '')
    name = rec.get('name', '')
    kind = rec.get('kind', '')
    comparator = rec.get('comparator', '')
    record_id = rec.get('id', '')

    if not module or not name:
        continue

    # Determine which probe to use based on kind/comparator
    probe_type = 'api'
    if kind in ('class',) or comparator in ('class_hierarchy', 'method_signature', 'property_existence'):
        probe_type = 'class'
    elif kind in ('function',) or comparator in ('async_callable_signature', 'method_signature'):
        probe_type = 'signature'

    probes.append({
        'id': record_id,
        'module': module,
        'name': name,
        'kind': kind,
        'comparator': comparator,
        'probe_type': probe_type,
    })

json.dump(probes, sys.stdout, indent=2)
" 2>/dev/null)

if [ -z "$PROBES_JSON" ] || [ "$PROBES_JSON" = "[]" ]; then
    echo "No probes to run (empty manifest or filter matched nothing)." >&2
    exit 0
fi

TOTAL=$(echo "$PROBES_JSON" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))")
echo "Running $TOTAL probes..." >&2

# --- Run probes against oracle ---
ORACLE_DIR="${OUTPUT_DIR}/oracle"
mkdir -p "$ORACLE_DIR"

ORACLE_PYTHON="${ORACLE_VENV}/bin/python3"
if [ ! -x "$ORACLE_PYTHON" ]; then
    echo "ERROR: Oracle venv python not found at $ORACLE_PYTHON" >&2
    echo "Create it with: python3 -m venv ${ORACLE_VENV} && ${ORACLE_VENV}/bin/pip install pproxy==2.7.9" >&2
    exit 2
fi

# --- Run probes against candidate ---
CANDIDATE_DIR="${OUTPUT_DIR}/candidate"
mkdir -p "$CANDIDATE_DIR"

CANDIDATE_PYTHON="${CANDIDATE_VENV}/bin/python3"
if [ ! -x "$CANDIDATE_PYTHON" ]; then
    echo "ERROR: Candidate venv python not found at $CANDIDATE_PYTHON" >&2
    echo "Create it with: python3 -m venv ${CANDIDATE_VENV} && ${CANDIDATE_VENV}/bin/pip install eggress-pproxy-compat" >&2
    exit 2
fi

echo "$PROBES_JSON" | python3 -c "
import sys, json, subprocess, os, re

probes = json.load(sys.stdin)
oracle_python = '${ORACLE_PYTHON}'
candidate_python = '${CANDIDATE_PYTHON}'
oracle_dir = '${ORACLE_DIR}'
candidate_dir = '${CANDIDATE_DIR}'
probe_api = '${PROBE_API}'
probe_sig = '${PROBE_SIG}'
probe_class = '${PROBE_CLASS}'
repo_root = '${REPO_ROOT}'

def probe_script(probe_type):
    if probe_type == 'signature':
        return probe_sig
    elif probe_type == 'class':
        return probe_class
    return probe_api

def run_probe(python, script, module, name, probe_type):
    try:
        if probe_type == 'class':
            cmd = [python, script, '--module', module, '--class-name', name]
        else:
            cmd = [python, script, '--module', module, '--symbol', name]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30,
                                cwd=repo_root)
        return result.stdout, result.returncode
    except subprocess.TimeoutExpired:
        return json.dumps({'error': 'timeout', 'module': module, 'symbol': name}), 1
    except Exception as e:
        return json.dumps({'error': str(e), 'module': module, 'symbol': name}), 1

def sanitize_filename(record_id):
    return re.sub(r'[^a-zA-Z0-9_.-]', '_', record_id)

for probe in probes:
    record_id = probe['id']
    module = probe['module']
    name = probe['name']
    probe_type = probe['probe_type']
    script = probe_script(probe_type)
    safe_name = sanitize_filename(record_id)

    # Run oracle
    oracle_json, oracle_rc = run_probe(oracle_python, script, module, name, probe_type)
    oracle_path = os.path.join(oracle_dir, f'{safe_name}.json')
    with open(oracle_path, 'w') as f:
        f.write(oracle_json)

    # Run candidate
    candidate_json, candidate_rc = run_probe(candidate_python, script, module, name, probe_type)
    candidate_path = os.path.join(candidate_dir, f'{safe_name}.json')
    with open(candidate_path, 'w') as f:
        f.write(candidate_json)

    print(json.dumps({
        'id': record_id,
        'oracle_path': oracle_path,
        'candidate_path': candidate_path,
        'oracle_rc': oracle_rc,
        'candidate_rc': candidate_rc,
    }))
" > "${OUTPUT_DIR}/probe_manifest.json" 2>&1

PROBE_MANIFEST_EXIT=$?
if [ $PROBE_MANIFEST_EXIT -ne 0 ]; then
    echo "ERROR: Probe execution failed (exit $PROBE_MANIFEST_EXIT)" >&2
    cat "${OUTPUT_DIR}/probe_manifest.json" >&2
    exit 2
fi

# --- Compare all observation pairs ---
COMPARISON_REPORT="${OUTPUT_DIR}/comparison_report.json"
TOTAL_MATCH=0
TOTAL_MISMATCH=0
TOTAL_ERROR=0

while IFS= read -r line; do
    oracle_path=$(echo "$line" | python3 -c "import sys,json; print(json.load(sys.stdin)['oracle_path'])")
    candidate_path=$(echo "$line" | python3 -c "import sys,json; print(json.load(sys.stdin)['candidate_path'])")
    record_id=$(echo "$line" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

    if [ ! -f "$oracle_path" ] || [ ! -f "$candidate_path" ]; then
        echo "SKIP: $record_id (missing observation files)" >&2
        TOTAL_ERROR=$((TOTAL_ERROR + 1))
        continue
    fi

    result=$(python3 "$COMPARE" --oracle "$oracle_path" --candidate "$candidate_path" 2>/dev/null) || true
    rc=$?

    if [ $rc -eq 2 ]; then
        echo "ERROR: $record_id (comparison harness error)" >&2
        TOTAL_ERROR=$((TOTAL_ERROR + 1))
    elif [ $rc -eq 1 ]; then
        echo "MISMATCH: $record_id" >&2
        TOTAL_MISMATCH=$((TOTAL_MISMATCH + 1))
        # Append to combined report
        echo "$result" | python3 -c "
import sys, json
data = json.load(sys.stdin)
data['id'] = '${record_id}'
json.dump(data, sys.stdout)
" >> "${OUTPUT_DIR}/mismatches.jsonl"
    else
        TOTAL_MATCH=$((TOTAL_MATCH + 1))
    fi
done < "${OUTPUT_DIR}/probe_manifest.json"

# --- Write summary ---
python3 -c "
import json
summary = {
    'total': ${TOTAL_MATCH} + ${TOTAL_MISMATCH} + ${TOTAL_ERROR},
    'match': ${TOTAL_MATCH},
    'mismatch': ${TOTAL_MISMATCH},
    'error': ${TOTAL_ERROR},
    'all_match': ${TOTAL_MISMATCH} == 0 and ${TOTAL_ERROR} == 0,
}
json.dump(summary, open('${COMPARISON_REPORT}', 'w'), indent=2)
print(json.dumps(summary, indent=2))
"

echo "" >&2
echo "Results written to: ${OUTPUT_DIR}/" >&2
echo "  - probe_manifest.json   (probe execution manifest)" >&2
echo "  - oracle/               (oracle observations)" >&2
echo "  - candidate/            (candidate observations)" >&2
echo "  - mismatches.jsonl      (mismatch details)" >&2
echo "  - comparison_report.json (summary)" >&2

if [ ${TOTAL_MISMATCH} -gt 0 ] || [ ${TOTAL_ERROR} -gt 0 ]; then
    echo "" >&2
    echo "RESULT: $TOTAL_MISMATCH mismatches, $TOTAL_ERROR errors out of $((TOTAL_MATCH + TOTAL_MISMATCH + TOTAL_ERROR)) probes" >&2
    exit 1
else
    echo "" >&2
    echo "RESULT: All $TOTAL_MATCH probes matched." >&2
    exit 0
fi
