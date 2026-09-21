# API Boundary Phase 1 — Configuration Authority Convergence

## Status

**IMPLEMENTED; CORRECTIVE CLOSURE OPEN — 2026-09-21**

Implementation landed in `c15c60b189d0588d254011e877b78b2ca8a6b3a9`. Residual post-implementation findings are tracked only in [`API_BOUNDARY_CORRECTIVE_CLOSURE.md`](API_BOUNDARY_CORRECTIVE_CLOSURE.md).

## Parent

[`API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md)

## Baseline

`2c9d064794a2b74831979f7f36fb25e4f5992707`

## Objective

Remove duplicated TOML parse/version/validate/compile implementations from `eggress-embed` and `eggress-outbound` so `eggress-config` is the single implementation authority, without changing any public constructor, reload behavior, error category, error text relied upon by current tests, feature gate, or capability.

## Current duplication

The same logical pipeline exists in three places:

- `eggress-config::validate_and_compile_toml()`;
- `eggress-embed::parse_validate_compile()`;
- `eggress-outbound::connector::parse_validate_compile()` behind feature `toml`.

Each performs:

```text
toml::from_str
→ optional version == 1 check
→ validate_config
→ compile_config
```

The facade-local versions flatten errors differently from `ConfigError::Display`. Consolidation therefore requires an explicit compatibility mapping, not a blind replacement with `eggress_config::validate_and_compile_toml(input).map_err(|e| e.to_string())`.

## Workstream 1 — Freeze existing facade error behavior

Before implementation, add focused tests at the public boundaries for representative failures:

- malformed TOML;
- unsupported `version`;
- validation failure;
- a compilation failure that reaches `compile_config()`, if one can be constructed independently of validation;
- valid minimal config;
- valid config with omitted version, preserving current accepted behavior.

Cover at least:

- `EggressConfig::from_toml_str`;
- `EggressHandle::reload_toml_str` where feasible without duplicating lifecycle setup;
- `OutboundConnector::from_toml`;
- `OutboundConnector::validate_outbound_config`.

Record exact current `EggressError::category()` / `OutboundError::category()` plus the stable message family. Tests should avoid over-freezing irrelevant parser line/column formatting that can legitimately vary with the TOML crate; freeze the facade-owned prefixes/categories and important content.

## Workstream 2 — Delegate compilation to eggress-config

Change the embed and outbound local helpers to call `eggress_config::validate_and_compile_toml()`.

Facade-specific adapters may pattern-match `eggress_config::ConfigError` to preserve the existing message representation:

- `Parse(source)`: retain the prior parser message rather than adding the config crate's `failed to parse TOML:` prefix unless existing facade tests already expect the prefix;
- `UnsupportedVersion(v)`: preserve `unsupported config version: {v}`;
- `Validation { message, .. }`: preserve the facade's current validation text rather than exposing an additional `configuration error at config:` prefix;
- compilation-originated `ConfigError`: preserve the current equivalent message.

Keep the mapping private to the facade unless there is already an appropriate shared non-public helper. Do not add a new public `ConfigError` formatting API solely for this migration.

After delegation, delete the duplicate parse/version/validate/compile bodies.

## Workstream 3 — Keep reload semantics unchanged

`EggressHandle::reload_toml_str()` must still:

- classify parse/validation/compile failure as `EggressError::Reload`, not `Config`;
- record reload failure metrics exactly once;
- preserve current generation on rejection/failure;
- reject listener topology changes exactly as before.

No reload transaction logic should move into `eggress-config`.

## Workstream 4 — Simplify dependency/feature topology only when proven

After the outbound duplicate parser is removed, inspect whether the direct optional `toml` dependency in `eggress-outbound` is still used.

If it is unused:

- remove the direct dependency;
- change the outbound `toml` feature so it enables only the dependencies still required for TOML construction;
- preserve the feature name `toml` and every existing feature combination.

Do not remove `toml` from `eggress-embed` if redaction/serialization still requires it.

Use `cargo tree -e features` before and after to confirm that no protocol or transport feature is accidentally activated.

## Workstream 5 — Correct architecture comments

Update comments/rustdoc that currently call multiple boundaries “canonical.” The maintained wording should be:

- `eggress-config`: canonical parse/version/validate/compile implementation;
- `eggress-embed`: stable service facade and error-category adapter;
- `eggress-outbound`: outbound-specific post-compilation validation/route extraction.

Do not rewrite historical plans that intentionally describe the pre-convergence state.

## Verification

Focused:

```bash
cargo test -p eggress-config
cargo test -p eggress-embed
cargo test -p eggress-outbound

cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp
```

Then:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Stop conditions

Stop and document rather than expanding scope if:

1. preserving the existing facade error contract would require adding a new public API to `eggress-config`;
2. a current facade intentionally accepts input rejected by `validate_and_compile_toml()`;
3. removing the outbound direct `toml` dependency changes an established feature slice unexpectedly.

In those cases, keep the narrowest adapter necessary and record why the remaining duplication cannot safely be removed.

## Acceptance criteria

- [ ] Embed and outbound no longer independently implement TOML parse/version/validation/compilation.
- [ ] `eggress-config::validate_and_compile_toml()` is the single implementation authority.
- [ ] Existing public constructors and reload methods retain their signatures.
- [ ] Existing facade error categories and redacted message families are preserved.
- [ ] Reload generation/metrics/listener-topology semantics are unchanged.
- [ ] Outbound feature `toml` still compiles in isolation.
- [ ] Any now-unused direct `toml` dependency is removed without adding a replacement dependency.
- [ ] Architecture comments identify one canonical compilation boundary.
- [ ] Workspace tests and feature-slice checks are green.
