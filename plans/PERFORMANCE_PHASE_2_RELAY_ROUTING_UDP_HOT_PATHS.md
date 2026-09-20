# Performance Phase 2 — Relay, Routing, and UDP Hot Paths

## Status

**IMPLEMENTED — 2026-09-20**

## Parent roadmap

[`PERFORMANCE_OPTIMIZATION_ROADMAP.md`](PERFORMANCE_OPTIMIZATION_ROADMAP.md)

## Planning baseline

`80c8f2f2464bb3c4182afee73a8fdbdc170b1cdb`

## Objective

Remove unnecessary synchronization and algorithmic work from three high-frequency data/control paths while preserving all established public contracts:

1. the generic bidirectional relay engine;
2. upstream health eligibility reads during route selection;
3. standalone UDP target-flow admission accounting.

This phase is intentionally internal. It must not add a TCP-specific public relay API, change scheduler semantics, or change UDP admission limits/timeouts.

## Workstream 1 — Replace generic split relay with a no-lock single-task engine

### Existing contract to preserve

The completed relay extraction established `eggress-relay` as a generic leaf crate with:

- generic `AsyncRead + AsyncWrite + Unpin` stream inputs;
- configurable per-direction buffer sizing;
- explicit `HalfClosePolicy::{Drain, DrainFor}`;
- directional byte accounting;
- rich directional I/O failure reporting;
- explicit drain-timeout reporting;
- cancellation by dropping the outer relay future;
- no detached child tasks;
- `eggress-core` compatibility facade preserving the historical 64 KiB/one-second server behavior.

Those are compatibility requirements for this optimization.

### Current performance issue

The generic implementation uses `tokio::io::split` for each input stream. Tokio's generic split wraps the stream in shared synchronization and locks around I/O polling. This is necessary for independently owned generic halves but unnecessary if one relay future owns and polls both full streams.

A TCP-only `TcpStream::split` fast path would avoid the lock only for one transport type and would either expand the public API or require fragile downcasting. Do not pursue that design.

### Required implementation shape

Implement the copy engine as a single future/state machine that owns/pins both streams and maintains two directional copy states.

Conceptual state:

```text
client stream
server stream

upstream state:
  read client -> buffer -> write server -> shutdown server write on client EOF

downstream state:
  read server -> buffer -> write client -> shutdown client write on server EOF
```

The future's `poll` should advance both directions fairly until one direction terminates, then honor the existing configured drain policy for the remaining direction.

Use ordinary safe Rust/Tokio traits. Workspace policy forbids `unsafe`.

### State-machine requirements

Each direction must retain:

- buffer and read/write cursor state;
- bytes successfully written;
- EOF observation;
- shutdown progress;
- terminal I/O error if one occurs.

The parent relay state must retain:

- which direction completed first;
- whether it completed cleanly or failed;
- drain deadline state for `DrainFor`;
- ability to return partial counts if the second direction fails or times out.

Do not allocate/box separate copy futures solely to satisfy `select!` if the state machine can poll the directional states directly.

### Fairness

A single poll call must not spin indefinitely draining one direction while starving the other. Bound per-poll progression naturally through underlying `Poll::Pending` boundaries or an explicit small step budget if required.

Do not invent a tunable fairness public option.

### Half-close compatibility

Preserve exactly:

- clean EOF in one direction triggers shutdown of the peer write side;
- tolerated shutdown errors remain tolerated as before;
- `Drain` permits the other direction to complete without an added relay-level deadline;
- `DrainFor(d)` stops polling the remaining direction after the configured duration and returns the existing timeout termination representation;
- error in either direction returns the existing `RelayFailure` direction/source/count data;
- dropping/cancelling the relay future drops all owned state with no detached task.

The `eggress-core` facade must continue to request the legacy 64 KiB and one-second bounded drain behavior.

### Tests

Retain all existing `eggress-relay` contract tests and add tests specifically sensitive to state-machine mistakes:

