# Phase C5 — Asyncio semantic compatibility

> **Status: COMPLETED** — 67 tests passing across all 10 workstreams.
> All existing tests (1234 total) continue to pass.

## Objective

Close the remaining event-loop, awaitable, cancellation, task ownership, shutdown, and interpreter-lifecycle differences between Eggress’s Python compatibility layer and pinned pproxy. This phase is the integration and hardening gate for C2–C4: the API is not truly drop-in if equivalent methods exist but behave differently under cancellation, loop reuse, concurrent shutdown, or task failure.

## Dependencies

- C1 exact coroutine and lifecycle contract.
- C2 `Connection` implementation.
- C3 `Server` implementation.
- C4 callback/plugin bridge.
- Existing PyO3 bindings and runtime lifecycle code.

## Core invariants

1. No nested Tokio runtime is created from an active runtime path.
2. No Python-visible awaitable resolves or raises more than once.
3. Python cancellation propagates to native operations promptly.
4. Native task failure resolves the Python future with a stable exception.
5. No blocking network or shutdown wait holds the GIL.
6. Close and wait semantics are idempotent and race-safe.
7. Interpreter shutdown cannot leave non-daemon native threads or orphaned sockets.

## Workstream 1: loop-affinity model

Characterize and implement pproxy behavior for:

- object construction outside a running loop;
- first use inside a loop;
- reuse from a different loop;
- use from another thread;
- default event-loop policy differences;
- multiple loops sequentially in one process;
- multiple active loops in different threads.

Choose one explicit model per object type and test it. If Eggress must reject cross-loop use, raise a stable compatibility exception before native work begins.

## Workstream 2: native awaitable bridge

Audit all async APIs:

- connect/start/astart;
- read/write/send/receive;
- reload/status where async;
- close/aclose;
- wait_closed;
- callback completion;
- server drain.

Use a single maintained bridge pattern that:

- binds native cancellation to Python future cancellation;
- converts panic/internal failure into a safe exception;
- handles loop closure;
- does not retain the event loop or object indefinitely;
- schedules completion thread-safely;
- preserves contextvars where the reference behavior requires callback execution in the caller context.

## Workstream 3: cancellation semantics

Add explicit cancellation points and tests for:

- DNS resolution;
- TCP connect;
- TLS handshake;
- proxy protocol handshake;
- server bind/start;
- reads and writes;
- UDP receive/send waiting;
- pool acquisition;
- callback/plugin execution;
- graceful drain;
- reconnect backoff;
- `wait_closed`.

Define whether cancellation closes the object, leaves it reusable, or transitions it to failed state based on the reference contract. Prevent cancellation races from leaking tasks or returning success after cancellation.

## Workstream 4: close and shutdown ordering

Specify state-machine behavior for concurrent:

- multiple `close()` calls;
- multiple `wait_closed()` awaiters;
- `close()` during connect/start;
- cancellation of `wait_closed()`;
- server close while child connections are active;
- interpreter shutdown during active operations;
- native error concurrent with user close.

Use one terminal result and notify all waiters. Preserve graceful drain deadlines, then force cancellation. Ensure waiters observe actual resource cleanup.

## Workstream 5: callback and context behavior

For C3/C4 callbacks:

- invoke on the owning loop;
- preserve ordering rules;
- define task creation and ownership;
- capture callback exceptions;
- cancel callbacks during shutdown according to the contract;
- avoid unhandled-task warnings;
- propagate contextvars where expected;
- prevent callback reentrancy from corrupting object state.

Add overload/backpressure behavior rather than unbounded task spawning.

## Workstream 6: exception and task reporting

1. Map cancellation to `asyncio.CancelledError` where required.
2. Preserve causal exceptions with safe chaining.
3. Ensure background failures are surfaced through awaited lifecycle methods, status, or loop exception handling rather than silently logged.
4. Avoid duplicate logging when an exception is delivered to Python.
5. Test behavior with asyncio debug mode enabled.
6. Define compatibility behavior for `ExceptionGroup` on newer Python while maintaining Python 3.9 support.

## Workstream 7: interpreter and garbage-collection safety

Test and harden:

- object collection while active;
- reference cycles involving callbacks/futures;
- module teardown;
- interpreter finalization;
- fork behavior if supported;
- `atexit` interaction;
- native thread/runtime termination;
- pending task and unclosed transport warnings.

`__del__` must never block. It may request cancellation and issue a resource warning consistent with the reference behavior.

## Workstream 8: compatibility across Python versions

Run the supported Python matrix, at least the project minimum through current stable. Account for differences in:

- `CancelledError` inheritance/history;
- event-loop acquisition;
- TaskGroup/ExceptionGroup availability;
- deprecations in loop parameters;
- typing and stub syntax;
- interpreter finalization behavior.

Avoid version checks scattered across the package; centralize compatibility helpers.

## Workstream 9: stress and race testing

Add deterministic and repeated tests for:

- cancellation at each lifecycle point;
- close/connect races;
- server close with many children;
- many simultaneous waiters;
- callback exception storms;
- loop closure before native completion;
- cross-thread scheduling;
- repeated `asyncio.run()` cycles;
- pytest-asyncio strict mode;
- asyncio debug mode;
- anyio compatibility only where it naturally wraps asyncio;
- GIL contention with native I/O;
- task/socket/file-descriptor leak detection.

Use bounded stress loops in CI and larger ignored soak tests.

## Workstream 10: documentation and contract closure

Document:

- loop-affinity rules;
- cancellation effects per method;
- close/wait semantics;
- callback execution context;
- thread safety;
- interpreter shutdown expectations;
- known divergences from pproxy.

Update C1 classifications and promote Python capabilities only when contract and stress tests pass.

## Acceptance criteria

- All C1 async methods have matching coroutine classification and lifecycle behavior.
- Cancellation propagates promptly to native operations and releases resources.
- Close/wait operations are idempotent, race-safe, and observable by multiple waiters.
- No nested-runtime panic path remains.
- Asyncio debug-mode tests emit no pending-task, unhandled-exception, or unclosed-resource warnings.
- Repeated loop creation/destruction does not leak native threads, tasks, sockets, or references.
- Callback/plugin tasks are bounded and owned.
- Supported Python versions pass the same semantic suite.
- Representative pproxy async programs run unchanged with equivalent observable behavior.
- Stubs, docs, manifest, and Python compatibility report agree.

## Out of scope

- Trio-native APIs;
- generalized anyio backend support beyond asyncio compatibility;
- synchronous API redesign;
- adding missing protocols;
- performance tuning unrelated to event-loop correctness.

## Recommended commit sequence

1. loop-affinity and awaitable bridge audit;
2. cancellation propagation;
3. close/wait state machines;
4. callback/context/task ownership;
5. exception and interpreter-shutdown hardening;
6. cross-version compatibility helpers;
7. race/stress/debug-mode tests;
8. docs, stubs, and manifest promotion.