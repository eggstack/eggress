# Reusable Relay Extraction and API Stabilization

## Status

Implementation plan for handoff.

This plan is motivated by a concrete external consumer need (Synvoid wants to reuse Eggress's raw bidirectional relay without importing Eggress's routing, URI, TLS, listener, runtime, or protocol stack), but the implementation must remain a general Eggress architectural improvement. No Synvoid-specific types, feature flags, adapters, callbacks, naming, configuration schema, or policy belong in Eggress.

The work is intentionally bounded to extracting the protocol-neutral byte-relay engine from `eggress-core`, making its half-close/error behavior explicit enough for safe external reuse, preserving the existing `eggress_core::relay` API and Eggress server behavior, and correcting the documentation/benchmark drift discovered during the audit.

No Synvoid repository changes are part of this plan.

## Objective

Create a small public `eggress-relay` crate that owns generic asynchronous bidirectional stream copying and can be consumed independently of the rest of Eggress.

The target outcome is:

```text
external Rust consumer
        |
        v
  eggress-relay
  generic AsyncRead + AsyncWrite relay engine
        ^
        |
  eggress-core::relay
  compatibility facade preserving existing BoxStream API/behavior
        ^
        |
  eggress-server
  existing connection/session policy and metrics
```

The extraction should reduce coupling for low-level consumers without changing Eggress's central architectural rule that protocol/transport boundaries inside Eggress use `BoxStream`.

The new crate must not become a second proxy runtime. It should know nothing about:

- proxy protocols;
- `TargetAddr` or upstream routing;
- TLS/SSH/QUIC;
- URI parsing;
- Eggress configuration;
- listener lifecycle;
- metrics backends;
- pproxy compatibility;
- Synvoid WAF/routing/policy concepts.

It should be a small Tokio stream primitive with a stable, explicit relay contract.

## Current-state findings

### 1. The reusable primitive is trapped behind a broad crate boundary

The current implementation is `crates/eggress-core/src/relay.rs`:

```rust
pub async fn relay(client: BoxStream, server: BoxStream) -> RelayResult
```

`eggress-core` is intentionally much broader than relay functionality. It also owns:

- `BoxStream` and destination/identity types;
- listener limits and socket handling;
- direct connection/DNS-rebinding policy;
- protocol detection and replay buffering;
- chain execution and hop handling;
- capability classification;
- URI-facing protocol disposition.

Its dependency graph consequently includes `eggress-uri`, `socket2`, `tokio-util`, `serde`, `rustls`, `trait-variant`, `bytes`, and other dependencies that a generic byte relay does not need.

Depending on all of `eggress-core` merely to copy two duplex streams would move maintenance burden rather than reduce it for an external consumer.

### 2. The current relay has hidden transport policy

`crates/eggress-core/src/relay.rs` currently hard-codes:

```rust
const RELAY_HALF_CLOSE_DRAIN: Duration = Duration::from_secs(1);
const RELAY_ABORT_GRACE: Duration = Duration::from_secs(1);
```

When one direction reaches EOF cleanly, Eggress calls `shutdown()` on the corresponding peer write half. It then gives the opposite direction one second to finish. If that second direction is still reading after the deadline, Eggress aborts it.

That behavior prevents leaked connections when a peer never answers a FIN, but it is not a protocol-neutral TCP semantic. A client may legitimately finish writing its request and half-close while an upstream takes longer than one second to produce its response. Arbitrary TCP relays therefore need the half-close drain policy to be explicit/configurable rather than an undocumented fixed property of the primitive.

The existing Eggress behavior must remain unchanged through the compatibility facade unless a separate behavioral decision explicitly changes it.

### 3. The current public result collapses useful failures

`relay()` returns:

```rust
pub struct RelayResult {
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
    pub termination_reason: TerminationReason,
}
```

and collapses all directional I/O/task failures into:

```rust
TerminationReason::Error
```

The underlying I/O error is only written to a debug log.

That is sufficient for the current `eggress-server` path, which maps `Error` to `SessionOutcome::RelayFailed`, but it is weak as a reusable library contract. An embedding application may need the actual `io::ErrorKind`, the failing direction, and byte counts when deciding whether a reset, broken pipe, timeout, or other condition is expected.

The extracted crate should preserve those details. The `eggress-core` compatibility wrapper should continue collapsing them exactly as the existing API does.

