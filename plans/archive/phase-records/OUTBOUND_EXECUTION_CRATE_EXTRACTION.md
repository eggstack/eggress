# Outbound Execution Crate Extraction

## Status

**READY FOR IMPLEMENTATION — 2026-09-18**

## Target repository

`eggstack/eggress`

Planning code baseline:

`0d348977974d41fb08675504181009569e3273c4` (`main`, Eggress 1.0.7)

Parent roadmap:

- `plans/OUTBOUND_FACADE_DEPENDENCY_CLEANUP_ROADMAP.md`

The roadmap commit itself is plan-only and does not change the implementation baseline.

## Objective

Extract Eggress's reusable listener-free outbound execution stack into a new public crate, tentatively named:

`eggress-outbound`

The crate should own the concrete proxy-hop composition that currently lives under `eggress-server` and the listener-free `OutboundConnector` implementation that currently lives under `eggress-embed`.

The extraction must preserve existing server behavior and preserve the established `eggress_embed::outbound::*` compatibility path.

The target ownership is:

```text
eggress-core
    ^
    |
eggress-outbound
    |-- concrete HopHandler implementations
    |-- ChainExecutor factory / TLS composition
    |-- typed failure classification
    |-- OutboundConnector
    |-- optional UDP
    |-- optional config/pproxy constructors
    |
    +--------> eggress-server
    |
    +--------> eggress-embed (compatibility re-export + full service)
```

This crate must remain general-purpose. It must not contain EggPool, Eggfetch, provider-account, LLM, retry-policy, or downstream-specific concepts.

---

# Current ownership to change

At the planning baseline:

- `eggress-core::chain` correctly owns the generic `ChainExecutor`, `HopHandler`, chain error types, TLS-wrapper seam, and boxed stream boundary.
- `eggress-server/src/execute/hops.rs` owns concrete HTTP/SOCKS/raw/Unix/H2 and optional extended/legacy/SSH/QUIC/H3 hop handlers.
- `eggress-server::execute::build_chain_executor()` constructs the concrete handler registry and TLS wrapper.
- `eggress-server::classify` owns the shared typed outbound error classifier.
- `eggress-embed/src/outbound.rs` owns `OutboundConnector`, detailed errors, pproxy/TOML construction, direct TCP execution, and listener-free UDP.
- `OutboundConnector` calls `eggress_server::build_chain_executor()` and `eggress_server::classify::*`, which forces the listener-free facade through the server crate.
- `eggress-embed` unconditionally depends on service-oriented crates even when a consumer only needs listener-free outbound TCP.

The generic executor in `eggress-core` is already at the correct layer. Do not move it again. Extract only concrete outbound composition and the listener-free facade.

---

# Workstream 0 — Refresh baseline and capture dependency evidence

Before moving code:

1. Re-read current `Cargo.toml`, `crates/eggress-embed/Cargo.toml`, `crates/eggress-server/Cargo.toml`, `architecture/{core,server,embed}.md`.
2. Confirm no `eggress-outbound` crate has already been introduced.
3. Confirm current 1.0.7 SSH cache behavior remains in `build_outbound_executor()`.
4. Confirm detailed typed errors remain implemented and no consumer string parsing is required on current `main`.
5. Capture current dependency trees for comparison:
   - `cargo tree -p eggress-embed -e features --no-default-features --features pproxy-compat`;
   - `cargo tree -p eggress-embed -e features --no-default-features --features ssh,pproxy-compat`;
   - `cargo tree -p eggress-server -e features --no-default-features`;
   - `cargo tree -p eggress-server -e features --no-default-features --features extended,ssh,pproxy-legacy,legacy-crypto`.
6. Record the current package count for the relevant slices.
7. Record the current `eggress-embed` public outbound signatures with rustdoc or source references.

Do not use the full workspace lockfile package count as the only footprint metric. Feature-specific trees are the useful baseline.

---

