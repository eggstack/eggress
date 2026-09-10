# pproxy Maintenance Phase 3 — Runtime Policy Isolation and Internal Ownership

## Status

**PLANNED**

## Parent roadmap

[`PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md`](PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md)

## Dependency

Phases 1 and 2 must be complete. Compatibility truth and URI/protocol conversion boundaries should be stable before changing runtime ownership.

## Objective

Push pproxy-specific policy out of the generic runtime where that policy can be resolved before startup, retain only compatibility behavior that genuinely requires runtime/post-bind participation, and reduce responsibility concentration in the highest-value remaining internal modules without changing crate topology or public behavior.

This is the final phase of the maintenance-convergence roadmap. It is not a license for another architecture rewrite.

## Current state

The architecture-convergence work already established several important canonical paths and should be preserved:

- compiled in-memory startup;
- one canonical reload transaction;
- direct pproxy-to-native lowering rather than TOML round-trips for internal consumers;
- one maintained Python async bridge;
- narrower metrics ownership/interfaces;
- typed compatibility issues;
- decomposed runtime/config/server/PyO3 modules;
- listener-free outbound UDP;
- native reverse TLS/mTLS.

However, `eggress-runtime::ServiceSupervisor` still owns a `CompatibilityOptions` struct with fields including compatibility mode, authentication reuse timeout, system-proxy behavior, debug state, and compatibility verbosity. Some of these fields are adapter/presentation policy rather than runtime lifecycle state.

Large internal files also remain, notably routing and config compilation. Previous convergence work correctly rejected artificial common traits or crate reshaping merely to reduce file length. That principle remains binding here.

## Governing constraints

1. Preserve the public `ServiceSupervisor` API where practical. If an internal constructor changes, facade compatibility should remain.
2. Preserve compiled runtime configuration as the canonical normal runtime input.
3. Do not create a second runtime architecture for compatibility mode.
4. Do not create a broad dependency-injection framework or generic hook/plugin system.
5. Do not add crates or merge crates.
6. Do not reopen listener hot-reload topology.
7. Do not redesign metrics, reload, reverse, UDP, Python async bridging, or TLS unless a regression directly blocks this phase.
8. Compatibility behavior that genuinely depends on bound listener addresses or runtime connection events may remain a runtime concern.
9. Logging/diagnostic presentation that can be resolved by the CLI/compatibility facade before startup should not require generic supervisor state.
10. No arbitrary source-file line-count goals. Extract only coherent responsibilities with narrow interfaces.
11. Do not change externally observable pproxy semantics without pinned-oracle evidence.
12. Keep routine CI topology unchanged beyond the Phase 1 bounded feature compile step.

## Workstream 1 — Audit every `CompatibilityOptions` field by ownership

Before editing, trace every read/write of the current compatibility options and classify each field into one of these categories:

### Category A — pre-start adapter policy

Examples likely to include:

- default log level derived from `-d` / `-v`;
- compatibility diagnostics already produced by the parser/translator;
- CLI-only output formatting.

These should be resolved in `eggress-cli` / `eggress-pproxy-compat` before supervisor startup wherever possible.

### Category B — ordinary native configuration

If compatibility translation already has a native compiled-config representation for a behavior, lower the compatibility setting into that representation instead of adding a supervisor-only switch.

Do not add config fields merely to make every option fit. Use existing configuration semantics only where they are genuinely equivalent.

### Category C — runtime/post-bind compatibility hooks

Examples may include:

- applying system-proxy state after a dynamically assigned local listener port is known;
- restoring system-proxy state during failed startup/shutdown;
- source-IP authentication reuse if the behavior is owned by the live inbound authentication path;
- compatibility-specific session/event text if it genuinely depends on completed session reports and cannot be emitted by an outer facade.

These may remain runtime-facing, but their ownership must be narrow and documented.

The implementation summary must include a table listing each existing field, its old owner, new owner, and rationale for any field retained in runtime.

## Workstream 2 — Remove pre-start logging/debug policy from generic supervisor state

### Required direction

The compatibility CLI already parses `-d` and `-v`; determine the tracing filter/default log level before constructing the runtime. The generic supervisor should not need a `debug` bit merely to know how the compatibility frontend wants logs displayed.

Preferred outcome:

- CLI startup resolves compatibility `-d` / verbosity into tracing configuration;
- `ServiceSupervisor` and connection/runtime code consume ordinary tracing, not compatibility flag state;
- structured compatibility warnings about semantic differences remain in the compatibility layer;
- explicit `RUST_LOG` precedence remains unchanged;
- tests for `-d`, `-v`, `-vv`, `-vvv`, and `RUST_LOG` precedence remain green.