### 4. `BoxStream` is appropriate inside Eggress but unnecessarily restrictive for the low-level engine

Eggress deliberately boxes streams at protocol/transport boundaries so multi-protocol composition does not leak generics through the architecture. That invariant should remain.

A foundational relay engine does not need type erasure. Requiring `BoxStream` forces unrelated consumers to allocate/box merely to call the primitive and makes the primitive depend on Eggress core types.

The new crate should therefore accept generic duplex streams (`AsyncRead + AsyncWrite + Unpin`) while the existing core facade continues accepting `BoxStream`.

### 5. The current implementation spawns two Tokio tasks primarily to obtain independent cancellation

The relay currently uses `JoinSet`, `AbortHandle`, `Arc<AtomicU64>`, and two spawned copy tasks.

For the extracted generic engine, task spawning is not inherently required. Two direction futures can be polled in the caller's task, with the unfinished direction dropped when an error or configured drain deadline occurs. An inline implementation has several advantages:

- generic streams do not need `'static` solely because the relay spawns them;
- the low-level API does not need a `Send` bound solely for task spawning;
- no child task can outlive the relay future;
- there is no task-abort grace period to expose as public policy;
- cancellation of the outer future naturally cancels both directions;
- the new crate can remain smaller and easier to reason about.

This is the preferred implementation shape unless focused testing uncovers a Tokio I/O cancellation issue that materially requires spawned tasks. If implementation retains spawned tasks, the reason must be documented in `architecture/relay.md` and the API must still avoid forcing Eggress-specific `BoxStream` on direct consumers.

### 6. The existing benchmark does not benchmark Eggress relay

`benches/tcp_relay.rs` currently creates an echo listener, connects the benchmark client directly to that listener, writes a payload, and reads it back. It never creates a proxy-side connection pair and never calls `eggress_core::relay::relay()`.

The benchmark therefore measures a local TCP echo round trip, not Eggress relay throughput. It cannot be used as evidence that relay extraction preserved performance.

The benchmark needs to be corrected as part of this work.

### 7. Relay documentation has already drifted from source

`architecture/core.md` currently states both that relay waits for both directions and, in reviewer notes, that `BothClosed` is reported when one side closes first and the other follows. The implementation actually applies the one-second drain cutoff, and `termination_reason()` preserves `ClientClosed`/`ServerClosed` when one direction completes first.

`crates/eggress-core/README.md` also shows an example importing `Relay` and `RelayDirection`, but those public items do not exist in the current core API.

The extraction is an appropriate point to correct these docs; do not carry inaccurate compatibility statements into the new crate.

### 8. A new public crate has release-order consequences

The workspace currently has 26 publishable `eggress-*` crates and `scripts/publish-remaining.sh` hard-codes both that count and dependency tiers.

Adding `eggress-relay` makes it a leaf library that must be published before `eggress-core` once core depends on it. Exact internal version pins and the manual dependency-first crates.io policy remain authoritative.

The feature implementation should not opportunistically perform a release/version bump. Normal release preparation handles the lockstep version bump. The new crate must use `version.workspace = true` and participate in the next release's publication order.

## Scope boundaries

### In scope

- create public `crates/eggress-relay`;
- move/reimplement the generic byte-copy engine there;
- provide an explicit half-close drain policy;
- provide configurable bounded buffer sizing suitable for different memory/throughput tradeoffs;
- preserve directional byte counts;
- preserve underlying directional I/O errors in the new low-level API;
- avoid spawned child tasks in the new engine unless testing proves a concrete need;
- retain `eggress_core::relay::relay(BoxStream, BoxStream) -> RelayResult` and all current public core relay types/paths;
- retain Eggress's existing one-second bounded-drain behavior through that legacy/core facade;
- keep `eggress-server` behavior and session outcome mapping unchanged;
- move focused relay tests into the new crate and retain compatibility tests in core/server;
- fix `benches/tcp_relay.rs` so it actually exercises the relay engine;
- add/update architecture and crate documentation;
- update workspace/release publication bookkeeping for the added crate.

### Out of scope

