# Outbound TCP Socket Metadata Recovery

## Status

**IMPLEMENTATION COMPLETE — QUALIFIED — 2026-09-23**

## Target repository

`eggstack/eggress`

Planning baseline:

`e55f8f3f642bd03a2a6477aff24d7ff837378c09` (`main`, workspace 1.0.8)

Downstream motivating evidence:

- Eggsec adopted published `eggress-outbound 1.0.8` as its listener-free
  proxy-hop engine.
- `OutboundInfo.local_addr` is always `None` on both direct and chain TCP
  paths in 1.0.8, forcing downstream callers with an established non-optional
  metadata field to represent the value as unknown.
- This is a generic Eggress metadata gap, not an Eggsec-specific feature
  request.

Follow-up release qualification:

- `plans/OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`

## Objective

Make `OutboundConnector` return truthful socket metadata for TCP-backed
listener-free connections without changing any existing connection method
signature, weakening the boxed-stream boundary, or duplicating chain execution.

For TCP-backed routes, `OutboundInfo` should report the actual local and peer
socket addresses from the socket that was really established:

```text
direct TCP:
    local_addr = actual client socket local_addr()
    peer_addr  = actual target socket peer_addr()

proxy chain with TCP-backed first hop:
    local_addr = actual first-hop TCP socket local_addr()
    peer_addr  = actual first-hop TCP socket peer_addr()

non-TCP first-hop transport (Unix / QUIC-H3 where no TCP SocketAddr exists):
    local_addr = None
    peer_addr  = None unless that transport exposes equivalent truthful
                 SocketAddr metadata through an existing safe API
```

Do not infer addresses from configuration, perform a second DNS lookup for
metadata, downcast `BoxStream`, or expose a raw `TcpStream` solely to recover
metadata.

The public `OutboundInfo` shape remains:

```rust
pub struct OutboundInfo {
    pub local_addr: Option<std::net::SocketAddr>,
    pub peer_addr: Option<std::net::SocketAddr>,
    pub hop_count: usize,
}
```

This plan changes the truthfulness/population of existing optional fields, not
their type or the `connect_tcp*` return signatures.

---

## Confirmed 1.0.8 baseline

### OutboundConnector

`crates/eggress-outbound/src/connector.rs` currently:

- returns `local_addr: None, peer_addr: None` for direct TCP;
- resolves the first configured chain endpoint separately via
  `resolve_endpoint_addr()` to produce `peer_addr`;
- calls `ChainExecutor::execute()`, which returns only `BoxStream`;
- then returns `local_addr: None` for chain TCP.

Consequences:

1. the actual local ephemeral/bound address is lost before
   `OutboundConnector` can report it;
2. direct-mode `peer_addr` is also unnecessarily absent;
3. chain `peer_addr` is an independently resolved approximation, not
   necessarily the address the connection actually used when a hostname has
   multiple DNS answers;
4. Python `OutboundStream.sockname` therefore stays `None` and direct
   `peername` is allowed to be `None` even after a successful TCP connect.

### Core DirectConnector

`eggress-core::connector::DirectConnector::connect_with_options()` resolves
targets, creates a concrete Tokio `TcpStream`, and immediately boxes it in
`connect_to_addrs()`.

At that point the implementation still has safe access to:

- `TcpStream::local_addr()`;
- `TcpStream::peer_addr()`.

That is the correct layer to capture socket metadata.

### ChainExecutor

`eggress-core::chain::ChainExecutor::execute()` opens the first-hop
transport, then repeatedly transforms the stream through optional TLS and
protocol handshakes. The first-hop TCP socket identity remains the relevant
underlying socket metadata even after those wrappers are applied.

The server/runtime currently consume `execute()`; changing its existing
signature would be an unnecessary public break.

---

## Architectural constraints

1. Preserve every existing public method signature.
2. Preserve `Connector` / `LocalConnector` trait signatures.
3. Preserve `ChainExecutor::execute()` exactly.
4. Preserve `OutboundConnector::connect_tcp()`,
   `connect_tcp_detailed()`, `connect_tcp_timeout()`, and
   `connect_tcp_timeout_detailed()` signatures.
5. Preserve `OutboundInfo` field names and types.
6. Preserve `eggress-embed::outbound::*` as a pure compatibility re-export.
7. Preserve the `BoxStream` boundary; no concrete socket type leaks through
   proxy/TLS/protocol architecture.
8. Do not use `Any`, unsafe downcasts, file descriptors, platform-specific
   socket extraction, or `unsafe`.
