#!/usr/bin/env bash
set -euo pipefail

# pproxy behavioral certification script.
#
# Runs only pproxy-specific behavioral validation: manifest checks,
# paired oracle/candidate observations, differential tests, interoperability
# tests, and process lifecycle probes.
#
# This script does NOT run: formatting, linting, workspace tests, dependency
# audits, wheel builds, release packaging, report freshness, JUnit generation,
# or Markdown report generation.
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
FAILURES_DIR="$CERT_DIR/failures"

# Clean previous run
rm -rf "$CERT_DIR"
mkdir -p "$FAILURES_DIR"

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
    local stdout_file="$CERT_DIR/check_$(echo "$name" | tr ' /' '__').stdout"
    local stderr_file="$CERT_DIR/check_$(echo "$name" | tr ' /' '__').stderr"
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
    elif [ "$required" = "optional" ]; then
        result="skip"
        SKIP=$((SKIP + 1))
        echo "  SKIP ($elapsed_fmt, rc=$rc) — optional, not blocking"
    else
        result="fail"
        FAIL=$((FAIL + 1))
        echo "  FAIL ($elapsed_fmt, rc=$rc)"
        # Copy failure diagnostics
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

echo "=== pproxy behavioral certification ==="
echo "Started at $(date)"
echo "Commit: $COMMIT"
echo "Certification dir: $CERT_DIR"
echo ""

# ── Check 1: strict manifest validator tests ──────────────────────
run_check "strict_manifest_tests" required cargo test -p eggress-testkit strict_manifest

# ── Check 2: pproxy differential tests (mandatory) ────────────────
# Requires pproxy==2.7.9 installed in the test environment.
run_check "pproxy_differential" required bash -c '
    python3 -c "import pproxy; assert getattr(pproxy, \"__version__\", \"\") == \"2.7.9\", f\"expected pproxy==2.7.9, got {getattr(pproxy, \"__version__\", \"unknown\")}\"" 2>/dev/null || {
        echo "WARNING: pproxy not installed or wrong version, skipping differential tests" >&2
        exit 1
    }
    EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1 2>&1
'

# ── Check 3: paired API runner (mandatory) ─────────────────────────
run_check "paired_api_runner" required bash -c './scripts/run_strict_pproxy_api.sh --closure-required'

# ── Check 4: strict Python differential tests (mandatory) ──────────
# Requires observation directories from the paired API job (check 3).
OBS_DIR="$CERT_DIR/paired_observations"
# Link check 3's output directory if it exists and OBS_DIR doesn't
if [ ! -e "$OBS_DIR" ] && [ -d "target/strict/paired_observations" ]; then
    ln -sfn "$(pwd)/target/strict/paired_observations" "$OBS_DIR"
fi
if [ ! -e "$OBS_DIR" ]; then
    mkdir -p "$OBS_DIR"
fi
run_check "strict_python_differential" required bash -c "
    OBS_COUNT=\$(ls '$OBS_DIR'/*_oracle.json 2>/dev/null | wc -l)
    if [ \"\$OBS_COUNT\" -gt 0 ]; then
        EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python3 -m pytest python/tests/strict -q \
            --oracle-observations-dir '$OBS_DIR' \
            --candidate-observations-dir '$OBS_DIR' \
            --tb=short 2>&1
    else
        echo 'ERROR: No paired observations available; strict differential tests require observation directories.' >&2
        echo 'Run check 3 (paired_api_runner) first to generate them.' >&2
        echo 'Expected location: target/strict/paired_observations/' >&2
        exit 1
    fi
"

# ── Check 5: required runtime examples/scenarios ─────────────────
run_check "runtime_examples" required cargo test -p eggress-testkit pproxy_oracle -- --ignored

# ── Check 6: external TCP interoperability (mandatory) ─────────────
run_check "external_tcp_interop" required bash -c 'EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/run_strict_pproxy_interop.sh'

# ── Check 7: external UDP interoperability (mandatory) ─────────────
run_check "external_udp_interop" required bash -c 'EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/compat_udp_pproxy.sh'

# ── Check 8: cipher KAT and interop probes ──────────────────────
run_check "cipher_kat" required bash -c 'python3 -m pytest python/tests/test_protocol_cipher.py::TestAEADKnownAnswerVectors -v --tb=short --import-mode=importlib 2>&1'

# ── Check 9: plugin transformed-traffic probe ────────────────────
run_check "plugin_probe" required bash -c 'python3 -m pytest python/tests/test_plugin.py -q --tb=short --import-mode=importlib 2>&1'

# ── Check 10: process lifecycle probe ─────────────────────────────
run_check "process_lifecycle" required bash -c 'python3 -m pytest python/tests/test_server_lifecycle.py -q --tb=short --import-mode=importlib 2>&1'

# ── Check 11: runtime/failure/cleanup probe ──────────────────────
run_check "runtime_failure_cleanup" required cargo test -p eggress-runtime --test lifecycle_invariants

# ── Check 12: resource-leak and process-cleanup checks ────────────
run_check "resource_leak_check" required bash -c 'python3 -m pytest python/tests/test_connection_behavioral.py -q --tb=short --import-mode=importlib 2>&1'

# ── Generate compact summary ──────────────────────────────────────
END_TOTAL=$(date +%s)
TOTAL_ELAPSED=$((END_TOTAL - START_TOTAL))

# Build checks JSON array
CHECKS_JSON=$(printf '%s\n' "${CHECKS[@]}" | paste -sd, -)

# Detect oracle and candidate metadata
ORACLE_PYTHON=$(python3 --version 2>/dev/null | awk '{print $2}' || echo "unknown")
ORACLE_PLATFORM=$(uname -s 2>/dev/null || echo "unknown")

SUMMARY="$CERT_DIR/summary.json"
cat > "$SUMMARY" <<SUMMARY_EOF
{
  "schema_version": 1,
  "commit": "$COMMIT",
  "oracle": {
    "distribution": "pproxy",
    "version": "$ORACLE_VERSION",
    "python": "$ORACLE_PYTHON",
    "platform": "$ORACLE_PLATFORM"
  },
  "candidate": {
    "python": "$ORACLE_PYTHON",
    "platform": "$ORACLE_PLATFORM"
  },
  "result": "$([ "$FAIL" -eq 0 ] && echo pass || echo fail)",
  "passed": $PASS,
  "failed": $FAIL,
  "skipped": $SKIP,
  "elapsed_ms": $((TOTAL_ELAPSED * 1000)),
  "checks": [$CHECKS_JSON]
}
SUMMARY_EOF

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
echo "Summary: $SUMMARY"

if [ "$FAIL" -gt 0 ]; then
    echo "CERTIFICATION FAILED: $FAIL check(s) failed"
    echo "Failure diagnostics: $FAILURES_DIR/"
    exit 1
else
    echo "CERTIFICATION PASSED: all $PASS required checks passed ($SKIP skipped)"
    exit 0
fi
