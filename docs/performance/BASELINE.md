# Performance Baseline (Index)

This file is a stable alias pointing to the latest dated performance
baseline. Historical baselines are immutable records; new baselines
should be created as dated files and this index updated.

## Current baseline

**Date:** 2026-09-20
**File:** [`BASELINE_2026_09_20.md`](BASELINE_2026_09_20.md)

## Environment

- OS: Ubuntu Linux 6.8.0-139-generic
- CPU: Intel Core i9-9900K (x86_64)
- Rust: rustc 1.85.0
- Build profile: Criterion bench profile (release)

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
