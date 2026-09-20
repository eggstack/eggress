# Performance Phase 3 — Benchmark and Qualification

## Status

**READY FOR IMPLEMENTATION — 2026-09-20**

## Parent roadmap

[`PERFORMANCE_OPTIMIZATION_ROADMAP.md`](PERFORMANCE_OPTIMIZATION_ROADMAP.md)

## Planning baseline

`80c8f2f2464bb3c4182afee73a8fdbdc170b1cdb`

## Objective

Create enough path-accurate performance evidence to qualify Phases 1–2 and make evidence-based decisions about the two remaining secondary tuning opportunities:

- relay buffer size / per-connection memory;
- UDP per-datagram owned-buffer allocation.

This phase is not a general benchmark framework. It extends the existing Criterion benches and focused smoke coverage only where the current suite cannot measure the optimized path.

## Principles

1. Keep benchmarks understandable and local to existing `benches/`.
2. Separate connection/setup cost from steady-state data-path cost.
3. Record environment details alongside results used for decisions.
4. Do not turn noisy wall-clock benchmarks into mandatory CI thresholds.
5. Use relative comparisons on the same machine/run where possible.
6. Preserve at least one macrobenchmark that includes realistic setup; add steady-state fixtures rather than replacing all setup-heavy coverage.
7. A memory-saving change may land without a throughput win if throughput/latency stays materially neutral and the memory reduction is meaningful.
8. A throughput optimization that materially increases memory, complexity, or maintenance burden requires stronger evidence.

## Workstream 1 — Correct/extend TCP relay benchmarking

### Existing limitation

The current `tcp_relay` benchmark now correctly sends data through Eggress relay and includes a Tokio `copy_bidirectional` baseline, but each Criterion iteration includes listener creation, connects, task setup, and payload allocation. For small payloads this can dominate the relay engine cost.

### Required benchmark topology

Retain an end-to-end setup-inclusive benchmark and add a steady-state fixture variant where:

- upstream echo endpoint is already listening;
- proxy listener/relay service is already running;
- repeated iterations reuse the service fixture;
- each sample may still open a connection if the intent is connection cost, but listener/task setup is not recreated inside the timed inner operation;
- a separate established-stream/data-transfer measurement may be added using in-memory duplex streams or a persistent fixture if it cleanly isolates copy-engine cost.

Measure at least:

- 1 KiB;
- 64 KiB;
- 1 MiB or another larger transfer that exercises steady-state copy behavior.

Add a concurrent case representative of many active tunnels. Keep concurrency bounded enough for stable local execution.

### Comparison

Compare:

- current `eggress-relay`;
- equivalent Tokio `copy_bidirectional` baseline where semantics/topology can be made comparable.

Do not claim the Tokio baseline has identical half-close/error reporting semantics; it is a lower-level throughput reference only.

## Workstream 2 — Add route-selection benchmarks that reach `select()`

### Existing limitation

`route_match` measures rule matching/`Router::decide()` but not the upstream selection path containing health eligibility and scheduler work.

### Required cases

Add selection benchmarks for upstream groups approximately:

- 1 member;
- 8 members;
- 32 members;
- 128 members.

For each useful size, include representative scenarios:

- all healthy;
- mixed healthy/unhealthy/recovering state;
- round-robin;
- least-connections or other built-in scheduler whose selection path materially differs.

Use deterministic health/runtime fixtures. Do not perform network I/O in this benchmark.

The benchmark should exercise the same `Router::route`/`select` path used by runtime code, not call a private helper that bypasses candidate filtering.

## Workstream 3 — Add end-to-end UDP relay PPS/throughput benchmarks

### Existing limitation

`udp_relay` currently measures SOCKS5 UDP encoding/decoding mechanics, not `udp_relay_loop` or standalone UDP relay flow management.

### Required benchmark coverage

Add a bounded local UDP fixture that exercises:

- a single client -> single target hot flow after establishment;
- multiple clients/targets;
- flow creation/admission path;
- steady-state datagrams on existing flows;
- reap/removal accounting in a focused non-throughput test or benchmark where practical.

Collect:

- datagrams/sec or time per fixed datagram batch;
- payload size(s), at least one small packet and one moderate packet;
- active client/target-flow cardinality for each case.

The benchmark must not depend on external networks.

Keep codec-only benchmarks because they remain useful for separating framing cost.

## Workstream 4 — Measure TLS/setup construction directly enough to validate Phase 1

Add focused benches or inexpensive timing harnesses for:

- default outbound TLS client configuration access/construction;
- chain executor construction for representative direct/TLS/H2-capable paths;
- listener TLS preparation/server-config construction;
- per-accepted-connection path after prepared listener creation.

The goal is to show that expensive configuration work moved out of per-connection setup, not to optimize Rustls internals.

If pointer-identity/unit tests plus end-to-end connection benchmarks make a dedicated Criterion group redundant, document that and avoid duplicate benches.

## Workstream 5 — Record memory evidence for relay buffer sizing

### Current policy

The compatibility facade deliberately uses two 64 KiB relay buffers per active tunnel. That choice was preserved by the completed relay extraction.

### Required experiment

Using the generic relay options or an internal benchmark harness, compare at least:

- 16 KiB;
- 32 KiB;
- 64 KiB.

Evaluate:

- small interactive/request-sized transfers;
- larger sequential transfers;
- concurrent active tunnels;
- approximate resident memory or deterministic buffer allocation accounting at representative concurrency.

A simple deterministic calculation of buffer bytes per connection is acceptable for the allocation component:

```text
2 * buffer_size * active_relays
```