1. simultaneous bidirectional large transfer with both sides applying backpressure;
2. asymmetric transfer where one direction remains busy while the other repeatedly becomes ready;
3. client-first and server-first half-close;
4. delayed response under `Drain`;
5. timeout under `DrainFor`;
6. write failure after partial progress in either direction with exact directional counts;
7. cancellation while both directions are pending;
8. tiny buffer crossing many read/write boundaries.

A structural/source test may assert the implementation no longer calls generic `tokio::io::split` if that is maintainable; behavioral and benchmark evidence is more important than a brittle text assertion.

## Workstream 2 — Make health-state eligibility reads atomic

### Current shape

`HealthCell` owns a full `HealthSnapshot` behind an `RwLock`. `state()` takes a read lock. Routing prefilters eligible upstreams, and built-in schedulers may re-evaluate eligibility. Under high selection concurrency this puts lock traffic on a path where the only required datum is a compact state enum.

### Public-contract constraint

The `Scheduler` trait and the router's candidate semantics are public behavior. Do not optimize by handing custom schedulers unfiltered members or otherwise changing which candidates they receive.

### Required design

Add a compact atomic representation of the current health state, for example `AtomicU8`, with an internal total mapping between the public/internal health enum variants and atomic values.

The full snapshot remains under `RwLock` for:

- failure/success counts;
- timestamps/deadline/cooldown data;
- transition computation;
- diagnostics/admin snapshot reads.

Transition operations should:

1. lock/update the full snapshot;
2. determine the authoritative new state;
3. publish the atomic state with a documented memory ordering;
4. release/return as appropriate.

`state()` and simple eligibility checks should read only the atomic state.

Choose the weakest correct memory ordering. If state is only a summarized value and callers that need the associated full snapshot still take the lock, `Relaxed` may be sufficient; document the reasoning in code. Use Acquire/Release only if required by an actual cross-field visibility invariant.

### Consistency contract

A concurrent caller may observe the just-before or just-after health state during a transition, as is already possible across lock acquisition boundaries. It must never observe an invalid enum value.

Do not duplicate transition policy in both atomic and locked state. The lock-backed snapshot remains the place where transitions are computed; the atomic is the published fast-path state.

### Tests

Cover:

- exact enum<->atomic mapping;
- success/failure transition sequences;
- concurrent state reads during transitions never panic/produce invalid state;
- full snapshots agree with the published state once a transition operation completes;
- router selection behavior remains unchanged for healthy/unhealthy/recovering states;
- custom scheduler tests continue receiving the same eligible candidate set.

## Workstream 3 — Replace standalone UDP flow-count scans with exact aggregate accounting

### Current issue

Standalone UDP admission calls a helper that scans all client entries and sums/caps target flows before deciding whether another target flow may be created. This is O(number of active clients) on a per-datagram path.

The owning runtime loop already serializes the client/flow mutations needed to keep an exact total count.

### Required design

Maintain:

```rust
let mut total_target_flows: usize = ...;
```

inside the standalone UDP relay owner task.

Update it on every lifecycle path:

- increment only after a target flow is successfully inserted/accepted as live;
- decrement when target flows are reaped for idle timeout;
- decrement when an entire client association/state entry is removed;
- decrement on explicit close/cleanup paths;
- reset naturally when the owner task exits.

Refactor private reap/removal helpers to return removed-flow counts or otherwise make accounting explicit. Avoid hidden side effects spread across unrelated helpers.

Use checked/debug-asserted arithmetic where useful during development. Production logic must not panic if a defensive mismatch occurs; structure the code so mismatches are not representable under the single-owner mutation model.

### Admission semantics

Preserve exactly:

- existing global target-flow cap;
- existing per-client cap, if separate;
- existing "at cap" rejection/drop/error behavior;
- existing idle/reap timing;
- existing metrics updates;
- existing target identity/equivalence rules.

Do not replace the exact count with an eventually consistent atomic. The owner loop already permits a plain `usize`.

### Tests

Add deterministic tests for:

1. first flow increments total once;
2. repeated datagrams to an existing flow do not increment;
3. multiple clients/targets produce exact total;
4. idle reap decrements exact number removed;
5. client removal decrements all nested target flows;
6. hitting the global cap preserves current admission behavior;
7. capacity becomes available immediately after reap/removal;
8. shutdown/cleanup leaves no logically counted flows.

If the old `total_target_flows_capped` helper becomes unused, remove it rather than retaining duplicate accounting implementations.

## Workstream 4 — Optional local cleanup exposed by the changes

Only perform these if they are mechanically enabled by the work above:

- remove now-unused relay imports/types related solely to generic split/future boxing;
- remove now-unused health read-lock helper paths;
- simplify UDP reap helper return values around removal counts.

Do not use this allowance for unrelated refactors.

## Focused verification

At minimum:

```bash
cargo test -p eggress-relay
cargo test -p eggress-core
cargo test -p eggress-routing
cargo test -p eggress-udp
cargo test -p eggress-runtime
cargo test -p eggress-server
```

Run existing route, health, UDP association, standalone UDP, and relay integration tests.

Before merge:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Performance evidence

Formal before/after benchmark closure belongs to Phase 3. During this phase, however:

- verify the relay implementation no longer uses generic split/lock polling;
- run the existing `tcp_relay` benchmark as a smoke, recognizing its current setup overhead;
- run `route_match` after health-state changes to detect gross regressions;
- add a focused unit/micro benchmark only if needed to guide implementation, but register durable benchmark work in Phase 3;
- instrument standalone UDP flow-count helper in tests if useful to prove no O(n) scan remains.

Do not add production tracing/counters solely for the optimization proof.

## Stop conditions

Stop and reassess if:

- the custom relay state machine cannot preserve cancellation/half-close/error semantics cleanly without substantial complexity;
- a simpler safe Tokio primitive is found that avoids generic split synchronization while preserving the generic public API;
- health eligibility requires synchronized access to additional snapshot fields not captured by a state enum;
- custom scheduler behavior would need to change;
- UDP flow mutations occur concurrently outside the presumed single owner and therefore cannot maintain an exact plain aggregate safely.

A retained existing implementation for one blocked sub-workstream does not invalidate unrelated optimizations in the phase.

## Closure

- `eggress-relay` now polls two directional states while owning both complete
  streams, with bounded per-poll progress and the existing half-close/error/
  cancellation contract. No generic `tokio::io::split` remains in the engine.
- Health state transitions remain lock-backed and authoritative while
  eligibility reads use a total `AtomicU8` mapping with documented Relaxed
  ordering.
- Both standalone UDP owners maintain exact target-flow aggregates and update
  them on insert, reap, client removal, and shutdown; the capped scan helper
  was removed.
- Focused routing, UDP, relay, core, and runtime checks passed.

## Acceptance criteria

This phase is complete when:

1. `eggress-relay` core copying no longer depends on generic `tokio::io::split` synchronization.
2. No TCP-only public relay API or downcast path is added.
3. Relay public types/functions remain source-compatible.
4. `HalfClosePolicy::Drain` and `DrainFor` behavior remains covered and unchanged.
5. Directional byte counts/errors remain correct on clean completion, error, and timeout.
6. Cancelling the relay leaves no detached work.
7. Built-in relay/server compatibility facade still uses the historical 64 KiB buffer and one-second bounded drain.
8. Simple health `state()`/eligibility reads no longer take the full snapshot `RwLock`.
9. Full health snapshots and transition policy remain lock-backed and authoritative.
10. Public `Scheduler` trait/candidate semantics are unchanged.
11. Routing tests show the same eligible upstream choices for equivalent health inputs.
12. Standalone UDP global target-flow admission is O(1) with an exact maintained aggregate.
13. Every flow creation/removal/reap path updates the aggregate exactly once.
14. UDP limits, timing, metrics, and wire behavior are unchanged.
15. Workspace fmt/clippy/tests pass.
16. No public API, protocol capability, or pproxy compatibility regression is introduced.