# Workstream 1 — Add the new workspace crate

Create:

```text
crates/eggress-outbound/
  Cargo.toml
  README.md
  src/
    lib.rs
    connector.rs
    executor.rs
    hops.rs
    classify.rs
    ...
```

Exact file names may vary, but keep responsibilities visible.

Add the crate to:

- workspace `members`;
- `[workspace.dependencies]` with the normal exact internal version pin;
- release/publishing topology later in the closure plan.

Package metadata should follow existing library conventions:

- workspace version;
- edition 2021;
- MSRV 1.85;
- `MIT OR Apache-2.0`;
- repository link;
- network/asynchronous categories;
- `[lints] workspace = true`.

The crate must be independently packageable.

## Base dependency profile

The base TCP profile should include only the packages actually needed for ordinary chain execution, expected to include:

- `eggress-core`;
- `eggress-uri`;
- `eggress-protocol-http`;
- `eggress-protocol-socks`;
- `eggress-transport-tls`;
- Tokio/tracing/error utilities required by implementation.

Do not depend on:

- `eggress-runtime`;
- `eggress-server`;
- `eggress-metrics`;
- `eggress-admin`;
- `eggress-system-proxy`;
- `eggress-udp` unless the `udp` feature is enabled.

Do not add a direct Eggfetch dependency.

---

# Workstream 2 — Define feature ownership before moving behavior

Prefer a small direct-crate feature graph rather than copying `eggress-embed/full`.

A recommended semantic model is:

```toml
[features]
default = []

toml = ["dep:eggress-config"]
pproxy-compat = ["dep:eggress-pproxy-compat"]
udp = ["dep:eggress-udp"]

extended = [
  "dep:eggress-protocol-shadowsocks",
  "dep:eggress-protocol-trojan",
  "dep:eggress-protocol-websocket",
]

pproxy-legacy = [
  "extended",
  "eggress-protocol-shadowsocks/pproxy-legacy",
]

legacy-crypto = [
  "extended",
  "eggress-protocol-shadowsocks/legacy-crypto",
]

ssh = ["dep:eggress-transport-ssh"]
quic = [
  "dep:eggress-transport-quic",
  "dep:eggress-protocol-h3",
]
```

Use exact feature forwarding required by current protocol crates; the sketch above is semantic, not permission to guess manifest edges.

## Important feature rules

- `pproxy-compat` must not automatically enable SSH.
- `ssh` must not automatically enable pproxy compatibility.
- pproxy-style SSH remains available only when both features are enabled, preserving current behavior.
- `udp` must be independent of TCP.
- `toml` must be independent of pproxy syntax.
- `legacy-crypto` remains explicitly opt-in.
- `quic` remains explicitly opt-in.
- No insecure/test-only transport feature belongs in normal defaults.

If H2 is currently part of the ordinary outbound registry, preserve it in the base profile unless source inspection proves a clean optional boundary that does not change existing default behavior.

---

# Workstream 3 — Move the concrete hop registry below the server

Move the reusable outbound code from:

`crates/eggress-server/src/execute/hops.rs`

into `eggress-outbound`.

This includes the protocol-specific `HopHandler` implementations needed for chain execution:

- HTTP CONNECT;
- HTTP-only;
- SOCKS5;
- SOCKS4;
- raw;
- Unix;
- H2;
- feature-gated Shadowsocks;
- feature-gated Trojan;
- feature-gated WebSocket;
- feature-gated SSR;
- feature-gated SSH;
- feature-gated QUIC/H3.

Move the reusable target conversion/rewrite helpers with the handlers when they are outbound-only.

Do not move inbound accept/reply/session code.

## Visibility

The new crate may keep concrete handlers private if consumers only need the executor factory and connector.

Do not expose every protocol handler as public API without a concrete use case.

The stable public abstraction should remain:

- native chain input;
- executor/connector construction;
- boxed stream output;
- typed generic errors.

