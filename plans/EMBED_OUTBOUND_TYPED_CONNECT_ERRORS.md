# Embed Outbound Typed Connect Errors

Planning baseline: `db56b099e599e29e71bc65d180034edf86391982` (`main`, 2026-09-17; workspace 1.0.7)
Status: complete
Implementation: `f69f25e` (`main`; `OutboundConnectError` + shared `eggress-server::classify`, SOCKS5 REP 0x05 typed refusal, `outbound_detailed` matrix; fmt/clippy/workspace + `full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon` check + embed `ssh`/`pproxy-compat` slices + OpenSSH regression green locally)

## Objective

Add an opt-in typed TCP connection-failure surface to `eggress-embed::OutboundConnector` so embedding consumers can distinguish route establishment failures without parsing `EggressError::Runtime` display strings.

The new surface must remain protocol-neutral and general-purpose. It should expose stable failure categories plus route-stage/hop provenance, while existing `connect_tcp()` and `connect_tcp_timeout()` behavior remains compatible for current consumers.

This is not an Eggpool-specific adapter. Eggpool is useful motivating evidence because it currently has to recover authentication/target/connect categories from redacted strings, but the API must be useful to any embedded consumer.

## Current state on the planning baseline

`OutboundConnector::connect_tcp()` currently collapses both direct and proxied failures:

```rust
DirectConnector::connect_with_options(...)
    .await
    .map_err(|e| EggressError::Runtime(e.to_string()))?;

self.chain_executor
    .execute(...)
    .await
    .map_err(|e| EggressError::Runtime(e.to_string()))?;
```

`connect_tcp_timeout()` similarly converts the outer timeout to:

```text
EggressError::Runtime("connection timed out")
```

The underlying layers contain substantially more structure:

- `ConnectError` distinguishes refusal, timeout, DNS, TLS, reserved-target and I/O failures;
- `ChainError::ConnectFailed` carries `hop_index`, endpoint and typed `ConnectError`;
- `ChainError::HandshakeFailed` carries `hop_index`, protocol and the boxed concrete protocol error;
- HTTP proxy errors distinguish authentication, refusal, bad gateway and gateway timeout;
- SOCKS5 errors distinguish authentication and connection refusal;
- `eggress-server::SessionOpenError` already has protocol-neutral timeout, DNS, refusal, network/host unreachable, upstream-authentication, policy and hop concepts.

However, `From<ChainError> for SessionOpenError` currently maps every `HandshakeFailed` source to `SessionOpenError::Other(source.to_string())`, so the typed HTTP/SOCKS information is lost at that boundary too.

## Important finding: no SSH session-cache implementation is needed

The 1.0.7 upstream already fixes the SSH facade limitation that existed in the 1.0.6 consumer snapshot.

`egress-embed::outbound::build_outbound_executor()` now supplies:

- `SshSessionCache::new()` for native mode;
- `SshSessionCache::new_compatibility()` for pproxy-compatible mode;
- no cache only for explicit direct mode.

Therefore this plan must **not** add another SSH executor, cache, fallback API, or Eggpool-specific SSH path. Downstream consumers should qualify and adopt 1.0.7+ instead.

The only SSH-related work permitted here is error-classification coverage for the existing stable facade when the `ssh` feature is enabled.

## Non-goals

Do not:

- change proxy URI syntax;
- change chain execution or routing decisions;
- change SSH session reuse behavior;
- specialize Eggress for Eggfetch/Eggpool;
- expose provider/router retry policy;
- add retries or fallback routes;
- make proxied failures fall back to direct networking;
- replace the public `EggressError` enum in this release line;
- change UDP error APIs in the same patch;
- redesign all protocol error enums;
- make consumers depend directly on `eggress-core`, `eggress-server`, or protocol crates solely to inspect the new embed error.

Keep the change additive and narrow.

## Compatibility rule

Existing methods are compatibility surfaces:

```rust
pub async fn connect_tcp(
    &self,
    host: &str,
    port: u16,
) -> Result<(BoxStream, OutboundInfo), EggressError>;

pub async fn connect_tcp_timeout(
    &self,
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<(BoxStream, OutboundInfo), EggressError>;
```

Do not change their return types in the 1.x line.

Add an opt-in detailed surface, preferably:

```rust
pub async fn connect_tcp_detailed(
    &self,
    host: &str,
    port: u16,
) -> Result<(BoxStream, OutboundInfo), OutboundConnectError>;

pub async fn connect_tcp_timeout_detailed(
    &self,
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<(BoxStream, OutboundInfo), OutboundConnectError>;
```

