# API Boundary Phase 5 — Rust Public API Qualification

## Status

**IMPLEMENTED — 2026-09-21**

Implementation landed in `c15c60b189d0588d254011e877b78b2ca8a6b3a9`. Residual post-implementation findings were closed by [`API_BOUNDARY_CORRECTIVE_CLOSURE.md`](API_BOUNDARY_CORRECTIVE_CLOSURE.md).

## Parent

[`API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md)

## Baseline

`2c9d064794a2b74831979f7f36fb25e4f5992707`

## Objective

Qualify and document the existing Rust library exposure after the outbound extraction and boundary cleanup so future maintenance can distinguish preferred facades from lower-level published contracts without removing or changing current APIs.

This is not an API redesign.

## Current state

The workspace publishes 28 crates. Several low-level crates intentionally expose reusable APIs, while higher-level consumers are generally expected to use:

- `eggress-embed` for full in-process service lifecycle;
- `eggress-outbound` (or the compatibility re-export `eggress_embed::outbound`) for listener-free chains;
- `eggress-relay` for generic byte relay;
- protocol/transport/routing crates when a consumer explicitly wants those lower-level libraries.

At the same time, published APIs include cross-crate types such as `EggressConfig::from_compiled(eggress_config::RuntimeConfig, ...)`. These already constrain internal refactoring and must not be treated as private merely because a facade exists.

## Workstream 1 — Inventory public exposure by crate

For every published crate, classify public items into one of:

- **primary supported surface** — intended direct downstream use;
- **supporting library surface** — lower-level but legitimate downstream API;
- **compatibility re-export** — retained to preserve source compatibility while implementation authority moved;
- **implementation-facing public seam** — currently public because crates compose through it; still part of semver until a separately approved major-version strategy exists.

This classification is documentation only. It does not authorize changing visibility.

Pay particular attention to:

- `eggress-embed`;
- `eggress-outbound`;
- `eggress-server`;
- `eggress-runtime`;
- `eggress-config`;
- `eggress-routing`;
- `eggress-core`;
- `eggress-relay`;
- protocol and transport crates.

## Workstream 2 — Freeze representative source-compatibility contracts

Add lightweight compile tests/examples using the existing public paths most likely to be consumed externally.

Cover at minimum:

### eggress-embed

- `EggressConfig::from_toml_str`;
- `from_compiled`, `compiled`, `into_compiled`;
- `EggressService::new`, blocking/async start signatures;
- `EggressHandle` status/reload/shutdown;
- `eggress_embed::outbound::{OutboundConnector, OutboundConnectErrorKind, OutboundConnectStage}`.

### eggress-outbound

- `OutboundConnector::direct`;
- `from_chain`;
- TOML constructor under its feature;
- pproxy constructor under its feature;
- TCP compatibility + detailed error methods;
- UDP association under `udp`.

### lower-level reusable crates

Representative construction/use for:

- `eggress-relay`;
- `eggress-core::TargetAddr` / stream aliases / chain interfaces that are already public;
- `eggress-routing` model/router exports;
- `eggress-config::RuntimeConfig` and validation entry point;
- compatibility re-exports from `eggress-server` that downstream code may already import.

Tests should compile and execute only enough to prove paths/signatures; do not build another exhaustive API snapshot framework.

## Workstream 3 — Qualify feature slices

The public API depends heavily on feature topology.

Retain the existing required checks and add missing narrow compile fixtures only where Phase 1–4 changes exposed a gap.

Required slices include:

```text
eggress-outbound:
  base
  toml
  pproxy-compat
  ssh
  ssh + pproxy-compat
  udp

eggress-embed:
  ssh
  pproxy-compat
  ssh + pproxy-compat
  default/full
```

Also confirm optional `quic`, `legacy-crypto`, and `pproxy-legacy` still expose their existing symbols through the established feature gates where touched.

Do not substitute `--all-features` for the repository's deliberate feature matrix.

## Workstream 4 — Clarify facade vs authority in documentation

Update maintained architecture/README text so downstream users can tell:

- where implementation authority lives;
- which compatibility re-export path remains supported;
- when to depend directly on `eggress-outbound` vs `eggress-embed`;
- that `RuntimeConfig` crossing the embed boundary is an established supported coupling;
- that `eggress-server` / `eggress-runtime` public items are not permission for internal code to duplicate their functionality elsewhere.

Avoid language such as “internal” for a public item if that would imply downstream use is unsupported after it has already been published without such qualification.

## Workstream 5 — Add a low-maintenance API reggression gate

Prefer ordinary compile tests checked by the existing Rust toolchain.

Do not make release/CI depend on `cargo-semver-checks`, `cargo-public-api`, nightly rustdoc JSON, or a generated API database unless the repository separately chooses that tooling.

A small set of representative contract tests plus feature-slice CI is sufficient for this campaign.

If maintainers later want automated semver diff tooling, write a separate tooling proposal rather than embedding it here.

## Workstream 6 — Review doc(hidden) and compatibility re-exports without removal

Inspect seams such as shared classifier/target conversion and re-exported executor helpers.

Where an item is already public:

- do not make it private;
- do not move it;
- do not change its type;
- use `#[doc(hidden)]` only if that attribute already reflects the intended contract or adding it is demonstrably documentation-only and does not conflict with current published docs.

The safest default in this campaign is documentation and contract testing, not visibility changes.

## Verification

```bash
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp

cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Run the required OpenSSH reggression when SSH-facing contract tests are touched.

## Stop conditions

This phase must not:

1. remove/deprecate an existing public item;
2. move a public item to another crate;
3. change a public feature name/default;
4. introduce a new mandatory semver-analysis tool;
5. declare a previously public supported path “internal only” without a separately approved compatibility strategy.

If the inventory reveals an API that should eventually be reduced, record it as future major-version debt only.

## Acceptance criteria

- [ ] Published crates have a maintained public-surface classification.
- [ ] Representative compile contracts cover the primary embed/outbound and lower-level library paths.
- [ ] Existing compatibility re-exports remain source-compatible.
- [ ] Cross-crate public types such as `RuntimeConfig` remain usable through current paths.
- [ ] Required feature slices compile.
- [ ] No public item, feature, or method is removed, renamed, moved, or signature-changed.
- [ ] Documentation clearly distinguishes implementation authority from supported import paths.
- [ ] No new mandatory API-diff tooling is added to ordinary CI.
- [ ] Workspace gate is green.
