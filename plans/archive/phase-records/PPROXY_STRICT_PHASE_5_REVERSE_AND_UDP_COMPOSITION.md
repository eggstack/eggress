# pproxy Strict Phase 5 — Reverse/Backward and UDP Composition Closure

## Objective

Close the remaining composition gaps without weakening Eggress's native protocols or adding a Cartesian matrix of protocol-specific special cases.

This phase has two substantial work packages:

A. a dedicated pproxy backward-wire compatibility adapter;
B. a composable UDP-hop pipeline for the UDP-capable pproxy protocols.

They share URI/jump composition concerns but should remain separate modules and test suites.

---

## Work package A — pproxy backward/reverse compatibility

## Problem

Eggress already has `eggress-protocol-reverse`, but its native protocol intentionally uses stronger/different framing, including newline-delimited authentication and explicit accept/reject handling. pproxy 2.7.9 `ProxyBackward` uses a simpler observable wire protocol and repeated reconnect loops. Strict parity therefore requires an adapter, not mutation of the native reverse protocol.

## Primary files

- `crates/eggress-protocol-reverse/src/lib.rs`
- `crates/eggress-protocol-reverse/src/client.rs`
- `crates/eggress-protocol-reverse/src/server.rs`
- `crates/eggress-runtime/src/*`
- `crates/eggress-config/src/*`
- `crates/eggress-pproxy-compat/src/uri.rs`
- `crates/eggress-pproxy-compat/src/translate.rs`
- reverse compatibility integration tests

Prefer adding a module such as `compat_pproxy.rs` rather than changing native handshake functions.

## Exact compatibility behavior to reproduce

Capture the 2.7.9 oracle before implementation and encode the following behavior explicitly:

- every `+in` occurrence contributes one persistent backward connection worker;
- each worker reconnects indefinitely while the compatibility service is alive;
- reconnect backoff grows from a small delay and caps near the upstream behavior;
- the backward endpoint may itself be reached through the preceding jump chain;
- server/listener protocol selection is the innermost non-direct proxy in the backward composition;
- raw authentication bytes use pproxy framing, not the native Eggress newline protocol;
- a usable returned channel is queued and consumed by the proxy accept path;
- closed/stale queued channels are discarded;
- shutdown stops reconnect workers and closes outstanding channels;
- QUIC-specific marker behavior is deferred until Phase 8 and must not be faked in TCP-only mode.

## Architecture

Create an explicit compatibility state machine, for example:

```text
Disconnected -> Connecting -> Authenticating/Preface -> ReadyChannel
      ^                                            |
      +--------------- retry/backoff <-------------+
```

Use cancellation tokens already present in Eggress runtime. Do not add a second global task supervisor.

Native reverse mode must continue using its existing stronger protocol and security checks.

## Composition rules

At minimum test the exact documented pproxy forms:

- local HTTP listener -> `http+in://public:port` remote;
- compatibility backward listener -> direct client;
- backward connection through an HTTP jump;
- repeated `+in` count > 1;
- TLS-wrapped transport where pproxy accepts it and existing Eggress TLS primitives can represent it.

Do not claim arbitrary Cartesian reverse composition until each accepted form is executable.

## Interop matrix

Required:

1. Eggress backward client -> pproxy backward endpoint.
2. pproxy backward client -> Eggress backward endpoint.
3. one documented jump-through topology in each direction.
4. forced disconnect followed by successful reconnect.
5. clean shutdown with no reconnect task left alive.

---

## Work package B — composable multi-hop UDP

## Problem

Current Eggress UDP support exposes bounded modes and dedicated upstream paths such as one-hop SOCKS5. pproxy recursively prepares UDP datagrams through its jump graph. Strict parity cannot be reached by adding one special case per chain combination.

## Primary files

- `crates/eggress-udp/src/lib.rs`
- `crates/eggress-udp/src/flow.rs`
- `crates/eggress-udp/src/relay.rs`
- `crates/eggress-udp/src/registry.rs`
- `crates/eggress-udp/src/upstream_socks5.rs`
- `crates/eggress-udp/src/standalone_shadowsocks.rs`
- Shadowsocks UDP codec files
- compatibility URI/translation/config lowering

## Bounded architecture

Introduce a transport-neutral UDP hop abstraction. Exact naming is implementation choice, but it should express:

```rust
trait UdpHop {
    fn encode_request(&self, target: &TargetAddr, payload: Bytes) -> Result<Bytes, Error>;
    fn decode_response(&self, packet: Bytes) -> Result<Bytes, Error>;
    async fn send(&self, flow: &UdpFlow, packet: Bytes) -> Result<Bytes, Error>;
}
```

The real implementation may need associated metadata instead of this exact signature. The important property is recursive composition rather than mode-specific branching.

## Supported hop set

Only protocols that pproxy 2.7.9 actually supports for UDP may participate. Phase 0 is authoritative.

Expected useful set:

- direct/fixed target;
- SOCKS5 UDP;
- Shadowsocks/SSR UDP where proven;
- QUIC UDP transport only after Phase 8.

HTTP, SOCKS4, Trojan, H2/H3 CONNECT, or other protocols without a real pproxy UDP path must fail validation rather than being coerced into a chain.

## Flow lifecycle

Preserve Eggress safety properties while matching legitimate pproxy behavior:

- client endpoint pinning;
- bounded flow registry;
- idle expiry;
- per-flow upstream association reuse where required;
- no unbounded task or socket creation;
- response decoding in reverse hop order;
- destination metadata preserved through nested encodings;
- amplification protections remain effective.

## Tests

Unit tests:

- encode order A->B->target;
- decode order target->B->A;
- domain/IPv4/IPv6 target preservation;
- expiry and registry cleanup;
- invalid non-UDP hop rejected at config validation.

Runtime/oracle tests:

- direct UDP echo;
- one-hop SOCKS5;
- one-hop Shadowsocks;
- every two-hop UDP chain that pproxy 2.7.9 demonstrably supports and that uses implemented protocols;
- response path through the same chain;
- mixed valid/invalid chain startup behavior.

Avoid public DNS as a test dependency; use local UDP echo fixtures.

## Non-goals

- Replacing native secure reverse protocol semantics.
- Reverse UDP unless pproxy 2.7.9 has a proven corresponding path and Phase 0 explicitly adds it.
- UDP over protocols pproxy itself does not support.
- A generalized overlay network abstraction.

## Acceptance criteria

1. Native reverse protocol tests remain unchanged and passing.
2. A separate pproxy backward adapter reproduces the exact 2.7.9 auth/preface/channel behavior.
3. Repeated `+in` creates the correct number of maintained backward workers.
4. Bidirectional pproxy/Eggress backward interop passes for direct and at least one documented jump-through topology.
5. Reconnect/backoff and shutdown are deterministic and leak-free.
6. UDP chains are represented as composable hops rather than a growing pairwise special-case matrix.
7. Every pproxy-supported two-hop UDP combination among implemented protocols is either executable with oracle evidence or explicitly rejected with a recorded reason.
8. Response decoding occurs in reverse hop order and survives fragmented/independent datagrams correctly.
9. Existing SOCKS5 UDP ASSOCIATE and standalone UDP behavior regressions remain covered.