9. Do not add a second chain executor or duplicate protocol handshakes.
10. No behavior change to route selection, DNS policy, proxy fallback,
    timeouts, typed errors, TLS, SSH, QUIC, or UDP.
11. No new dependency should be required.
12. Rust 1.89 and `unsafe_code = "deny"` remain fixed.
13. Metadata must describe the socket actually connected, not a separately
    resolved candidate.
14. Failure to read metadata must not turn an otherwise successful connection
    into a connection failure; fields remain optional.

---

# Workstream 0 — freeze the baseline and public contract

Before editing, record:

```sh
git rev-parse HEAD
git status --short
cargo test -p eggress-core --locked
cargo test -p eggress-outbound --locked
cargo test -p eggress-embed --locked --test outbound_detailed
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp
```

Capture the current public signatures for:

- `DirectConnector::connect_with_options`;
- `ChainExecutor::execute`;
- `OutboundConnector::connect_tcp*`;
- `OutboundInfo`.

Confirm the Python metadata behavior in
`python/tests/test_outbound_stream_verification.py`: direct `peername` is
currently allowed to be `None`, and `sockname` is backed by
`OutboundInfo.local_addr`.

Do not use downstream sentinel behavior as the upstream contract. Eggress owns
an `Option<SocketAddr>`; it should report `Some` only when it has truthful
socket metadata.

---

# Workstream 1 — add a metadata-preserving direct-connect seam in eggress-core

Refactor the internal TCP connection loop so the concrete `TcpStream` is
queried before boxing.

Add a small protocol-neutral socket metadata type in
`eggress-core::connector`, for example:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectionMetadata {
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
}

