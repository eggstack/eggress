# Phase C3 — Python `Server` compatibility

## Objective

Implement a pproxy-compatible low-level `Server` object backed by the Eggress runtime. Existing Python applications should be able to construct listeners, start them asynchronously, inspect bound addresses, receive or route connections through the expected callback/protocol model, and close/wait without rewriting around `PPProxyService`.

## Dependencies

- C1 API contract.
- C2 native connection/state patterns.
- A2 composition matrix.
- Existing Eggress supervisor/embed lifecycle.

## Architecture constraints

- Reuse the existing supervisor and protocol implementations.
- Do not run a second listener stack in Python.
- Python callbacks must not force all relay traffic through the GIL unless the pproxy contract explicitly requires callback-level data handling.
- Listener ownership, tasks, cancellation, and shutdown must remain deterministic.

## Workstream 1: exact API shell

Implement constructor signatures, class/instance attributes, methods, aliases, coroutine status, representations, and exception behavior from C1. Preserve positional/keyword acceptance and defaults.

Define the mapping between pproxy protocol/cipher/server arguments and A2 composition cells. Invalid combinations must fail before sockets are bound.

## Workstream 2: listener lifecycle

Implement:

- creation without immediate bind if pproxy behaves that way;
- `start_server` or equivalent exact coroutine;
- one or multiple listener specifications;
- port `0` and actual bound-address reporting;
- IPv4, IPv6, Unix, TLS, and promoted protocol listeners;
- partial-start rollback;
- repeated start behavior;
- close and `wait_closed`;
- async context management where contracted;
- graceful drain and forced cancellation.

Ensure listener topology changes follow documented reload/restart rules.

## Workstream 3: connection handling model

Characterize whether pproxy `Server`:

- routes internally from URI/config;
- invokes Python callbacks with connection objects;
- exposes asyncio protocol/transport hooks;
- supports custom target resolution;
- supports UDP callbacks.

Implement the exact supported model. Where callbacks are required:

- create bounded callback queues;
- define ordering and concurrency;
- convert callback exceptions into deterministic connection closure and server diagnostics;
- allow async callbacks if contracted;
- avoid holding Rust locks while invoking Python;
- release the GIL during native work;
- provide backpressure and overload behavior.

## Workstream 4: integration with `Connection`

Use C2 objects for accepted or outbound connection handles where the API requires them. Maintain clear ownership:

- server owns accepted native session until transferred;
- Python handle closure propagates once;
- server shutdown cancels or drains child sessions according to pproxy behavior;
- child exceptions do not corrupt listener state.

## Workstream 5: TCP and UDP roles

Support all A2 cells exposed by pproxy's `Server` API:

- HTTP/SOCKS and promoted transports;
- authentication;
- TLS settings;
- routing/chains;
- direct and supported UDP listener behavior;
- Unix listeners;
- platform-specific restrictions.

Unsupported cells must produce typed compatibility errors rather than silently falling back.

## Workstream 6: observability and state

Expose contracted state and safe Eggress extensions:

- started/closing/closed;
- bound addresses;
- listener names/protocols;
- active and total sessions;
- bytes and failure groups;
- last error;
- reload/status where applicable.

Do not expose mutable internal Rust objects or secrets.

## Workstream 7: loop and thread behavior

Define and test:

- construction outside a loop;
- start in a running loop;
- loop affinity;
- calls from other threads;
- synchronous close requests if contracted;
- interpreter shutdown;
- multiple independent servers in one process;
- coexistence with `PPProxyService` and C2 connections.

Never create a nested Tokio runtime from an active runtime path.

## Workstream 8: exceptions

Map configuration, bind, TLS, auth, callback, route, protocol, loop, cancellation, and shutdown errors into the C1 hierarchy. Preserve the original causal error as a safe attribute or chained exception.

## Workstream 9: tests

Contract tests:

- signatures/defaults;
- coroutine status;
- attributes and aliases;
- exception inheritance;
- repr/redaction.

Behavioral tests:

- start and bind on ephemeral port;
- multiple listeners;
- partial bind failure rollback;
- TCP echo/proxy sessions;
- authentication;
- TLS;
- chains;
- IPv6 and Unix platform gates;
- supported UDP;
- callback success, failure, cancellation, overload, and ordering;
- concurrent sessions;
- server close with idle/active sessions;
- repeated close/wait;
- garbage collection and interpreter shutdown;
- loop mismatch and cross-thread calls;
- GIL release;
- file-descriptor/task leak loops.

Run representative original pproxy server examples against both implementations.

## Acceptance criteria

- C1 `Server` contract matches for supported surfaces.
- Existing representative pproxy server programs run unchanged.
- Listener startup, bound addresses, child sessions, close, and wait semantics match the oracle.
- Callback paths are bounded, exception-safe, and do not deadlock the GIL/runtime.
- Partial startup leaves no bound sockets or tasks.
- Repeated lifecycle tests produce no pending tasks, resource warnings, or descriptor growth.
- A2, stubs, runtime, docs, and manifest agree.

## Out of scope

- arbitrary plugin/cipher object compatibility beyond adapters required to start servers, handled in C4;
- broad asyncio policy work, handled in C5;
- transports not yet available in runtime;
- redesign of the native supervisor.

## Recommended commit sequence

1. API shell and state model;
2. listener lifecycle;
3. connection/callback integration;
4. TCP/UDP and protocol adapters;
5. exceptions and observability;
6. loop/thread/resource hardening;
7. differential and contract tests;
8. stubs/docs/manifest promotion.