If `verbose_level` is also used to emit pproxy-style per-session lines from runtime reports, separate that concern from log-level selection. Do not keep a broad debug/verbosity struct simply because one runtime callback still needs a presentation level.

A narrow compatibility session reporter/sink is acceptable only if it removes multiple compatibility branches from generic runtime and can be implemented with an existing trait/callback boundary. Do not introduce a general event bus.

## Workstream 3 — Narrow authentication reuse ownership

Trace `auth_timeout` / `AuthReuseCache` from pproxy CLI translation through listener/session construction.

Preferred ownership is the inbound authentication/session layer because the behavior is tied to source-IP authentication reuse rather than supervisor lifecycle.

Requirements:

- compatibility parser/translator computes the requested timeout;
- runtime listener construction may pass an already-typed authentication policy/cache handle into connection configuration;
- the generic supervisor should not interpret pproxy argument semantics;
- native listener authentication remains unchanged;
- cache expiry remains monotonic/bounded and process-local as currently documented;
- reload/start/shutdown semantics remain unchanged unless current behavior is already tested otherwise.

Do not introduce a globally shared authentication service or generalized identity cache.

## Workstream 4 — Keep system-proxy behavior as a narrow post-bind hook if required

`--sys` differs from ordinary static configuration because pproxy-style startup may bind an ephemeral/local listener whose actual address is required before OS proxy state can be applied.

It is acceptable for the runtime startup path to retain a narrow compatibility post-bind action when all of the following are true:

- the compatibility layer explicitly opts into it;
- it does not alter native startup when absent;
- it receives only the bound listener information it needs;
- apply/rollback remains idempotent and failure-safe;
- shutdown restoration remains guaranteed;
- platform limitations remain explicit.

Do not force this into TOML purely to eliminate one compatibility hook. A small well-owned runtime hook is preferable to misrepresenting global OS mutation as ordinary listener configuration.

If the current hook can cleanly move into the CLI facade after supervisor binding while retaining lifecycle rollback guarantees, that is also acceptable, but do not weaken failure cleanup to achieve conceptual purity.

## Workstream 5 — Define the minimal remaining compatibility runtime surface

After the preceding workstreams, either:

1. remove `CompatibilityOptions` entirely if no runtime-only compatibility state remains; or
2. rename/narrow it to reflect the actual residual responsibility, with only fields that genuinely require runtime participation.

The residual type must not contain parser diagnostics, CLI log-level state, URI semantics, or fields that simply mirror normal compiled config.

Required invariants:

- native startup passes no compatibility state;
- compatibility startup is explicit and opt-in;
- ordinary runtime code does not branch on a generic `compatibility_mode` boolean when a typed optional capability/hook can express the real behavior;
- no second supervisor implementation is created.

Prefer `Option<SpecificCompatibilityHook>` or a tiny typed residual policy over a boolean mode that changes unrelated behavior throughout the runtime.

## Workstream 6 — Decompose `eggress-routing/src/lib.rs` by responsibility

Only perform this extraction if the current file still contains the responsibilities identified at implementation time. Preserve public names through re-exports as needed.

Likely coherent internal modules:

```text
routing/
  lib.rs          public facade/re-exports
  model.rs        IDs, request/result/action types
  matcher.rs      MatchExpr, PortMatcher, normalization/matching helpers
  rule.rs         CompiledRule and rule evaluation
  router.rs       Router / RouteService implementation
  explain.rs      RouteExplanation construction
  scheduler.rs    existing scheduler implementation
  health.rs       existing health implementation
  upstream.rs     existing upstream state
  lease.rs        existing lease/accounting
```

Exact names may differ. The important ownership rules are:

- matching logic belongs together;
- route selection orchestration is separate from data-model definitions;
- explanation/diagnostic DTO construction should not dominate the core matcher module;
- public API remains stable through `pub use` where necessary;
- no new crate or generalized rule engine is introduced.

Do not alter matching semantics during module movement. Preserve hostname normalization, suffix behavior, raw regex semantics, CIDR matching, port matching, source matching, transport matching, reverse-listener matching, and scheduler behavior exactly.

## Workstream 7 — Decompose `eggress-config/src/compile.rs` by compilation domain

If `compile.rs` remains a large mixture of compiled model definitions and domain-specific compilation functions, extract coherent modules without changing the external configuration schema.

Likely structure:

```text
config/compile/
  mod.rs          public compile facade
  model.rs        RuntimeConfig and compiled DTOs
  listeners.rs    listener/TLS/UDP/transparent/unix compilation
  upstreams.rs    upstream chains/groups/health compilation
  rules.rs        routing rule compilation
  reverse.rs      reverse server/client + TLS compilation
  process.rs      process/timeouts/admin compilation if useful
```

