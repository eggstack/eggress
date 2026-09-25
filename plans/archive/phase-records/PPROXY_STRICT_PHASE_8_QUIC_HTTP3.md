# pproxy Strict Phase 8 — Optional QUIC and HTTP/3 Parity

## Objective

Implement pproxy 2.7.9 QUIC stream transport and HTTP/3 CONNECT behavior as optional Eggress transports.

Standards compliance alone is insufficient: strict parity requires interoperability with pproxy's particular mapping of proxy TCP/UDP flows onto QUIC streams.

## Product gate

This phase adds substantial dependencies and may raise MSRV. Before implementation, record:

- selected QUIC crate and version;
- selected H3 crate and version;
- MSRV delta;
- release binary-size delta with feature enabled and disabled;
- TLS/crypto backend duplication relative to `rustls`/`ring` already in Eggress.

Preferred ecosystem candidates are `quinn` plus `h3`/`h3-quinn`, subject to current maintenance/MSRV review at implementation time.

Do not enable this stack by default solely for parity accounting.

## Suggested crate split

```text
crates/eggress-transport-quic/
crates/eggress-protocol-h3/
```

`eggress-transport-quic` owns connection/session/stream behavior. `eggress-protocol-h3` owns HTTP/3 CONNECT header semantics. Keep compatibility lowering in `eggress-pproxy-compat`.

## Work package A — raw pproxy QUIC transport

pproxy 2.7.9 uses QUIC as a multiplexed stream transport:

- one QUIC connection can carry many bidirectional proxy streams;
- TCP proxy requests run on independent streams;
- UDP compatibility traffic is associated with QUIC streams rather than requiring modern QUIC DATAGRAM semantics;
- reconnect occurs when the QUIC connection terminates;
- listener mode accepts stream handlers on UDP-bound QUIC transport.

Capture exact ALPN/configuration/certificate behavior with the oracle before finalizing the connector.

### Required implementation

- client connection cache keyed by configured remote;
- bidirectional stream open;
- stream adapter into Eggress `BoxStream`/runtime abstraction;
- listener accepting streams and dispatching them independently;
- clean connection termination and reconnect;
- bounded stream/connection resources;
- compatibility UDP stream map after Phase 5's generic UDP-hop model is available.

Do not expose QUIC internals to generic protocol crates.

## Work package B — HTTP/3 CONNECT

Layer H3 over the QUIC transport:

- H3 client session;
- per-request bidirectional stream;
- CONNECT headers matching pproxy 2.7.9 (`:method`, `:authority`, relevant scheme/path behavior);
- proxy authorization where configured;
- listener/server parsing of incoming H3 headers;
- success response status;
- DATA stream relay;
- independent stream close/reset handling;
- connection termination without leaking per-stream tasks.

Use standard H3 APIs rather than private implementation details, while preserving pproxy-visible headers/behavior.

## TLS/certificate behavior

pproxy requires server certificate material for QUIC/H3 listener use and disables certificate verification on its compatibility client path.

For strict compatibility:

- require configured certificate/key for listener mode where pproxy does;
- allow pproxy-compatible insecure verification only in compatibility mode with an explicit warning;
- native Eggress QUIC/H3 APIs must keep secure verification defaults;
- reuse existing certificate loading utilities if possible.

## URI/config integration

Support exact 2.7.9 accepted forms after oracle validation, including combinations such as:

- `quic+http://...`;
- H3 scheme forms;
- TLS/cert requirement errors;
- `+in` interactions only after Phase 5 backward compatibility is stable;
- UDP use only where pproxy really supports it.

Invalid combinations must fail at startup.

## Interoperability tests

Required local tests:

1. Eggress QUIC client -> pproxy 2.7.9 QUIC listener.
2. pproxy QUIC client -> Eggress listener.
3. multiple simultaneous TCP streams over one QUIC connection.
4. forced connection termination followed by reconnect.
5. Eggress H3 client -> pproxy H3 server.
6. pproxy H3 client -> Eggress H3 server.
7. HTTP CONNECT payload to local echo target.
8. compatibility UDP over QUIC if Phase 0 confirms and Phase 5 exposes it.
9. certificate-required listener error.

Add standards-client tests only where useful; pproxy interop is the strict contract.

## Resource/safety requirements

- explicit idle timeout;
- bounded concurrent streams where the library requires configuration;
- no unbounded UDP flow map;
- cancellation propagated to all stream tasks;
- no panic on malformed H3 events;
- no compatibility insecure-cert policy outside compatibility mode.

## Non-goals

- WebTransport.
- MASQUE CONNECT-UDP unless pproxy 2.7.9 implements it (it does not follow merely from H3 support).
- Generic QUIC DATAGRAM API solely for modernity.
- Making QUIC/H3 part of default Eggress builds.

## Acceptance criteria

1. QUIC/H3 dependencies and MSRV impact are explicitly approved.
2. Default builds remain free of the optional QUIC/H3 stack.
3. Raw QUIC stream mode interoperates bidirectionally with pproxy 2.7.9.
4. H3 CONNECT interoperates bidirectionally with pproxy 2.7.9.
5. Multiple proxy streams share one QUIC connection without head-of-line coupling at the application layer.
6. Reconnect and stream cleanup are deterministic.
7. Any pproxy-compatible UDP-over-QUIC path uses Phase 5's composable UDP abstraction rather than a second UDP subsystem.
8. Compatibility insecure certificate verification is isolated and warned; native mode stays secure.
9. Binary-size delta is recorded for feature-off and feature-on release builds.
