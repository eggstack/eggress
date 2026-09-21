# API Boundary Phase 3 — Async Outbound I/O

## Status

**IMPLEMENTED — 2026-09-21**

Implementation landed in `c15c60b189d0588d254011e877b78b2ca8a6b3a9`. Residual post-implementation findings were closed by [`API_BOUNDARY_CORRECTIVE_CLOSURE.md`](API_BOUNDARY_CORRECTIVE_CLOSURE.md).

## Parent

[`API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md)

## Baseline

`2c9d064794a2b74831979f7f36fb25e4f5992707`

## Objective

Ensure `AsyncOutboundStream` does not perform transport-blocking write I/O on the Python asyncio event-loop thread while preserving the current public API, stream ordering, arbitrary `BoxStream` transport support, and deterministic close behavior.

## Current problem

`AsyncOutboundStream.write(data) -> int` delegates synchronously to `PyOutboundStream.write()`.

The native method releases the GIL but calls the shared Tokio runtime with `block_on(stream.write(data))`. Under transport backpressure the asyncio event-loop thread can therefore block even though other Python threads may continue.

`drain()` being asynchronous does not repair time already spent blocked inside `write()`.

## Governing constraints

1. `AsyncOutboundStream.write(data) -> int` remains synchronous.
2. `read()`, `readexactly()`, `drain()`, `write_eof()`, `wait_closed()`, and context-manager shapes remain unchanged.
3. Do not require a file descriptor or `socket.socket`; TLS, WebSocket, H2, SSH, QUIC/H3, and multi-hop streams must remain representable through `BoxStream`.
4. Preserve write ordering.
5. Do not add a thread per stream.
6. Do not expose a new public buffer-size/high-water configuration knob.
7. Do not silently drop write failures. A failure must become observable no later than the next operation that can report it, especially `drain()`.
8. Explicit `close()` remains non-blocking from Python's perspective.
9. Do not weaken cancellation or loop-affinity guarantees established by Phase 2.

## Workstream 1 — Add a deterministic event-loop stall reggression

Build a local fixture that creates real backpressure:

- peer accepts but stops reading or reads slowly;
- write payload is large enough to exceed immediate transport buffering;
- a heartbeat coroutine runs concurrently.

Prove the current path can delay the heartbeat when `AsyncOutboundStream.write()` reaches native blocking I/O. The final test should assert the corrected semantic without using a fragile wall-clock microbenchmark.

Prefer synchronization primitives/events that show the loop remains schedulable while writes are pending.

## Workstream 2 — Implement an ordered internal write pump

Introduce a private implementation that decouples synchronous `write()` submission from transport I/O.

Acceptable designs include a private queue/actor or another serialized mechanism backed by the existing shared Tokio/Python async infrastructure. The design must satisfy:

```text
write(A)
write(B)
await drain()
```

means the transport observes A before B and `drain()` does not complete until all writes submitted before that drain point have either completed or failed.

The write pump must not require a new public Rust or Python method name. If a PyO3-internal helper is unavoidable, keep it private/underscored and do not document it as supported API; prefer implementing through existing private binding internals.

Do not use one dedicated OS thread per `AsyncOutboundStream`.

## Workstream 3 — Define failure and drain semantics

Maintain a single pending terminal write error.

Required behavior:

- `write()` may synchronously reject local misuse such as already-closed stream or invalid input;
- transport failures occurring after enqueue are retained;
- the next `drain()` raises the retained failure;
- subsequent operations fail consistently rather than continuing on a poisoned writer;
- no error is reported twice as two unrelated causes;
- messages remain bounded/redacted.

If the existing native stream closes as part of a fatal write error, reflect that state through `closed`/`is_closing` consistently.

## Workstream 4 — Preserve close and half-close ordering

Specify and test:

- `close()`: remains non-blocking and idempotent; it may cancel/discard queued writes consistent with the current immediate-close behavior, but must not leak worker tasks;
- `wait_closed()`: waits for write-pump termination/cleanup, not merely a Python flag;
- `write_eof()`: must execute after writes submitted before it, then prevent later writes as appropriate for the underlying stream;
- destructor cleanup must never block interpreter finalization;
- cancellation of one `drain()` waiter must not corrupt the underlying ordered queue.

Do not introduce implicit retry.

## Workstream 5 — Keep sync OutboundStream unchanged

`OutboundStream` is intentionally blocking. Do not route its `write()`, `sendall()`, or reads through the async pump.

The correction belongs only to `AsyncOutboundStream` and any private native machinery required to support it.

## Workstream 6 — Resource and concurrency tests

Cover:

- many small ordered writes;
- concurrent producer tasks;
- large backpressured write;
- write followed by read;
- drain failure;
- cancellation during drain;
- close with queued writes;
- write-after-close;
- write_eof ordering;
- repeated create/connect/close cycles;
- no thread-count growth proportional to stream count;
- no hidden listener creation.

Where practical, retain the existing loop-affinity tests and assert the write path obeys them.

## Stop conditions

Stop and write a narrow follow-up design rather than changing public semantics if the only feasible correction would require any of:

1. making `write()` awaitable;
2. exposing the underlying socket/file descriptor;
3. adding a dedicated OS thread per stream;
4. changing `BoxStream` or public Rust stream types;
5. introducing a new public buffering configuration API.

If stopped, keep the existing API and document the limitation; do not disguise blocking behavior as non-blocking.

## Verification

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)

.venv/bin/python -m pytest   python/tests/test_outbound_stream_verification.py   python/tests/test_asyncio_semantic.py   python/tests/test_performance_smoke.py -q

.venv/bin/python -m pytest python/tests tests/compat -q

cargo test -p eggress-python
cargo test -p eggress-outbound
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Acceptance criteria

- [x] `AsyncOutboundStream.write()` retains its synchronous `int` return API.
- [x] The event-loop thread remains schedulable while transport write I/O is backpressured.
- [x] Write submission order is preserved.
- [x] `drain()` waits for all prior writes and reports retained transport failures.
- [x] Close, wait_closed, cancellation, write_eof, and destructor behavior are deterministic and leak-free.
- [x] No thread is created per outbound stream.
- [x] Synchronous `OutboundStream` behavior is unchanged.
- [x] Arbitrary boxed transports remain supported.
- [x] Full Python and workspace gates are green.

## Closure evidence (2026-09-21 polish)

- Sync API preserved: `python/eggress/outbound.py::AsyncOutboundStream.write(data) -> int` remains synchronous; native `PyOutboundStream.write()` keeps blocking completion via private `WritePump::submit_and_wait()` (`crates/eggress-python/src/outbound.rs`), which is `submit()` + ordered `barrier()`. High-level `OutboundStream.write()` unchanged via `write_blocking()`.
- Authoritative gated proof (replaces the old loopback peer-receipt claim): `crates/eggress-python/src/outbound.rs::outbound::tests::native_sync_write_waits_for_transport_completion` holds a gate-controlled `BoxStream` closed, proves the sync result is withheld until `poll_write` completion, then opens the gate and proves byte-count success; `outbound::tests::async_submit_returns_before_transport_completion` proves queue-only `submit()` returns before completion while `barrier()` stays pending. No wall-clock/kernel-buffer assumption is the primary proof; short timeouts are harness safety bounds only after the `write_polled` sync point.
- Python smoke (non-authoritative): `test_api_boundary_closure.py::TestNativeWriteContract::test_native_write_round_trip_without_explicit_drain` and `test_sync_wrapper_write_echoes_without_explicit_drain` are worded as round-trip smoke only.
- Schedulability/ordering/failure: `test_async_write_keeps_loop_schedulable`, `test_async_ordered_writes_plus_drain`, `test_async_write_uses_private_submit_and_drain_completes`, `test_async_drain_surfaces_terminal_failure`, `test_write_eof_ordered_after_async_submissions`, `test_close_wait_closed_leaves_no_pump_running` in `test_api_boundary_closure.py`; `test_async_writes_are_ordered`, `test_async_write_keeps_event_loop_schedulable_under_backpressure`, `test_async_write_eof_follows_prior_writes` in `test_outbound_stream_verification.py`; cancellation/affinity in `test_asyncio_semantic.py`.
- Pump design: one ordered Tokio pump per stream, no thread per stream, arbitrary `BoxStream` transports, no fd assumption, `drain()` as ordered barrier with retained terminal failure, `close()` non-blocking/idempotent, `wait_closed()` joins pump cleanup.
- Broad gate: focused Python set (205 passed), full `python/tests tests/compat` (2308 passed), `cargo test --workspace --locked` (2947 passed), fmt/clippy green.