- changing Eggress's global `BoxStream` architecture;
- changing pproxy compatibility claims or manifests;
- changing listener, route, upstream, health, or runtime semantics;
- changing the server's one-second relay drain behavior as part of extraction;
- adding a Synvoid feature or adapter;
- moving `OutboundConnector` out of `eggress-embed`;
- extracting `DirectConnector`;
- replacing Eggress's HTTP forward-proxy request pump;
- changing UDP relay behavior;
- adding new CI workflows;
- adding OpenSSL/C dependencies/build scripts;
- introducing a generic middleware/callback/filter framework in the relay crate;
- adding application-level WAF inspection hooks to the relay primitive;
- adding telemetry/metrics ownership to `eggress-relay`.

A future listener-free outbound-connector extraction may be useful to other projects, but it is not required for the initial raw-stream reuse case and should not be bundled into this work.

## Target crate design

Add:

```text
crates/eggress-relay/
  Cargo.toml
  README.md
  src/lib.rs
```

Preferred dependency surface:

```toml
[dependencies]
tokio = { workspace = true }
```

If the implementation uses only Tokio and `std`, do not add `thiserror` merely for consistency; a structured error containing `std::io::Error` can implement `Display`/`Error` directly or use `thiserror` only if it materially simplifies the public type. Do not depend on any other `eggress-*` crate.

The crate must remain `unsafe_code = "deny"` through workspace lints.

### Generic stream boundary

The engine should work with ordinary Tokio-compatible duplex streams:

```rust
C: AsyncRead + AsyncWrite + Unpin
S: AsyncRead + AsyncWrite + Unpin
```

Do not require `Box<dyn ...>`.

If the implementation is inline rather than spawning child tasks, do not add `Send + 'static` bounds that are unnecessary for the actual algorithm.

This permits direct use with:

- `TcpStream`;
- `UnixStream`;
- TLS streams;
- joined read/write halves;
- in-memory duplex streams;
- application-specific wrappers implementing the Tokio traits.

Inside Eggress, protocol/transport boundaries still use `BoxStream`; the compatibility facade simply passes those boxes to the generic engine.

## Proposed public API contract

Exact names can be adjusted during implementation if Rust ergonomics demand it, but the semantic contract below is required.

### Relay options

Provide an explicit options type with at least:

```rust
pub struct RelayOptions {
    pub buffer_size: NonZeroUsize,
    pub half_close: HalfClosePolicy,
}
```

Recommended policy type:

```rust
pub enum HalfClosePolicy {
    /// After one direction reaches EOF, allow the opposite direction to finish
    /// without an additional relay-level deadline.
    Drain,

    /// After one direction reaches EOF, allow the opposite direction to finish
    /// for at most this duration, then stop polling/drop that direction.
    DrainFor(Duration),
}
```

Do not use an ambiguous boolean such as `graceful_half_close`.

`buffer_size` must be non-zero by construction. Prefer `NonZeroUsize` or an equivalent validated constructor rather than accepting zero and repairing it silently.

The generic crate default should be protocol-neutral and safe for arbitrary streams. Preferred default:

- buffer size: 64 KiB initially, matching current Eggress copy behavior;
- half close: `HalfClosePolicy::Drain`.

The Eggress compatibility wrapper must **not** use the generic default for half-close semantics. It must explicitly request `DrainFor(Duration::from_secs(1))` to retain current Eggress behavior.

Making the generic default unbounded avoids surprising truncation for arbitrary request/response protocols while still allowing bounded-drain consumers to opt in deliberately.

### Rich termination report

The new crate should distinguish normal completion from a configured drain cutoff. A recommended shape is:

```rust
pub enum RelaySide {
    Client,
    Server,
}

pub enum RelayTermination {
    ClientClosed,
    ServerClosed,
    DrainTimedOut { first_closed: RelaySide },
}

pub struct RelayReport {
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
    pub termination: RelayTermination,
}
```

`ClientClosed` means the client-to-server read direction reached EOF first and the opposite direction subsequently completed cleanly. `ServerClosed` is the reverse.

Do not claim a `BothClosed` distinction unless the implementation can define it deterministically and usefully. The legacy core type may retain its existing `BothClosed` variant for API compatibility even if the new rich API does not need it.

A drain deadline expiration must be visible in the new API rather than silently reported as an ordinary close. The existing core wrapper may intentionally map it back to the legacy first-closed reason to preserve current Eggress behavior.

### Directional failures

Expose the actual I/O error and transfer counts. Recommended shape:

```rust
pub enum RelayDirection {
    Upstream,   // client -> server
    Downstream, // server -> client
}

pub struct RelayFailure {
    pub direction: RelayDirection,
    pub source: std::io::Error,
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
}
```

