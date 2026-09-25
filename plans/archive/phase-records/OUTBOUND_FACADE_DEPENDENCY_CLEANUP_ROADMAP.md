# Outbound Facade Dependency Cleanup Roadmap

## Status

**READY FOR IMPLEMENTATION — 2026-09-18**

## Target repository

`eggstack/eggress`

Planning baseline:

`0d348977974d41fb08675504181009569e3273c4` (`main`, Eggress 1.0.7)

Relevant downstream observation baseline:

- EggPool `main` still pins Eggress `=1.0.6`.
- EggPool uses `eggress-embed` with `pproxy-compat`, `pproxy-legacy`, and `legacy-crypto`.
- EggPool also carries a default `eggress-ssh-fallback` feature with direct dependencies on `eggress-core`, `eggress-config`, `eggress-pproxy-compat`, `eggress-server`, `eggress-uri`, and `eggress-transport-ssh`.
- Provider HTTP framing/TLS/pooling is already owned by Eggfetch in EggPool; this roadmap is only about Eggress route/proxy ownership.

Before implementation, refresh both repository heads. Do not recreate work already present on current `main`.

## Objective

Make Eggress's listener-free outbound capability a first-class, small, general-purpose Rust dependency that can be consumed without pulling the full embedded-service/server/runtime graph.

The intended architecture is:

```text
eggress-core
  ChainExecutor / HopHandler / BoxStream / TargetAddr
        ^
        |
eggress-outbound                     new reusable outbound composition crate
  - concrete outbound hop handlers
  - TLS wrapper / chain-executor construction
  - typed outbound failure classifier
  - OutboundConnector TCP surface
  - optional TOML constructor
  - optional pproxy constructor
  - optional UDP association surface
  - optional SSH / extended / QUIC / legacy protocol support
        ^                     ^
        |                     |
eggress-server            eggress-embed
  inbound/session          full service lifecycle
  orchestration            + compatibility re-export of outbound API
```

A downstream that only needs listener-free routing should be able to depend directly on `eggress-outbound`. A downstream that already uses `eggress-embed::outbound::*` should continue to compile with the established API path.

This is a general Eggress layering improvement. EggPool is motivating evidence and a useful qualification consumer, not the semantic owner of the new API.

## Current-state findings

### 1. The former SSH facade gap is already closed

Eggress 1.0.7 already makes `OutboundConnector` own the SSH session cache:

- native/TOML mode uses `SshSessionCache::new()`;
- pproxy compatibility mode uses `SshSessionCache::new_compatibility()`;
- direct mode does not allocate SSH state.

The OpenSSH regression is already required in CI for the relevant embed feature slice.

Do not add a second SSH executor, EggPool-specific cache, or fallback API.

### 2. The former typed-error gap is already closed

Current `main` already exposes:

- `connect_tcp_detailed()`;
- `connect_tcp_timeout_detailed()`;
- `OutboundConnectError`;
- `OutboundConnectErrorKind`;
- `OutboundConnectStage`.

The implementation classifies direct and handshake failures without consumer-side display-string parsing.

Do not create another error taxonomy for this cleanup.

### 3. The remaining problem is crate layering and dependency reachability

`eggress-embed::outbound::OutboundConnector` is listener-free in behavior, but its crate has unconditional dependencies on service-oriented packages including:

- `eggress-runtime`;
- `eggress-server`;
- `eggress-metrics`;
- `eggress-udp`;
- `eggress-config`;
- routing/configuration support used by the full service.

The connector also constructs `ChainExecutor` through `eggress-server::build_chain_executor()`, and its typed classifier currently lives in `eggress-server::classify`.

This means a consumer that needs only a TCP route dialer still crosses a server/runtime boundary and resolves a substantially broader graph than the actual listener-free capability requires.

### 4. The concrete outbound registry is currently owned by the wrong layer

`crates/eggress-server/src/execute/hops.rs` contains reusable outbound hop implementations for HTTP, HTTP-only, SOCKS4/5, raw, Unix, H2, and feature-gated extended/legacy/SSH/QUIC/H3 protocols.

`build_chain_executor()` in `eggress-server::execute` constructs those handlers plus shared TLS wrapper state.

Those types are not intrinsically inbound-server concepts. The server is one consumer of the outbound chain engine.

The reusable concrete outbound composition should therefore move below `eggress-server`.

