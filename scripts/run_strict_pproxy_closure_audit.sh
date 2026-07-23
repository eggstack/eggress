#!/usr/bin/env bash
set -euo pipefail

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

# ── Gate 9: release-doc consistency ──────────────────────────────
run_gate "09_release_doc_consistency" python3 scripts/check_release_docs.py

# ── Gate 10: canonical wheel build ───────────────────────────────
run_gate "10_canonical_wheel_build" bash -c 'cd crates/eggress-python && maturin build --release --out ../../dist'

# ── Gate 11: compat wheel build ──────────────────────────────────
run_gate "11_compat_wheel_build" bash -c 'python3 -m pip wheel --no-deps --wheel-dir dist ./python-pproxy-compat'

# ── Gate 12: candidate Python test suite ─────────────────────────
VENV_DIR="$AUDIT_DIR/venv-pytest"
run_gate "12_python_test_suite" bash -c "
    python3 -m venv '$VENV_DIR' && \
    '$VENV_DIR/bin/pip' install --upgrade pip >/dev/null 2>&1 && \
    EGGRESS_WHEEL=\$(ls dist/eggress-*.whl 2>/dev/null | head -1) && \
    COMPAT_WHEEL=\$(ls dist/eggress_pproxy_compat-*.whl 2>/dev/null | head -1) && \
    [ -n \"\$EGGRESS_WHEEL\" ] || { echo 'ERROR: eggress wheel not found' >&2; exit 1; } && \
    [ -n \"\$COMPAT_WHEEL\" ] || { echo 'ERROR: compat wheel not found' >&2; exit 1; } && \
    '$VENV_DIR/bin/pip' install \"\$EGGRESS_WHEEL\" pytest pytest-asyncio >/dev/null 2>&1 && \
    '$VENV_DIR/bin/pip' install \"\$COMPAT_WHEEL\" >/dev/null 2>&1 && \
    '$VENV_DIR/bin/python' -m pytest python/tests -x -q \
        --junitxml='$AUDIT_DIR/junit-python.xml' \
        --tb=short
"

# ── Gate 13: paired API runner ───────────────────────────────────
run_gate_optional "13_paired_api_runner" bash -c './scripts/run_strict_pproxy_api.sh'

# ── Gate 14: strict Python differential tests ────────────────────
OBS_DIR="$AUDIT_DIR/paired_observations"
mkdir -p "$OBS_DIR"
run_gate "14_strict_python_differential" bash -c "
    # Generate observations if not already present from gate 13
    if [ ! -d '$OBS_DIR' ] || [ -z \"\$(ls '$OBS_DIR'/*_oracle.json 2>/dev/null)\" ]; then
        echo 'Generating observations via paired API runner...'
        ./scripts/run_strict_pproxy_api.sh 2>&1 || true
        # Move observations from default location if needed
        if [ -d 'target/strict/paired_observations' ] && [ \"\$(ls target/strict/paired_observations/*_oracle.json 2>/dev/null)\" ]; then
            cp target/strict/paired_observations/*_oracle.json '$OBS_DIR/' 2>/dev/null || true
            cp target/strict/paired_observations/*_candidate.json '$OBS_DIR/' 2>/dev/null || true
        fi
    fi
    EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python3 -m pytest python/tests/strict -q \
        --oracle-observations-dir '$OBS_DIR' \
        --candidate-observations-dir '$OBS_DIR' \
        --tb=short
"

# ── Gate 15: required runtime examples/scenarios ─────────────────
run_gate "15_runtime_examples" cargo test -p eggress-testkit pproxy_oracle -- --ignored

# ── Gate 16: external TCP interoperability ───────────────────────
run_gate_optional "16_external_tcp_interop" bash -c 'EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/run_strict_pproxy_interop.sh'

# ── Gate 17: external UDP interoperability ───────────────────────
run_gate_optional "17_external_udp_interop" bash -c 'EGRESS_REQUIRE_EXTERNAL_INTEROP=1 ./scripts/compat_udp_pproxy.sh'

# ── Gate 18: cipher KAT and interop probes ──────────────────────
run_gate "18_cipher_kat" bash -c 'python3 -m pytest python/tests/test_protocol_cipher.py::TestAEADKnownAnswerVectors -v --tb=short 2>&1'

# ── Gate 19: plugin transformed-traffic probe ────────────────────
run_gate "19_plugin_probe" bash -c 'python3 -m pytest python/tests/test_plugin.py -q --tb=short'

# ── Gate 20: process lifecycle probe ─────────────────────────────
run_gate "20_process_lifecycle" bash -c 'python3 -m pytest python/tests/test_server_lifecycle.py -q --tb=short'

# ── Gate 21: runtime/failure/cleanup probe ──────────────────────
run_gate "21_runtime_failure_cleanup" cargo test -p eggress-runtime --test lifecycle_invariants

# ── Gate 22: resource-leak and process-cleanup checks ────────────
run_gate "22_resource_leak_check" bash -c 'python3 -m pytest python/tests/test_connection_behavioral.py -q --tb=short'

# ── Gate 23: report and evidence hash binding ────────────────────
EVIDENCE_DIR="$AUDIT_DIR/evidence"
mkdir -p "$EVIDENCE_DIR"
run_gate "23_evidence_hash_binding" bash -c "
    COMMIT_SHA=\$(git rev-parse HEAD) && \
    echo \"\$COMMIT_SHA\" > '$EVIDENCE_DIR/candidate_commit.sha' && \
    echo '--- Manifest SHA-256 ---' && \
    sha256sum docs/parity/pproxy_capability_manifest.toml > '$EVIDENCE_DIR/manifest_sha256.txt' && \
    sha256sum docs/parity/pproxy_2_7_9_strict_manifest.toml >> '$EVIDENCE_DIR/manifest_sha256.txt' && \
    echo '--- Oracle package info ---' && \
    python3 -c 'import importlib.metadata; print(importlib.metadata.version(\"pproxy\"))' > '$EVIDENCE_DIR/oracle_version.txt' 2>/dev/null || echo 'pproxy not installed globally' > '$EVIDENCE_DIR/oracle_version.txt' && \
    sha256sum compat/pproxy-2.7.9/requirements-oracle.txt > '$EVIDENCE_DIR/oracle_hash.txt' 2>/dev/null || echo 'N/A' > '$EVIDENCE_DIR/oracle_hash.txt' && \
    echo '--- Environment lock ---' && \
    python3 --version > '$EVIDENCE_DIR/python_version.txt' && \
    rustc --version > '$EVIDENCE_DIR/rust_version.txt' && \
    cargo --version >> '$EVIDENCE_DIR/rust_version.txt' && \
    echo '--- Evidence bound ---' && \
    echo \"Commit: \$COMMIT_SHA\" && \
    cat '$EVIDENCE_DIR/manifest_sha256.txt' && \
    echo 'Evidence hash binding complete.'
"

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
- Evidence dir: \`$EVIDENCE_DIR\`
- Report: \`$REPORT\`

## Evidence Files

$(ls -la "$EVIDENCE_DIR"/ 2>/dev/null || echo "No evidence directory")

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