impl ConnectionMetadata {
    pub fn local_addr(&self) -> Option<SocketAddr>;
    pub fn peer_addr(&self) -> Option<SocketAddr>;
}
```

Exact naming may follow repository conventions, but prefer private fields plus
accessors so future metadata can grow without making a public struct-literal
contract.

Add an additive DirectConnector method such as:

```rust
pub async fn connect_with_options_and_metadata(
    &self,
    target: &TargetAddr,
    options: &ConnectOptions,
) -> Result<(BoxStream, ConnectionMetadata), ConnectError>;
```

Required implementation shape:

1. resolve exactly once using the current `resolve_target()` behavior;
2. run the existing ordered address-attempt loop;
3. on successful `TcpStream`, obtain `local_addr().ok()` and
   `peer_addr().ok()` before boxing;
4. return the boxed stream plus metadata;
5. make existing `connect_with_options()` delegate to the new single
   implementation and discard metadata;
6. keep the `Connector` trait implementation unchanged.

Do not duplicate the address-attempt loop between the two methods.

Metadata lookup errors are metadata absence, not transport failure. A
successful TCP connection must remain successful if either socket-address
query unexpectedly fails.

### Required core tests

Use local listeners only.

Cover:

- direct IPv4 connect reports `local_addr=Some` with a nonzero ephemeral
  port and `peer_addr == listener.local_addr()`;
- IPv6 equivalent when the platform supports loopback IPv6, with an explicit
  skip only for genuine platform absence;
- `local_bind = 127.0.0.1:0` reports the actual post-connect ephemeral local
  port, not the configured zero-port hint;
- existing connect error ordering/classification remains unchanged;
- existing `connect_with_options()` still works through the delegated path.

---

# Workstream 2 — propagate first-hop socket metadata through ChainExecutor

Add an additive metadata-preserving execution method while preserving
`ChainExecutor::execute()`.

Preferred shape:

```rust
pub async fn execute_with_metadata(
    &self,
    chain: &[ProxyHopSpec],
    target: &TargetAddr,
) -> Result<(BoxStream, ConnectionMetadata), ChainError>;
```

Existing `execute()` should call the same internal execution authority and
discard metadata.

Do not maintain two copies of the chain algorithm.

### Metadata semantics

For a TCP-backed first hop:

- capture metadata from the actual DirectConnector connection to hop 0;
- retain that same metadata while TLS/protocol wrappers transform the
  `BoxStream`;
- for multi-hop chains, `peer_addr` remains the actual first hop, not the
  second hop or final target;
- `local_addr` remains the local side of that first-hop transport.

For a Unix first hop:

- return no IP socket metadata.

For QUIC/HTTP3 first-hop transport opened through `HopHandler::open()`:

- return `None` unless the existing transport API already exposes truthful
  `SocketAddr` values without new architecture;
- do not synthesize TCP metadata for a non-TCP transport;
- do not broaden this plan into QUIC metadata API work.

Preserve all current preflight validation, handler ordering, TLS wrapping,
SSH session behavior, and `ChainError` construction.

### Required chain tests

Add deterministic local fixtures proving:

- single-hop SOCKS/HTTP/raw-style TCP-backed chain retains first-hop local and
  peer metadata through handshake/wrapping;
- two-hop chain metadata still identifies hop 0;
- configured first-hop `local_bind` is reflected in the actual local socket
  address;
- existing `execute()` remains behaviorally equivalent apart from discarded
  metadata;
- invalid/empty chain errors are unchanged.

Use the smallest existing test handlers/fixtures; do not add external network
dependencies.

---

# Workstream 3 — make OutboundInfo use actual execution metadata

Update `eggress-outbound::OutboundConnector::connect_tcp_inner()`.

### Direct route

Use the new DirectConnector metadata-preserving method and populate:

```text
OutboundInfo.local_addr = actual direct TCP local address
OutboundInfo.peer_addr  = actual direct TCP peer address
OutboundInfo.hop_count  = 0
```

### Chain route

Use the new ChainExecutor metadata-preserving method and populate:

```text
OutboundInfo.local_addr = actual first-hop TCP local address when available
OutboundInfo.peer_addr  = actual first-hop TCP peer address when available
OutboundInfo.hop_count  = chain length
```

Remove `resolve_endpoint_addr()` if it has no remaining owner.

This is important: do not retain a second DNS lookup merely to fill
`peer_addr`. Metadata must follow the socket actually chosen by the
connection path, including multi-address hostname cases.

All four public TCP connect methods must continue to share
`connect_tcp_inner()`; timeout and typed-error behavior must remain exactly
as before.

### Required outbound tests

Add local deterministic tests for:

- `OutboundConnector::direct()` returns non-`None` local address and exact
  target peer address;
- one-hop SOCKS5 returns actual local address and exact proxy-listener peer;
- one-hop HTTP CONNECT returns actual local address and exact proxy-listener
  peer;
- multi-hop returns the entry proxy as peer and correct hop count;
- `from_chain` with a first-hop local bind reports the actual bound local
  address;
- timeout/failure paths do not manufacture metadata;
- no proxy failure falls back direct;
- credentials remain absent from diagnostics.

Do not weaken private/reserved-target policy to make tests convenient.

---

# Workstream 4 — qualify embed and Python projections

`eggress-embed` should require no implementation fork because it re-exports
`eggress-outbound`.

Add/adjust representative Rust API tests so the compatibility path proves the
same populated `OutboundInfo` behavior.

The Python binding already projects:

```text
OutboundInfo.peer_addr  -> OutboundStream.peername
OutboundInfo.local_addr -> OutboundStream.sockname
```

Strengthen the Python verification rather than adding another metadata path.

For a local direct TCP connection, require:

- `peername` is present and identifies the echo listener;
- `sockname` is present, loopback-family-correct, and has a nonzero local
  port;
- `get_extra_info("peername")` and `get_extra_info("sockname")` agree with
  the properties.

If a local proxy fixture already exists cheaply in Python tests, add one
proxy-chain metadata assertion. Otherwise keep semantic chain coverage in Rust;
do not create a large Python proxy harness only for this change.

No .pyi type change should be necessary because the properties are already
optional.

---

# Workstream 5 — documentation and API qualification

Update current documentation to define exact metadata semantics:

- `architecture/outbound.md`;
- `crates/eggress-outbound/README.md`;
- `docs/RUST_API.md` if the additive core seam must be classified;
- Python docs only where `peername` / `sockname` behavior is described.

Document:

- TCP-backed direct: local + peer are actual socket values;
- TCP-backed chain: metadata belongs to the first-hop transport;
- multi-hop peer is hop 0;
- Unix/non-TCP first hops may legitimately return `None`;
- metadata absence never changes successful connection semantics;
- metadata is observational only and is not a route/retry/policy decision;
- no second metadata-only DNS lookup occurs.

Do not make a pproxy parity claim from this work; this is an Eggress
listener-free API correctness improvement.

---

# Workstream 6 — full verification and release handoff evidence

Run focused slices first:

```sh
cargo fmt --all -- --check
cargo test -p eggress-core --locked
cargo test -p eggress-outbound --locked
cargo test -p eggress-embed --locked --test outbound_detailed
cargo test -p eggress-embed --locked --test public_api
```

Required outbound feature compilation:

```sh
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp
```

If Python tests are changed:

```sh
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests/test_outbound_stream_verification.py -q
```

Before merge:

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Run the OpenSSH regression only if SSH-specific code/feature boundaries are
changed; ordinary metadata propagation through the shared TCP first-hop path
must not gratuitously touch SSH implementation.

Record exact final SHA and test counts/results in this plan.

---

## Expected files touched

Likely:

```text
crates/eggress-core/src/connector.rs
crates/eggress-core/src/chain.rs
crates/eggress-outbound/src/connector.rs
crates/eggress-outbound/README.md
crates/eggress-embed/tests/outbound_detailed.rs
crates/eggress-embed/tests/public_api.rs
python/tests/test_outbound_stream_verification.py
architecture/outbound.md
docs/RUST_API.md
plans/OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md
```

Possibly existing local testkit files if they already provide the smallest
proxy fixture. Do not add a new runtime crate or dependency for this work.

---

## Non-goals

- no resolver injection API;
- no downstream authorization callback;
- no Eggsec-specific type or mode;
- no change to proxy hostname acceptance or DNS security policy;
- no retry/fallback policy;
- no listener/server lifecycle change;
- no UDP metadata redesign;
- no QUIC/H3 metadata project;
- no Unix-address projection into `SocketAddr`;
- no raw `TcpStream` public return;
- no `BoxStream` downcast API;
- no change to typed connect error taxonomy;
- no feature/default change;
- no pproxy compatibility claim change;
- no version bump, crates.io publish, or release tag in this implementation
  phase.

The separately registered release-qualification plan prepares the next
immutable patch after this implementation is green.

---

## Stop conditions

Stop and record a blocker rather than forcing the change if:

- obtaining metadata would require downcasting `BoxStream`;
- the only implementation would duplicate the chain algorithm;
- the existing `Connector` trait would need a breaking signature change;
- direct/chain error behavior changes;
- TCP metadata cannot be captured before boxing without leaking concrete
  transport types through public proxy APIs;
- a proposed implementation reports configured/resolved guesses instead of
  actual socket values.

A non-TCP first hop returning `None` is an expected supported outcome, not a
blocker.

---

## Acceptance criteria

This phase is complete only when:

1. direct TCP success populates actual `OutboundInfo.local_addr`;
2. direct TCP success populates actual `OutboundInfo.peer_addr`;
3. TCP-backed proxy-chain success populates actual first-hop local address;
4. TCP-backed proxy-chain success populates the actual first-hop peer address;
5. chain metadata survives TLS/protocol wrapping without changing its meaning;
6. multi-hop peer metadata identifies hop 0;
7. no second DNS lookup exists solely to manufacture `peer_addr`;
8. Unix/non-TCP first hops return `None` rather than fabricated TCP values;
9. existing public connection signatures remain unchanged;
10. existing `ChainExecutor::execute()` and DirectConnector compatibility
    paths delegate to one implementation authority;
11. no BoxStream downcast or unsafe code is introduced;
12. timeout, cancellation, typed-error, redaction, and no-direct-fallback
    behavior remain unchanged;
13. Python direct `peername` and `sockname` are proven populated from the
    native metadata path;
14. focused core/outbound/embed tests are green;
15. all required outbound feature slices compile;
16. workspace Clippy and tests are green;
17. docs define truthful metadata semantics;
18. completion record contains the final SHA and identifies any transport
    classes that legitimately still report `None`.

## Completion record

Implementation is present in the working tree. Focused qualification passed:

- `cargo test -p eggress-core --locked`: 119 passed;
- `cargo test -p eggress-outbound --locked`: 15 passed;
- `cargo test -p eggress-embed --locked --test outbound_detailed`: 23 passed;
- `cargo test -p eggress-embed --locked --test public_api`: 5 passed;
- outbound no-default base, `toml`, `pproxy-compat`, `ssh`,
  `ssh,pproxy-compat`, and `udp` compile slices passed;
- Python outbound metadata suite: 43 passed, 11 optional tests skipped.

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --
-D warnings`, and `cargo test --workspace --locked` passed. The CLI optional
feature build (`full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon`),
embed `ssh`, `pproxy-compat`, and combined feature checks, required OpenSSH
regression (3 passed), and standalone fuzz compile also passed. Python's
outbound metadata suite passed (43 passed, 11 optional skips).

Direct IPv4/IPv6 and local-bind tests verify the actual socket values. Detailed
embed tests cover direct TCP and HTTP CONNECT chain metadata. `ChainExecutor`
captures the first-hop socket metadata, including multi-hop identity, before
wrapping; `resolve_endpoint_addr()` was removed, so metadata does not trigger a
second DNS lookup. Unix and other non-TCP first hops continue to return `None`.
Existing connection signatures and the boxed stream boundary are preserved.

Phase 1 is fully qualified. Final implementation SHA will be recorded after
the implementation commit is created.
