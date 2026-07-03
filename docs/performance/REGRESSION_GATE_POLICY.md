# Regression Gate Policy

Phase 34 — Performance, Soak, and Regression Gates

## Purpose

Define which performance tests run in which contexts, so developers know
what to run before merges vs releases.

## Tiers

### Tier 0 — Microbenchmarks (informational)

**When:** Every `cargo bench` run, CI benchmark job if available.

**Commands:**
```bash
cargo bench --workspace
```

**What it covers:**
- Criterion benchmarks: TCP relay, UDP codec, route match, HTTP CONNECT upstream.

**Blocking:** No. Results are informational. Future phases may add regression
threshold checks against stored baselines.

**CI integration:** Not currently in CI (hosted CI billing issue). When CI
resumes, consider adding `cargo bench --workspace --no-run` to verify
benchmarks compile.

### Tier 1 — Performance Smoke (before release)

**When:** Before tagging a release, after large changes touching relay code.

**Commands:**
```bash
# Full Tier 1 suite
cargo test -p eggress-runtime --test performance_smoke

# Individual checks
cargo test -p eggress-runtime --test performance_smoke -- performance_tcp_relay_smoke
cargo test -p eggress-runtime --test performance_smoke -- performance_udp_relay_smoke
cargo test -p eggress-runtime --test performance_smoke -- resource_leak_fd_cleanup
cargo test -p eggress-runtime --test performance_smoke -- resource_leak_task_cleanup

# Python binding overhead
python -m pytest python/tests/test_performance_smoke.py -q

# Benchmark compilation check
cargo bench --workspace --no-run
```

**What it covers:**
- TCP echo throughput via SOCKS5 listener
- UDP datagram throughput with flow cleanup verification
- File descriptor count returns to baseline after churn
- Task tracker empties after session drain
- Python binding overhead (import, URI translation, config compile)

**Blocking:** Yes for release tags. No for normal development.

**Failure action:** Investigate before release. May be flaky on resource-
constrained systems; re-run once to confirm.

### Tier 2 — Soak Tests (manual/gated)

**When:** Manual verification during release candidate phase, after
significant async runtime or connection management changes.

**Commands:**
```bash
# Reverse proxy soak
EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1

# Load tests
cargo test -p eggress-runtime --test load -- --ignored
```

**What it covers:**
- Reverse proxy sustained relay for 30–120 seconds
- Reconnect churn and auth failure handling
- 100 concurrent TCP sessions
- UDP association limit enforcement

**Blocking:** No. Manual verification. Record results in release notes
if run.

**Failure action:** If soak reveals leaks or stalls, file an issue.
Do not block release on Tier 2 unless a critical regression is found.

### Tier 3 — pproxy Comparison (cross-platform RC only)

**When:** Release candidate validation, when comparing against pproxy
performance is meaningful.

**Commands:**
```bash
EGRESS_REQUIRE_PPROXY_PERF=1 scripts/perf/run_pproxy_comparison.sh
```

**What it covers:**
- HTTP CONNECT local relay throughput comparison
- SOCKS5 CONNECT local relay throughput comparison
- Requires pproxy==2.7.9 installed

**Blocking:** No. Contextual comparison, not acceptance gate.

**Failure action:** Record environment and results. Do not claim
superiority without reproducible numbers.

## Developer Quick Reference

| Context | Run these | Blocking? |
|---------|-----------|-----------|
| Normal development | nothing extra | — |
| Before merge (relay changes) | `cargo test --workspace` | Yes |
| Before release | Tier 0 + Tier 1 | Yes |
| Release candidate | Tier 0 + Tier 1 + Tier 2 | Tier 2 advisory |
| Cross-platform RC | All tiers | Tier 3 informational |
