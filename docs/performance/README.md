# Performance and Benchmarking

Performance qualification — setup, hot paths, and coarse regression gates

## Overview

This directory contains performance documentation for the eggress proxy
framework. The 2026-09-20 runtime campaign establishes path-accurate
measurement for TLS/setup, relay, route selection, and standalone UDP without
turning noisy wall-clock results into mandatory CI thresholds.

## Documents

- [Benchmark Inventory](BENCHMARK_INVENTORY.md) — complete list of benchmarks with purpose, command, duration, and gating tier
- [Regression Gate Policy](REGRESSION_GATE_POLICY.md) — which tests to run when, and what's blocking
- [Python Binding Overhead](PYTHON_BINDING_OVERHEAD.md) — PyO3 wrapper overhead measurements
- [Baseline 2026-07-03](BASELINE_2026_07_03.md) — initial baseline numbers
- [Qualification 2026-09-20](BASELINE_2026_09_20.md) — Phase 1–3 local evidence

## Quick Start

```bash
# Tier 0: Microbenchmarks (~90s)
cargo bench --workspace

# Tier 1: Performance smoke tests (~15s)
cargo test -p eggress-runtime --test performance_smoke

# Tier 2: Soak tests (manual, 30-120s)
EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1

# Tier 3: pproxy comparison (manual, requires pproxy)
EGRESS_REQUIRE_PPROXY_PERF=1 scripts/perf/run_pproxy_comparison.sh
```

## Qualification workflow

The durable Criterion surfaces are also used for same-host before/after
qualification. Keep setup-inclusive and steady-state workloads separate, and
record exact source SHAs, toolchain, host, profile, and Criterion settings in
the dated record. The TLS connection comparison is explicitly a component
benchmark: it contrasts per-connection server-config construction with a
prepared `Arc<ServerConfig>` while keeping listener startup outside the timed
operation.

The relay buffer matrix reports elapsed observations separately from the
deterministic allocation calculation (`2 * buffer_size` per active relay).
Neither observation is a portable CI threshold.

## Design Principles

1. **Stable measurement, not premature optimization.** The performance
   measurement infrastructure. Future phases use it to detect regressions.
2. **Gated long-running tests.** Soak and load tests are gated behind
   environment variables. Normal development is not slowed.
3. **Transparent baselines.** All numbers are recorded with environment
   details. No claims without reproducible evidence.
4. **pproxy comparison as context.** When behavior is comparable, pproxy
   comparison provides context, not an acceptance gate.