### 5. `OutboundConnector` retains more configuration than execution needs

The connector currently stores an `Arc<eggress_config::compile::RuntimeConfig>` and then always executes the first upstream chain.

For pproxy construction, the code builds a synthetic `RuntimeConfig` solely to retain one translated `ProxyChainSpec`.

The listener-free execution object should instead retain an outbound route/chain representation directly. TOML parsing can remain an optional constructor that compiles a normal Eggress config and extracts the selected chain.

This is important because it lets the direct pproxy/native-chain API avoid an unconditional `eggress-config` dependency.

## Program structure

This cleanup is split into two implementation plans.

1. `OUTBOUND_EXECUTION_CRATE_EXTRACTION.md`
   - create `eggress-outbound`;
   - move reusable hop composition and typed classification below the server;
   - move the listener-free connector into the new crate;
   - preserve `eggress_embed::outbound::*` as a compatibility re-export;
   - make the new crate feature-compositional rather than service-shaped.

2. `OUTBOUND_FACADE_FEATURE_AND_FOOTPRINT_CLOSURE.md`
   - close feature/dependency ownership;
   - update server/embed feature forwarding;
   - remove superseded direct dependencies;
   - update package/release order for the new crate;
   - measure the actual resolved graph and artifact impact;
   - document downstream adoption requirements.

These plans are sequential. Do not perform footprint/release closure until the extraction compiles and behavioral parity is established.

## Desired feature model

The exact Cargo spelling may change during implementation, but the semantic split should be approximately:

```text
eggress-outbound base
  TCP chain execution
  HTTP / HTTP-only
  SOCKS4 / SOCKS5
  raw / Unix
  H2
  TLS wrapping
  typed errors

optional:
  toml             -> Eggress TOML/config constructor + validation
  pproxy-compat    -> from_pproxy_uri()
  udp              -> associate_udp() family
  extended         -> Shadowsocks / Trojan / WebSocket
  pproxy-legacy    -> SSR compatibility
  legacy-crypto    -> legacy Shadowsocks ciphers
  ssh              -> SSH hop + session cache
  quic             -> QUIC / H3
```

Do not make the new crate's default feature set equivalent to full `eggress-embed`.

The direct crate should have a deliberately small default/base profile. `eggress-embed` may enable the features required to preserve its existing public surface.

## API compatibility rule

The following existing path must remain valid for normal `eggress-embed` consumers:

```rust
eggress_embed::outbound::OutboundConnector
eggress_embed::outbound::OutboundInfo
eggress_embed::outbound::OutboundConnectError
eggress_embed::outbound::OutboundConnectErrorKind
eggress_embed::outbound::OutboundConnectStage
eggress_embed::outbound::UdpAssociation
```

Existing method names and broad behavior must remain source-compatible, including:

- `from_toml`;
- `from_pproxy_uri` under compatibility features;
- `connect_tcp`;
- `connect_tcp_detailed`;
- timeout variants;
- UDP methods where they are currently available;
- `upstream_count`;
- `validate_outbound_config`.

The new direct crate may additionally expose a cleaner native chain constructor when useful, but do not require existing embed callers to migrate immediately.

## General-purpose native constructor

The extraction should strongly consider an additive constructor that accepts the already-compiled native chain representation, conceptually:

```rust
OutboundConnector::from_chain(ProxyChainSpec)
```

or an equivalent typed builder.

This avoids forcing general Rust consumers through TOML or pproxy compatibility syntax when they already own a native Eggress chain.

Do not expose `RuntimeConfig` as the minimal outbound contract.

## Dependency goals

For a direct `eggress-outbound` TCP consumer using ordinary HTTP/SOCKS routing, the resolved graph should not require:

- `eggress-runtime`;
- `eggress-server`;
- `eggress-metrics`;
- `eggress-admin`;
- `eggress-system-proxy`;
- reverse-listener machinery;
- UDP machinery unless `udp` is enabled.

For a pproxy TCP consumer without TOML construction, `eggress-config` should also be avoidable if the translated `ProxyChainSpec` can be retained directly.

Feature-gated protocols should pull only their current protocol/transport families.

Do not promise a byte reduction before measurement.

## Server ownership after extraction

`eggress-server` remains responsible for:

- inbound protocol acceptance/detection;
- listener/session orchestration;
- inbound authentication;
- route selection;
- HTTP forward behavior;
- failure replies;
- UDP ASSOCIATE server behavior;
- session metrics/finalization.

It should call the reusable outbound executor factory from `eggress-outbound` rather than owning the concrete hop registry itself.

Server-specific `SessionOpenError` remains server-owned. It may map from the shared outbound classifier.

## Embed ownership after extraction

`eggress-embed` remains responsible for:

- `EggressConfig`;
- `EggressService`;
- `EggressHandle`;
- lifecycle/start/reload/shutdown;
- full-service metrics/status;
- compatibility re-export of the listener-free outbound API.

The actual listener-free implementation should no longer live in `eggress-embed`.

## Explicit non-goals

Do not use this work to:

- redesign `ChainExecutor` unless extraction proves a small generic defect;
- add Eggfetch-specific `Dialer` traits to Eggress;
- add EggPool provider/account/routing concepts;
- change proxy URI grammar;
- change pproxy compatibility claims;
- change retry/fallback policy;
- add direct fallback after proxy failure;
- redesign SSH host-key policy;
- merge Eggress with Eggfetch;
- move inbound server behavior into the new outbound crate;
- rewrite UDP beyond the minimum needed to feature-gate it;
- change the public Python API;
- change CLI behavior;
- raise MSRV above 1.85 as part of this cleanup;
- force every protocol into the minimal feature set;
- delete `eggress-embed` or make it an outbound-only crate.

## Validation philosophy

Preserve the repository's deliberately lean verification policy.

This is a dependency/feature-boundary change, so the specialized checks are justified, but do not add a combinatorial CI matrix.

At minimum implementation should exercise:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0009
```

Plus bounded feature checks for the new outbound crate and the existing required OpenSSH regression.

## Release boundary

The new crate must be independently packageable and publishable through crates.io.

Update the manual publish topology so every internal dependency required by `eggress-outbound` is published before it, and every crate that depends on `eggress-outbound` is published after it.

Do not use a permanent path/git dependency for downstream adoption.

## Downstream handoff expectation

After the Eggress release containing this work is published, EggPool should be able to perform a separate downstream pass that:

1. bumps off Eggress 1.0.6;
2. deletes the 1.0.6 SSH fallback;
3. replaces display-string proxy classification with the published typed error surface;
4. evaluates direct `eggress-outbound` consumption instead of `eggress-embed`;
5. selects only the protocol features EggPool intentionally supports;
6. measures its own release binary and lockfile deltas.

Those changes belong in EggPool, not in Eggress runtime code.

## Program acceptance criteria

This roadmap is complete when:

- [ ] `eggress-outbound` exists as a general-purpose published crate.
- [ ] It owns concrete outbound hop composition and typed failure classification.
- [ ] It owns the listener-free `OutboundConnector` implementation.
- [ ] `eggress-server` consumes the shared outbound composition rather than defining a duplicate registry.
- [ ] `eggress-embed::outbound::*` remains source-compatible through re-export/adaptation.
- [ ] A TCP-only direct consumer does not resolve service/runtime/metrics/UDP ownership.
- [ ] TOML/config ownership is optional for consumers that construct from native or pproxy chains.
- [ ] SSH behavior remains the already-correct 1.0.7 behavior.
- [ ] Detailed typed errors remain the already-correct current behavior.
- [ ] Existing CLI/Python/full-service behavior is unchanged.
- [ ] Package/release ordering handles the new crate.
- [ ] Dependency and artifact measurements are recorded without unsupported size claims.
- [ ] Normal workspace and relevant feature/OpenSSH gates pass.
- [ ] No EggPool-specific runtime type or branch is added to Eggress.

## Stop conditions

Stop and revise the design rather than forcing extraction if:

- moving the concrete hop registry creates a dependency cycle;
- preserving `eggress_embed::outbound::*` would require incompatible public type changes;
- the new crate still necessarily depends on `eggress-server` or `eggress-runtime`;
- a supposed minimal feature unexpectedly activates most full-service dependencies;
- extraction changes pproxy compatibility semantics or SSH host-key/session behavior;
- the only way to support server metrics is to make service/metrics crates mandatory for all outbound consumers;
- package publication cannot be ordered without cyclic internal version dependencies.

In those cases, fix the ownership boundary first. Do not paper over it with downstream-specific feature flags.
