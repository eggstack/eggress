# Performance and Benchmarking

Phase 34 — Performance, Soak, and Regression Gates

## Overview

This directory contains performance documentation for the eggress proxy
framework. Phase 34 establishes a baseline for measurement discipline,
not premature optimization.

## Documents

- [Benchmark Inventory](BENCHMARK_INVENTORY.md) — complete list of benchmarks with purpose, command, duration, and gating tier
- [Regression Gate Policy](REGRESSION_GATE_POLICY.md) — which tests to run when, and what's blocking
- [Python Binding Overhead](PYTHON_BINDING_OVERHEAD.md) — PyO3 wrapper overhead measurements
- [Baseline 2026-07-03](BASELINE_2026_07_03.md) — initial baseline numbers

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

## Design Principles

1. **Stable measurement, not premature optimization.** Phase 34 adds
   measurement infrastructure. Future phases use it to detect regressions.
2. **Gated long-running tests.** Soak and load tests are gated behind
   environment variables. Normal development is not slowed.
3. **Transparent baselines.** All numbers are recorded with environment
   details. No claims without reproducible evidence.
4. **pproxy comparison as context.** When behavior is comparable, pproxy
   comparison provides context, not an acceptance gate.
