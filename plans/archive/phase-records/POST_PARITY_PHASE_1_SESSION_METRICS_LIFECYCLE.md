# Post-Parity Phase 1 — Session Metrics Lifecycle Correctness

## Status

**COMPLETE**

Parent roadmap: `POST_PARITY_CORRECTIVE_AND_REDUCTION_ROADMAP.md`

## Problem statement

At the planning baseline, `eggress-server::serve_connection()` calls
`SessionMetrics::record_session_start()` before the inbound handshake. Normal
execution later calls `record_session(&SessionReport)`, and
`MetricsRegistry::record_session()` decrements the active gauge and records
totals/failures.

Several early exits construct a `SessionReport` and return immediately instead:

- authentication failure;
- inbound protocol failure;
- handshake timeout.

Those paths do not pass through `record_session()`. For the concrete Prometheus
registry this can leave `eggress_connections_active` incremented after the socket
has ended, and it also means total/failure accounting is not uniformly applied.

The listener semaphore is separate and already lifetime-bound to the stream. This
phase must not confuse the metrics bug with listener concurrency limiting.

## Objective

Make session accounting structurally balanced: once a session has been counted as
started, exactly one terminal report is recorded before `serve_connection()`
returns.

The preferred solution is a single normal finalization path, not a new guard
framework.

## Likely files

Primary:

```text
crates/eggress-server/src/lib.rs
crates/eggress-metrics/src/lib.rs
crates/eggress-server/tests/          # only if a dedicated integration test is clearer
crates/eggress-runtime/tests/         # only if runtime-level wiring is required
```

Potentially relevant:

```text
crates/eggress-server/src/execute.rs
docs/architecture/server.md
docs/architecture/metrics.md
```

Do not change listener semaphore ownership in `eggress-core` unless a new,
independent defect is discovered.

## Implementation requirements

### 1. Create one terminal reporting path

Refactor `serve_connection()` so early handshake/authentication/timeout branches
produce a `SessionReport` but do not bypass final metrics reporting.

A simple shape is:

```rust
let report = match accepted {
    Ok(Ok(session)) => execute::execute(session, &config).await,
    Ok(Err(AcceptError::AuthenticationFailed)) => SessionReport { ... },
    Ok(Err(_)) => SessionReport { ... },
    Err(_) => SessionReport { ... },
};

if let Some(metrics) = &config.metrics {
    metrics.record_session(&report);
}

report
```

The exact code may differ, but the invariant must be visible in control flow:
after `record_session_start()`, every non-panicking return from
`serve_connection()` goes through exactly one matching finalization call.

Do not solve this by adding a second decrement call in each error branch. That
would preserve duplicated lifecycle logic and make future branches easy to miss.

### 2. Preserve specialized failure observations

Authentication failures currently call `record_auth_failure()`. Preserve that
specialized counter while also finalizing the session.

Do not double-count general connection failures. The terminal `SessionReport`
must remain the input that determines whether `connection_failures` increments.

### 3. Define metrics semantics explicitly

For one connection attempt admitted to `serve_connection()`:

```text
connections_active: +1 at start, -1 at terminal finalization
connections_total:  +1 at terminal finalization
connection_failures:+1 iff terminal outcome is a failure category
auth_failures:      +1 additionally for authentication failure
```

A handshake timeout and malformed protocol are completed connection attempts for
metrics purposes even though they never reach routing.

If existing public documentation implies different semantics, update the metrics
documentation narrowly.

### 4. Test through the trait boundary

Add a small test metrics implementation or use the concrete registry to count
calls. Cover at least:

- successful SOCKS5 or HTTP CONNECT session;
- authentication failure;
- malformed/unsupported client protocol;
- handshake timeout.

For each case, prove one start and one terminal record.

If testing concrete Prometheus text is straightforward, also assert that after
the failed session completes:

```text
eggress_connections_active == 0
eggress_connections_total == 1
eggress_connection_failures_total == 1
```

and that auth failure additionally increments the auth counter.

### 5. Protect route/relay failure behavior

Route and relay failures already flow through `execute()` and should continue to
finalize once. Add or retain a focused regression assertion so the refactor does
not introduce double-finalization for post-handshake failures.

### 6. Do not redesign the metrics API

Do not:

- add a session object hierarchy;
- add a generic RAII framework unless the single-path refactor is impossible;
- move metrics ownership into the listener;
- duplicate state between server and runtime;
- add new exported counters unrelated to this defect.

## Focused verification

Run the narrowest relevant tests during implementation, for example:

```bash
cargo test -p eggress-server serve_connection
cargo test -p eggress-server authentication
cargo test -p eggress-metrics
```

If runtime wiring changes:

```bash
cargo test -p eggress-runtime
```

Then run the standard Rust gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

No external interoperability suite is required for this phase.

## Explicit acceptance criteria

Phase 1 is complete only when:

1. `serve_connection()` has one structurally obvious terminal session-reporting
   path after `record_session_start()`.
2. Authentication failure does not return before terminal metrics finalization.
3. Protocol/handshake parse failure does not return before terminal metrics
   finalization.
4. Handshake timeout does not return before terminal metrics finalization.
5. A successful session still records exactly one terminal report.
6. Route failure and relay failure still record exactly one terminal report.
7. A test proves `connections_active` returns to its baseline after successful
   completion.
8. A test proves `connections_active` returns to its baseline after
   authentication failure.
9. A test proves `connections_active` returns to its baseline after malformed
   protocol input.
10. A test proves `connections_active` returns to its baseline after handshake
    timeout.
11. Failed handshake attempts increment `connections_total`.
12. Failed handshake attempts classified as failures increment
    `connection_failures`.
13. Authentication failure increments the specialized auth-failure metric exactly
    once.
14. No error path decrements the active gauge twice.
15. Listener connection-limit/semaphore behavior is unchanged.
16. No new metric, runtime subsystem, or public feature is added.
17. The focused crate tests pass.
18. `cargo fmt --all -- --check` passes.
19. `cargo clippy --workspace --all-targets -- -D warnings` passes.
20. `cargo test --workspace --locked` passes.

## Stop condition

If inspection shows an already-existing shared finalizer is invoked indirectly
for all early returns, first add the regression tests that demonstrate balanced
metrics. If those tests pass without code changes, close this phase as already
satisfied and do not introduce a redundant refactor.
