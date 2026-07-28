#!/usr/bin/env bash
set -euo pipefail

# pproxy behavioral certification script.
#
# Runs pproxy-specific behavioral gates: format, lint, workspace tests,
# dependency checks, strict manifest validation, wheel builds, differential
# tests, interoperability tests, and process lifecycle probes.
#
# This script does NOT run: release-document consistency checks, evidence
# hash binding, SBOM generation, container builds, or general release
# gatekeeping. Those are release concerns, not compatibility concerns.
#
# Run from the workspace root:
#   ./scripts/run_pproxy_certification.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

AUDIT_DIR="target/closure-audit"
mkdir -p "$AUDIT_DIR"

PASS=0
FAIL=0
SKIP=0
RESULTS=()
START_TOTAL=$(date +%s)

run_gate() {
    local name="$1"
    shift
    echo "=== GATE: $name ==="
    local start
    start=$(date +%s%N)
    local output_file="$AUDIT_DIR/gate_$(echo "$name" | tr ' /' '__').log"
    local rc=0
    "$@" > "$output_file" 2>&1 || rc=$?
    local end
    end=$(date +%s%N)
    local elapsed_ms=$(( (end - start) / 1000000 ))
    local elapsed_s=$((elapsed_ms / 1000))
    local remainder=$((elapsed_ms % 1000))
    local elapsed_fmt="${elapsed_s}.${remainder}s"
    if [ "$rc" -eq 0 ]; then
        RESULTS+=("PASS|$name|$rc|$elapsed_fmt|$output_file")
        PASS=$((PASS + 1))
        echo "  PASS ($elapsed_fmt, rc=$rc)"
    else
        RESULTS+=("FAIL|$name|$rc|$elapsed_fmt|$output_file")
        FAIL=$((FAIL + 1))
        echo "  FAIL ($elapsed_fmt, rc=$rc) — see $output_file"
    fi
    echo ""
}

run_gate_optional() {
    local name="$1"
    shift
    echo "=== GATE (optional): $name ==="
    local start
    start=$(date +%s%N)
    local output_file="$AUDIT_DIR/gate_$(echo "$name" | tr ' /' '__').log"
    local rc=0
    "$@" > "$output_file" 2>&1 || rc=$?
    local end
    end=$(date +%s%N)
    local elapsed_ms=$(( (end - start) / 1000000 ))
    local elapsed_s=$((elapsed_ms / 1000))
    local remainder=$((elapsed_ms % 1000))
    local elapsed_fmt="${elapsed_s}.${remainder}s"
    if [ "$rc" -eq 0 ]; then
        RESULTS+=("PASS|$name|$rc|$elapsed_fmt|$output_file")
        PASS=$((PASS + 1))
        echo "  PASS ($elapsed_fmt, rc=$rc)"
    else
        RESULTS+=("SKIP|$name|$rc|$elapsed_fmt|$output_file")
        SKIP=$((SKIP + 1))
        echo "  SKIP ($elapsed_fmt, rc=$rc) — optional, not blocking"
    fi
    echo ""
}

echo "=== MILESTONES A-C FINAL CLOSURE AUDIT ==="
echo "Started at $(date)"
echo "Commit: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo "Artifact dir: $AUDIT_DIR"
echo ""

# Fresh-run: clean stale observation directories and venvs
echo "Cleaning stale environments..."
rm -rf "$AUDIT_DIR/paired_observations"
rm -rf "$AUDIT_DIR/venv-pytest"
rm -rf .venv-oracle-api .venv-candidate-api
mkdir -p "$AUDIT_DIR"
echo ""

# Ensure pytest is available for Python test gates
if ! python3 -c "import pytest" 2>/dev/null; then
    echo "Installing pytest (required for Python test gates)..."
    pip install pytest pytest-asyncio pytest-timeout >/dev/null 2>&1
fi

# ── Gate 1: cargo fmt ──────────────────────────────────────────────
run_gate "01_cargo_fmt" cargo fmt --all -- --check

# ── Gate 2: cargo check ───────────────────────────────────────────
run_gate "02_cargo_check" cargo check --workspace --all-targets

# ── Gate 3: cargo clippy ──────────────────────────────────────────
run_gate "03_cargo_clippy" cargo clippy --workspace --all-targets -- -D warnings