Reuse existing `validate/` modules; do not duplicate validation in compilation modules.

Required ownership rule:

```text
parse -> validate -> compile
```

must remain one-way. Compilation modules may assume validation invariants only where the current design already guarantees them; avoid panics on externally constructible intermediate values unless the type system makes the invariant impossible to violate.

Public compiled types should remain reachable at their existing paths through re-exports where they are part of the public/internal cross-crate API.

## Workstream 8 — Do not reopen `supervisor.rs` decomposition without a concrete gain

The prior convergence phase already extracted accounting, connection construction, operations, reload, shutdown, startup, state, and UDP runtime modules and deliberately retained transport-specific orchestration in the facade where common abstraction would be artificial.

Therefore:

- do not create common listener traits solely to shrink `supervisor.rs`;
- do not refactor Unix/TCP/transparent/QUIC accept loops into a generic framework unless current duplicated behavior is demonstrably causing a defect;
- small extraction caused directly by compatibility-policy removal is acceptable;
- otherwise leave supervisor transport orchestration structurally stable.

This workstream is primarily a guardrail against unnecessary churn.

## Workstream 9 — Architecture documentation

Update only the deep dives affected by real ownership changes:

- `architecture/runtime.md`
- `architecture/cli.md`
- `architecture/pproxy-compat.md`
- `architecture/routing.md`
- `architecture/config.md`

Document where compatibility policy ends and native runtime begins. The architecture should make the following dependency direction obvious:

```text
pproxy syntax/policy
      -> compatibility lowering / CLI startup decisions
      -> compiled native runtime config + narrow runtime-only hooks
      -> generic runtime/data plane
```

## Required tests and verification

### Compatibility startup/logging

Run existing focused tests covering:

- `-d` debug behavior;
- `-v/-vv/-vvv` behavior;
- `RUST_LOG` precedence;
- compatibility run startup;
- `--auth` source-IP reuse;
- `--sys` apply/rollback where deterministic/platform-safe tests exist;
- native startup proving no compatibility behavior leaks into ordinary configuration.

Likely commands should include current equivalents of:

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli --test pproxy_run_process
cargo test -p eggress-runtime
```

### Routing/config extraction

```bash
cargo test -p eggress-routing
cargo test -p eggress-config
cargo test -p eggress-runtime routing
cargo test -p eggress-cli route
```

Use actual current test names rather than inventing new integration binaries if the repository layout changes before implementation.

### Broad closure

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Also rerun the Phase 1 optional compatibility compile command after runtime feature forwarding changes:

```bash
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon \
  --bins
```

Python smoke is required only if Python-facing APIs are touched. External pproxy oracle tests are required only if user-visible compatibility semantics change rather than merely moving ownership.

## Acceptance criteria

Phase 3 is complete only when all are true:

- every old `CompatibilityOptions` field has an explicit ownership disposition documented in the implementation summary;
- compatibility `-d` and default verbosity/log-filter policy are resolved outside the generic supervisor;
- explicit `RUST_LOG` precedence remains unchanged;
- authentication reuse behavior is owned by the inbound authentication/session boundary rather than interpreted as pproxy semantics deep in runtime lifecycle code;
- system-proxy compatibility behavior remains lifecycle-safe and narrowly scoped to the post-bind/runtime behavior it actually requires;
- the generic runtime no longer uses a broad `compatibility_mode` boolean to change unrelated behavior where a narrower typed option can express the requirement;
- if a residual compatibility runtime type remains, it contains only genuinely runtime-dependent behavior and is absent from native startup;
- no second runtime/supervisor architecture, event bus, plugin system, dependency-injection framework, or new crate was introduced;
- routing decomposition, if performed, preserves all public paths and matching/scheduling semantics while producing coherent matcher/model/router ownership;
- config compilation decomposition, if performed, preserves the TOML schema, compiled output semantics, parse/validate/compile ordering, and public cross-crate paths;
- `supervisor.rs` is not subjected to another broad generic-listener rewrite;
- affected architecture deep dives match the final module/ownership layout;
- the Phase 1 optional feature bundle still compiles;
- focused tests and the broad workspace gate pass.

## Final roadmap stop condition

When this phase closes, remaining pproxy differences are not automatically actionable. Do not create another parity phase for PF, legacy ciphers, SSR UDP/plugins, QUIC UDP, backward TLS wire composition, or unrelated modern proxy features without concrete demand or a demonstrated compatibility failure.