---

# Workstream 4 — Move chain-executor construction

Move the reusable body of:

`eggress_server::execute::build_chain_executor()`

into `eggress-outbound`.

The new owner must preserve:

- handler ordering;
- shared TLS client-config construction;
- ALPN-specific TLS wrapper caching;
- current per-hop insecure behavior under its existing gated policy;
- SSH session-cache injection;
- extended/legacy/QUIC feature behavior;
- current failure behavior when TLS configuration cannot be built.

Avoid making the builder take server-only metrics/config types.

## Server metrics seam

The current Shadowsocks handler optionally accepts `ShadowsocksMetrics`, whose type belongs to the protocol crate rather than `eggress-metrics`.

A reusable builder may expose a small options struct, conceptually:

```rust
pub struct OutboundExecutorOptions {
    pub tls_override: Option<Arc<rustls::ClientConfig>>,
    #[cfg(feature = "extended")]
    pub shadowsocks_metrics: Option<Arc<ShadowsocksMetrics>>,
    #[cfg(feature = "ssh")]
    pub ssh_sessions: Option<Arc<SshSessionCache>>,
}
```

Exact naming is flexible.

Do not make `eggress-metrics` a required dependency merely to preserve optional server instrumentation.

Do not expose a huge server `ConnectionConfig` through the outbound crate.

---

# Workstream 5 — Move the typed classifier

Move the reusable type-based classifier from:

`eggress-server/src/classify.rs`

into `eggress-outbound`, or into another already-correct lower-level crate only if dependency direction proves better.

Preferred location:

`eggress_outbound::classify`

because it classifies concrete outbound protocol/transport errors and shares their feature gates.

Preserve:

- `ClassifiedKind`;
- `classify_io_kind`;
- `classify_connect_error`;
- `classify_handshake_source`;
- type-based/downcast-only behavior;
- no string heuristics;
- current protocol coverage.

The classifier may remain `#[doc(hidden)]` if it is only an internal shared seam.

## Server adaptation

Change `eggress-server::SessionOpenError` conversions to consume the shared classifier.

Do not duplicate the mapping back into the server.

If source compatibility requires keeping `eggress_server::classify` reachable, leave a temporary/internal re-export rather than a second implementation. Do not create two classifier authorities.

---

# Workstream 6 — Move `OutboundConnector` into the new crate

Move the implementation currently under:

`crates/eggress-embed/src/outbound.rs`

into `eggress-outbound`.

Preserve the established public types and semantics:

- `OutboundConnector`;
- `OutboundInfo`;
- `OutboundConnectError`;
- `OutboundConnectErrorKind`;
- `OutboundConnectStage`;
- `UdpAssociation` when UDP is enabled;
- `OUTBOUND_MAX_DATAGRAM_SIZE` where applicable.

Preserve legacy and detailed TCP methods exactly at the behavior level.

## Internal state cleanup

Do not retain the full `RuntimeConfig` solely to execute one chain.

Replace the runtime-config field with a small outbound route representation, conceptually:

```rust
enum OutboundRoute {
    Direct,
    Chain(Arc<eggress_uri::ProxyChainSpec>),
}
```

plus only metadata needed to preserve existing methods such as `upstream_count()`.

Exact type placement depends on current `ProxyChainSpec` ownership.

For `from_toml`:

1. parse/validate/compile with the canonical Eggress config boundary;
2. preserve current outbound-specific checks;
3. extract the first upstream's chain into the outbound-owned route;
4. record the original upstream count if `upstream_count()` must preserve current semantics;
5. drop the full compiled service configuration.

For `from_pproxy_uri`:

1. parse through the current pproxy compatibility parser;
2. preserve redaction and fail-closed validation;
3. compile directly to native `ProxyChainSpec`;
4. store that chain directly;
5. do not synthesize a `RuntimeConfig`.

This change is important for making `toml` an optional construction feature rather than an execution dependency.

