#!/usr/bin/env bash
set -euo pipefail

# pproxy behavioral certification script.
#
# Runs only pproxy-specific behavioral validation with isolated oracle
# and candidate environments. Does NOT run formatting, linting, workspace
# tests, dependency audits, wheel builds, or release packaging.
#
# Run from the workspace root:
#   ./scripts/run_pproxy_certification.sh
#
# Output:
#   target/pproxy-certification/summary.json
#   target/pproxy-certification/failures/ (diagnostics for failed checks only)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

CERT_DIR="target/pproxy-certification"
ORACLE_VENV="$CERT_DIR/oracle-venv"
CANDIDATE_VENV="$CERT_DIR/candidate-venv"
OBS_DIR="$CERT_DIR/observations"
FAILURES_DIR="$CERT_DIR/failures"
TMP_DIR="$CERT_DIR/tmp"

# ── Helpers ───────────────────────────────────────────────────────

fatal_step() {
    local label="$1"
    shift
    echo "FATAL: $label" >&2
    "$@" || {
        echo "FATAL: $label failed (exit $?)" >&2
        exit 1
    }
}

# ── Preflight ─────────────────────────────────────────────────────

for tool in git cargo rustc python3; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "FATAL: required tool not found: $tool" >&2
        exit 1
    }
done

python3 - <<'PY'
import sys
if sys.version_info[:2] not in {(3, 11), (3, 12)}:
    raise SystemExit(
        f"pproxy certification requires Python 3.11 or 3.12; got {sys.version.split()[0]}"
    )
PY

# ── Clean and create directory structure ───────────────────────────

rm -rf "$CERT_DIR"
mkdir -p "$CERT_DIR" "$OBS_DIR" "$FAILURES_DIR" "$TMP_DIR"

# ── Create oracle environment ─────────────────────────────────────

echo "=== Creating oracle environment ==="
fatal_step "create oracle venv" python3 -m venv "$ORACLE_VENV"
fatal_step "upgrade oracle pip" "$ORACLE_VENV/bin/python" -m pip install --upgrade pip
fatal_step "install oracle pproxy" "$ORACLE_VENV/bin/python" -m pip install -r compat/pproxy-2.7.9/requirements-oracle.txt

# Verify oracle version via distribution metadata
fatal_step "verify oracle version" "$ORACLE_VENV/bin/python" - <<'PY'
from importlib.metadata import version
actual = version("pproxy")
expected = "2.7.9"
if actual != expected:
    raise SystemExit(f"expected pproxy=={expected}, got {actual}")
print(f"oracle pproxy version: {actual}")
PY

ORACLE_PYTHON="$ORACLE_VENV/bin/python"

# ── Create candidate environment ──────────────────────────────────

echo ""
echo "=== Creating candidate environment ==="
fatal_step "create candidate venv" python3 -m venv "$CANDIDATE_VENV"
fatal_step "upgrade candidate pip" "$CANDIDATE_VENV/bin/python" -m pip install --upgrade pip
fatal_step "install candidate deps" "$CANDIDATE_VENV/bin/python" -m pip install \
    "maturin>=1.0,<2.0" \
    pytest \
    "pytest-asyncio>=0.23,<1" \
    "cryptography>=42,<47"

# Build and install the native extension
echo "Building eggress native extension..."
fatal_step "build eggress extension" bash -c "
    VIRTUAL_ENV='$CANDIDATE_VENV' \
    PATH='$CANDIDATE_VENV/bin:\$PATH' \
    '$CANDIDATE_VENV/bin/maturin' develop \
    --manifest-path crates/eggress-python/Cargo.toml
"

# Install local compatibility package
fatal_step "install compat package" "$CANDIDATE_VENV/bin/python" -m pip install --no-deps ./python-pproxy-compat

# Verify candidate imports
fatal_step "verify candidate imports" "$CANDIDATE_VENV/bin/python" - <<'PY'
import importlib.metadata
try:
    import eggress
    print(f"eggress: OK")
except ImportError as e:
    raise SystemExit(f"eggress import failed: {e}")
try:
    import pproxy
    print(f"pproxy: OK")
except ImportError as e:
    raise SystemExit(f"pproxy import failed: {e}")
PY

