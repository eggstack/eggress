# Maintenance Convergence Roadmap

## Status

**IMPLEMENTED — 2026-09-21**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `fa1d9bf73c5c162c62a6086cf0e7361f1cfb3131`
- Workspace release line: `1.0.7`
- Governing constraint: reduce residual feature overlap and maintenance burden while preserving the complete existing Rust, Python, CLI, configuration, protocol, transport, feature-gate, and compatibility surfaces.

## Purpose

The API-boundary and performance campaigns are complete. The current repository has clear implementation authorities for configuration compilation, outbound chain execution, generic stream relay, Python async lifecycle, exception identity, and pproxy compatibility translation.

This follow-up is therefore not another architecture rewrite. It addresses the remaining maintenance hot spots that are visible only after those earlier convergence passes:

1. `ServiceSupervisor::run()` still coordinates too many runtime lifecycle phases in one body despite the surrounding supervisor module decomposition.
2. `eggress-outbound/src/connector.rs` still combines several private implementation domains behind an otherwise healthy public facade.
3. `eggress-python` retains an architectural dependency on `eggress-cli` for operational pproxy test helpers. The edge may be removable through existing stable owner APIs, but must not be replaced with duplicated logic or a new public support API.
4. The published Rust surface is intentionally broad. Representative API qualification exists, but lower-level supporting crates and documented feature slices need stronger low-maintenance regression coverage.
5. Maintained Python documentation contains stale statements about Unix listeners and type stubs, and compatibility/native projection documentation should be reconciled with the implementation authorities that now exist.

The goal is lower cognitive and maintenance cost, not fewer features or fewer public symbols.

## Governing constraints

1. Do not remove, rename, move, deprecate, or signature-change any existing public Rust item, Python symbol, CLI option, configuration key, URI form, protocol/transport capability, compatibility tier, or feature name/default.
2. Do not add a new user-facing capability in this campaign.
3. Preserve pproxy behavior and compatibility claims. Any claim change requires a separate compatibility campaign and oracle/interop evidence.
4. Preserve listener startup/readiness semantics and shutdown ordering:
   readiness false -> listener stop -> UDP drain -> connection drain/cancel -> admin shutdown last.
5. Preserve reload authority in the existing runtime transaction. Listener topology remains startup-captured.
6. Preserve `eggress-relay` as the protocol-neutral relay authority and `eggress-core::relay` as the historical compatibility facade.
7. Preserve `eggress-outbound` as the listener-free chain execution authority and `eggress_embed::outbound::*` as the existing compatibility re-export.
8. Do not merge crates or add a new support crate merely to improve dependency aesthetics.
9. Do not add a public Rust item solely to remove an internal dependency edge.
10. Do not duplicate operational logic merely to avoid depending on another existing crate.
11. Do not change Python sync/async method shapes, exception identities, stubs, or import locations except to correct documentation/stub description of already-existing behavior.
12. Do not change protocol framing, TLS/SSH verification policy, UDP limits, routing policy, scheduler semantics, or error redaction.
13. Keep ordinary CI low-maintenance. Compile fixtures and targeted feature checks are preferred to a full combinatorial feature matrix or mandatory nightly/API-database tooling.
14. The intentionally unsupported capabilities recorded in `docs/CAPABILITIES.md` are non-goals here.

## Current-state findings

### Runtime supervisor concentration

`crates/eggress-runtime/src/supervisor.rs` is approximately 3,000 lines and `ServiceSupervisor::run()` still owns preparation and orchestration for health probes, standard/Unix/transparent/QUIC listeners, UDP services, reverse servers/clients, admin prebind, compatibility operations, readiness, signal handling, SIGHUP reload, and shutdown.

The surrounding `supervisor/{startup,state,reload,shutdown,connection,operations,udp_runtime,accounting}.rs` modules show the intended decomposition. The remaining work should therefore be mechanical extraction into private phase objects/helpers, not lifecycle redesign.

### Outbound connector concentration

`crates/eggress-outbound/src/connector.rs` is the correct implementation authority, but it currently contains typed TCP error classification, connector construction, TCP execution, UDP association state and conversion helpers, pproxy construction/error mapping, credential scrubbing, and extensive tests.

The public facade is already compact and should not change. Private module decomposition can reduce edit collision and review burden while retaining all existing re-exports and paths.

### Python -> CLI ownership edge

`crates/eggress-python/src/compat.rs::run_pproxy_test()` uses `eggress_cli::parse_pproxy_test_target()` and `eggress_cli::run_upstream_test()`. This makes the native binding crate depend on the CLI crate for operational logic.

The prior API-boundary campaign correctly retained this edge because removing it without an existing owner would have required new API or duplicated logic. This campaign may revisit the edge only through already-published owner APIs such as the outbound/config/core surfaces. If exact behavior cannot be reproduced cleanly, retaining and documenting the edge is the correct outcome.

### Broad published Rust surface

`docs/RUST_API.md` correctly treats all published crates as semver-relevant. Existing representative compile contracts focus primarily on embed/outbound/relay/core/routing. Supporting surfaces such as runtime/server/config/protocol/transport types have normal tests but relatively little downstream-style compile qualification.

The missing protection is not an exhaustive API database. It is a small set of compile contracts for important lower-level published paths and exact feature-slice checks matching the maintained feature map.

### Documentation drift

`docs/PYTHON_BINDINGS.md` currently contains at least two stale limitations:

- it says Unix-domain sockets are unsupported by the Rust runtime even though the config/runtime capability is implemented;
- it says `.pyi` stubs are future work even though the package ships `py.typed` and maintained public/native stubs.

This is documentation-state debt, not missing runtime capability.

## Registered execution plans

