# pproxy Maintenance Final Corrective Closure

## Status

**PLANNED**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Corrective-review baseline: `f011b66684742ba96d45bfc68fa59a2615385e57`
- Parent roadmap: [`PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md`](PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md)
- Implemented phase commits:
  - Phase 1 — contract truth + optional feature gate: `8c10331068f6a3e6c2e32154afeb373902fbc367`
  - Phase 2 — URI/protocol convergence: `5962d472cf97fd0bbd6291414915cc1608f9088f`
  - Phase 3 — runtime policy isolation/internal ownership: `f011b66684742ba96d45bfc68fa59a2615385e57`
- Frozen compatibility oracle remains `pproxy==2.7.9` at `09d4752f17ed6787e1a073c93980eec019887ee3`.

## Purpose

The maintenance-convergence implementation landed in the intended direction and its hosted Rust and Python checks are green. This corrective pass addresses the small set of issues found by the post-implementation review before the roadmap is marked closed.

This is a **closure pass**, not a fourth architecture phase and not another pproxy parity expansion. The implementation should preserve the new internal architecture from Phases 1–3 while repairing the public Rust source-compatibility regression, making the compatibility verbosity contract explicit and testable, and bringing the historical plan/docs record into sync with the code that already landed.

After this pass, stop this line of work unless a concrete interoperability, security, or maintenance defect is demonstrated.

## Confirmed corrective findings

### 1. Public Rust source compatibility regressed in Phase 3

Before Phase 3, `eggress-runtime` publicly exposed:

```rust
pub struct CompatibilityOptions {
    pub compatibility_mode: bool,
    pub auth_timeout: Option<Duration>,
    pub system_proxy: bool,
    pub debug: bool,
    pub verbose_level: u8,
}

ServiceSupervisor::start_from_config_with_options(
    RuntimeConfig,
    Option<String>,
    CompatibilityOptions,
)
```

and the `eggress-embed` compatibility facade publicly exposed:

```rust
EggressService::start_blocking_with_compatibility_options(
    CompatibilityOptions,
)
```

Phase 3 correctly replaced broad runtime-owned compatibility state with the narrower `CompatibilityRuntimeHooks` and `start_from_config_with_compatibility()`. However, the old public type/entry point disappeared and the existing embed method retained its old name while changing its argument type to `CompatibilityRuntimeHooks`.

Because the workspace is in the `1.0.x` line and `eggress-embed` is documented as the stable in-process Rust API, this is a source-level semver regression even though the internal architecture is better.

The correction must restore source compatibility **without putting `debug`, `verbose_level`, or a broad `compatibility_mode` boolean back into generic supervisor state**.

### 2. `-v` / `-vv` / `-vvv` now need an explicit observable contract

Phase 3 intentionally removed compatibility-specific per-session/per-traffic verbosity branches from the supervisor. The standalone `pproxy` entry point now resolves the flags before runtime construction through `PproxyArgs::default_log_level()` and `tracing_subscriber`:

- no `-v` / `-d` -> `info`
- `-v`, `-vv`, or `-d` -> `debug`
- `-vvv` -> `trace`
- explicit valid `RUST_LOG` remains authoritative

The current help text still describes `-v` as "Verbose connection output" and says `-vv` "adds traffic stats", which no longer describes the Phase 3 implementation. The canonical manifest/matrix already classify the behavior as a supported difference rather than exact pproxy logging parity, so there is no need to recreate the removed runtime presentation path. The contract should instead be made internally consistent and process-level tested.

### 3. The implemented roadmap and phase documents still say `PLANNED`

The maintenance roadmap and all three phase plans remain marked `PLANNED` despite corresponding implementation commits and green verification. Since `plans/` is historical implementation provenance, this is stale handoff state. The implementation pass should update these four files in place to `IMPLEMENTED`, record the implementing commit(s), and add concise closure evidence rather than creating another completion-document hierarchy.

### 4. Small documentation truth defects remain

Two concrete documentation defects remain after Phase 1:

- the top-level README calls `matched`, `supported_difference`, `platform_limited`, and `intentional_non_parity` the manifest/matrix **tier**. These are the compatibility **status** values. The runtime/reporter tier vocabulary is separate: `drop_in`, `compatible_with_warning`, `native_equivalent`, `intentional_non_parity`, `unsupported`;
- `docs/PPROXY_MIGRATION.md` contains a malformed sentence: `Diagnostics are produced by the StructuredDiagnostic type in The internal ...`.