CANDIDATE_PYTHON="$CANDIDATE_VENV/bin/python"
ORACLE_PYTHON_VERSION=$("$ORACLE_PYTHON" -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')")
CANDIDATE_PYTHON_VERSION=$("$CANDIDATE_PYTHON" -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')")

# ── Export interpreter paths for helper scripts ────────────────────

export EGRESS_ORACLE_PYTHON="$ORACLE_PYTHON"
export EGRESS_CANDIDATE_PYTHON="$CANDIDATE_PYTHON"
export EGRESS_ORACLE_OBSERVATIONS_DIR="$OBS_DIR/oracle"
export EGRESS_CANDIDATE_OBSERVATIONS_DIR="$OBS_DIR/candidate"

# ── Check runner ──────────────────────────────────────────────────

PASS=0
FAIL=0
SKIP=0
CHECKS=()
START_TOTAL=$(date +%s)

run_check() {
    local name="$1"
    local required="${2:-required}"
    shift 2
    echo "=== CHECK: $name ==="
    local start
    start=$(date +%s%N)
    local rc=0
    local stdout_file="$TMP_DIR/$(echo "$name" | tr ' /' '__').stdout"
    local stderr_file="$TMP_DIR/$(echo "$name" | tr ' /' '__').stderr"
    "$@" > "$stdout_file" 2> "$stderr_file" || rc=$?
    local end
    end=$(date +%s%N)
    local elapsed_ms=$(( (end - start) / 1000000 ))
    local elapsed_s=$((elapsed_ms / 1000))
    local remainder=$((elapsed_ms % 1000))
    local elapsed_fmt="${elapsed_s}.${remainder}s"
    local result
    if [ "$rc" -eq 0 ]; then
        result="pass"
        PASS=$((PASS + 1))
        echo "  PASS ($elapsed_fmt)"
        rm -f "$stdout_file" "$stderr_file"
    elif [ "$required" = "optional" ]; then
        result="skip"
        SKIP=$((SKIP + 1))
        echo "  SKIP ($elapsed_fmt, rc=$rc) — optional, not blocking"
        rm -f "$stdout_file" "$stderr_file"
    else
        result="fail"
        FAIL=$((FAIL + 1))
        echo "  FAIL ($elapsed_fmt, rc=$rc)"
        cp "$stderr_file" "$FAILURES_DIR/$(echo "$name" | tr ' /' '__').stderr" 2>/dev/null || true
        if [ -s "$stdout_file" ]; then
            cp "$stdout_file" "$FAILURES_DIR/$(echo "$name" | tr ' /' '__').stdout" 2>/dev/null || true
        fi
    fi
    CHECKS+=("{\"name\":\"$name\",\"result\":\"$result\",\"elapsed_ms\":$elapsed_ms,\"exit_code\":$rc}")
    echo ""
}

COMMIT=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
ORACLE_VERSION="2.7.9"

echo ""
echo "=== pproxy behavioral certification ==="
echo "Started at $(date)"
echo "Commit: $COMMIT"
echo "Certification dir: $CERT_DIR"
echo "Oracle python: $ORACLE_PYTHON_VERSION"
echo "Candidate python: $CANDIDATE_PYTHON_VERSION"
echo ""

# ── Check 1: pproxy differential tests (mandatory) ────────────────
run_check "pproxy_differential" required bash -c "
    EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1 2>&1
"

# ── Check 2: paired API runner (mandatory) ─────────────────────────
run_check "paired_api_runner" required bash -c '
    ORACLE_VENV="'"$ORACLE_VENV"'" CANDIDATE_VENV="'"$CANDIDATE_VENV"'" OUTPUT_DIR="'"$OBS_DIR"'" \
        ./scripts/run_strict_pproxy_api.sh --closure-required
'

# ── Check 3: strict Python differential tests (mandatory) ──────────
run_check "strict_python_differential" required bash -c "
    OBS_COUNT=\$(ls '$OBS_DIR'/*_oracle.json 2>/dev/null | wc -l)
    if [ \"\$OBS_COUNT\" -gt 0 ]; then
        EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 '$CANDIDATE_PYTHON' -m pytest python/tests/strict -q \
            --oracle-observations-dir '$OBS_DIR' \
            --candidate-observations-dir '$OBS_DIR' \
            --tb=short 2>&1
    else
        echo 'ERROR: No paired observations available.' >&2
        exit 1
    fi
"

# ── Check 4: external TCP interoperability (mandatory) ─────────────
run_check "external_tcp_interop" required bash -c '
    ORACLE_VENV="'"$ORACLE_VENV"'" CANDIDATE_VENV="'"$CANDIDATE_VENV"'" \
        EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/run_strict_pproxy_interop.sh
'

# ── Check 5: external UDP interoperability (mandatory) ─────────────
run_check "external_udp_interop" required bash -c '
    ORACLE_VENV="'"$ORACLE_VENV"'" CANDIDATE_VENV="'"$CANDIDATE_VENV"'" \
        EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/compat_udp_pproxy.sh
'

# ── Check 6: cipher KAT and interop probes ──────────────────────
run_check "cipher_kat" required bash -c '
    '"$CANDIDATE_PYTHON"' -m pytest python/tests/test_protocol_cipher.py::TestAEADKnownAnswerVectors -v --tb=short --import-mode=importlib 2>&1
'

# ── Check 7: plugin transformed-traffic probe ────────────────────
run_check "plugin_probe" required bash -c '
    '"$CANDIDATE_PYTHON"' -m pytest python/tests/test_plugin.py -q --tb=short --import-mode=importlib 2>&1
'

# ── Check 8: process lifecycle probe ─────────────────────────────
run_check "process_lifecycle" required bash -c '
    '"$CANDIDATE_PYTHON"' -m pytest python/tests/test_server_lifecycle.py -q --tb=short --import-mode=importlib 2>&1
'

# ── Generate compact summary via Python JSON encoder ───────────────
END_TOTAL=$(date +%s)
TOTAL_ELAPSED=$((END_TOTAL - START_TOTAL))
TOTAL_ELAPSED_MS=$((TOTAL_ELAPSED * 1000))

# Write check records to a temp file for Python to serialize
CHECKS_FILE="$TMP_DIR/checks.tsv"
> "$CHECKS_FILE"
for c in "${CHECKS[@]}"; do
    echo "$c" >> "$CHECKS_FILE"
done

"$CANDIDATE_PYTHON" - "$CHECKS_FILE" "$COMMIT" "$ORACLE_VERSION" "$ORACLE_PYTHON_VERSION" "$CANDIDATE_PYTHON_VERSION" "$PASS" "$FAIL" "$SKIP" "$TOTAL_ELAPSED_MS" "$CERT_DIR" <<'PYEOF'
import json
import sys
import os

checks_file = sys.argv[1]
commit = sys.argv[2]
oracle_version = sys.argv[3]
oracle_python = sys.argv[4]
candidate_python = sys.argv[5]
passed = int(sys.argv[6])
failed = int(sys.argv[7])
skipped = int(sys.argv[8])
elapsed_ms = int(sys.argv[9])
cert_dir = sys.argv[10]

checks = []
with open(checks_file) as f:
    for line in f:
        line = line.strip()
        if line:
            checks.append(json.loads(line))

result = "pass" if failed == 0 else "fail"

summary = {
    "schema_version": 2,
    "commit": commit,
    "oracle": {
        "distribution": "pproxy",
        "version": oracle_version,
        "python": oracle_python,
        "interpreter": "target/pproxy-certification/oracle-venv/bin/python"
    },
    "candidate": {
        "python": candidate_python,
        "interpreter": "target/pproxy-certification/candidate-venv/bin/python"
    },
    "result": result,
    "passed": passed,
    "failed": failed,
    "skipped": skipped,
    "elapsed_ms": elapsed_ms,
    "checks": checks
}

summary_path = os.path.join(cert_dir, "summary.json")
with open(summary_path, "w") as f:
    json.dump(summary, f, indent=2)
    f.write("\n")
PYEOF

# Clean up tmp directory
rm -rf "$TMP_DIR"

# ── Print summary ─────────────────────────────────────────────────

echo "=== CERTIFICATION SUMMARY ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo "Skipped: $SKIP"
echo "Total: $((PASS + FAIL + SKIP))"
echo "Elapsed: ${TOTAL_ELAPSED}s"
echo ""
for c in "${CHECKS[@]}"; do
    name=$(echo "$c" | sed 's/.*"name":"\([^"]*\)".*/\1/')
    result=$(echo "$c" | sed 's/.*"result":"\([^"]*\)".*/\1/')
    echo "  [$result] $name"
done
echo ""
echo "Summary: $CERT_DIR/summary.json"

if [ "$FAIL" -gt 0 ]; then
    echo "CERTIFICATION FAILED: $FAIL check(s) failed"
    echo "Failure diagnostics: $FAILURES_DIR/"
    exit 1
else
    echo "CERTIFICATION PASSED: all $PASS required checks passed ($SKIP skipped)"
    exit 0
fi