---

# Workstream 7 — Add a native-chain constructor

Add an additive typed constructor for general Rust consumers that already have a compiled Eggress chain.

Conceptually:

```rust
pub fn from_chain(chain: ProxyChainSpec) -> Result<Self, OutboundConstructionError>
```

or a small builder equivalent.

Requirements:

- reject empty/invalid chains using the same chain validation rules;
- preserve direct-vs-proxied semantics explicitly;
- do not silently turn invalid chains into direct routes;
- do not require TOML;
- do not require pproxy compatibility;
- do not expose server/runtime types.

If direct routing cannot naturally be represented by `ProxyChainSpec`, expose an explicit `direct()` constructor rather than encoding a fake chain.

Do not remove the existing constructors.

---

# Workstream 8 — Feature-gate UDP cleanly

Move listener-free UDP implementation with the connector, but compile it only when `udp` is enabled in `eggress-outbound`.

When `udp` is off:

- `eggress-udp` must not appear in the direct crate's dependency graph;
- TCP APIs must compile normally;
- no placeholder UDP behavior should silently fall back to TCP/direct routing.

For `eggress-embed`, preserve the existing public outbound UDP methods by enabling the outbound crate's `udp` feature in the compatibility dependency profile.

Do not redesign UDP protocol support in this extraction.

---

# Workstream 9 — Make `eggress-server` consume the extracted crate

Add an internal dependency:

`eggress-server -> eggress-outbound`

with default features disabled.

Forward server features to matching outbound features where needed:

- `extended`;
- `pproxy-legacy`;
- `legacy-crypto`;
- `ssh`;
- `quic`;
- test-only insecure TLS only if the existing test architecture genuinely requires it.

Then:

- delete/move the local concrete hop registry;
- delete/move the local chain-executor factory;
- update imports;
- map `SessionOpenError` via the shared classifier;
- preserve all inbound/session behavior.

The server must not depend on `eggress-embed`.

Avoid feature cycles.

---

# Workstream 10 — Preserve the `eggress-embed` API path

Replace the implementation module with a compatibility facade.

Preferred shape:

```rust
pub mod outbound {
    pub use eggress_outbound::*;
}
```

or an equivalent explicit re-export list if that produces better rustdoc/API control.

The following downstream source must continue to work:

```rust
use eggress_embed::outbound::{
    OutboundConnector,
    OutboundConnectErrorKind,
};
```

The full `eggress-embed` crate may continue to depend on runtime/server/metrics for `EggressService` and `EggressHandle`; this plan is not trying to make the full-service facade dependency-free.

Its outbound implementation, however, must have a single authority in `eggress-outbound`.

## Feature forwarding

Map current embed features to the new crate so existing build selections retain behavior.

In particular:

- `pproxy-compat` -> outbound `pproxy-compat`;
- `pproxy-legacy` -> outbound `pproxy-legacy`;
- `legacy-crypto` -> outbound `legacy-crypto`;
- `ssh` -> outbound `ssh`;
- `quic` -> outbound `quic`;
- existing embed behavior keeps outbound UDP available.

Do not silently remove methods from current embed feature combinations.

---

# Workstream 11 — Tests to relocate and add

Move tests to the crate that now owns the behavior.

## Outbound crate

The new crate should own tests for:

- direct TCP connect;
- SOCKS4/5;
- HTTP CONNECT;
- HTTP-only rewrite behavior that is specifically hop-owned;
- multi-hop execution;
- TLS wrapper;
- detailed error classification;
- pproxy construction/redaction;
- native-chain construction;
- TOML construction when enabled;
- feature-gated extended/legacy protocols;
- SSH session reuse/host-key behavior;
- optional UDP;
- cancellation/failure recovery.

Reuse current tests rather than rewriting equivalent coverage from scratch.

## Server

Retain server-level tests proving:

