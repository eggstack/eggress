# Phase C2 — Python `Connection` compatibility

## Objective

Implement a pproxy-compatible low-level `Connection` object backed by Rust-owned networking state. Existing Python code using pproxy connection primitives should run unchanged for supported protocols, with equivalent coroutine behavior, timeouts, cancellation, close semantics, addressing, exceptions, and TCP/UDP operations.

## Dependencies

- C1 generated API contract and exact signature targets.
- A2 composition model for protocol/transport legality.
- A3 differential infrastructure.
- Existing PyO3 service and handle architecture.

## Architecture constraints

- Rust owns sockets, relay state, buffers, cancellation tokens, and tasks.
- Python owns the compatibility object and user-facing coroutine contract.
- No network wait, DNS resolution, connect, send, receive, or shutdown operation may hold the GIL.
- Object destruction must not block indefinitely or orphan runtime tasks.
- Avoid a second protocol stack in Python.

## Workstream 1: exact contract implementation

Implement the constructor, attributes, methods, defaults, positional/keyword handling, and representations required by C1. Preserve aliases and import paths. Invalid arguments must raise compatible exception classes and messages where stable.

Map protocol and cipher arguments through typed adapters rather than accepting opaque values that fail later.

## Workstream 2: internal state model

Design explicit states such as:

- created;
- resolving;
- connecting;
- connected;
- UDP-associated;
- closing;
- closed;
- failed.

Transitions must be atomic and testable. Define behavior for repeated connect, send before connect, close during connect, repeated close, wait before close, and method calls after failure.

Store bound/local/remote addresses, selected route/upstream, terminal error, and safe status metadata.

## Workstream 3: TCP operations

Implement the exact C1 contract for TCP connection establishment and stream use:

- direct and supported proxy protocols;
- IPv4, IPv6, and domains;
- connect and handshake timeouts;
- routing and chain integration;
- authentication;
- read/write or exposed stream adapter behavior;
- half-close if exposed;
- close and wait-closed;
- cancellation propagation;
- target refusal and DNS errors.

If pproxy exposes asyncio reader/writer or transport/protocol objects, provide compatible wrappers without copying the full data path through Python unnecessarily.

## Workstream 4: UDP operations

Implement `udp_sendto`, receive behavior, association setup, source-address reporting, and cleanup exactly as characterized.

Requirements:

- direct, SOCKS5, Shadowsocks, and other supported A2 cells;
- per-datagram target metadata;
- standard versus pproxy-compat framing selection;
- association timeout and close semantics;
- concurrent targets and reply-source correctness;
- bounded queues;
- cancellation safety;
- explicit errors for unsupported multi-hop/transport cells.

## Workstream 5: asyncio bridge

Use PyO3 async integration or a maintained equivalent to expose native awaitables. Define loop affinity:

- object creation outside a running loop;
- first use binding to a loop where required;
- rejection or safe adaptation when used from a different loop;
- cancellation of Python futures propagating to Rust;
- Rust task failure resolving the Python future once;
- no nested runtime creation.

## Workstream 6: resource ownership

1. Ensure `close()` is idempotent.
2. Ensure `wait_closed()` observes actual terminal cleanup and does not initiate an unexpected close unless pproxy does.
3. Provide nonblocking best-effort cleanup in `__del__` with warnings for leaked live resources.
4. Prevent reference cycles between callbacks, futures, and native handles.
5. Release sockets/tasks on cancellation, interpreter shutdown, and failed initialization.
6. Add counters or test hooks for leaked native objects.

## Workstream 7: exceptions and diagnostics

Map Rust errors into the C1 exception hierarchy for:

- invalid config/protocol;
- DNS failure;
- connect refusal/timeout;
- authentication failure;
- TLS failure;
- unsupported composition;
- UDP association failure;
- cancellation;
- use after close;
- loop mismatch;
- internal runtime failure.

Retain structured diagnostic codes as attributes where compatible extensions are allowed.

## Workstream 8: tests

Add contract tests for signatures, attributes, coroutine classification, and exceptions.

Add behavioral tests for:

- direct TCP echo;
- HTTP/SOCKS/Trojan/Shadowsocks connections as supported;
- chains;
- IPv4/IPv6/domain;
- refusal/timeout/auth/TLS failures;
- cancellation during DNS/connect/handshake/read/write;
- half-close;
- repeated close/wait;
- garbage collection of live and closed objects;
- multiple concurrent connections;
- loop mismatch;
- UDP send/receive, multiple targets, timeout, and close;
- unsupported cells failing early;
- GIL release verified with a competing Python thread.

Run representative programs against original pproxy and Eggress and compare observations.

## Acceptance criteria

- C1 `Connection` symbols, signatures, defaults, methods, and exception bases match.
- Representative existing pproxy `Connection` examples run unchanged.
- TCP and supported UDP operations use Rust-owned state and do not hold the GIL while blocked.
- Cancellation and close deterministically release sockets and tasks.
- `wait_closed()` semantics match the oracle.
- Unsupported composition errors occur before partial network setup where possible.
- No pending-task, unclosed-resource, or file-descriptor leaks appear in repeated tests.
- Stubs, runtime, docs, A2 matrix, and manifest agree.

## Out of scope

- `Server`, handled in C3;
- arbitrary Python protocol plugins, handled in C4;
- broad asyncio policy cleanup beyond this object, handled in C5;
- unsupported transports not yet promoted in Track B/D/E.

## Recommended commit sequence

1. state model and native handle;
2. constructor/contract shell;
3. TCP operations;
4. close/cancellation/resource ownership;
5. UDP operations;
6. exceptions and diagnostics;
7. differential/contract/leak/GIL tests;
8. stubs, docs, manifest promotion.