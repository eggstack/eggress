# Performance Evidence and Documentation Polish Pass

## Status

**IMPLEMENTED — 2026-09-20**

## Parent roadmap

[`PERFORMANCE_OPTIMIZATION_ROADMAP.md`](PERFORMANCE_OPTIMIZATION_ROADMAP.md)

## Planning baseline

`fbca72cd0fea195236a22984ad6c963f4a92bcd2` (`perf: optimize runtime hot paths`)

Pre-optimization comparison point:

`d9abcffee95c079cc7938f51ec719c73155c5052` (performance campaign planning commit immediately before the runtime implementation)

## Purpose

Close the remaining evidence and documentation gaps in the runtime performance campaign without reopening the implementation architecture or introducing another optimization campaign.

The runtime changes from Phases 1 and 2 are implemented and CI-clean. The remaining defects are qualification defects:

1. the 2026-09-20 performance record contains post-change measurements but not same-host pre-change measurements, despite the parent roadmap requiring before/after evidence;
2. the planned 16/32/64 KiB relay-buffer experiment was not recorded, so retaining the 64 KiB compatibility setting is prudent but not fully evidenced;
3. TLS qualification measures configuration construction but not the normal prepared-listener/per-accepted-connection path;
4. relay architecture/rustdoc text still contains references to the old pinned-direction-future/`tokio::select!` implementation;
5. the performance record identifies a working tree "based on" the planning commit instead of the exact optimized source commit used for qualification.

This pass must repair those defects and then close the parent roadmap. It is primarily an evidence/documentation pass. It is not authorization to change runtime behavior based on the results.

## Governing constraints

1. Do not change public Rust APIs, configuration schemas, protocol behavior, routing policy, TLS policy, UDP limits, pproxy compatibility, or reload semantics.
2. Do not change the compatibility relay facade's 64 KiB buffer size in this pass. If the experiment shows a compelling reason to tune it, record the evidence and create a separate narrowly approved implementation plan.
3. Do not introduce UDP pooling or change datagram ownership/allocation behavior in this pass. Record observations only.
4. Do not implement HTTP forward upstream reuse.
5. Do not add mandatory performance thresholds to CI.
6. Do not add a benchmark framework, benchmark database, new daemon, custom profiler dependency, or generated evidence system.
7. Existing Criterion benches remain the durable benchmark surface. Temporary comparison harness patches/worktrees must not become production code unless they are independently useful as current benchmarks.
8. Compare the pre-optimization and optimized code using the same host, toolchain, features, benchmark topology, Criterion settings, and power/CPU conditions as closely as practical.
9. Never compare a setup-heavy historical benchmark directly with a new steady-state fixture and label the delta as an optimization result. The timed topology must be equivalent.
10. Documentation must distinguish measured observations, deterministic memory calculations, and qualitative structural evidence.

---

# Finding 1 — Before/after evidence is missing

## Problem

`docs/performance/BASELINE_2026_09_20.md` records representative medians for the optimized tree, but the parent roadmap acceptance criterion requires before/after evidence for the optimized paths.

The old 2026-07-03 baseline is not an acceptable substitute because it used a different date/environment and predates the specific campaign. Several current benchmarks also did not exist at the pre-optimization commit.

## Required comparison method

Use two clean worktrees on the same benchmark host:

```text
baseline worktree  -> d9abcffee95c079cc7938f51ec719c73155c5052
optimized worktree -> fbca72cd0fea195236a22984ad6c963f4a92bcd2
```

The optimized worktree may advance only for benchmark/documentation fixes belonging to this plan; record the exact final SHA actually measured.

For benchmark groups that already exist at both revisions with an equivalent timed topology, run them directly.

For benchmark groups added by `fbca72c`, create one **benchmark-only comparison patch** that can be applied identically to both worktrees. The patch may add benchmark/dev dependencies and benchmark source, but it must not alter production crate source or substitute optimized implementation code into the baseline worktree.

If an identical benchmark harness cannot compile against both revisions because an internal API changed as part of the optimization, use the narrowest adapter required in benchmark-only code and document the adapter difference. The workload and timed boundary must remain equivalent.

Do not commit the baseline worktree or temporary comparison adapters to `main` merely to obtain numbers.

## Required environment record

Record for every comparison run:

- exact baseline SHA;
- exact optimized SHA;
- benchmark-harness patch SHA/hash or a concise description if temporary/uncommitted;
- `rustc --version --verbose`;
- `cargo --version`;
- target triple;
- OS/kernel;
- CPU model and logical core count;
- release/bench profile;
- workspace feature set;
- Criterion sample size, warm-up, and measurement time;
- any relevant CPU governor/power-mode setting that can be determined without privileged system changes.