Fix these directly. Do not reopen the compatibility documentation redesign.

## Governing constraints

1. Preserve the Phase 3 internal architecture: generic runtime receives only `Option<CompatibilityRuntimeHooks>` and must not regain broad compatibility presentation/parser state.
2. Preserve native startup as `None` compatibility state.
3. Do not reintroduce runtime-owned `debug` or `verbose_level` branches.
4. Do not reintroduce a generic `compatibility_mode` switch inside supervisor/data-plane logic.
5. Do not recreate pproxy's exact logging formatter, per-session display engine, or traffic-stat presentation solely to honor old field names.
6. Do not add a new event bus, callback framework, dependency-injection layer, or logging abstraction.
7. Do not add dependencies solely for semver testing; ordinary Rust compile-time tests are sufficient for this bounded source-compatibility surface.
8. Do not add another GitHub Actions workflow, matrix, or release gate.
9. Keep the existing optional compatibility compile gate unchanged unless the corrective API shim itself requires a trivial feature-forwarding fix.
10. Do not change protocol, routing, URI, UDP, reverse, SSH, QUIC/H3, Shadowsocks, TLS, system-proxy, or Python semantics unless a corrective regression test proves the closure change requires it.
11. Do not bump the workspace major version as a substitute for repairing this small accidental break. Restore compatibility in the current line.
12. Do not remove or rewrite historical plan content beyond status/closure metadata needed to accurately record implementation.
13. Keep the frozen pproxy oracle unchanged.
14. Do not create new parity percentages, manifests, certification artifacts, or evidence dashboards.

## Workstream 1 — Restore the legacy `CompatibilityOptions` public source surface as an adapter

Reintroduce the public `eggress_runtime::CompatibilityOptions` type with the same public field names and types that existed immediately before Phase 3:

```rust
#[derive(Debug, Clone, Default)]
pub struct CompatibilityOptions {
    pub compatibility_mode: bool,
    pub auth_timeout: Option<Duration>,
    pub system_proxy: bool,
    pub debug: bool,
    pub verbose_level: u8,
}
```

The type is a **legacy facade DTO only**. It must not become a field on `ServiceSupervisor`, `RuntimeState`, connection configuration, or other generic runtime state.

Add documentation making the ownership transition explicit:

- `auth_timeout` and `system_proxy` can be translated into narrow runtime hooks;
- `compatibility_mode` exists for source compatibility and controls only the legacy compatibility startup disposition needed to reproduce the previous SSH host-key path;
- `debug` and `verbose_level` are legacy presentation inputs that cannot safely configure an already-initialized process-global tracing subscriber from deep runtime code;
- new code should use `CompatibilityRuntimeHooks` plus facade-level logging initialization rather than `CompatibilityOptions`.

Mark the type deprecated if that is consistent with the crate's public deprecation policy. If deprecating the entire type would create excessive warning noise inside maintained compatibility code, keep the type supported for the current major line and deprecate only the old entry point documentation. The key acceptance condition is source compatibility plus a clear migration path, not maximal annotation.

### Conversion ownership

Add one canonical conversion/helper in `eggress-runtime`, for example:

```rust
CompatibilityRuntimeHooks::from_legacy_options(&CompatibilityOptions)
```

or an equivalent private/public conversion with the same ownership.

Required mapping:

- `auth_timeout: Some(d)` -> `auth_reuse = Some(Arc<AuthReuseCache::new(d)))`;
- `auth_timeout: None` -> no auth-reuse cache for callers using the legacy low-level API, preserving the old `CompatibilityOptions::default()` behavior;
- `system_proxy: true` -> `Some(SystemProxyRequest)`;
- `system_proxy: false` -> `None`;
- old `compatibility_mode` must retain the previous SSH security disposition: compatibility mode permits the compatibility SSH cache **only when** `EGRESS_SSH_INSECURE_HOST_KEYS` has the explicit accepted value; otherwise known-host verification remains enabled and the existing warning behavior remains;
- `compatibility_mode: false` must not activate insecure SSH behavior even if the environment variable is present.