| Order | Plan | Status | Purpose |
|---|---|---|---|
| 1 | [`MAINTENANCE_PHASE_1_RUNTIME_SUPERVISOR_DECOMPOSITION.md`](MAINTENANCE_PHASE_1_RUNTIME_SUPERVISOR_DECOMPOSITION.md) | Implemented | Extract private runtime preparation/execution phases without changing lifecycle semantics. |
| 2 | [`MAINTENANCE_PHASE_2_OUTBOUND_INTERNAL_DECOMPOSITION.md`](MAINTENANCE_PHASE_2_OUTBOUND_INTERNAL_DECOMPOSITION.md) | Implemented | Split the outbound implementation hotspot into private domains while freezing every public path and behavior. |
| 3 | [`MAINTENANCE_PHASE_3_PYTHON_CLI_OWNERSHIP.md`](MAINTENANCE_PHASE_3_PYTHON_CLI_OWNERSHIP.md) | Implemented | Reassess and, only if behaviorally exact, remove the binding-to-CLI operational dependency using existing owner APIs. |
| 4 | [`MAINTENANCE_PHASE_4_PUBLIC_API_AND_FEATURE_QUALIFICATION.md`](MAINTENANCE_PHASE_4_PUBLIC_API_AND_FEATURE_QUALIFICATION.md) | Implemented | Expand lightweight public-path compile contracts and align CI feature slices with the maintained contract. |
| 5 | [`MAINTENANCE_PHASE_5_DOCUMENTATION_AND_COMPAT_PROJECTION_CONVERGENCE.md`](MAINTENANCE_PHASE_5_DOCUMENTATION_AND_COMPAT_PROJECTION_CONVERGENCE.md) | Implemented | Reconcile stale maintained docs and strengthen native/TOML compatibility projection drift protection without redesign. |

## Sequencing

Phase 1 and Phase 2 are internal structural work and may be implemented independently, but each should land with its own focused regression evidence before another large file movement begins.

Phase 3 should follow Phase 2 because the outbound owner surface is one of the possible existing seams for eliminating the Python-to-CLI edge. The phase explicitly permits a documented retain decision if removing the edge would require duplication or API expansion.

Phase 4 should run after the structural phases so its compile contracts describe the final internal ownership while preserving the same public surface.

Phase 5 closes last. It reconciles documentation and compatibility-projection evidence against the post-cleanup tree and must not become a backdoor feature campaign.

## Explicit non-goals

Do not use this campaign to:

- add Python exposure for listener-free UDP associations;
- add Trojan UDP or MASQUE/CONNECT-UDP;
- add reverse UDP;
- add pproxy reverse/backward TLS composition;
- add macOS PF original-destination recovery;
- add SSH listeners;
- change QUIC/H3 defaults or support claims;
- change legacy Shadowsocks/SSR behavior;
- merge runtime/server/outbound/embed crates;
- introduce a new `eggress-python-support` or generic utility crate;
- replace PyO3;
- replace the current pproxy semantic intermediate model;
- remove the TOML compatibility rendering path;
- add a mandatory cargo-semver-checks/public-api/nightly-rustdoc gate;
- create a combinatorial feature matrix;
- tune performance or memory policy without separate evidence.

## Campaign verification

Each phase owns focused checks. Before final closure, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp

cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features ssh,pproxy-compat

(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

Run the OpenSSH regression only when SSH-facing code/contracts are touched. Run external pproxy/shadowsocks oracle suites only if compatibility behavior or claims are touched; this campaign should normally avoid doing so.

## Campaign acceptance criteria

- [x] Runtime orchestration is internally decomposed enough that listener preparation, auxiliary service startup, run-loop/signals, and shutdown coordination have explicit private ownership boundaries.
- [x] Runtime startup/readiness/reload/shutdown behavior is unchanged and covered by focused lifecycle tests.
- [x] Outbound implementation domains are separated privately without changing any public item or compatibility re-export.
- [x] The Python-to-CLI dependency is either removed through existing stable owner APIs with exact behavior preservation, or retained with concrete evidence that removal would violate campaign constraints.
- [x] No duplicated operational helper is introduced merely to eliminate a dependency edge.
- [x] Representative downstream-style compile contracts cover important supporting Rust library surfaces in addition to the preferred facades.
- [x] CI feature-slice checks match the maintained documented slices, including the outbound TOML slice.
- [x] Ordinary CI remains bounded; no exhaustive feature matrix or mandatory API-diff database is added.
- [x] Maintained Python documentation no longer describes implemented Unix listener/stub capabilities as absent.
- [x] Native/TOML pproxy projection equivalence remains protected without eliminating either supported path.
- [x] No supported Rust/Python/CLI/config/protocol/transport/feature/compatibility surface changes.
- [x] Workspace Rust and Python gates are green.

## Closure record

All five phases landed in one maintenance convergence commit (see `git log --oneline -1`):

- Phase 1 (runtime): private `listeners`/`services`/`signals` extraction; `run()` orchestrates; lifecycle unchanged.
- Phase 2 (outbound): private `connect_error`/`udp`/`compat` extraction; public facade frozen.
- Phase 3 (python/cli): Outcome B retain (`parse_pproxy_test_target` + `run_upstream_test` shared tester); contract test added.
- Phase 4 (api/features): outbound `toml` slice in CI + `AGENTS.md`; representative contracts in embed/server; `RUST_API.md` reconciled.
- Phase 5 (docs/compat): stale Python docs corrected; `native_equivalence` strengthened (auth/group); no behavior/claim change.

Verification: workspace fmt/clippy/locked tests + outbound/embed feature slices + compat translator suites (see phase closures for focused evidence). No public Rust/Python/CLI/config/protocol/transport/feature/compat surface changes.