- inbound routing still opens direct/chained routes;
- failure categories/replies still map correctly;
- HTTP forward/tunnel behavior is unchanged;
- server metrics lifecycle remains correct.

Server tests should treat the outbound crate as a dependency, not re-test every protocol implementation.

## Embed

Retain focused compatibility tests proving:

- `eggress_embed::outbound::*` compiles and behaves through the re-export;
- current feature combinations still expose expected methods;
- full-service lifecycle remains unrelated and unchanged.

---

# Workstream 12 — Required compile matrix

Do not add a Cartesian product.

At minimum qualify these direct outbound slices:

```sh
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp
```

Also one broad optional compatibility slice equivalent to the repository's current CLI gate:

```text
extended + ssh + quic + pproxy-legacy + legacy-crypto + pproxy-compat
```

Do not enable test-only insecure QUIC in the normal broad slice.

Existing embed compile slices and the required OpenSSH regression must remain green.

---

# Security and redaction invariants

The extraction must not regress:

- credential redaction in malformed pproxy expressions;
- typed error Display/Debug secrecy;
- no raw credential-bearing URI in errors;
- verified native SSH host-key policy;
- compatibility SSH policy only through the explicit compatibility constructor;
- no proxy failure -> direct fallback;
- private/reserved-target rules already owned by the core connector;
- bounded HTTP-only buffering;
- bounded UDP payloads where UDP is enabled.

Moving code between crates is not permission to relax these invariants.

---

# Documentation

Update:

- `architecture/overview.md`;
- `architecture/core.md`;
- `architecture/server.md`;
- `architecture/embed.md`;
- add `architecture/outbound.md`;
- workspace README Rust-library section;
- `crates/eggress-embed/README.md`;
- new `crates/eggress-outbound/README.md`.

Durable docs should explain ownership, not the implementation campaign history.

Document that:

- `eggress-outbound` is the direct listener-free Rust dependency;
- `eggress-embed` remains the full-service facade and re-exports the outbound API;
- feature selection is explicit;
- typed error facts are diagnostic, not retry recommendations.

---

# Acceptance criteria

- [ ] New `eggress-outbound` workspace crate exists and packages independently.
- [ ] Base outbound crate has no dependency on `eggress-server`, `eggress-runtime`, `eggress-metrics`, `eggress-admin`, or `eggress-system-proxy`.
- [ ] `eggress-udp` is absent unless the outbound `udp` feature is selected.
- [ ] `eggress-config` is not required for pproxy/native-chain execution when TOML construction is not selected.
- [ ] Concrete hop handlers have one implementation authority in `eggress-outbound`.
- [ ] Chain-executor/TLS composition has one implementation authority in `eggress-outbound`.
- [ ] Typed outbound classification has one implementation authority in `eggress-outbound`.
- [ ] `eggress-server` consumes those authorities.
- [ ] `OutboundConnector` implementation lives in `eggress-outbound`.
- [ ] `eggress_embed::outbound::*` remains source-compatible.
- [ ] Current SSH session-cache behavior is unchanged.
- [ ] Current detailed error behavior is unchanged.
- [ ] A general native-chain constructor exists without server/runtime types.
- [ ] Full workspace tests pass.
- [ ] Required reduced feature slices pass.
- [ ] Required OpenSSH regression passes.
- [ ] No EggPool/Eggfetch-specific runtime concept is introduced.

## Stop conditions

Stop and revise if:

- the extraction creates a crate cycle;
- preserving server metrics requires pulling `eggress-metrics` into the minimal outbound crate;
- preserving the embed public path requires duplicating types instead of re-exporting them;
- native-chain construction cannot avoid service configuration without changing semantics;
- the direct outbound graph still contains `eggress-server` or `eggress-runtime`;
- protocol feature forwarding becomes materially different from existing runtime behavior;
- MSRV 1.85 cannot be preserved without a separately approved migration.

Do not solve any stop condition by adding downstream-specific feature flags.
