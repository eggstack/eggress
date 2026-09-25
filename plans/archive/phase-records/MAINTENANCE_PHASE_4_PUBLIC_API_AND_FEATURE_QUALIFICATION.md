# Maintenance Phase 4 — Public API and Feature Qualification

## Status

**IMPLEMENTED — 2026-09-21**

## Parent

[`MAINTENANCE_CONVERGENCE_ROADMAP.md`](MAINTENANCE_CONVERGENCE_ROADMAP.md)

## Baseline

`fa1d9bf73c5c162c62a6086cf0e7361f1cfb3131`

## Objective

Strengthen low-maintenance regression protection for the already-published Rust library surface and make continuous feature-slice checks match the maintained contract, without changing visibility or adding a heavy API-database/tooling requirement.

This phase protects the existing API; it does not redefine it.

## Current state

`docs/RUST_API.md` classifies all published workspace crates as semver-relevant and identifies the preferred facade surfaces.

The existing `crates/eggress-embed/tests/public_api.rs` provides representative compile/use coverage for:

- embed configuration handoff;
- outbound authority plus embed compatibility re-export;
- selected relay/core/routing paths.

Normal crate tests cover lower-level implementation behavior, but they are not always downstream-shaped compile contracts.

CI currently checks several important no-default feature slices, but the documented API-boundary qualification includes an outbound `toml` slice that is not present in the current `.github/workflows/ci.yml` feature-boundary step.

## Governing constraints

1. Treat every currently published public item as semver-relevant.
2. Do not make an item private, `pub(crate)`, hidden, deprecated, renamed, moved, or signature-changed in this phase.
3. Do not change feature names/defaults.
4. Do not add a generated exhaustive API snapshot database.
5. Do not make ordinary CI depend on nightly rustdoc JSON.
6. Do not add mandatory `cargo-semver-checks` or `cargo-public-api` to normal push/PR CI.
7. Keep compile-contract fixtures representative rather than exhaustive.
8. Do not use `--all-features` as a substitute for deliberate feature-boundary checks.
9. Do not exercise external network/oracle integration merely for compile qualification.
10. Any future major-version API reduction belongs in a separate proposal.

## Workstream 1 — Extend downstream-style compile contracts

Add focused compile/use tests for important supporting surfaces that are currently classified as supported but not represented in `public_api.rs` or equivalent fixtures.

Cover representative, stable construction/use only; do not duplicate every unit test.

At minimum evaluate coverage for:

### `eggress-config`

- canonical TOML validation/compile entry point;
- public `RuntimeConfig` handoff shape already used by embed consumers.

### `eggress-runtime`

- public `ServiceSupervisor` start/state/shutdown-token signatures that are already published;
- compatibility startup types already exposed.

Tests must not start external services unless necessary; compile-time/type-level use is sufficient.

### `eggress-server`

- existing public session/config/metrics seam types and exported executor/result paths used by downstream crates.

### protocol/transport crates

Pick a small representative set of public constructors/types from the currently documented supported lower-level surfaces, especially where feature-gated:

- HTTP/SOCKS protocol types;
- TLS transport config/connector types;
- optional SSH and QUIC public symbols under their existing feature slices.

Do not create one giant integration test importing all 28 crates. Keep fixtures near the natural owning crate or in a small number of downstream-style contract tests.

## Workstream 2 — Preserve compatibility re-export contracts

Retain explicit compile evidence for:

- `eggress_embed::outbound::*`;
- public `eggress-server` re-exports;
- any compatibility path touched by Phase 1/2 file movement.

The implementation authority may move privately; public paths must not.

## Workstream 3 — Align CI with the documented feature slices

Update the existing bounded feature-boundary checks so CI continuously covers the documented required slices.

At minimum ensure:

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
```

The outbound `toml` check is specifically required because it is documented as part of public qualification but is missing from the current CI block.

Review whether a single compile-only QUIC slice should be retained/added for paths touched by this campaign. Do not expand into every possible cross-product.

## Workstream 4 — Add a release/manual semver observation only if zero-maintenance

The default implementation for this phase is compile contracts plus feature slices.

Optionally, if `cargo-semver-checks` can be invoked in an existing manual/release-preflight context without:

- pinning a fragile nightly toolchain;
- changing normal CI duration materially;
- creating an API baseline artifact committed to the repo;
- blocking development on false positives;

then document a manual/release command as an informational maintainer check.

It must not become required ordinary CI under this plan.

If those conditions are not satisfied, do not add it.

## Workstream 5 — Reconcile `docs/RUST_API.md`

Update the maintained classification only to reflect stronger qualification evidence and any private implementation movement from Phases 1–2.

Do not reclassify a published path as unsupported/internal.

Document that compile contracts are representative and that ordinary crate tests continue to own semantic behavior.

## Verification

Required feature checks:

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
```

