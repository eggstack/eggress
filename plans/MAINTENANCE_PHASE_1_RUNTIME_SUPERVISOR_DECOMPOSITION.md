# Maintenance Phase 1 — Runtime Supervisor Decomposition

## Status

**PLANNED — 2026-09-21**

## Parent

[`MAINTENANCE_CONVERGENCE_ROADMAP.md`](MAINTENANCE_CONVERGENCE_ROADMAP.md)

## Baseline

`fa1d9bf73c5c162c62a6086cf0e7361f1cfb3131`

## Objective

Reduce the maintenance and review burden of `ServiceSupervisor::run()` by extracting private preparation and orchestration phases into the existing supervisor module structure while preserving every public API and every observable lifecycle invariant.

This is a structural refactor. It is not a runtime redesign.

## Current state

`crates/eggress-runtime/src/supervisor.rs` remains roughly 3,000 lines. The public `ServiceSupervisor` surface is relatively small, but `run()` coordinates:

- health runtime registration/probe startup;
- listener authentication materialization;
- TLS preparation;
- Unix-domain listeners;
- transparent listeners and platform capability fallback;
- QUIC/H3 listeners;
- standard TCP listeners;
- listener address publication;
- UDP service construction;
- reverse server/client startup;
- admin listener prebind and task spawn;
- compatibility system-proxy application;
- readiness publication;
- Unix signal handling and SIGHUP reload;
- non-Unix signal handling;
- ordered shutdown.

The repository already has `supervisor/startup.rs`, `state.rs`, `reload.rs`, `shutdown.rs`, `connection.rs`, `operations.rs`, `udp_runtime.rs`, and `accounting.rs`. The goal is to make the central run body reflect those existing ownership boundaries.

## Governing invariants

The implementation must preserve all of the following exactly:

1. Listener bind/preparation failure is surfaced before readiness.
2. Admin bind failure remains a startup error and occurs before readiness.
3. Readiness is not published until required signal handling/startup state is installed.
4. Unix/transparent/QUIC/standard listener selection and fallback behavior do not change.
5. Authentication reuse hooks and compatibility system-proxy behavior do not change.
6. Health probes still start inside an active Tokio runtime using the same upstream snapshot.
7. Runtime reload continues to use the canonical `RuntimeState::apply_compiled_config()` transaction.
8. SIGHUP remains file-backed only; in-memory embed startup does not acquire file reload semantics.
9. Task ownership remains explicit and no detached runtime task is introduced.
10. Shutdown order and grace/cancellation behavior remain unchanged.
11. Listener topology remains startup-captured.
12. No public type, method, field, feature, config key, error, metric, or log contract is intentionally changed.

## Workstream 1 — Characterize the current run phases

Before moving code, record a private implementation map in source comments or the architecture document for:

1. runtime-context initialization;
2. health startup;
3. listener preparation;
4. listener/admin/reverse task spawn;
5. compatibility operations;
6. readiness/signal run loop;
7. ordered shutdown.

Identify every value that currently crosses between those phases. Pay special attention to cancellation tokens, `TaskTracker`/task sets, prepared listener state, admin state, compatibility hooks, metrics, SSH session cache, and feature-gated QUIC/reverse resources.

Do not introduce a generic “context bag” containing the entire supervisor. Group only values naturally shared by a phase.

## Workstream 2 — Extract listener preparation

Move the listener preparation loop out of `run()` into private helpers/types in the existing supervisor module hierarchy.

The extracted boundary should own the construction of:

- standard TCP prepared listeners;
- Unix listener arguments/resources;
- transparent listener arguments/resources;
- feature-gated QUIC prepared listeners;
- prepared TLS state;
- per-listener UDP services;
- listener address metadata required by runtime/admin state.

Preferred output is a private typed aggregate such as a prepared-listener set, not a tuple with dozens of fields.

Requirements:

- keep protocol vectors, auth material, TLS state, UDP config, fixed target, local bind, limits, and listener names exactly equivalent;
- preserve all current skip/fallback/error semantics for unsupported platform listener modes;
- do not expose the aggregate outside `eggress-runtime`;
- do not move protocol-specific accept/handshake logic out of `eggress-server`.