Do **not** map `compatibility_mode` to a broad runtime flag. Resolve it during conversion/startup into the existing `allow_insecure_ssh_host_keys` hook boolean.

### Legacy logging fields

`debug` and `verbose_level` must not be threaded back into the supervisor.

For direct users of the deprecated low-level runtime method, choose one of these bounded behaviors and document/test it:

1. preferred: accept the fields for source compatibility, emit at most one clear tracing warning when either is non-default explaining that logging policy is now facade-owned, and otherwise ignore them; or
2. if there is an existing non-global facade helper that can apply the requested default filter before subscriber initialization without new architecture, use it there.

Do not call `tracing_subscriber::init()` from `eggress-runtime`. Do not attempt to replace a caller-owned global subscriber. Do not restore per-session compatibility branches.

## Workstream 2 — Restore `ServiceSupervisor::start_from_config_with_options()` as a compatibility shim

Restore the old method with its old parameter shape:

```rust
pub fn start_from_config_with_options(
    rt_config: RuntimeConfig,
    config_path: Option<String>,
    compatibility_options: CompatibilityOptions,
) -> Result<Self, RuntimeError>
```

It should immediately convert the legacy DTO to `CompatibilityRuntimeHooks` and delegate to the Phase 3 canonical path:

```text
start_from_config_with_options(... CompatibilityOptions)
        -> legacy conversion
        -> start_from_config_with_compatibility(... hooks)
        -> startup::init_supervisor(... Some(hooks))
```

No duplicated startup path is allowed.

A default legacy options value should preserve native-equivalent behavior rather than forcing compatibility hooks with fabricated state. If the cleanest implementation produces an empty hook set, either delegate to `start_from_config()` or pass an empty hooks value; choose the path that keeps native behavior and tests identical.

The current `start_from_config_with_compatibility()` remains the preferred typed internal/new API.

Add `#[deprecated(note = "use start_from_config_with_compatibility with CompatibilityRuntimeHooks")]` to the legacy method if appropriate.

## Workstream 3 — Restore the `eggress-embed` method signature without sacrificing the new hook API

The existing public method name must once again accept the legacy type:

```rust
pub fn start_blocking_with_compatibility_options(
    self,
    options: eggress_runtime::CompatibilityOptions,
) -> Result<EggressHandle, EggressError>
```

It must delegate through the canonical legacy conversion and then the existing shared `startup_in_memory` path.

Preserve access to the Phase 3 typed hook path under an unambiguous new name if it is useful to maintained internal/Python consumers, for example:

```rust
pub fn start_blocking_with_compatibility_hooks(
    self,
    hooks: eggress_runtime::CompatibilityRuntimeHooks,
) -> Result<EggressHandle, EggressError>
```

Preferred ownership:

- old external Rust code continues to compile against `..._with_compatibility_options(CompatibilityOptions)`;
- maintained Python/PyO3 code and new Rust code use `..._with_compatibility_hooks(CompatibilityRuntimeHooks)` or the lowest existing shared helper;
- both routes converge on the same `startup_in_memory(rt, Some(hooks))` implementation;
- native `start_blocking()` continues to use `startup_in_memory(rt, None)`.

Do not use a generic `Into<CompatibilityRuntimeHooks>` method merely to avoid adding a second clearly named facade unless it materially improves API clarity. The old exact signature plus one explicit new hook method is easier to reason about and protects semver expectations.

## Workstream 4 — Add compile-time source-compatibility regression tests

Add small Rust tests that would fail to compile if the old public surface disappears again. Do not add `trybuild`, `cargo-semver-checks`, or another dependency for this narrow case.

At minimum, type-check all of the following:

```rust
let options = eggress_runtime::CompatibilityOptions {
    compatibility_mode: true,
    auth_timeout: Some(Duration::from_secs(60)),
    system_proxy: false,
    debug: true,
    verbose_level: 2,
};
```

and a function/method reference compatible with:

```rust
ServiceSupervisor::start_from_config_with_options
```

For `eggress-embed`, compile a helper function or test body that constructs/accepts an `EggressService` and type-checks:

```rust
service.start_blocking_with_compatibility_options(options)
```

The embed compile-contract helper does not need to actually start a listener; code inside an uncalled helper is sufficient to force Rust type checking. Use `#[allow(deprecated)]` inside the compatibility regression tests if the shim is marked deprecated.