but throughput/latency must still be measured.

### Decision rule

Changing the Eggress compatibility facade from 64 KiB is authorized inside this phase only if:

- 32 KiB (or another tested lower size) has no material throughput/latency regression across representative cases;
- memory reduction is meaningful at realistic concurrency;
- relay correctness tests pass unchanged;
- no protocol-specific regression appears in large/slow bidirectional transfers.

As a default interpretation, treat a consistent >5% regression in representative steady-state throughput or p95-like sample latency as material unless measurement noise explains it. Do not mechanize this into CI.

If evidence is mixed, keep 64 KiB.

The generic `eggress-relay` default is a public behavior/tuning choice; do not change it solely because Eggress's compatibility facade can safely use a different internal option. Any generic-default change requires explicit API/release consideration.

## Workstream 6 — Evaluate UDP owned-buffer allocation before pooling

### Current costs

Several UDP paths allocate:

- a new `Vec` for outbound SOCKS5 UDP framing;
- a new owned payload (`to_vec`) when handing received datagrams across bounded MPSC channels.

The channel ownership model provides useful backpressure/isolation, so eliminating allocations must not accidentally replace it with borrowed buffers whose lifetime crosses an await unsafely.

### Measurement first

Use the new UDP relay benchmark plus an allocation profiler/counter if readily available locally. Do not add a required repository dependency solely for allocation counting.

Determine whether allocation cost is material relative to socket syscalls, routing, and framing.

### Permitted bounded optimizations

If allocation is demonstrably material, prefer in this order:

1. reuse a per-flow outbound framing `Vec`/buffer where only one owner mutates it;
2. reserve/reuse capacity for known maximum header + payload patterns;
3. use an existing lightweight owned-byte type already in the dependency graph if it clearly reduces copies;
4. only then consider a small bounded pool local to the UDP runtime.

Do not create a process-global generic buffer pool. Do not add lock-heavy pooling that simply trades allocator cost for contention. Do not make buffer ownership observable in public APIs.

### Qualification

Any allocation optimization must preserve:

- bounded MPSC backpressure;
- datagram boundaries/content;
- client/target association;
- shutdown/reap behavior;
- memory cap behavior;
- existing metrics.

If profiling shows allocation is not a leading cost, record that conclusion in the plan status/closure note and make no code change.

## Workstream 7 — Preserve useful existing smoke tests

`crates/eggress-runtime/tests/performance_smoke.rs` uses broad timing thresholds. Keep it as a regression smoke, but do not cite it as proof of a speedup.

If thresholds are currently so broad or flaky that they provide no signal, adjust only with evidence and preserve their role as coarse regression guards. Do not transform them into microbenchmarks.

## Benchmark environment record

For before/after results used to make decisions, record at least:

- git commit;
- `rustc --version`;
- target triple;
- OS/kernel;
- CPU model/architecture;
- release profile;
- relevant feature set;
- Criterion sample configuration if changed.

The record may live in the final plan closure note/commit or PR description. Do not create a generated benchmark-results subsystem.

## Suggested commands

Adjust names to actual Criterion filters after implementation:

```bash
cargo bench --bench tcp_relay
cargo bench --bench route_match
cargo bench --bench udp_relay
cargo bench --bench http_connect_upstream
```

For Phase 1/2 before-after comparison, use a clean build and the same host/power conditions where practical.

Final correctness gate remains:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## HTTP forward mode — explicitly deferred

The current HTTP forward request loop opens a route/upstream for each client request and sends `Connection: close` upstream. Reusing upstream connections could avoid route/connect/TLS/proxy-chain setup, but it is not a safe mechanical optimization.

A future plan must first define:

- reuse only for the same origin/target vs broader pooling;
- route/snapshot/reload changes between client requests;
- health failure attribution;
- active upstream lease lifetime;
- authentication boundary;
- response framing/EOF and upstream `Connection: close`;
- stale connection detection;
- retry/idempotency rules;
- interaction with proxy chains, TLS SNI, and H2 pooling already present elsewhere.

Do not implement HTTP forward upstream reuse in this performance campaign.

## Acceptance criteria

This phase is complete when:

1. `tcp_relay` contains both realistic setup-inclusive coverage and a fixture that reduces repeated listener/task setup from the timed path.
2. Relay benchmarks include small, medium, large, and bounded-concurrency transfer cases.
3. A Tokio copy baseline remains available where topology is comparable.
4. Route benchmarks exercise upstream selection, not only rule matching.
5. Route selection covers multiple group sizes and mixed health states.
6. UDP benchmarks exercise actual relay/flow runtime paths in addition to codec framing.
7. UDP benchmarks include existing-flow and flow-admission cases.
8. Phase 1 setup changes have direct or end-to-end evidence showing expensive TLS/server configuration work is no longer per connection.
9. Before/after Phase 1 and Phase 2 results are recorded on the same or sufficiently comparable environment.
10. No mandatory CI performance threshold is introduced.
11. Relay buffer size is changed only if the specified memory/performance evidence supports it; otherwise 64 KiB is retained.
12. Any UDP allocation optimization is based on observed allocation significance and remains bounded/local in ownership; otherwise current allocation behavior is retained.
13. Existing performance smoke coverage remains coarse regression coverage rather than being misrepresented as benchmark proof.
14. HTTP forward upstream reuse is not implemented as part of this phase.
15. Full workspace fmt/clippy/tests pass after any evidence-driven secondary tuning.
16. The parent roadmap is updated to `IMPLEMENTED`/closed with concise measured outcomes and any intentionally retained costs.