## Workstream 3 — Extract auxiliary service startup

Move reverse server/client setup and admin prebind/spawn into private helpers that consume the already-prepared runtime state.

Keep feature gates local and explicit:

- `operations` continues to own admin/system-proxy runtime wiring;
- `reverse` continues to own reverse server/client startup;
- `ssh` state remains shared exactly as it is today;
- `quic` remains listener/transport feature-gated.

Do not hide failures behind spawned tasks when the current code surfaces them synchronously before readiness.

## Workstream 4 — Isolate the signal/readiness run loop

Extract the Unix and non-Unix wait loops into a private helper that receives only the state required to:

- publish readiness at the same point;
- wait on cancellation/CTRL-C/SIGTERM;
- process SIGHUP only for file-backed configuration;
- call the same canonical reload transaction;
- record reload failure metrics exactly as today.

Do not alter logging severity/messages unless a move requires minor source-location-neutral wording.

## Workstream 5 — Keep shutdown as the single closure path

The refactored run path must still converge on the existing `shutdown_ordered(ShutdownPlan { ... })` authority.

Do not duplicate shutdown logic into listener or signal helpers.

If phase extraction makes ownership difficult, pass a typed private run-resource aggregate into `ShutdownPlan` construction rather than introducing independent cleanup branches.

## Workstream 6 — Update maintained architecture documentation

Update `architecture/runtime.md` only where necessary to describe the private phase ownership after extraction.

Do not rewrite historical plans.

## Verification

Focused tests:

```bash
cargo test -p eggress-runtime --locked
cargo test -p eggress-runtime --locked --test lifecycle_invariants
cargo test -p eggress-runtime --locked --test upstream_protocols
cargo test -p eggress-runtime --locked --test udp
cargo test -p eggress-embed --locked --test start_stop
cargo test -p eggress-embed --locked --test reload
cargo test -p eggress-embed --locked --test reload_convergence
```

Feature/build checks when touched:

```bash
cargo check -p eggress-runtime --locked --no-default-features
cargo check -p eggress-runtime --locked --no-default-features --features common
cargo check -p eggress-runtime --locked --no-default-features --features extended
cargo check -p eggress-runtime --locked --no-default-features --features operations,reverse
```

If SSH/QUIC paths move, run their repository-required feature checks and OpenSSH regression.

Final gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Stop conditions

Stop the extraction rather than changing behavior if:

1. a helper would need to become public to break a module ownership cycle;
2. listener preparation can only be shared by changing protocol/server public types;
3. exact readiness ordering becomes ambiguous;
4. cleanup would require changing shutdown semantics;
5. a proposed abstraction collapses feature-gated resources into dynamic/opaque state that is harder to review than the current code.

A smaller extraction that preserves explicit ownership is preferable to a generalized supervisor framework.

## Acceptance criteria

- [ ] `ServiceSupervisor::run()` no longer contains the full listener preparation implementation.
- [ ] Standard, Unix, transparent, and QUIC listener preparation have explicit private ownership boundaries.
- [ ] Reverse/admin auxiliary startup is separated from the signal wait loop.
- [ ] Readiness publication occurs at the same semantic point as the baseline.
- [ ] SIGHUP reload continues through `RuntimeState::apply_compiled_config()`.
- [ ] File-backed versus in-memory reload behavior is unchanged.
- [ ] Existing compatibility auth/system-proxy hooks behave identically.
- [ ] Existing task/cancellation ownership is preserved; no detached cleanup work is added.
- [ ] `shutdown_ordered` remains the single ordered shutdown authority.
- [ ] No public Rust API, config schema, feature, protocol behavior, metric, or compatibility claim changes.
- [ ] Focused runtime/embed lifecycle tests pass.
- [ ] Workspace fmt, clippy, and locked tests pass.

## Closure record

Fill in place with implementation commit, extracted private boundaries, focused lifecycle evidence, feature slices run, and any intentionally retained large block.