Also test conversion behavior directly:

- default legacy options -> no auth reuse, no system proxy, secure SSH disposition;
- explicit auth timeout -> cache present;
- `system_proxy=true` -> request present;
- compatibility mode + absent/false SSH opt-in -> secure SSH disposition;
- compatibility mode + accepted SSH env opt-in -> insecure compatibility disposition;
- non-compatibility mode + accepted SSH env opt-in -> still secure disposition.

Environment-variable tests must be serialized or scoped using the repository's existing deterministic env-test pattern to avoid parallel-test races. Do not introduce a global test framework for this.

## Workstream 5 — Lock the post-Phase-3 `-d` / `-v` observable contract

The intended compatibility contract after Phase 3 is logging-level translation, not recreation of pproxy's exact presentation layer.

Keep `PproxyArgs::default_log_level()` as the canonical pure resolver unless a clearly superior existing helper already owns it.

Required mapping:

```text
no -v and no -d -> info
-d              -> debug
-v              -> debug
-vv              -> debug
-vvv             -> trace
-vvvv+            -> trace (saturating/highest level)
```

Mixed clustered short forms such as `-dv`, `-vd`, `-dvv`, and repeated flags must resolve consistently according to the parsed counters. `--daemon` remains independent.

Explicit valid `RUST_LOG` must retain precedence over the compatibility-derived default filter.

### Help/docs correction

Update the standalone `pproxy` help text so it no longer promises a removed per-session/traffic-output distinction. Preferred wording should describe the actual behavior, e.g.:

```text
-d     Debug-level compatibility diagnostics
-v     Increase compatibility tracing verbosity (repeatable)
```

Do not claim `-vv` adds traffic statistics unless the current code actually provides a stable tested traffic-stat output at that level.

Keep the manifest classification as `supported_difference` unless process-level evidence shows the observable semantics now match upstream exactly. This corrective pass should not upgrade compatibility status.

### Tests

Retain/add pure resolver tests for all levels and mixed forms, but also add focused process-level coverage for the standalone compatibility binary so the wiring—not only the helper—is protected.

The process tests should verify behavior without depending on unstable timestamp/ANSI formatting. Suitable assertions include:

- a deterministic `debug!`/`trace!` startup event is absent/present at the expected level; or
- a test-only deterministic tracing event is exposed through an existing code path when the binary runs with a short terminating action.

Prefer existing `pproxy_binary` / `pproxy_run_process` infrastructure. Do not create a new logging test harness.

For `RUST_LOG`, test that an explicit restrictive filter suppresses a compatibility-derived debug/trace event even when `-vvv` is supplied, and that an explicit permissive filter can enable it when no verbosity flag is supplied.

Avoid tests that must open long-lived network listeners solely to inspect logging; use the shortest current process path that initializes logging and terminates deterministically.

## Workstream 6 — Correct the remaining documentation defects

### README status vs tier

In the top-level README, replace language equivalent to:

```text
manifest/matrix tier (`matched`, `supported_difference`, ...)
```

with:

```text
manifest/matrix status (`matched`, `supported_difference`, ...)
```

Where the README later says "see the matrix for the tier" for Trojan or similar prose, use `status` or simply `compatibility classification` unless the sentence actually refers to the five-level reporter tier.

Do not rename the runtime/reporter `tier` concept. `docs/parity/README.md` already correctly distinguishes the two vocabularies and remains authoritative.

### Migration guide malformed sentence

Repair the Structured Diagnostics paragraph to a complete sentence, for example:

```text
Diagnostics are produced by the `StructuredDiagnostic` type in the internal
`eggress-pproxy-compat` crate and are serializable to JSON. The Rust crate is
not a separate Python distribution.
```

Do not otherwise reopen the migration guide.

## Workstream 7 — Close the historical maintenance plan record in place

After the code/docs corrections pass all required verification, update these four files:

- `plans/PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md`
- `plans/PPROXY_MAINTENANCE_PHASE_1_CONTRACT_AND_FEATURE_GATES.md`
- `plans/PPROXY_MAINTENANCE_PHASE_2_URI_PROTOCOL_CONVERGENCE.md`
- `plans/PPROXY_MAINTENANCE_PHASE_3_RUNTIME_POLICY_AND_INTERNAL_OWNERSHIP.md`

