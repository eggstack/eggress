# Performance Baseline (Index)

This file is a stable alias pointing to the latest dated performance
baseline. Historical baselines are immutable records; new baselines
should be created as dated files and this index updated.

## Current baseline

**Date:** 2026-07-03
**File:** [`BASELINE_2026_07_03.md`](BASELINE_2026_07_03.md)

## Environment

- OS: macOS (darwin)
- CPU: Apple Silicon (arm64)
- Rust: stable (via rust-toolchain.toml)
- Build profile: debug

## Quick comparison

```bash
# Run benchmarks and compare against baseline
cargo bench --workspace

# Run performance smoke tests
cargo test -p eggress-runtime --test performance_smoke
```

## Adding a new baseline

1. Create `docs/performance/BASELINE_YYYY_MM_DD.md` with the new numbers.
2. Update the "Current baseline" section above to point to the new file.
3. Keep the old baseline file as an immutable historical record.