Focused compile contracts:

```bash
cargo test -p eggress-embed --locked --test public_api
cargo test -p eggress-runtime --locked
cargo test -p eggress-server --locked
cargo test -p eggress-config --locked
```

Run additional protocol/transport crate checks only for contract fixtures actually added.

Final gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Stop conditions

Do not expand this phase into an API tooling project if:

1. representative compile contracts already catch the relevant paths;
2. a semver tool requires a nightly/API baseline maintenance burden;
3. optional feature combinations grow combinatorially;
4. a compile fixture needs production API additions solely to be testable.

Record uncovered major-version debt in `docs/RUST_API.md`; do not fix it by changing the current surface.

## Acceptance criteria

- [x] Representative supporting-crate public paths have downstream-shaped compile/use contracts.
- [x] Preferred embed/outbound/relay facade contracts remain covered.
- [x] Compatibility re-export paths touched by structural refactors remain covered.
- [x] The outbound `toml` no-default feature slice is enforced in CI.
- [x] Documented outbound and embed qualification slices agree with actual CI commands.
- [x] Optional QUIC/SSH/legacy checks remain bounded to touched/meaningful slices.
- [x] No public item or feature changes visibility, name, location, default, or signature.
- [x] No mandatory nightly/API-database/semver tool is added to ordinary CI.
- [x] `docs/RUST_API.md` accurately describes the qualification strategy.
- [x] Workspace fmt, clippy, and locked tests pass.

## Closure record

Implementation: added outbound `toml` slice to `.github/workflows/ci.yml` + `AGENTS.md` (now base/`toml`/`pproxy-compat`/`ssh`/`ssh,pproxy-compat`/`udp`; embed `ssh`/`pproxy-compat`/`ssh+pproxy-compat` unchanged); extended `eggress-embed/tests/public_api.rs` with `supporting_config_runtime_paths_compile` (config TOML compile, runtime supervisor/classify signatures) and `protocol_representative_paths_compile` (HTTP/SOCKS via URI + `HttpDetector`/`ConnectRequest`); added `eggress-server/tests/public_api.rs` (NoopMetrics, handles, reports, config/context, auth cache). No visibility/feature/signature change; no mandatory semver tool (manual `cargo-semver-checks` rejected as baseline burden); `docs/RUST_API.md` reconciled.

Evidence map (baseline `ba4102f68c3965a8cadb7634febd2d610e239e71`):

- supporting compile contracts → `crates/eggress-embed/tests/public_api.rs::supporting_config_runtime_paths_compile` and `::protocol_representative_paths_compile`; `crates/eggress-server/tests/public_api.rs`.
- facade contracts → existing embed/outbound/relay/core/routing representative coverage retained.
- re-export contracts → `eggress_embed::outbound::*` and `eggress-server` re-exports covered by the same compile tests after Phase 1/2 moves.
- outbound `toml` slice → `.github/workflows/ci.yml` feature-boundary step now includes `--no-default-features --features toml`; `AGENTS.md` documents base/`toml`/`pproxy-compat`/`ssh`/`ssh,pproxy-compat`/`udp`.
- slice agreement → documented outbound slices match CI commands; embed `ssh`/`pproxy-compat`/`ssh+pproxy-compat` unchanged and matching.
- bounded optionals → QUIC/SSH/legacy checks limited to touched/meaningful slices; no combinatorial matrix, no `--all-features`.
- no surface change → representative-only fixtures; public-surface policy in `docs/RUST_API.md` (documentation/qualification only).
- no heavy tooling → no mandatory nightly/API-database/semver gate in ordinary CI.
- focused evidence → `cargo test -p eggress-embed --locked --test public_api` (5 passed); `cargo test -p eggress-server --locked --test public_api` (1); `cargo test -p eggress-config/runtime/server` suites; feature slices checked; workspace fmt/clippy/locked tests green at implementation commit (remote CI `35646523487`).

(End of file - total 209 lines)