Change `**PLANNED**` to `**IMPLEMENTED**` and add a compact closure block near the status/baseline containing the implementation commit and verification evidence.

Minimum historical mapping:

```text
Phase 1 -> 8c10331068f6a3e6c2e32154afeb373902fbc367
Phase 2 -> 5962d472cf97fd0bbd6291414915cc1608f9088f
Phase 3 -> f011b66684742ba96d45bfc68fa59a2615385e57
```

The roadmap should record those three commits plus the final corrective commit produced by this plan.

Do not rewrite the original baseline/findings to pretend the implementation state existed when the plan was written. Plans are historical records; preserve the original planning context and append/annotate closure state.

Do not generate separate Phase 1/2/3 completion files.

## Workstream 8 — Update architecture/API documentation only where the shim changes the public surface

Update the minimum current architecture/reference material necessary to explain the compatibility shim:

- `architecture/runtime.md`
- `architecture/embed.md`
- `architecture/pproxy-compat.md` if it enumerates runtime entry points
- Rust doc comments on the restored legacy type/methods

Required message:

```text
CompatibilityOptions / start_from_config_with_options
    = legacy public source-compatible facade
    -> converts immediately to CompatibilityRuntimeHooks

CompatibilityRuntimeHooks / start_from_config_with_compatibility
    = canonical typed runtime path

ServiceSupervisor internal state
    = Option<CompatibilityRuntimeHooks>, never CompatibilityOptions
```

If `docs/EMBED_API.md` documents the compatibility method, update it to show the old method as compatibility-preserving/deprecated and the hook method as the preferred new API.

Do not add a general semver-policy document or API versioning framework in this pass.

## Required implementation order

Perform the work in this order to reduce churn:

1. restore the runtime legacy DTO and canonical conversion;
2. restore the runtime legacy method shim;
3. restore the exact embed facade signature and move maintained internal callers to the explicit hook method;
4. add source-compatibility and conversion tests;
5. lock `-d`/`-v` resolver/process behavior and fix help text;
6. fix README/migration prose;
7. update affected architecture/API docs;
8. run focused and broad verification;
9. mark the parent roadmap/phases `IMPLEMENTED` with commit/evidence metadata;
10. mark this corrective plan `IMPLEMENTED` in the final implementation commit or an immediate documentation-only closure commit.

Do not interleave unrelated refactors while restoring the shim.

## Required focused verification

### Runtime API/conversion

```bash
cargo test -p eggress-runtime
```

Run the exact focused test names added for legacy API conversion/source compatibility if available.

### Embed API

```bash
cargo test -p eggress-embed
```

The source-compatibility test must compile with the `pproxy-compat` feature path enabled if that API is feature-gated. Use the minimum existing feature selection needed to exercise it.

### Compatibility parser/log-level contract

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli --test pproxy_run_process
```

Use the actual current integration-test names if one of these binaries has moved. Do not create duplicate integration targets merely to match this document.

### Optional feature bundle

Because the shim touches compatibility runtime startup and may affect SSH feature forwarding, rerun the Phase 1 bounded product gate:

```bash
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon \
  --bins