The generic API should then be conceptually:

```rust
pub async fn relay<C, S>(client: C, server: S) -> Result<RelayReport, RelayFailure>
where
    C: AsyncRead + AsyncWrite + Unpin,
    S: AsyncRead + AsyncWrite + Unpin;

pub async fn relay_with_options<C, S>(
    client: C,
    server: S,
    options: RelayOptions,
) -> Result<RelayReport, RelayFailure>
where
    C: AsyncRead + AsyncWrite + Unpin,
    S: AsyncRead + AsyncWrite + Unpin;
```

If implementation uses different names, preserve these properties:

1. direct consumers can identify the failing direction;
2. direct consumers receive the underlying `io::Error`;
3. byte counts survive failures;
4. drain timeout is distinguishable from clean completion;
5. generic consumers can choose bounded or unbounded post-half-close draining.

Do not expose Eggress's internal `TerminationReason` as the rich new contract if doing so would force the new crate to preserve legacy ambiguities forever.

## Preferred relay algorithm

Prefer a single-task state machine rather than spawning two child tasks.

Conceptually:

1. split `client` and `server` into independent read/write halves;
2. create one copy future for client -> server and one for server -> client;
3. poll both concurrently with `tokio::select!`;
4. each direction:
   - reads into its own bounded buffer;
   - increments its byte counter after successful writes;
   - on EOF, calls `shutdown()` on the opposite writer;
   - treats `BrokenPipe` / `ConnectionReset` from shutdown with the same compatibility tolerance as the current implementation;
5. if the first completed direction returns an I/O error:
   - stop polling/drop the other direction;
   - return `RelayFailure` containing direction, source, and both counters;
6. if the first direction completes cleanly:
   - `Drain`: await the other direction normally;
   - `DrainFor(d)`: await it with `tokio::time::timeout(d, ...)`;
7. if the second direction fails while draining, return the directional failure;
8. if bounded drain expires, drop the unfinished future and return `RelayTermination::DrainTimedOut` with current byte counts;
9. if the outer relay future is cancelled/dropped, both direction futures and owned stream halves are dropped with it; no detached tasks remain.

The implementation may retain `Arc<AtomicU64>` counters because they make partial counts observable when a drain future is dropped at timeout. If a simpler state representation preserves those partial counts without atomics, prefer the simpler representation.

### Buffer allocation

Avoid embedding two fixed `[u8; 65536]` arrays directly in a large parent async future if doing so materially inflates per-connection future allocation. Prefer heap-backed bounded buffers (`Vec<u8>` / boxed slice) when that keeps the relay future small and makes `buffer_size` configurable.

Do not introduce a global buffer pool in `eggress-relay`; that is application policy and would immediately enlarge the crate's ownership surface.

The buffer size is a throughput/memory tuning control, not a framing boundary. Reads/writes must remain semantically transparent at any valid non-zero buffer size.

## Eggress compatibility facade

Retain `crates/eggress-core/src/relay.rs` as a small compatibility adapter instead of deleting the module.

The following existing path and signature must remain source-compatible:

```rust
eggress_core::relay::relay(
    client: eggress_core::BoxStream,
    server: eggress_core::BoxStream,
) -> eggress_core::relay::RelayResult
```

Existing public types must remain:

```rust
eggress_core::relay::TerminationReason
eggress_core::relay::RelayResult
```

Do not change their fields/variants in this extraction.

The wrapper should call `eggress-relay` with explicit legacy options:

- buffer size = 64 KiB;
- `HalfClosePolicy::DrainFor(Duration::from_secs(1))`.

Mapping contract:

- rich `ClientClosed` -> legacy `ClientClosed`;
- rich `ServerClosed` -> legacy `ServerClosed`;
- rich drain timeout after client first-close -> legacy `ClientClosed`;
- rich drain timeout after server first-close -> legacy `ServerClosed`;
- any rich `RelayFailure` -> debug-log the source/direction and return legacy `TerminationReason::Error` with the captured byte counts.

This intentionally preserves current observable server behavior while allowing direct `eggress-relay` consumers to obtain richer semantics.

Keep the current root-level `eggress_core::RelayError` type unchanged during this work even if it is not used by the new engine. Removing or repurposing a public type is a separate compatibility decision.

## Eggress-server integration

`crates/eggress-server/src/execute/mod.rs` should continue to call the core compatibility surface initially:

```rust
let result = eggress_core::relay::relay(pending.client, opened.stream).await;
```

Do not migrate the server directly to the rich `eggress-relay` API in the extraction commit unless there is a concrete need. Keeping the server on the compatibility facade proves that existing application behavior survives the refactor.

Existing behavior to preserve:

- same deferred success-reply ordering;
- same byte fields in `SessionReport`;
- same mapping of legacy `TerminationReason::Error` to `SessionOutcome::RelayFailed` / `FailureCategory::Relay`;
- same normal close outcome (`Completed`);
- same one-second post-half-close bounded drain policy;
- same metrics lifecycle and active upstream lease lifetime.

A later Eggress-specific behavioral improvement may adopt the rich failure categories or a different drain policy, but it must not be hidden inside this extraction.

## Work phases

### Phase 1 — Lock current compatibility behavior with focused tests

Before moving code, add/adjust tests that capture the public behavior that must remain through `eggress-core`.

Required compatibility cases:

1. bidirectional echo transfers correct bytes;
2. client half-close propagates shutdown and returns client-first termination;
3. server half-close propagates shutdown and returns server-first termination;
4. a peer that never completes after the opposite half-close does not hang the legacy Eggress relay indefinitely;
5. an I/O error maps to legacy `TerminationReason::Error`;
6. byte counts remain populated on failure/timeout where bytes were transferred before termination;
7. existing `eggress_core::relay::{relay, RelayResult, TerminationReason}` imports continue to compile.

Do not encode inaccurate `BothClosed` semantics merely because architecture documentation currently says so. Source behavior is authoritative for this extraction; fix the documentation.

### Phase 2 — Add the leaf `eggress-relay` crate

Create `crates/eggress-relay` with:

- workspace package metadata;
- README documenting intended direct-consumer use;
- generic relay options/report/failure API;
- no internal Eggress dependencies;
- focused unit/integration tests.

Required new-engine tests:

#### Clean bidirectional transfer

- TCP or `tokio::io::duplex` round-trip;
- exact upstream/downstream counts;
- both client-first and server-first EOF paths.

#### Long response after client half-close

Construct a server that:

1. reads the complete request;
2. observes client EOF;
3. waits longer than one second;
4. sends a response;
5. closes cleanly.

With `HalfClosePolicy::Drain`, the response must be delivered completely. This is the key regression test proving the generic API is safe for protocols where request-side EOF precedes a slow response.

Do not sleep for multi-second wall-clock time in ordinary tests if Tokio paused time or a shorter synthetic duration can prove the same state transition deterministically.

#### Bounded drain timeout

With `DrainFor(short_duration)`, a peer that never closes must return `DrainTimedOut` rather than hang. Verify the first-closed side and byte counts.

#### Directional I/O failure

Use a deterministic test stream or injected AsyncRead/AsyncWrite implementation that fails in one selected direction. Verify:

- `RelayFailure.direction`;
- underlying `io::ErrorKind`;
- bytes copied before failure;
- no detached work remains.

#### Small buffer correctness

Run with a deliberately tiny non-zero buffer and a payload spanning many reads. Content and byte counts must remain exact.

#### Cancellation safety

Cancel/drop the outer relay future while both directions are pending. The test should demonstrate that resources are released and no spawned relay task continues independently. If the engine contains no `tokio::spawn`, this can be a structural invariant plus a bounded resource-drop test rather than elaborate task instrumentation.

### Phase 3 — Convert `eggress-core::relay` into a compatibility facade

Add `eggress-relay` as an exact-version workspace dependency of `eggress-core`.

Replace the current engine implementation in `crates/eggress-core/src/relay.rs` with:

- legacy public type definitions;
- explicit legacy relay options;
- call into `eggress_relay::relay_with_options`;
- rich-to-legacy result mapping;
- existing debug logging for failures;
- compatibility tests.

Do not re-export the entire rich relay API from `eggress-core`. The point of the new crate is to provide the low-level public surface directly without expanding core's permanent API further.

`eggress-server` should need no behavior change.

### Phase 4 — Correct the relay benchmark

Rewrite `benches/tcp_relay.rs` so traffic actually traverses a relay instance.

The benchmark topology should be:

```text
benchmark client
      |
      v
proxy listener ---- eggress-relay ---- upstream echo server
      |
      +---- measured end-to-end payload round trip
```

