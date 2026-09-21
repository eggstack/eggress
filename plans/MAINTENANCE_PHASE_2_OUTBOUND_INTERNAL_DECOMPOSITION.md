# Maintenance Phase 2 — Outbound Internal Decomposition

## Status

**PLANNED — 2026-09-21**

## Parent

[`MAINTENANCE_CONVERGENCE_ROADMAP.md`](MAINTENANCE_CONVERGENCE_ROADMAP.md)

## Baseline

`fa1d9bf73c5c162c62a6086cf0e7361f1cfb3131`

## Objective

Reduce edit collision and maintenance burden inside `eggress-outbound` by splitting the current `connector.rs` implementation into private cohesive modules while preserving the complete public `eggress_outbound::*` and `eggress_embed::outbound::*` surfaces and all runtime behavior.

No type is being redesigned and no public path is being moved.

## Current state

`crates/eggress-outbound/src/connector.rs` currently owns several distinct private domains behind the public connector facade:

- `OutboundInfo`;
- typed connect error kinds/stages and failure classification adapters;
- `UdpAssociation` lifecycle and SOCKS5/direct datagram handling;
- target/SOCKS address conversion and UDP resolution;
- `OutboundRoute` and executor-mode construction;
- TOML configuration adaptation;
- `OutboundConnector` constructors and TCP/UDP operations;
- pproxy chain redaction/scrubbing and compatibility error mapping;
- a large inline regression suite.

The implementation authority is correct. The problem is concentration, not overlap with another crate.

## Governing constraints

1. Every existing public item remains exported from the same crate/path with the same visibility and signature.
2. `eggress_embed::outbound::*` remains source-compatible.
3. Do not split into another crate.
4. Do not redesign `OutboundRoute`, `OutboundConnector`, `UdpAssociation`, typed error enums, or executor state.
5. Preserve direct fast paths, SSH policy selection, timeout behavior, hop/upstream counts, redaction, and fail-closed unsupported composition behavior.
6. Preserve listener-free UDP support exactly: direct plus the currently supported SOCKS5 form; do not expand capability.
7. Preserve the canonical configuration authority in `eggress-config`.
8. Preserve the pproxy native compilation authority in `eggress-pproxy-compat`.
9. Do not introduce generic utility modules whose only purpose is reducing file length.

## Workstream 1 — Establish private module boundaries

Preferred private decomposition:

- `error.rs` or `connect_error.rs`: typed outbound failure structs/enums and private classifier adaptation;
- `udp.rs`: `UdpAssociation`, UDP association inner state, direct/SOCKS5 send/recv, resolution/conversion/accounting;
- `compat.rs`: pproxy-specific constructor adaptation, credential-term extraction, redaction/scrubbing, compat error mapping;
- `connector.rs`: `OutboundConnector`, route representation, constructors, TCP execution, feature dispatch;
- existing `executor.rs` / `hops.rs` remain implementation authorities for their current domains.

Exact file names may differ. Do not create a module unless it has a stable conceptual owner and more than trivial code.

## Workstream 2 — Preserve public exports explicitly

Keep `crates/eggress-outbound/src/lib.rs` as the stable public facade.

After moving definitions, preserve all current re-exports including:

- `OutboundConnector`;
- `OutboundInfo`;
- `OutboundConnectError`;
- `OutboundConnectErrorKind`;
- `OutboundConnectStage`;
- `UdpAssociation`;
- `OUTBOUND_MAX_DATAGRAM_SIZE`;
- executor/public helper exports already present.

Add or retain compile-contract tests proving both:

```rust
use eggress_outbound::OutboundConnector;
use eggress_embed::outbound::OutboundConnector;
```

and the existing typed error/UDP paths continue to resolve under their feature gates.

## Workstream 3 — Keep TCP execution behavior single-sourced

`connect_tcp`, `connect_tcp_detailed`, timeout variants, and the private inner execution path must continue to share the same implementation authority.

Do not create separate compatibility and detailed execution engines while moving code.

Preserve:

- direct DNS/connect failure classification;
- chain failure provenance;
- outer deadline semantics;
- hop index/protocol labels;
- credential-free `Display`/`Debug`;
- no silent direct fallback.

## Workstream 4 — Keep UDP lifecycle/accounting exact

Move UDP code mechanically.

Freeze:

- connected fixed-target semantics;
- family-aware wildcard direct bind;
- SOCKS5 datagram encode/decode;
- max datagram rejection;
- cancellation/close idempotence;
- `active_udp_associations()` accounting on close/drop/timeout;
- unsupported multi-hop/composition errors;
- IPv4/IPv6 behavior.

Do not introduce pooling or listener-backed implementation.

## Workstream 5 — Keep compatibility redaction local to the compatibility boundary

The pproxy scrubber exists because malformed expressions may fail before canonical URI parsing can safely redact them.

When moving it:

- retain over-redaction preference;
- retain bracket-aware `@` handling and `#` auth fragment masking;
- do not replace tolerant fallback scrubbing with a parser-only path;
- keep all credentials absent from returned/displayed errors.

Do not attempt to make `eggress-uri` understand every malformed pproxy-only syntax merely to remove these helpers.

## Workstream 6 — Split tests only where it improves ownership

Move inline tests with their implementation domains where practical:

- constructor/TCP tests with connector;
- UDP tests with UDP module;
- redaction/compat tests with compatibility module.

Do not rewrite tests into an abstract test framework.

Keep externally visible behavior tests in existing integration suites.

## Verification

Core crate:

```bash
cargo test -p eggress-outbound --locked
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp
```

Facade and behavior:

```bash
cargo test -p eggress-embed --locked --test public_api
cargo test -p eggress-embed --locked --test outbound_detailed
```

If SSH code placement changes, run the required OpenSSH regression.

Final gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Stop conditions

Do not continue a split if it would:

1. move a public item to a different public module path;
2. require a new public re-export not already part of the surface;
3. introduce circular crate dependencies;
4. duplicate executor or classifier logic;
5. turn straightforward feature-gated code into dynamic dispatch solely to make files smaller.

A somewhat large `connector.rs` is acceptable if further extraction harms locality.

## Acceptance criteria

- [ ] Public `eggress-outbound` import paths and signatures are byte-for-byte/source-compatible at the API level.
- [ ] `eggress_embed::outbound::*` compatibility paths still compile.
- [ ] TCP construction/execution and typed error handling retain one implementation authority.
- [ ] UDP association lifecycle is privately isolated without capability or semantic changes.
- [ ] pproxy redaction/error adaptation is privately isolated without weakening credential safety.
- [ ] No new crate or user-facing API is introduced.
- [ ] Base/TOML/pproxy/SSH/SSH+pproxy/UDP feature slices compile.
- [ ] Existing outbound and embed focused tests pass.
- [ ] Workspace fmt, clippy, and locked tests pass.

## Closure record

Fill in place with implementation commit, final private module map, preserved re-export evidence, feature slices, and test results.