Exact names may be adjusted if repository conventions strongly prefer another suffix, but ordinary methods must remain source-compatible.

## Public typed error shape

Prefer a small stable embed-owned type rather than leaking `ChainError` or protocol-specific enums through `egress-embed`.

Suggested categories:

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundConnectErrorKind {
    Timeout,
    Dns,
    ConnectionRefused,
    NetworkUnreachable,
    HostUnreachable,
    Authentication,
    Tls,
    Protocol,
    Policy,
    Other,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundConnectStage {
    DirectConnect,
    HopConnect,
    HopHandshake,
    Deadline,
}
```

`OutboundConnectError` should expose, through fields or accessors:

- `kind()`;
- `stage()`;
- `hop_index() -> Option<usize>`;
- `protocol() -> Option<&str>` for handshake failures;
- a bounded redacted `Display` message suitable for logs.

A struct with private fields and accessors is preferred over a large public data-carrying enum because it leaves room to extend metadata without making every consumer exhaustively match internal details.

Do not expose proxy credentials, passwords, auth headers, full credential-bearing URIs, or raw configuration snippets in `Display`, `Debug`, protocol metadata or source chains.

### Why stage matters

A refusal while opening the TCP connection to hop 0 means something different from a proxy protocol handshake reporting that the requested destination was refused.

Downstream consumers should be able to distinguish:

```text
kind = ConnectionRefused, stage = HopConnect
```

from:

```text
kind = ConnectionRefused, stage = HopHandshake
```

without Eggress adding a downstream-specific `ProxyTargetConnect` category.

The consumer can map those generic facts into its own policy/error taxonomy.

## 1. Centralize one internal connection implementation

Do not implement detailed and legacy methods as two network paths.

Create one private connection routine that preserves the typed source until the final public mapping. Conceptually:

```rust
async fn connect_tcp_inner(...) -> Result<(BoxStream, OutboundInfo), ClassifiedOutboundError>;
```

where the private error retains enough information to produce both:

- `OutboundConnectError` for detailed methods;
- the current `EggressError::Runtime(...)` compatibility representation for legacy methods.

`connect_tcp()` and `connect_tcp_detailed()` must execute the same route construction and chain executor exactly once.

Likewise, implement timeout wrapping once and map the outer deadline to the appropriate public representation.

Do not create a second chain executor or duplicate route selection.

## 2. Classify direct `ConnectError` without strings

Direct connection errors are already typed. Map them before formatting.

Required baseline mapping:

```text
ConnectError::ConnectionRefused -> ConnectionRefused / DirectConnect
ConnectError::Timeout           -> Timeout / DirectConnect
ConnectError::DnsResolution     -> Dns / DirectConnect
ConnectError::TlsHandshake      -> Tls / DirectConnect
ConnectError::ReservedTarget    -> Policy / DirectConnect
ConnectError::Io(kind=ConnectionRefused) -> ConnectionRefused
ConnectError::Io(kind=TimedOut)          -> Timeout
ConnectError::Io(kind=NetworkUnreachable)-> NetworkUnreachable
ConnectError::Io(kind=HostUnreachable)   -> HostUnreachable
other I/O                                 -> Other
```

Use the available stable `std::io::ErrorKind` variants supported by the workspace MSRV. If a named variant is unavailable at Rust 1.85, keep the mapping conservative rather than raising MSRV for this plan.

Do not inspect `io::Error::to_string()` to infer categories.

## 3. Preserve chain-stage and hop provenance

For `ChainError`:

### `ConnectFailed`

Map the nested `ConnectError` using the typed mapping above and set:

- stage = `HopConnect`;
- `hop_index = Some(hop_index)`;
- protocol = `None` unless a stable protocol identifier is already available without reparsing config.

Do not expose the endpoint string if doing so risks leaking configuration details. Hop index is sufficient for the stable public contract.

### `HandshakeFailed`

Set:

- stage = `HopHandshake`;
- `hop_index = Some(hop_index)`;
- protocol to a bounded/redacted protocol label from the existing `ChainError` field;
- kind from a typed protocol-error classifier where possible.

### `EmptyChain` / `InvalidChain`

These are construction/policy failures rather than network establishment failures. Detailed methods should report `Policy` or `Protocol`/`Other` consistently; construction-time errors should still normally be caught by the connector constructors.

Do not turn an invalid chain into a direct connection.

## 4. Complete protocol handshake classification without changing `HopHandler`

`HopHandler` currently returns `Box<dyn Error + Send + Sync>`, and built-in handlers preserve concrete errors in that box. Do not break the public `HopHandler` trait in a 1.x corrective merely to improve embed diagnostics.

Instead, add one internal classifier for boxed handshake errors and reuse it from both:

- detailed `OutboundConnector` error mapping;
- `From<ChainError> for SessionOpenError`, so the existing server-side structured diagnostics stop throwing away the same information.

The classifier should inspect concrete built-in error types by type/downcast, not by message substring.

At minimum cover the built-in errors used by normal outbound chains:

### HTTP CONNECT

```text
HttpError::AuthRequired/AuthFailed -> Authentication
HttpError::ConnectionRefused       -> ConnectionRefused
HttpError::GatewayTimeout          -> Timeout
HttpError::BadGateway              -> Protocol or Other (do not pretend it is a local TCP refusal)
HttpError::Io                      -> classify io::ErrorKind
remaining HTTP parse/status errors -> Protocol
```

### SOCKS5

```text
Socks5Error::AuthFailed            -> Authentication
Socks5Error::ConnectionRefused     -> ConnectionRefused
Socks5Error::Io                    -> classify io::ErrorKind
method/protocol/malformed failures -> Protocol
```

### SOCKS4

Inspect its concrete error enum and map refusal/rejection and I/O failures where the protocol exposes them. Unsupported/malformed replies remain `Protocol`.

### TLS-wrapped hops / Trojan

A failure in the explicit chain TLS wrapping stage should classify as `Tls` when provenance is available. Trojan protocol/TLS errors should distinguish TLS or I/O where their concrete types permit it; do not infer authentication failure merely because a Trojan peer closes the stream after a bad password if the protocol implementation cannot prove that fact.

### SSH

When the `ssh` feature is enabled, map explicit SSH authentication failures to `Authentication`, connection timeout/refusal to their transport kinds, and protocol/session failures conservatively. Do not change session-cache construction or compatibility mode.

### Shadowsocks / SSR / WebSocket / H2/H3/QUIC

Classify typed transport/I/O conditions where their existing error types expose them. Unknown protocol-specific failures should become `Protocol` or `Other`; do not add message-string heuristics just to increase apparent coverage.

Feature-gate classifier arms/imports consistently with the protocol's existing build feature. Do not make a minimal embed feature pull in every optional protocol solely for error downcasting.

## 5. Reuse `SessionOpenError` concepts, but do not leak it as the embed contract

`eggress-server::SessionOpenError` is useful precedent and should share the same internal classification helper where practical.

Improve its `From<ChainError>` conversion so a built-in `HandshakeFailed` with typed HTTP/SOCKS/etc. source does not automatically become `Other(source.to_string())`.

However, do not require embed consumers to add a direct `egress-server` dependency or match `SessionOpenError` as the new stable API. `OutboundConnectError` belongs to `egress-embed` and should insulate embedding consumers from server-internal evolution.

If a cross-crate helper must be public for Rust visibility, keep it narrowly named/documented (or `#[doc(hidden)]` if repository policy allows) and avoid presenting it as a second end-user API.

## 6. Preserve legacy error behavior

Existing `connect_tcp()` callers may log, compare or test the current `EggressError::Runtime` messages. Do not silently replace legacy failures with a new `EggressError` variant in this plan.

The internal classified error should retain a sanitized compatibility display string so legacy methods can continue to return the same broad representation.

At minimum preserve these behavioral contracts:

- direct failures remain `EggressError::Runtime`;
- chain failures remain `EggressError::Runtime`;
- outer timeout remains `EggressError::Runtime("connection timed out")` unless existing tests demonstrate another established spelling;
- configuration/construction failures remain their existing `Config`/`UnsupportedFeature` categories;
- no credentials appear in messages.

If exact string parity is not formally tested today, avoid unnecessary message changes anyway.

## 7. Timeout ownership

`connect_tcp_timeout_detailed()` should distinguish the caller-supplied outer deadline from an underlying connection/proxy timeout.

Recommended mapping:

```text
outer tokio timeout -> kind=Timeout, stage=Deadline
DirectConnector timeout -> kind=Timeout, stage=DirectConnect
first/intermediate hop TCP timeout -> kind=Timeout, stage=HopConnect
proxy handshake timeout if protocol exposes it -> kind=Timeout, stage=HopHandshake
```

Do not create another timer inside `connect_tcp_inner()` beyond the existing timeout method.

Cancellation must remain cancellation by future drop; do not convert caller cancellation into a synthetic network error.

## 8. Redaction and security

Add explicit tests proving that typed errors are safe to log.

Use credentials containing recognizable sentinel strings and verify they are absent from:

- `Display`;
- `Debug` if `Debug` is public/derived;
- `protocol()`;
- compatibility `EggressError` mapping;
- nested source output if a public `source()` is exposed.

Do not include the original pproxy URI in the typed error.

Host/port for the final requested target are not inherently secret, but do not add them to the public error unless there is a concrete general need. `OutboundConnector` callers already know the target they requested.

## Tests

Add deterministic public-boundary tests for the detailed methods. Avoid testing only private classifier helpers.

Required matrix:

1. direct TCP refusal -> `ConnectionRefused`, `DirectConnect`;
2. direct DNS failure -> `Dns`, `DirectConnect`;
3. explicit outer timeout -> `Timeout`, `Deadline`;
4. HTTP CONNECT bad credentials -> `Authentication`, `HopHandshake`, correct hop/protocol;
5. HTTP CONNECT target refusal/bad gateway fixture -> handshake-stage non-auth category without string parsing;
6. HTTP CONNECT gateway timeout -> `Timeout`, `HopHandshake`;
7. SOCKS5 bad credentials -> `Authentication`, `HopHandshake`;
8. SOCKS5 target refusal -> `ConnectionRefused`, `HopHandshake`;
9. multi-hop chain failure reports the correct failing hop index;
10. malformed/unsupported protocol reply -> `Protocol` rather than accidental authentication/refusal;
11. chain TLS certificate/handshake failure -> `Tls` where provenance exists;
12. legacy `connect_tcp()` for the same failures still returns compatible `EggressError` categories/messages;
13. a failure followed by a valid request leaves the connector usable;
14. cancellation/drop during establishment does not poison the connector/session cache;
15. no credential sentinel appears in error formatting.

When features are enabled, add focused SSH and extended-protocol cases for categories those implementations can prove. Do not fabricate classification for protocols that do not expose it.

## Downstream-oriented acceptance fixture

A tiny test inside `egress-embed` should demonstrate the intended generic consumer pattern without importing Eggpool:

```rust
match connector.connect_tcp_detailed("example.invalid", 443).await {
    Err(error) if error.kind() == OutboundConnectErrorKind::Dns => { /* route policy */ }
    Err(error) => { /* generic failure */ }
    Ok(_) => { /* connected */ }
}
```

Also demonstrate that stage + hop lets a consumer distinguish proxy transport failure from proxy-reported destination failure without matching display strings.

Do not add an Eggfetch `Dialer` adapter to Eggress as part of this plan. Eggfetch already has a generic dialer seam; consumers can map the generic Eggress error themselves.

## Documentation

Update `egress-embed` README/API documentation to explain:

- ordinary methods remain the simple compatibility API;
- detailed methods provide stable failure categories for embedding/routing policy;
- `kind`, `stage`, hop and protocol are diagnostic facts, not retry recommendations;
- callers remain responsible for deciding retry/backoff/suppression policy;
- no direct fallback occurs on proxy failure;
- SSH session caching is already handled by the 1.0.7 facade when enabled.

Do not document downstream-specific mappings.

## Validation

Run the repository's current validation policy. At minimum:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

Also verify relevant reduced feature slices, especially:

- `egress-embed` common/non-extended build;
- pproxy compatibility build;
- `ssh` build;
- extended protocols;
- MSRV 1.85 if the repository currently qualifies it.

Do not raise MSRV for this work.

## Release/adoption note

This work should ship in a normal Eggress release after qualification. A downstream currently pinned to 1.0.6 should not carry a local patch for either problem:

- the SSH session-cache facade gap is already resolved in 1.0.7;
- the typed-error surface should be consumed from the first upstream release that contains this plan's implementation.

Downstream removal of compatibility fallbacks/string predicates belongs in that downstream repository and is not part of this implementation plan.

## Completion criteria

This plan is complete when:

- `OutboundConnector` has additive detailed TCP connect + timeout methods;
- the public detailed error exposes stable kind/stage/hop/protocol facts without protocol-specific dependencies for consumers;
- direct `ConnectError` classification is typed;
- built-in HTTP/SOCKS handshake authentication/refusal/timeout errors are classified without string matching;
- other built-in protocols classify typed failures where their current error surfaces permit it;
- multi-hop provenance reports the failing hop;
- `SessionOpenError` reuses the same handshake classifier rather than flattening every handshake source to a string;
- legacy `connect_tcp()` and `connect_tcp_timeout()` remain source-compatible and behaviorally compatible;
- error formatting is credential-safe;
- no route fallback/retry semantics change;
- SSH session-cache behavior remains the existing 1.0.7 implementation;
- full/reduced-feature/MSRV qualification is green;
- the implementation/release SHA is recorded here at closure.