```

Do not add `insecure-quic`.

### Python

If the PyO3 caller is changed from the old embed method name to the explicit hook method—as expected—run the existing Python smoke suite:

```bash
python -m pytest python/tests tests/compat -q
```

Use the repository's documented installed-extension environment rather than source-tree shadowing.

## Broad closure gate

Before marking the plan and parent roadmap implemented:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

No external pproxy oracle run is required unless the implementation changes observable compatibility semantics beyond the already-documented logging supported difference. If the corrective pass changes actual parser/CLI behavior relative to `pproxy==2.7.9`, run only the corresponding focused ignored differential test.

## Acceptance criteria

This corrective closure is complete only when **all** of the following are true:

### Public Rust API compatibility

- `eggress_runtime::CompatibilityOptions` once again exists with the pre-Phase-3 public field names/types and `Default`/`Clone`/`Debug` behavior expected by source callers;
- `ServiceSupervisor::start_from_config_with_options(RuntimeConfig, Option<String>, CompatibilityOptions)` once again type-checks for existing callers;
- `EggressService::start_blocking_with_compatibility_options(CompatibilityOptions)` once again type-checks for existing callers under its documented feature gate;
- compile-time regression tests cover these exact old forms;
- the restored API is an adapter only: `ServiceSupervisor` stores `Option<CompatibilityRuntimeHooks>`, not `CompatibilityOptions`;
- `start_from_config_with_options()` delegates to the canonical Phase 3 startup path and does not duplicate supervisor initialization;
- the embed legacy method and any new hook method share the same `startup_in_memory` implementation;
- no public-call compatibility fix requires a major-version bump.

### Legacy option conversion/security

- `auth_timeout` maps to the same process-local bounded `AuthReuseCache` semantics as before;
- `auth_timeout=None` on the low-level legacy DTO does not fabricate the pproxy CLI's 30-day default; the CLI facade remains responsible for calling `effective_auth_timeout()`;
- `system_proxy` maps only to the narrow post-bind `SystemProxyRequest` hook;
- `compatibility_mode` is consumed during conversion and does not become a generic runtime mode flag;
- SSH compatibility still requires explicit `EGRESS_SSH_INSECURE_HOST_KEYS` acknowledgement and non-compatibility startup cannot become insecure solely because the environment variable is set;
- `debug` and `verbose_level` do not re-enter supervisor/runtime connection state;
- non-default legacy logging fields are handled explicitly/documented rather than silently implying the old runtime presentation path still exists.

### `-d` / `-v` contract

- `PproxyArgs::default_log_level()` or its canonical replacement has tests for no flags, `-d`, `-v`, `-vv`, `-vvv`, higher counts, and representative clustered mixed forms;
- standalone `pproxy` process tests prove the selected default tracing level is actually wired into the binary;
- explicit valid `RUST_LOG` precedence is process-level regression-tested;
- help text no longer promises that `-vv` adds traffic statistics unless that output is actually restored and tested;
- the compatibility manifest/matrix continue to describe `-d`/`-v` as supported differences unless new oracle evidence justifies a different classification;
- no pproxy-specific per-session/event subsystem, event bus, or runtime verbosity state was reintroduced.

### Documentation/closure truth

- top-level README uses `status` for `matched` / `supported_difference` / `platform_limited` / `intentional_non_parity` and reserves `tier` for the five-level reporter vocabulary;
- the malformed Structured Diagnostics sentence in `docs/PPROXY_MIGRATION.md` is fixed;
- the four parent maintenance plan files are marked `IMPLEMENTED` with accurate implementation commit metadata rather than remaining `PLANNED`;
- this corrective plan is marked `IMPLEMENTED` when its acceptance criteria pass;
- current architecture/embed docs describe the legacy shim -> typed hooks boundary accurately;
- no new completion-document hierarchy, manifest, dashboard, workflow, or certification framework was introduced.

### Verification

- focused runtime/embed/compatibility/CLI tests pass;
- the optional product compatibility feature bundle compiles;
- Python smoke passes if PyO3/Python-facing call sites changed;
- `cargo fmt --all -- --check` passes;
- `cargo clippy --workspace --all-targets -- -D warnings` passes;
- `cargo test --workspace --locked` passes;
- fuzz targets compile;
- hosted Rust/Python workflows are green on the final corrective head when their path filters trigger.

## Explicit non-goals

Do **not** use this corrective pass to implement or revisit:

- macOS PF original-destination recovery;
- the four unavailable legacy cipher names;
- SSR UDP or external/SIP003 plugins;
- QUIC/H3 UDP association;
- pproxy backward/reverse TLS wire composition;
- Trojan UDP;
- MASQUE / CONNECT-UDP;
- Linux TPROXY;
- Linux IPv6 original-destination recovery;
- certificate hot reload;
- routing/URI/config architecture redesign;
- further `supervisor.rs` decomposition;
- crate merges/splits;
- dynamic protocol registries;
- Python compatibility expansion;
- new CI workflows, operating-system matrices, release automation, or broad certification suites;
- post-2.7.9 pproxy `master` features.

## Stop condition

Once this plan is implemented and the parent roadmap/phase records are closed, the maintenance-convergence line of work is complete. Remaining pproxy differences are documented compatibility boundaries, not an automatic backlog. Further work requires a concrete user requirement, demonstrated interoperability failure, security issue, or new maintainability regression.