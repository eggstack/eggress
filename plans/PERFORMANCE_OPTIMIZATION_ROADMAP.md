# Runtime Performance Optimization Roadmap

## Status

**IMPLEMENTED — 2026-09-20**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `80c8f2f2464bb3c4182afee73a8fdbdc170b1cdb`
- Current workspace release line: `1.0.7`
- Primary constraint: improve runtime efficiency without reducing protocol capability, changing documented public API, or weakening pproxy compatibility.

## Purpose

This roadmap captures a bounded performance pass identified by review of the current TCP relay, outbound execution, TLS transport, routing/health, UDP runtime, and benchmark surfaces.

The architecture does not need a rewrite. The highest-value opportunities are repeated setup work, avoidable indirection/synchronization, and one algorithmic UDP accounting cost. The work is therefore split into three focused implementation phases:

1. remove repeated TLS/configuration construction and redundant stream/setup allocations;
2. reduce hot-path synchronization and per-datagram bookkeeping while preserving runtime semantics;
3. strengthen the benchmark/evidence suite and use measurements to decide secondary tuning.

This is not a feature roadmap and must not become one.

## Governing constraints

1. Preserve the current public Rust API unless an internal type is explicitly `pub(crate)` or private.
2. Preserve current CLI, configuration, URI, pproxy compatibility, protocol, routing, TLS verification, reload, health, metrics, and shutdown semantics.
3. Do not add platform-specific zero-copy paths, unsafe code, custom allocators, global buffer pools, DNS caching, or new public performance knobs without measured need.
4. Do not replace existing modular crate boundaries merely to improve a microbenchmark.
5. Prefer build-once/cache-once immutable state and simpler ownership over new background tasks or caches with invalidation protocols.
6. Benchmark changes must measure the actual code path being optimized. Setup-heavy microbenchmarks may remain as macrobenchmarks but cannot be the sole evidence.
7. No claim of percentage improvement is required in advance. Each implementation should remove a demonstrable source of repeated work and must not regress representative throughput/latency materially.
8. If a proposed optimization complicates semantics more than its measured benefit justifies, do not land it.
9. Existing completed plans remain historical records. In particular, `REUSABLE_RELAY_EXTRACTION_AND_API_STABILIZATION.md` stays complete; this campaign optimizes the extracted relay implementation rather than reopening its public API design.
10. HTTP forward upstream connection reuse is explicitly deferred. It changes connection lifetime, routing/reload interaction, health/lease ownership, response framing, and retry behavior and therefore requires a separate design investigation if pursued later.

## Current-state findings

### Repeated outbound TLS configuration construction

The server constructs a fresh chain executor when opening a selected upstream route. The outbound executor in turn builds verified Rustls client configuration, H2 ALPN configuration, and feature-gated insecure variants. Those configurations are immutable and derive from process-static/default trust material in the ordinary path.

The comments describe these objects as shared, but the current server construction scope is per route/open attempt. Rebuilding trust/configuration state for each connection is unnecessary.

### Inbound TLS server configuration rebuilt per accepted connection

The standard TCP listener path clones the compiled listener TLS material into the connection task. Server wrapping then reparses certificate/key PEM and constructs a new Rustls `ServerConfig` for each connection.

TLS material should instead be compiled once per prepared listener generation and shared by `Arc` across accepted sessions. Reload already provides a natural generation boundary.

### Ordinary TCP streams carry redundant heap/type-erasure layers

The listener currently boxes `TcpStream` before placing it inside a permit-holding wrapper, then boxes the wrapper. The supervisor can add another box before TLS wrapping even though the accepted stream is already a `BoxStream`.

The externally visible `AcceptedConnection.stream: BoxStream` boundary can remain unchanged while removing the inner and re-boxing layers.

### Generic relay splitting adds synchronization to every I/O poll

The extracted `eggress-relay` crate is intentionally generic. Its current use of generic Tokio `io::split` places the stream behind shared synchronization. Tokio's specialized TCP split avoids this cost, but a TCP-only public fast path would conflict with the generic relay contract and would not help TLS/other wrapped streams.

The preferred solution is an internal single-task bidirectional copy state machine that owns/pins both streams and two directional buffers without generic split locking. Existing relay options, errors, byte accounting, half-close policy, and cancellation semantics remain authoritative.

### Health-state reads are lock-backed in routing hot paths

Upstream runtime counters are already atomic, but health state is read through an `RwLock`. Selection prefilters eligible members and built-in schedulers may inspect eligibility again, so state reads can occur multiple times during a route selection.

The public scheduler contract must not change. A compact atomic health-state representation can remove read locking while retaining the full health snapshot under its existing lock for diagnostics and transition bookkeeping.

### Per-listener UDP service construction is repeated per TCP connection

The standard TCP accept loop creates a UDP service for each accepted connection even though the service inputs are listener-scoped: routing, registry, metrics, configuration, and task tracking.

Build the immutable/shareable service once per prepared listener generation and clone its `Arc` into per-connection configuration.

### Standalone UDP admission scans all client flows

Standalone UDP currently computes a capped total target-flow count by scanning client state on each datagram before admitting a new flow. The owning loop already serializes the mutations required to maintain an exact aggregate count.

Track total target flows incrementally and update it on creation/reap/removal. This changes an O(number of clients) admission step to O(1) without changing public policy.

### Secondary costs require evidence first

Two real costs should not be changed blindly:

- relay buffers are currently 64 KiB per direction, or 128 KiB per active tunnel;
- several UDP send/receive paths allocate owned buffers per datagram.

Both may be worth tuning, but the current benchmark suite does not provide enough end-to-end concurrency/PPS evidence to select a safe replacement.

## Registered execution plans

| Order | Plan | Status | Purpose |
|---|---|---|---|
| 1 | [`PERFORMANCE_PHASE_1_TLS_AND_CONNECTION_SETUP.md`](PERFORMANCE_PHASE_1_TLS_AND_CONNECTION_SETUP.md) | Implemented | Cache/build TLS state at the correct lifetime and remove redundant TCP/UDP connection setup work. |
| 2 | [`PERFORMANCE_PHASE_2_RELAY_ROUTING_UDP_HOT_PATHS.md`](PERFORMANCE_PHASE_2_RELAY_ROUTING_UDP_HOT_PATHS.md) | Implemented | Remove generic relay split synchronization, lock-backed health reads, and O(1) UDP flow accounting. |
| 3 | [`PERFORMANCE_PHASE_3_BENCHMARK_AND_QUALIFICATION.md`](PERFORMANCE_PHASE_3_BENCHMARK_AND_QUALIFICATION.md) | Implemented | Establish path-accurate evidence, qualify phases 1–2, and gate any secondary memory/allocation tuning. |

Do not split these into per-function plans unless implementation discovers a correctness blocker that cannot safely be handled inside the owning phase.

## Sequencing

Phase 1 may land independently and should be implemented first because it is lower-risk and removes clearly repeated construction/allocation work.

Phase 2 may begin after Phase 1 or on a separate branch, but final qualification should use the Phase 1 state so benchmark evidence reflects the intended aggregate runtime.

Phase 3 is not optional documentation polish. It is the evidence/closure phase and must run after the code changes. Secondary relay buffer or UDP allocation changes may be implemented inside Phase 3 only when the new measurements justify them and the plan's stop conditions are satisfied.

## Explicit non-goals

Do not use this campaign to:

- change supported protocols/transports or pproxy parity claims;
- redesign `ProxyChainSpec`, routing rule syntax, scheduler public traits, or health policy thresholds;
- add an upstream connection pool to HTTP forward mode;
- cache DNS answers in `DirectConnector`;
- add Linux `splice`, `io_uring`, kTLS, platform-specific syscalls, or `unsafe`;
- remove `ArcSwap`, task tracking, session metrics, or current atomic counters;
- alter TLS certificate verification defaults or insecure-mode semantics;
- change reload visibility or listener generation semantics;
- add a generic object pool framework;
- make Criterion benchmarks mandatory CI gates;
- set hard performance percentages that are not portable across machines;
- reduce the legacy 64 KiB relay buffer before representative measurements exist;
- rewrite HTTP parsing merely because byte-at-a-time head parsing may be optimizable.

## Verification policy

Every phase runs focused tests during development and the normal workspace gate before merge:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Feature-specific tests should be run for the crates touched by that phase. Existing compatibility and differential tests remain authoritative for behavior; do not replace them with synthetic performance tests.

Benchmark results are evidence, not a compatibility oracle. A benchmark change that invalidates old setup-heavy numbers must document that the topology changed rather than pretending the historical numbers are directly comparable.

## Roadmap acceptance criteria

This roadmap is complete only when all of the following are true:

1. Default outbound TLS client configurations are not rebuilt per upstream connection.
2. Inbound listener certificate/key material is not reparsed and rebuilt into a Rustls server configuration per accepted TLS connection.
3. Ordinary TCP acceptance no longer boxes the raw `TcpStream` inside an already-boxed permit wrapper, and supervisor TLS wrapping does not re-box an existing `BoxStream`.
4. The extracted relay engine no longer relies on generic `tokio::io::split` synchronization for the core copy loop.
5. Relay public API, error reporting, byte accounting, half-close behavior, and cancellation semantics remain compatible with the completed extraction contract.
6. Routing eligibility can read current health state without taking the full health snapshot `RwLock`.
7. The public scheduler candidate contract remains unchanged.
8. UDP service construction is listener-generation scoped rather than repeated per accepted TCP connection.
9. Standalone UDP target-flow admission uses O(1) aggregate accounting rather than rescanning clients per datagram.
10. Benchmarks measure actual relay, route-selection, and UDP relay paths rather than only codec/setup behavior.
11. Before/after evidence is recorded for the optimized paths with test environment details sufficient to interpret the result.
12. Any relay buffer-size or UDP allocation/pooling change is justified by the new measurements and can be reverted independently if it fails the qualification thresholds.
13. Workspace fmt/clippy/tests pass.
14. No public API, configuration, protocol capability, or pproxy compatibility regression is introduced.
15. HTTP forward upstream reuse remains out of scope unless a separate approved design plan is created.

## Closure

The three phases are implemented. Default TLS configuration access is
process-shared, listener TLS and UDP state are generation-scoped, relay
copying no longer uses generic split locks, health eligibility reads use an
atomic summary, and standalone UDP admission maintains an exact owner-local
aggregate. New Criterion fixtures exercise setup-inclusive and steady-state
relay, route selection, actual standalone UDP flows, and TLS construction.

The compatibility relay facade remains at two 64 KiB buffers with its bounded
one-second drain. The qualification run did not provide evidence sufficient to
change that public compatibility tuning or justify UDP pooling; both remain
explicitly retained.