# ── Gate 4: cargo test ────────────────────────────────────────────
run_gate "04_cargo_test" cargo test --workspace

# ── Gate 5: cargo deny check ──────────────────────────────────────
run_gate "05_cargo_deny" cargo deny check

# ── Gate 6: cargo audit ───────────────────────────────────────────
run_gate "06_cargo_audit" cargo audit

# ── Gate 7: strict manifest validator tests ───────────────────────
run_gate "07_strict_manifest_tests" cargo test -p eggress-testkit strict_manifest

# ── Gate 8: strict report freshness ──────────────────────────────
run_gate "08_strict_report_freshness" cargo run -p eggress-testkit --bin strict-report -- --check

# ── Gate 9: canonical wheel build ───────────────────────────────
run_gate "09_canonical_wheel_build" bash -c 'cd crates/eggress-python && maturin build --release --out ../../dist'

# ── Gate 10: compat wheel build ──────────────────────────────────
run_gate "10_compat_wheel_build" bash -c 'python3 -m pip wheel --no-deps --wheel-dir dist ./python-pproxy-compat'

# ── Gate 11: candidate Python test suite ─────────────────────────
VENV_DIR="$AUDIT_DIR/venv-pytest"
run_gate "11_python_test_suite" bash -c "
    python3 -m venv '$VENV_DIR' && \
    '$VENV_DIR/bin/pip' install --upgrade pip >/dev/null 2>&1 && \
    EGGRESS_WHEEL=\$(ls dist/eggress-*.whl 2>/dev/null | head -1) && \
    COMPAT_WHEEL=\$(ls dist/eggress_pproxy_compat-*.whl 2>/dev/null | head -1) && \
    [ -n \"\$EGGRESS_WHEEL\" ] || { echo 'ERROR: eggress wheel not found' >&2; exit 1; } && \
    [ -n \"\$COMPAT_WHEEL\" ] || { echo 'ERROR: compat wheel not found' >&2; exit 1; } && \
    '$VENV_DIR/bin/pip' install \"\$EGGRESS_WHEEL\" pytest pytest-asyncio >/dev/null 2>&1 && \
    '$VENV_DIR/bin/pip' install \"\$COMPAT_WHEEL\" >/dev/null 2>&1 && \
    '$VENV_DIR/bin/python' -m pytest python/tests -x -q \
        --import-mode=importlib \
        --rootdir='$AUDIT_DIR' \
        --junitxml='$AUDIT_DIR/junit-python.xml' \
        --tb=short
"

# ── Gate 12: pproxy differential tests (mandatory) ────────────────
# Requires pproxy==2.7.9 installed in the test venv.
run_gate "12_pproxy_differential" bash -c "
    '$VENV_DIR/bin/pip' install pproxy==2.7.9 >/dev/null 2>&1 && \
    EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 '$VENV_DIR/bin/python' -m pytest python/tests/test_pproxy_differential.py -v --tb=short --import-mode=importlib 2>&1
"

# ── Gate 13: paired API runner (mandatory) ─────────────────────────
run_gate "13_paired_api_runner" bash -c './scripts/run_strict_pproxy_api.sh --closure-required'

# ── Gate 14: strict Python differential tests (mandatory) ──────────
# Requires observation directories from the paired API job (gate 13).
# Missing directories are a hard failure, not a skip.
OBS_DIR="$AUDIT_DIR/paired_observations"
# Link gate 14's output directory if it exists and OBS_DIR doesn't
if [ ! -e "$OBS_DIR" ] && [ -d "target/strict/paired_observations" ]; then
    ln -sfn "$(pwd)/target/strict/paired_observations" "$OBS_DIR"
fi
if [ ! -e "$OBS_DIR" ]; then
    mkdir -p "$OBS_DIR"
fi
run_gate "14_strict_python_differential" bash -c "
    # Check if observations from the paired API job were pre-staged
    OBS_COUNT=\$(ls '$OBS_DIR'/*_oracle.json 2>/dev/null | wc -l)
    if [ \"\$OBS_COUNT\" -gt 0 ]; then
        EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 '$VENV_DIR/bin/python' -m pytest python/tests/strict -q \
            --oracle-observations-dir '$OBS_DIR' \
            --candidate-observations-dir '$OBS_DIR' \
            --tb=short
    else
        echo 'ERROR: No paired observations available; strict differential tests require observation directories.' >&2
        echo 'Run gate 13 (paired_api_runner) first to generate them.' >&2
        echo 'Expected location: target/strict/paired_observations/' >&2
        exit 1
    fi
"

# ── Gate 15: required runtime examples/scenarios ─────────────────
run_gate "15_runtime_examples" cargo test -p eggress-testkit pproxy_oracle -- --ignored

# ── Gate 16: external TCP interoperability (mandatory) ─────────────
run_gate "16_external_tcp_interop" bash -c 'EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/run_strict_pproxy_interop.sh'

# ── Gate 17: external UDP interoperability (mandatory) ─────────────
run_gate "17_external_udp_interop" bash -c 'EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/compat_udp_pproxy.sh'

# ── Gate 18: cipher KAT and interop probes ──────────────────────
run_gate "18_cipher_kat" bash -c "'$VENV_DIR/bin/python' -m pytest python/tests/test_protocol_cipher.py::TestAEADKnownAnswerVectors -v --tb=short --import-mode=importlib 2>&1"

# ── Gate 19: plugin transformed-traffic probe ────────────────────
run_gate "19_plugin_probe" bash -c "'$VENV_DIR/bin/python' -m pytest python/tests/test_plugin.py -q --tb=short --import-mode=importlib"

# ── Gate 20: process lifecycle probe ─────────────────────────────
run_gate "20_process_lifecycle" bash -c "'$VENV_DIR/bin/python' -m pytest python/tests/test_server_lifecycle.py -q --tb=short --import-mode=importlib"

# ── Gate 21: runtime/failure/cleanup probe ──────────────────────
run_gate "21_runtime_failure_cleanup" cargo test -p eggress-runtime --test lifecycle_invariants

# ── Gate 22: resource-leak and process-cleanup checks ────────────
run_gate "22_resource_leak_check" bash -c "'$VENV_DIR/bin/python' -m pytest python/tests/test_connection_behavioral.py -q --tb=short --import-mode=importlib"

# ── Generate summary report ──────────────────────────────────────
END_TOTAL=$(date +%s)
TOTAL_ELAPSED=$((END_TOTAL - START_TOTAL))

REPORT="$AUDIT_DIR/CLOSURE_AUDIT_REPORT.md"
cat > "$REPORT" <<REPORT_EOF
# Milestones A-C Final Closure Audit Report

**Date**: $(date -u '+%Y-%m-%dT%H:%M:%SZ')
**Commit**: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)
**Total elapsed**: ${TOTAL_ELAPSED}s

## Gate Results

| # | Gate | Result | Exit | Elapsed | Log |
|---|------|--------|------|---------|-----|
REPORT_EOF

idx=0
for r in "${RESULTS[@]}"; do
    IFS='|' read -r result name rc elapsed log <<< "$r"
    idx=$((idx + 1))
    printf "| %d | %s | %s | %s | %s | \`%s\` |\n" "$idx" "$name" "$result" "$rc" "$elapsed" "$log" >> "$REPORT"
done

cat >> "$REPORT" <<REPORT_EOF

## Summary

- **Passed**: $PASS
- **Failed**: $FAIL
- **Skipped**: $SKIP
- **Total gates**: $((PASS + FAIL + SKIP))

## Artifacts

- Audit dir: \`$AUDIT_DIR\`
- Gate logs: \`$AUDIT_DIR/gate_*.log\`
- Python JUnit XML: \`$AUDIT_DIR/junit-python.xml\`
- Report: \`$REPORT\`

REPORT_EOF

echo "=== AUDIT SUMMARY ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo "Skipped: $SKIP"
echo "Total: $((PASS + FAIL + SKIP))"
echo "Elapsed: ${TOTAL_ELAPSED}s"
echo ""
for r in "${RESULTS[@]}"; do
    IFS='|' read -r result name rc elapsed log <<< "$r"
    echo "  [$result] $name (rc=$rc, ${elapsed})"
done
echo ""
echo "Full report: $REPORT"
echo ""

if [ "$FAIL" -gt 0 ]; then
    echo "AUDIT FAILED: $FAIL gate(s) failed"
    exit 1
else
    echo "AUDIT PASSED: all $PASS required gates passed ($SKIP optional skipped)"
    exit 0
fi
