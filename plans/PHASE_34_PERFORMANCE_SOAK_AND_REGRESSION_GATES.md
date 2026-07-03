# Phase 34 Plan: Performance, Soak, and Regression Gates

## Purpose

Phase 34 establishes a performance and reliability baseline for Eggress after the broad pproxy parity work. The goal is not to chase maximum benchmark numbers; the goal is to prove that the Rust implementation remains stable, efficient, and predictable across supported proxy modes, especially compared with Python pproxy where comparison is meaningful.

This phase adds benchmark discipline, long-running soak tests, resource-leak detection, and regression gates that future phases can rely on.

## Scope

This phase covers:

- Performance baseline design.
- Per-protocol throughput and latency benchmarks.
- Connection churn and scheduler benchmarks.
- UDP relay performance checks.
- Reverse/backward proxy soak tests.
- Python binding overhead tests.
- Memory/file-descriptor/task leak checks.
- CI/local gating policy for slow tests.
- Documentation and release thresholds.

## Non-goals

Do not rewrite core protocols for optimization unless a benchmark identifies a specific bottleneck.

Do not require long-running soak tests in every normal development run.

Do not compare every feature to pproxy if pproxy behavior differs structurally.

Do not optimize at the expense of compatibility, safety, or readable diagnostics.

## Work items

### 34.1 Define benchmark taxonomy

Create a benchmark inventory with explicit categories:

- microbenchmarks: parser, URI translation, protocol frame encode/decode;
- local relay benchmarks: TCP echo, HTTP CONNECT, SOCKS5 CONNECT;
- upstream chain benchmarks: direct, HTTP upstream, SOCKS5 upstream, Shadowsocks upstream;
- UDP relay benchmarks: standalone UDP, SOCKS5 UDP ASSOCIATE, Shadowsocks UDP;
- reverse/backward benchmarks;
- Python embedding overhead benchmarks;
- admin/metrics overhead benchmarks;
- reload/config compile benchmarks.

Output:

```text
docs/performance/BENCHMARK_INVENTORY.md
```

Acceptance:

- Every benchmark has purpose, command, expected duration, and gating tier.

### 34.2 Add benchmark harness structure

Add or standardize benchmark crates/scripts.

Potential paths:

```text
benches/protocols.rs
benches/runtime_relay.rs
benches/udp_relay.rs
benches/reverse.rs
benches/python_overhead.py
scripts/perf/run_local_baseline.sh
scripts/perf/run_soak.sh
docs/performance/README.md
```

Recommended tools:

- Criterion for Rust microbenchmarks.
- Custom Tokio harness for relay throughput/latency.
- Python pytest/bench helpers for Python binding overhead if lightweight.
- External tools only if easy to install and optional.

Acceptance:

- Benchmarks are reproducible locally without special infrastructure for baseline cases.

### 34.3 Establish baseline metrics

Record initial baseline metrics in a versioned file.

Suggested output:

```text
docs/performance/BASELINE_YYYY_MM_DD.md
```

Metrics to include:

- OS/CPU/Rust version;
- build profile;
- protocol/mode;
- request/connection count;
- throughput;
- p50/p95/p99 latency where available;
- CPU and memory notes;
- file descriptor counts before/after;
- known caveats.

Acceptance:

- Baselines are transparent enough to compare future regressions.

### 34.4 TCP relay performance tests

Add local benchmarks for core TCP modes.

Modes:

- direct TCP echo through HTTP CONNECT listener;
- direct TCP echo through SOCKS5 listener;
- HTTP listener through HTTP upstream;
- SOCKS5 listener through SOCKS5 upstream;
- multi-hop TCP chain if implemented;
- TLS-wrapped listener/upstream if supported;
- Shadowsocks TCP listener/upstream.

Measure:

- successful connections/sec;
- bytes/sec;
- latency distribution;
- error rate;
- active connection count consistency;
- task count before/after if measurable.

Acceptance:

- Core TCP modes have repeatable performance smoke tests.

### 34.5 UDP performance and loss tests

Add UDP-specific performance checks.

Modes:

- standalone UDP direct relay;
- SOCKS5 UDP ASSOCIATE relay;
- Shadowsocks UDP relay;
- UDP upstream one-hop where supported.

Measure:

- datagrams/sec;
- bytes/sec;
- loss/reorder rate under local load;
- flow table growth and cleanup;
- per-client and per-target limit behavior under stress.

Acceptance:

- UDP relay performance does not regress silently.
- Flow cleanup is verified after sustained traffic.

### 34.6 Reverse/backward soak tests

Add long-running reverse/backward tests.

Scenarios:

- one reverse server and one client relaying repeated payloads;
- repeated client disconnect/reconnect;
- auth failure churn;
- max pending external clients;
- graceful shutdown during relay;
- `parallel_connections > 1` if supported.

Gating:

```bash
EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1
```

Acceptance:

- Reverse mode can run for an extended local soak without task leaks or stuck listeners.

### 34.7 Python binding performance and overhead

Measure overhead of Python wrapper paths.

Tests:

- import cost;
- URI translation cost from Python vs Rust CLI utility path;
- config compile cost through PyO3;
- service start/stop overhead;
- status/metrics polling overhead;
- `check_upstream` timeout behavior;
- GIL release smoke under concurrent Python threads.

Output:

```text
docs/performance/PYTHON_BINDING_OVERHEAD.md
```

Acceptance:

- Python overhead is documented and bounded enough for embedding use cases.

### 34.8 Leak detection and resource accounting

Add resource leak checks.

Track:

- file descriptors before/after tests;
- task tracker empty/drained status;
- active connection count returns to zero;
- UDP flow table cleanup;
- reverse registry cleanup;
- Unix socket cleanup;
- temporary files and rollback files.

Implementation:

- platform-gated FD count helper;
- test utility for before/after assertions;
- optional `loom`/model checks only if already feasible.

Acceptance:

- High-churn tests assert resources return to baseline or documented tolerance.

### 34.9 Regression gate policy

Define which performance tests run where.

Suggested tiers:

- Tier 0: fast unit perf invariants; normal test suite.
- Tier 1: local performance smoke; optional before release.
- Tier 2: soak tests; manual/gated.
- Tier 3: cross-platform benchmark matrix; release candidate only.

Add docs:

```text
docs/performance/REGRESSION_GATE_POLICY.md
```

Acceptance:

- Developers know which commands to run before merges vs releases.

### 34.10 pproxy comparison benchmarks

Where behavior is comparable, add optional pproxy comparison scripts.

Compare:

- HTTP CONNECT local relay;
- SOCKS5 CONNECT local relay;
- standalone UDP relay if compatible enough;
- Python pproxy library start/stop overhead if relevant.

Gate with:

```bash
EGRESS_REQUIRE_PPROXY_PERF=1
```

Rules:

- Do not claim superiority without reproducible numbers.
- Treat pproxy comparison as context, not acceptance gate unless explicitly chosen.
- Record environment and exact commands.

Acceptance:

- pproxy comparisons are reproducible and not mixed into normal tests.

### 34.11 Documentation and manifest updates

Update:

```text
docs/performance/README.md
docs/performance/BENCHMARK_INVENTORY.md
docs/performance/REGRESSION_GATE_POLICY.md
docs/COMPATIBILITY_EVIDENCE.md
docs/PARITY_MATRIX.md
docs/CI_STATUS.md
```

Manifest entries may include:

```text
performance_tcp_relay_smoke
performance_udp_relay_smoke
performance_reverse_soak
performance_python_binding_overhead
resource_leak_fd_cleanup
resource_leak_task_cleanup
pproxy_perf_comparison_http
pproxy_perf_comparison_socks5
```

Acceptance:

- Performance claims in docs are evidence-backed and environment-scoped.

## Validation commands

Fast validation:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p eggress-testkit manifest
```

Benchmark smoke:

```bash
cargo bench --workspace --no-run
scripts/perf/run_local_baseline.sh
```

Soak/manual:

```bash
EGRESS_REQUIRE_SOAK=1 scripts/perf/run_soak.sh
EGRESS_REQUIRE_PPROXY_PERF=1 scripts/perf/run_pproxy_comparison.sh
```

Python:

```bash
maturin develop
python -m pytest python/tests/test_performance_smoke.py -q
```

## Acceptance criteria

Phase 34 is complete when:

- Benchmark inventory exists.
- Fast performance smoke tests exist for core TCP and UDP modes.
- Reverse soak testing exists and is gated.
- Python binding overhead is measured or documented.
- Resource leak checks cover file descriptors/tasks/flows where practical.
- Regression gate policy is documented.
- Optional pproxy comparison scripts are gated and reproducible.
- Baseline results are recorded with environment details.

## Handoff notes

Do not block ordinary development on slow performance tests. The value of this phase is stable measurement and regression visibility, not premature optimization.