Do not require privileged host tuning. The purpose is reproducibility, not laboratory certification.

---

# Workstream 1 — Re-run comparable pre/post benchmarks

## 1.1 TLS/executor setup

Measure with the same harness on baseline and optimized revisions:

- default verified client TLS setup/access path;
- default H2 client TLS setup/access path;
- `build_chain_executor` construction.

Interpretation requirement:

- On the baseline commit the operation may construct Rustls state.
- On the optimized commit it may resolve a cached `Arc`.
- The benchmark name/table must make that distinction explicit rather than pretending the underlying operation is identical internally.

The relevant comparison is elapsed cost of the same externally invoked setup operation.

## 1.2 Relay

Run equivalent setup-inclusive and steady-state relay workloads against the old generic-split engine and the optimized single-owner state machine.

At minimum compare:

- 1 KiB;
- 64 KiB;
- 1 MiB;
- 16 concurrent 64 KiB transfers.

Keep the same listener/echo fixture topology and payload generation outside the timed region where the current steady-state benchmark does so.

Record median estimates and Criterion confidence intervals when readily available. Do not reduce the result to a single percentage without retaining the raw representative timings.

## 1.3 Route selection

Using the same route-selection harness, compare at least:

- 1 member, all healthy;
- 32 members, mixed health;
- 128 members, mixed health;
- round-robin;
- least-connections.

The purpose is to detect the effect of lock-free health state reads and ensure there is no selection-path regression. Do not benchmark only `Router::decide()`; the workload must reach normal `route()/select` behavior.

## 1.4 Standalone UDP

Use the same local end-to-end UDP fixture on both revisions for:

- one established 64-byte hot flow;
- multi-target admission, using the existing eight-target workload or an equivalent fixed cardinality.

The admission case should expose the baseline O(number-of-clients/flows) accounting behavior and the optimized O(1) aggregate under a workload with enough active state to make the distinction observable.

If eight targets is too small to show the algorithmic effect reliably, add a larger **benchmark-only** cardinality such as 32 or 128 active flows while keeping the durable benchmark reasonably quick.

Do not change UDP runtime limits or semantics to manufacture a favorable result.

## Required result table

Add a concise table to the dated performance record with columns equivalent to:

```text
Path | Workload | Baseline | Optimized | Delta | Interpretation
```

Use `improved`, `neutral/noisy`, or `regressed` only after showing the actual measurements.

A regression larger than roughly 5% on a representative steady-state path should be investigated before closure. This is a review threshold, not an automated gate. If noise overlaps materially, classify it as inconclusive/neutral rather than manufacturing precision.

---

# Workstream 2 — Complete the relay buffer-size evidence matrix

## Problem

The original Phase 3 plan required a 16/32/64 KiB experiment before making a buffer-size decision. The optimized benchmark currently exercises the compatibility/default 64 KiB path but does not record the matrix.

## Required benchmark

Extend `benches/tcp_relay.rs` or add a tightly scoped benchmark group in the existing file that calls `eggress_relay::relay_with_options` with explicit buffer sizes:

- 16 KiB;
- 32 KiB;
- 64 KiB.

Use the same relay engine and fixture for every size.

Measure at minimum:

- 1 KiB transfer;
- 64 KiB transfer;
- 1 MiB transfer;
- 16 concurrent 64 KiB transfers.

If the matrix becomes excessively slow, it is acceptable to retain 64 KiB concurrency plus large-transfer cases for all sizes and keep the small case to a subset, but the final record must still contain enough evidence to assess latency/throughput and concurrency.

## Memory evidence

Record the deterministic relay-buffer allocation per active relay:

```text
2 * buffer_size
```

and representative totals for at least:

- 100 active relays;
- 1,000 active relays.

This is an allocation calculation, not an RSS measurement. Label it accordingly.

An optional same-host RSS observation may be added if it can be obtained simply and repeatably, but no new allocator/profiler dependency is required.

## Decision rule for this pass

Regardless of the result, **do not modify the production 64 KiB compatibility setting in this polish pass**.

Conclude one of:

- `retain-64k-supported`: lower sizes show a material representative regression;
- `retain-64k-inconclusive`: differences are noisy/mixed;
- `lower-buffer-follow-up-justified`: 16/32 KiB appears materially memory-better with no representative performance regression.

Only the third conclusion authorizes writing a new implementation plan. It does not authorize silently changing the buffer in this pass.

---

# Workstream 3 — Add prepared-listener/per-connection TLS evidence

## Problem

`benches/tls_setup.rs` currently demonstrates:

- cached default client config access;
- chain-executor construction;
- raw listener `ServerConfig` construction.