At minimum retain payload cases comparable to the existing names (1 KiB and 64 KiB).

Recommended additional comparison:

- generic `eggress-relay` path;
- direct `tokio::io::copy_bidirectional` proxy baseline with equivalent topology.

The benchmark is diagnostic, not a CI gate. Because the previous benchmark did not exercise relay code, do not manufacture a strict percentage regression threshold against old numbers. Establish a valid new baseline and document what is being measured.

If buffer-size tuning materially affects results, add clearly named benchmark cases rather than silently changing the generic default based on one machine.

### Phase 5 — Workspace, architecture, and release integration

Update root `Cargo.toml`:

- add `crates/eggress-relay` to workspace members;
- add an exact-version/path `eggress-relay` workspace dependency consistent with other internal crates;
- add the dependency to `eggress-core`;
- add `eggress-relay` to the benchmark root dependencies if the corrected benchmark calls it directly.

Update release tooling:

- `scripts/publish-remaining.sh`: 26 -> 27 crates;
- publish `eggress-relay` in a tier before `eggress-core`;
- update comments describing tier ownership/counts;
- retain verification (`cargo publish`, no `--no-verify`) and manual crates.io policy unchanged.

Do not add tag-triggered crate publishing.

Update repository guidance/docs:

- `AGENTS.md`: crate count/layout and the correct architecture deep-dive pointer;
- `architecture/overview.md`: add `eggress-relay` as a foundation leaf and state that `eggress-core` exposes the compatibility facade;
- add `architecture/relay.md`: public API, half-close semantics, cancellation, error model, dependency boundary, legacy mapping, tests/reviewer gotchas;
- `architecture/core.md`: remove ownership of the engine, document compatibility facade, correct termination semantics and test map;
- `architecture/server.md`: state that server still uses the core compatibility facade and legacy bounded-drain behavior;
- `architecture/testing-and-tooling.md`: correct the `tcp_relay` benchmark description;
- `crates/eggress-core/README.md`: remove the nonexistent `Relay`/`RelayDirection` example and document the real facade/API;
- `crates/eggress-relay/README.md`: direct low-level usage and explicit distinction from `eggress-embed`;
- root `README.md`: optional short library note for users who only need raw stream relaying, without displacing `eggress-embed` as the recommended way to embed a full proxy service.

Search the repository for hard-coded crate counts and statements that `eggress-core` owns the relay engine; update only authoritative/current documentation, not historical completed plans unless they would otherwise be interpreted as live policy.

## Verification strategy

### Focused development checks

During implementation:

```bash
cargo test -p eggress-relay
cargo test -p eggress-core relay
cargo test -p eggress-server
```

Also run any existing server session/metrics tests that specifically exercise relay completion/failure outcomes.

### Compile/API checks

Verify a minimal consumer can depend only on `eggress-relay` and compile a Tokio `TcpStream` relay without importing `eggress-core`, `eggress-uri`, config, runtime, TLS, or protocol crates.

A small doctest/example is sufficient; do not add a permanent heavyweight fixture workspace unless needed.

Verify existing code using:

```rust
use eggress_core::relay::{relay, RelayResult, TerminationReason};
```

continues to compile unchanged.

### Full workspace gate

Before merge:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Because this changes the dependency/package graph, also run:

```bash
cargo deny check
```

Run `cargo audit` during release prep per existing repository policy; it is not necessary to invent a new CI gate for this feature.

### Package verification

The new crate must pass:

```bash
cargo publish --dry-run -p eggress-relay
cargo publish --dry-run -p eggress-core
```

on the release candidate/version where it will first publish. The second dry run proves the core package resolves the new crates.io dependency correctly rather than accidentally relying on a workspace-only path.

Do not use `--no-verify`.

### Performance sanity

After correcting `benches/tcp_relay.rs`, run the focused relay benchmark locally:

```bash
cargo bench --bench tcp_relay
```

Do not block merge on absolute throughput numbers from a single host. Investigate only obvious order-of-magnitude regressions or pathological buffer/memory behavior.

## Acceptance criteria

This plan is complete when all of the following are true:

1. A public `eggress-relay` crate exists and has no dependencies on other Eggress crates.
2. A Rust application can relay two Tokio-compatible duplex streams using only `eggress-relay` plus Tokio.
3. The generic relay API does not require `BoxStream`, `'static`, or `Send` unless the implementation has a documented technical need for those bounds.
4. The generic API exposes an explicit post-half-close policy with both unbounded drain and bounded-drain modes.
5. The generic default does not silently impose Eggress's current one-second response cutoff on arbitrary protocols.
6. Buffer size is bounded/non-zero and configurable without introducing application-specific buffer-pool policy.
7. Clean completion reports directional byte counts.
8. Drain timeout is distinguishable from clean completion in the new rich API.
9. I/O failure preserves the failing direction, underlying `io::Error`, and transferred byte counts.
10. Cancelling/dropping the relay future does not leave detached direction tasks running.
11. `eggress_core::relay::relay(BoxStream, BoxStream) -> RelayResult` remains source-compatible.
12. Existing `eggress_core::relay::RelayResult` fields and `TerminationReason` variants remain unchanged.
13. The core compatibility wrapper explicitly preserves the current 64 KiB buffer and one-second bounded-drain behavior.
14. Rich failures continue to map to legacy `TerminationReason::Error` for existing Eggress server consumers.
15. `eggress-server` session outcomes, failure-category mapping, byte accounting, success-reply ordering, and metrics lifecycle remain unchanged.
16. A delayed response that arrives more than one second after client write-half close completes successfully when the generic engine uses `HalfClosePolicy::Drain`.
17. A never-closing peer terminates predictably under bounded drain.
18. The corrected `tcp_relay` Criterion benchmark actually calls the relay engine and routes traffic through a proxy hop.
19. `eggress-core/README.md` no longer documents nonexistent relay types.
20. Architecture docs accurately describe current first-close, drain-timeout, and compatibility behavior.
21. Workspace crate count and publish tiers include `eggress-relay` before `eggress-core`.
22. `cargo publish --dry-run -p eggress-relay` and `cargo publish --dry-run -p eggress-core` succeed on the release candidate.
23. Workspace fmt/clippy/test gates pass.
24. No pproxy compatibility manifest/matrix claim changes are required by this extraction.
25. No Synvoid-specific API or behavior exists anywhere in the new crate.

## Handoff file map

Expected implementation touch set:

```text
Cargo.toml
Cargo.lock
AGENTS.md
scripts/publish-remaining.sh

crates/eggress-relay/
  Cargo.toml
  README.md
  src/lib.rs

crates/eggress-core/
  Cargo.toml
  README.md
  src/relay.rs

benches/tcp_relay.rs

architecture/overview.md
architecture/relay.md                  # new
architecture/core.md
architecture/server.md
architecture/testing-and-tooling.md

README.md                              # small low-level-library note if appropriate
```

`crates/eggress-server/src/execute/mod.rs` should normally remain behaviorally unchanged. Touch it only if import paths need a mechanical adjustment or a focused compatibility test belongs there.

Do not modify unrelated protocol/config/runtime crates merely because they depend transitively on `eggress-core`.

## Suggested commit structure

Prefer reviewable commits rather than one broad mechanical change:

1. `test(relay): lock legacy half-close and failure behavior`
2. `feat(relay): add generic eggress-relay crate`
3. `refactor(core): delegate legacy relay facade to eggress-relay`
4. `bench(relay): measure actual relay data path`
5. `docs(relay): document extracted relay boundary and release integration`

The workspace/release bookkeeping may land with commit 2 or 5 depending on what keeps intermediate commits buildable. Every pushed commit intended for review should compile; do not leave a temporary state where `eggress-core` references an unpublished/non-workspace crate path incorrectly.

## Non-goals / anti-scope-creep guardrails

Do not turn this into a generalized networking framework extraction. In particular, do not move `DirectConnector`, `TargetAddr`, chain execution, routing, protocol detection, listener limits, TLS wrappers, metrics, UDP, or configuration into the new crate.

Do not add hook traits for packet inspection, WAF scanning, rate limiting, metering, logging, or application callbacks. Consumers that need inspection can wrap their streams or implement that policy above the relay layer.

Do not change Eggress's server drain policy merely because the new engine makes alternatives possible. Exposing a better primitive and changing Eggress product behavior are separate decisions and should remain separately reviewable.

Do not extract `OutboundConnector` in this plan. If a future external consumer needs Eggress proxy-chain egress without the current `eggress-embed` dependency surface, research that as a separate plan after this relay boundary lands and its real dependency/footprint effect can be measured.