# Performance Phase 1 — TLS and Connection Setup

## Status

**IMPLEMENTED — 2026-09-20**

## Parent roadmap

[`PERFORMANCE_OPTIMIZATION_ROADMAP.md`](PERFORMANCE_OPTIMIZATION_ROADMAP.md)

## Planning baseline

`80c8f2f2464bb3c4182afee73a8fdbdc170b1cdb`

## Objective

Remove repeated immutable TLS construction and redundant connection-wrapper work from ordinary connection setup while preserving the existing public APIs, reload model, certificate verification behavior, chain semantics, and listener/session accounting.

This phase intentionally targets work that is clearly redundant before any benchmark-guided micro-tuning.

## Scope

This phase owns four changes:

1. cache/share default outbound Rustls client configurations;
2. precompile inbound Rustls server configuration once per prepared listener generation;
3. remove redundant TCP stream boxing/re-boxing;
4. build listener-scoped UDP service state once per listener generation.

Do not include relay-copy algorithm changes or health/UDP flow-count changes here; those belong to Phase 2.

## Workstream 1 — Cache default outbound TLS client configurations

### Current behavior

The server selects an upstream and constructs a chain executor for the route. The outbound executor's construction path builds Rustls client configuration variants used by TLS and H2-capable hops.

The default verified trust roots and ALPN variants are immutable for the ordinary path. Rebuilding them per route/open attempt adds repeated trust-store/configuration work that does not depend on the destination.

### Required design

Introduce internal shared configuration accessors in the narrowest crate that currently owns TLS client configuration construction.

At minimum preserve distinct immutable configurations for:

- ordinary verified TLS;
- verified TLS with H2 ALPN where required;
- existing feature-gated insecure/verification-disabled variants, if those are currently constructed separately.

Preferred implementation:

- use `std::sync::LazyLock`, `OnceLock`, or an equivalently simple process-lifetime cache if the configuration is truly process-static;
- return/share `Arc<rustls::ClientConfig>` rather than clone/reconstruct the full configuration;
- retain explicit per-call construction only when the caller supplies a custom `tls_override` or other non-default state.

Do not add an invalidation layer to process-static default roots. Do not cache user-provided overrides by pointer/string/key. Do not introduce a generic TLS config registry.

### Compatibility requirements

- certificate verification roots/defaults are unchanged;
- ALPN lists are byte-for-byte/semantically equivalent;
- insecure feature behavior remains behind the same feature boundary;
- `tls_override` behavior remains caller-controlled and bypasses default cached state;
- existing outbound public constructors/builders keep their signatures;
- no new public global is required.

### Tests

Add focused tests that prove:

1. repeated default executor/TLS setup returns configurations with equivalent semantics;
2. H2 ALPN configuration still advertises the required protocols;
3. default verified and feature-gated insecure configurations remain distinct;
4. custom override construction does not accidentally use the default cache;
5. ordinary TLS integration tests still validate trusted certificates and reject the same untrusted cases.

A pointer-identity assertion may be used internally to prove reuse if it does not become the public contract.

## Workstream 2 — Precompile inbound TLS server configuration per listener generation

### Current behavior

The prepared listener carries compiled TLS configuration/material. The standard TCP loop clones that listener TLS value into accepted connection tasks. Server wrapping then constructs a TLS server builder, parses certificate/key material, and builds `rustls::ServerConfig` for the individual connection.

Reload already creates/replaces prepared listener generations, so per-connection TLS compilation is the wrong lifetime.

### Required design

Extend private/runtime-only prepared listener state to hold the ready-to-use server TLS configuration:

```text
compiled configuration snapshot
    -> prepare listener generation
        -> parse cert/key + construct Arc<ServerConfig> once
            -> accepted connections clone Arc only
```

Keep public configuration model types unchanged. If `CompiledListenerTlsConfig` is part of a broader config crate boundary, do not force Rustls runtime types into that public/config representation solely for this optimization. The prepared runtime/listener type is the preferred ownership location.

### Reload behavior

A configuration reload that changes listener TLS material must construct a new `ServerConfig` as part of preparing the new listener generation. Existing accepted sessions may continue using the `Arc` they already own. New sessions must use the new generation.

If preparation fails because certificate/key material is invalid, preserve the current reload/startup error policy. Do not defer a known-invalid certificate parse until the first connection merely to keep old timing.

### Compatibility requirements

- same certificate chain/key selection;
- same ALPN configuration;
- same verification/client-auth policy;
- same listener start/reload failure behavior;
- same session/reload generation ownership;
- no public config field/type change.

### Tests

Cover:

1. valid listener TLS startup and handshake;
2. invalid PEM/key failure at the same or earlier safe preparation point;
3. reload from TLS configuration A to B gives new sessions B while existing A sessions are not forcibly mutated;
4. non-TLS listeners do not allocate/build Rustls server state;
5. feature-gated TLS boundaries remain buildable.

## Workstream 3 — Remove redundant TCP stream boxing

### Current shape

The TCP listener permit wrapper currently contains a `BoxStream`, and the raw accepted `TcpStream` is boxed before insertion. The wrapper itself is then boxed to satisfy `AcceptedConnection.stream: BoxStream`. The supervisor can then pass `Box::new(conn.stream)` into TLS wrapping, creating another trait-object layer around an existing trait object.

### Required change

Keep the public/externally consumed accepted-connection stream field unchanged, but change private layout to:

```rust
struct PermitStream {
    inner: tokio::net::TcpStream,
    _permit: OwnedSemaphorePermit,
}
```

Implement the required async read/write traits by delegating directly to the concrete `TcpStream`.

At the accepted-connection boundary, box `PermitStream` exactly once as `BoxStream`.

When passing an existing `BoxStream` into TLS server wrapping or other functions that already accept `BoxStream`, pass it directly. Do not wrap `BoxStream` in another `Box`.

### Guardrails

- semaphore permit lifetime must still exactly cover the accepted stream/session lifetime;
- listener backpressure semantics stay unchanged;
- no public generic accepted-connection type is introduced;
- no attempt is made to remove all `BoxStream` boundaries from Eggress.

### Tests

Retain/extend tests for:

- listener connection-limit enforcement;
- permit release after close/error/cancellation;
- TLS and plaintext accepted paths;
- `AcceptedConnection.stream` source compatibility;
- shutdown with blocked/active connections.

## Workstream 4 — Build listener-scoped UDP service once

### Current behavior

The standard TCP accept loop constructs the UDP service used by per-connection SOCKS/UDP-capable handling for each accepted TCP connection. The inputs are listener/runtime scoped and the result is already shareable through `Arc`.

### Required design

Construct the `RuntimeUdpService` (or current equivalent) during listener/runtime preparation, once per listener generation, then clone the `Arc` into `ConnectionConfig` for accepted sessions.

If a listener/configuration does not enable the UDP service, preserve the existing `None`/disabled representation without constructing unused state.

Do not make the UDP service process-global; listener-specific routing/configuration and reload ownership must remain explicit.

### Reload behavior

New listener generation -> new service state using the new generation's configuration/snapshot references. Existing accepted sessions retain their old generation's `Arc` until completion, consistent with existing snapshot/lifecycle semantics.

### Tests

Cover:

- enabled service is shared by multiple accepted connections within one listener generation;
- disabled listener does not construct service state;
- reload creates/uses the appropriate generation;
- connection/session behavior and SOCKS5 UDP association tests remain unchanged.

## Implementation order

Recommended commit sequence:

1. `perf(tls): cache default outbound client configs`
2. `perf(runtime): precompile listener TLS server config`
3. `perf(listener): remove redundant TCP stream boxing`
4. `perf(runtime): share listener UDP service`

They may be squashed for merge if repository practice prefers, but each logical change should remain reviewable.

## Focused verification

Run at minimum the owning crate tests and relevant integration tests:

```bash
cargo test -p eggress-transport-tls
cargo test -p eggress-outbound
cargo test -p eggress-core
cargo test -p eggress-runtime
cargo test -p eggress-server
```

Also run feature-boundary checks used by the repository for common/full configurations if the touched crates participate in those boundaries.

Before merge:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Evidence expectations

Phase 3 owns formal benchmark closure, but implementation should record lightweight evidence that construction moved to the intended lifetime:

- counter/test instrumentation or pointer identity showing default TLS config reuse;
- listener preparation test showing one server config per generation rather than per handshake;
- allocation/structure review showing one accepted TCP stream box rather than nested boxes;
- service identity test showing one UDP service per listener generation.

Do not add production counters solely to prove these facts.

## Stop conditions

Stop and reassess rather than adding complexity if:

- default TLS configs actually depend on per-route mutable state not captured in the audit;
- caching would weaken override/verification isolation;
- prepared-listener TLS compilation would require making runtime TLS types part of a public config API;
- removing boxing changes permit lifetime/cancellation behavior;
- listener-scoped UDP service sharing reveals mutable state that was intentionally connection-local.

In those cases, document the discovered invariant and retain the safe existing behavior for that sub-workstream.

## Closure

- `eggress-transport-tls` now exposes process-shared verified and H2 client
  configurations, with separate feature-gated insecure caches. Overrides and
  custom builder state remain caller-owned.
- Runtime listener preparation builds one `Arc<ServerConfig>` per listener
  generation and shares one `RuntimeUdpService` per generation. Invalid TLS
  material still fails during preparation.
- `PermitStream` owns a concrete `TcpStream`; the accepted boundary boxes it
  once, and supervisor TLS wrapping passes the existing `BoxStream` directly.
- Focused compile/tests passed for TLS, outbound, core, and runtime paths.

## Acceptance criteria

This phase is complete when:

1. Ordinary verified outbound TLS configuration is reused rather than rebuilt for each route/open.
2. H2 ALPN configuration is reused with unchanged protocol advertisement.
3. Feature-gated insecure configuration behavior is preserved and remains isolated.
4. Custom TLS overrides bypass the default cache and remain behaviorally unchanged.
5. Inbound certificate/key parsing and Rustls server-config construction occur once per prepared listener generation, not once per connection.
6. TLS reload behavior is generation-correct.
7. Non-TLS listeners incur no new TLS construction.
8. The raw accepted `TcpStream` is not boxed before being stored in the private permit wrapper.
9. Existing `BoxStream` values are not re-boxed solely to call TLS wrapping.
10. Permit lifetime and connection-limit semantics are unchanged.
11. UDP service construction is listener-generation scoped and its `Arc` is shared across accepted connections.
12. Public Rust signatures/configuration schemas are unchanged.
13. Relevant TLS, listener, runtime, SOCKS/UDP, reload, and workspace tests pass.
14. No protocol or pproxy compatibility behavior changes are required.