That proves the construction cost exists, but it does not measure the normal runtime path after listener TLS state has been prepared.

## Required benchmark topology

Add one normal-runtime TLS connection benchmark that uses the same public startup/listener path as production rather than exposing a private supervisor helper merely for benchmarking.

Preferred shape:

1. generate one self-signed test certificate outside the timed loop;
2. construct/start a local Eggress TLS listener once;
3. perform repeated local TLS client connections/handshakes through that running listener;
4. keep supervisor/listener startup outside the timed operation;
5. time the accepted connection/handshake path;
6. shut the fixture down cleanly after the benchmark group.

Apply the equivalent benchmark harness to the pre-optimization worktree so the baseline runtime executes its former per-connection server-config construction naturally.

If the full supervisor fixture is disproportionately complex, a fallback comparison may reproduce the two exact path shapes in benchmark code:

- baseline: parse PEM/build `ServerConfig` + `tls_accept` per connection;
- optimized: clone prebuilt `Arc<ServerConfig>` + `tls_accept`.

The fallback must be labeled a component benchmark, not a full runtime benchmark.

## Guardrails

Do not:

- make `prepare_tls_server_config` public solely for Criterion;
- add a permanent public benchmark-support API;
- weaken certificate verification in production code;
- move certificate generation into the timed region;
- include listener bind/startup cost in the per-connection measurement.

Record both raw `ServerConfig` construction cost and prepared per-connection handshake cost so reviewers can see what work was moved out of the connection path.

---

# Workstream 4 — Correct stale relay documentation

## Required source/documentation corrections

Review and correct at minimum:

```text
crates/eggress-relay/src/lib.rs
architecture/relay.md
docs/performance/BASELINE_2026_09_20.md
docs/performance/BENCHMARK_INVENTORY.md
docs/performance/README.md
plans/PERFORMANCE_PHASE_3_BENCHMARK_AND_QUALIFICATION.md
plans/PERFORMANCE_OPTIMIZATION_ROADMAP.md
```

### Relay rustdoc

The current `relay_with_options` rustdoc still describes "two direction futures" being polled concurrently and the unfinished direction being dropped. Update it to describe the actual single `RelayFuture` with two directional states and no detached tasks.

### Architecture reviewer notes

Remove/update stale statements in `architecture/relay.md` that still claim:

- `tokio::select!` over pinned direction futures is load-bearing;
- timeout owns/drops a boxed surviving direction future.

Replace them with the actual invariants:

- one future owns both complete streams;
- bounded directional poll work provides fairness;
- first-close state determines which direction remains active;
- the optional `Sleep` deadline is stored on the relay future;
- timeout returns the current directional byte counters without spawning/detaching work.

Search the repository for other stale references to the old generic-split/select design:

```bash
rg -n 'tokio::io::split|pinned futures|direction futures|surviv(ing|or).*future|select!.*relay|split-lock' \
  README.md architecture docs crates plans
```

Do not rewrite historical plan descriptions that are intentionally describing the pre-change baseline unless they are currently presented as active architecture.

---

# Workstream 5 — Repair the qualification record

Update `docs/performance/BASELINE_2026_09_20.md` so it records exact provenance.

Required content:

1. exact pre-optimization SHA;
2. exact optimized SHA actually benchmarked;
3. exact environment/toolchain;
4. benchmark command/settings;
5. comparable before/after result table;
6. relay 16/32/64 KiB result matrix;
7. deterministic per-relay buffer-memory table;
8. prepared-listener/per-connection TLS evidence;
9. explicit statement that results are local qualification observations, not portable CI thresholds;
10. explicit statement of which potential optimizations remain intentionally unchanged.

Do not overwrite the older 2026-07-03 historical baseline. The 2026-09-20 file remains the record for this campaign.

If implementation/benchmark cleanup causes the final measured optimized SHA to differ from `fbca72c`, record both:

- runtime implementation commit: `fbca72c...`;
- qualification commit/working tree actually measured.

Do not use "working tree based on ..." as the sole provenance statement.

---

# Workstream 6 — Align plan status with evidence

Before evidence is complete, update the parent roadmap status to indicate the runtime work is implemented but evidence polish is open.

After all acceptance criteria below are satisfied:

- set this plan to `IMPLEMENTED`;
- set the parent roadmap back to a fully closed `IMPLEMENTED` status;
- update Phase 3's closure section with a short pointer to this follow-up rather than duplicating every result;
- ensure the roadmap's registered plan table lists this plan as the final closure item.

Do not create another completion-report file. The dated performance record plus plan closure sections are sufficient.

---

# Verification

## Benchmark compilation

All durable benchmark targets must compile:

```bash
cargo bench --bench tcp_relay --no-run
cargo bench --bench route_match --no-run
cargo bench --bench udp_relay --no-run
cargo bench --bench tls_setup --no-run
```

If Criterion/Cargo does not support `--no-run` in the repository's invocation shape, use the narrow equivalent `cargo check --benches` plus one smoke invocation per changed benchmark.

## Focused behavior checks

Because this pass should not change production behavior, run at minimum:

```bash
cargo test -p eggress-relay
cargo test -p eggress-transport-tls
cargo test -p eggress-runtime
cargo test -p eggress-udp
cargo test -p eggress-routing
```

## Final repository gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

The existing Python smoke and normal push CI should remain green. No new performance CI gate is required.

---

# Stop conditions

Stop and document the limitation instead of expanding scope if:

1. the pre-optimization commit cannot compile on the qualification host with the repository-pinned/declared toolchain without unrelated repairs;
2. an identical benchmark-only harness would require modifying production code on the baseline revision;
3. the full runtime TLS fixture requires a new public API solely for measurement;
4. performance variance is too high to distinguish the compared cases reliably;
5. the relay buffer matrix suggests a runtime tuning change but that change would exceed this evidence-only scope.

For cases 1–4, retain the strongest valid structural/component evidence and mark the affected comparison explicitly incomplete. Do not fabricate a closure claim.

---

# Acceptance criteria

This polish pass is complete only when all of the following are true:

- [x] The parent roadmap registers this plan as the final evidence/documentation closure item.
- [x] Same-host pre/post measurements exist for representative TLS/executor, relay, route-selection, and standalone UDP paths, or a specific stop-condition limitation is recorded.
- [x] The pre-optimization SHA and optimized measured SHA are exact and recorded.
- [x] Any temporary benchmark-only comparison patch is applied equivalently and does not alter production source in the baseline worktree.
- [x] The dated performance record shows actual baseline and optimized timings rather than only post-change medians.
- [x] Results distinguish setup-inclusive from steady-state workloads.
- [x] Route evidence reaches `route()/select`, not only rule matching.
- [x] UDP evidence exercises actual local runtime flow/admission paths, not only codec encode/decode.
- [x] A 16/32/64 KiB relay-buffer matrix is recorded.
- [x] Relay buffer allocation math is recorded for representative concurrency and clearly labeled as deterministic allocation, not measured RSS.
- [x] The 64 KiB production compatibility setting remains unchanged by this pass.
- [x] Prepared-listener/per-accepted-connection TLS behavior has benchmark or clearly labeled component evidence.
- [x] No private runtime helper is made public solely for benchmarking.
- [x] `relay_with_options` rustdoc describes the current state-machine implementation.
- [x] `architecture/relay.md` no longer contains active reviewer guidance for the old pinned-future/`tokio::select!` engine.
- [x] Repository search finds no other misleading active documentation for the removed generic-split relay architecture.
- [x] `docs/performance/BASELINE_2026_09_20.md` contains exact provenance and the completed evidence tables.
- [x] Phase 3 and the parent roadmap point to this follow-up closure without duplicative report files.
- [x] No runtime API, configuration, protocol, routing, TLS, UDP, or pproxy behavior is intentionally changed.
- [x] No UDP pooling, HTTP upstream reuse, zero-copy path, or unrelated optimization is introduced.
- [x] Benchmark targets compile and the focused tests pass.
- [x] Workspace fmt, clippy, and locked tests pass.
- [x] Push CI/Python smoke remain green; the last remote verification for the unchanged runtime was green, and this local-only pass did not trigger a new remote run.
- [x] After all evidence is recorded, this plan and the parent roadmap are marked fully implemented/closed.

## Closure — 2026-09-20

Implemented. The dated qualification record
[`docs/performance/BASELINE_2026_09_20.md`](../docs/performance/BASELINE_2026_09_20.md)
now records exact before/after SHAs, the temporary benchmark-only comparison
adapter, host/toolchain/Criterion settings, comparable TLS/executor, relay,
route-selection, and standalone-UDP results, the 16/32/64 KiB matrix, and
deterministic relay-buffer allocation arithmetic.

The TLS connection evidence uses the permitted component fallback: a local
running listener keeps startup outside the timed operation while contrasting
per-connection PEM/config construction with a prepared `Arc<ServerConfig>`.
No private runtime helper or permanent benchmark-support API was added.

The result is `retain-64k-supported`; the compatibility relay remains at 64
KiB, UDP allocation semantics remain unchanged, HTTP forward upstream reuse
remains deferred, and no production behavior or public API was changed by
this evidence-only pass. The parent roadmap and Phase 3 closure now point to
this completed follow-up.
