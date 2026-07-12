# Phase B4 — HTTP/2 runtime promotion

## Objective

Promote the existing HTTP/2 CONNECT protocol primitives into a complete runtime transport. The final implementation must support every characterized pproxy H2 role through configuration, supervisor lifecycle, chaining, TLS/ALPN, connection pooling, flow control, stream limits, Python exposure, observability, and interoperability evidence.

## Dependencies

- A1–A3 complete enough to classify and test composition cells.
- A2 matrix IDs for H2 listener/upstream/chain roles.
- B3 transport-promotion patterns should be reused where applicable.

## Current state

Eggress already contains H2 CONNECT client/server, stream adaptation, flow-control integration, reset/GOAWAY handling, pooling, TLS ALPN, and authentication primitives in protocol crates. Runtime/config currently refuses `h2://`, so this remains protocol-internal support.

## Workstream 1: characterize pproxy H2

Determine with pinned pproxy:

- URI scheme, aliases, and default ports;
- client/server roles;
- CONNECT request format and authority handling;
- cleartext H2 versus TLS-only support;
- ALPN and certificate behavior;
- authentication headers;
- connection reuse and pooling semantics;
- maximum/concurrent stream behavior;
- GOAWAY, reset, refused-stream, and reconnect behavior;
- chain positions and prior-hop support;
- IPv4, IPv6, and domain targets;
- whether UDP or extended CONNECT is supported;
- CLI and Python exposure.

Create A3 scenarios and retain protocol transcripts.

## Workstream 2: configuration and validation

Add typed H2 settings for:

- listener/upstream endpoint;
- TLS server/client configuration;
- cleartext mode if supported;
- authentication;
- max concurrent streams;
- connection pool size and idle timeout;
- stream and connection windows;
- header-list and frame limits;
- connect, handshake, idle, drain, and shutdown timeouts;
- keepalive/ping settings;
- chain linkage.

Validate unsupported cleartext/TLS combinations, invalid ALPN, impossible chain positions, unsupported UDP, unsafe unlimited values, and contradictory pool settings before startup.

## Workstream 3: supervisor integration

1. Add H2 listener and upstream constructors.
2. Integrate service ownership, task tracking, bound addresses, cancellation, drain, and reload policy.
3. Maintain shared upstream H2 connections in a dedicated bounded pool.
4. Ensure pool keys isolate destination, credentials, TLS identity, SNI, routing context, and compatibility policy.
5. Clean up partial startup and failed connections deterministically.
6. Preserve A2 validation at config compile time.

## Workstream 4: stream lifecycle and flow control

Audit and test:

- CONNECT success only after downstream establishment;
- stream-to-byte-adapter correctness;
- inbound/outbound flow-control updates;
- bounded buffering per stream and connection;
- half-close mapping;
- reset propagation in both directions;
- GOAWAY handling and draining;
- refused-stream retry policy;
- connection loss with active streams;
- cancellation safety;
- fairness across concurrent streams;
- stream-ID exhaustion and connection replacement.

Do not automatically replay application data after an ambiguous failure.

## Workstream 5: pooling and reuse

Characterize pproxy reuse behavior and implement:

- bounded active and idle connections;
- max concurrent streams per connection;
- health/idle validation;
- GOAWAY-aware retirement;
- backpressure when pool/stream capacity is exhausted;
- credential and route isolation;
- metrics for connections, streams, queueing, reuse, failures, and retirements;
- deterministic shutdown and reload behavior.

Coordinate this design with future G3 connection reuse so H2 does not gain a separate incompatible pool framework.

## Workstream 6: chaining

Explicitly model and test supported cells such as:

- direct H2 CONNECT to target;
- HTTP/SOCKS5 prior hop → H2 upstream;
- H2 → HTTP/SOCKS5 subsequent hop if pproxy supports it;
- H2 as intermediate hop in a three-hop chain;
- H2 over TLS with custom CA/SNI;
- multiple logical proxy sessions over one H2 connection.

Unsupported reverse, UDP, or extended-CONNECT cells must fail before startup.

## Workstream 7: CLI and Python exposure

CLI requirements:

- `h2://` parsing and translation;
- default ports, IPv6, credentials, TLS modifiers, and chain syntax;
- consistent `check`, `translate`, and `run` behavior;
- JSON composition diagnostics;
- drop-in binary startup.

Python requirements:

- service construction from args/URI/TOML;
- H2 TLS/pool settings;
- start/astart, close/wait_closed;
- status and pool metrics;
- typed exceptions for TLS, auth, stream reset, GOAWAY, pool exhaustion, target failure, and shutdown;
- type stubs and docs.

## Workstream 8: observability

Add bounded-cardinality metrics for:

- H2 connections active/created/reused/retired;
- streams active/opened/completed/reset/refused;
- GOAWAY counts and reason groups;
- flow-control stalls;
- pool wait time and exhaustion;
- handshake/TLS/auth failures;
- bytes relayed.

Expose safe status summaries through admin and Python APIs.

## Workstream 9: interoperability and fault testing

Test against:

- Eggress client/server;
- pproxy where functional;
- an independent H2 implementation.

Required scenarios:

- trusted/untrusted TLS and ALPN mismatch;
- IPv4/IPv6/domain targets;
- auth success/failure;
- concurrent streams;
- pool reuse;
- max-stream backpressure;
- flow-control pressure;
- target refusal/timeout;
- stream reset;
- GOAWAY graceful and abrupt;
- connection loss;
- half-close;
- malformed headers and oversized header lists;
- cancellation/drain/reload;
- chain combinations;
- soak with repeated stream churn.

## Diagnostics

Define stable codes for invalid H2 URI/config, ALPN failure, TLS verification, auth failure, CONNECT rejection, stream reset, GOAWAY, pool exhausted, unsupported cleartext mode, unsupported chain/UDP cell, header/frame limit, and target failure.

## Acceptance criteria

- Supported `h2://` configurations launch real listener/upstream services.
- Concurrent streams are isolated and bounded.
- Pooling does not cross credentials, TLS identity, or routing contexts.
- GOAWAY/reset/connection-loss semantics are deterministic and tested.
- Supported composition cells pass end-to-end and interoperability/differential scenarios.
- CLI, Python, matrix, manifest, and docs agree.
- Runtime refusal is removed only for proven cells.
- No unbounded per-stream or per-connection buffering remains.

## Out of scope

- HTTP/3 and QUIC;
- MASQUE/CONNECT-UDP;
- general-purpose HTTP/2 reverse proxying outside pproxy tunnel semantics;
- reverse H2 unless characterized separately;
- full Track C low-level API.

## Recommended commit sequence

1. pproxy characterization;
2. typed config and validation;
3. supervisor listener/upstream integration;
4. stream/flow-control hardening;
5. shared pooling framework;
6. chain integration;
7. CLI/Python exposure;
8. interop/fault/soak tests;
9. matrix, manifest, and docs promotion.