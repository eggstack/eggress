# Phase 2 Final Integration Pass Plan

## Purpose

This plan closes the remaining Phase 2 integration defects after the corrective-integration work. The repository now contains the correct major components—routing, schedulers, leases, health, metrics, admin, runtime supervision, reload, and TOML configuration—but several runtime objects are still duplicated or disconnected.

The objective of this pass is not to redesign Phase 2. It is to unify the existing components into one coherent runtime snapshot, correct shutdown/readiness semantics, make admin and health observe the same live state as routing, and ensure documentation claims match executable behavior.

This plan is intentionally decomposed into small, reviewable steps suitable for execution by a smaller coding model. Each commit should leave the workspace compiling and tests passing.

## Final blocking issues

1. Health probes update different `UpstreamRuntime` objects from those used by routing and admin.
2. Listener-name routing receives the local bind address rather than the configured listener name.
3. Shutdown cancels active connections before the grace period, so it is not actually graceful.
4. Listener bind failures occur in detached startup tasks and do not prevent readiness.
5. Admin receives an empty listener list.
6. Admin captures a stale `Arc<Router>` and does not reflect reloads.
7. Admin generation is disconnected from routing generation.
8. Reload is routing-only but is documented too broadly.
9. Health settings are hard-coded rather than compiled from TOML.
10. PAC/static content is marked complete but is not configured or served by the running binary.
11. Admin readiness does not reflect runtime readiness.
12. Live route explanation becomes stale after reload and omits source/identity context.
13. Direct fallback is not distinguishable from an explicit direct route.
14. Failure categorization remains imprecise for hop and timeout errors.
15. Required end-to-end integration tests are still missing.

## Non-goals

Do not add:

- UDP;
- TLS;
- new proxy protocols;
- persistent storage;
- distributed control-plane state;
- system proxy mutation;
- transparent proxying;
- reverse tunnels;
- plugin systems.

---

# Target runtime model

The runtime must construct one authoritative set of objects and share them everywhere.

```text
CompiledRuntimeSnapshot
├── generation
├── effective config
├── upstream registry: Arc<HashMap<UpstreamId, Arc<UpstreamRuntime>>>
├── router: Arc<Router>
├── listeners: Arc<[CompiledListener]>
├── admin content: PAC/static configuration
└── health plan

SharedRuntimeState
├── current snapshot: ArcSwap<CompiledRuntimeSnapshot>
├── metrics registry
├── readiness
├── active connection count
├── reload coordinator
└── process start time
```

The exact same `Arc<UpstreamRuntime>` objects must be used by:

- router groups;
- scheduler selection;
- health manager;
- metrics;
- admin `/upstreams`;
- route explanation;
- reload reconciliation.

No subsystem may rebuild its own private upstream runtime registry.

---

# Workstream 1: Build one shared compiled runtime snapshot

## Problem

`build_upstream_runtimes()` and `build_router_from_config()` currently construct separate upstream objects.

## Required design

Create a compiled runtime builder in `eggress-runtime` or `eggress-config`:

```rust
pub struct CompiledRuntimeSnapshot {
    pub generation: u64,
    pub effective_config: Arc<EffectiveConfig>,
    pub upstreams: Arc<HashMap<UpstreamId, Arc<UpstreamRuntime>>>,
    pub router: Arc<Router>,
    pub listeners: Arc<[CompiledListener]>,
    pub admin: Option<CompiledAdmin>,
    pub health_plan: Arc<[HealthTarget]>,
}
```

Suggested builder:

```rust
pub fn compile_runtime_snapshot(
    config: RuntimeConfig,
    generation: u64,
    previous: Option<&CompiledRuntimeSnapshot>,
) -> Result<CompiledRuntimeSnapshot, RuntimeBuildError>;
```

## Build order

1. validate upstream IDs;
2. reconcile or create `Arc<UpstreamRuntime>` objects;
3. build groups using the shared upstream registry;
4. build router rules and default action;
5. compile listeners;
6. compile admin/PAC/static content;
7. compile health targets that reference the same upstream `Arc`s;
8. create immutable snapshot.

## Reconciliation rules

For each upstream ID:

- same ID + equivalent chain + equivalent health config: reuse previous `Arc<UpstreamRuntime>`;
- same ID + changed chain or health config: create new runtime object;
- removed ID: omit from new snapshot; existing sessions retain old `Arc` through leases;
- added ID: create new object.

Comparison helper:

```rust
fn upstream_runtime_compatible(
    old: &UpstreamRuntime,
    new_config: &CompiledUpstream,
) -> bool;
```

Do not preserve runtime state when the proxy chain or credentials change.

## Required tests

- router and health plan point to pointer-equal upstream `Arc`s;
- admin registry and router groups use pointer-equal objects;
- unchanged upstream retains health and counters after reload;
- changed upstream receives fresh health state;
- removed upstream drains safely through existing lease;
- no duplicate upstream runtime objects for one ID in a snapshot.

## Acceptance criteria

- remove standalone `build_upstream_runtimes()`;
- remove router code that constructs new `UpstreamRuntime` objects independently;
- one builder produces the complete runtime snapshot.

---

# Workstream 2: Replace split routing/admin generation with one source

## Problem

`RuntimeState.generation` and `SharedRoutingService` maintain separate generation values.

## Required design

Prefer one `ArcSwap<CompiledRuntimeSnapshot>` owned by `RuntimeState`.

```rust
pub struct RuntimeState {
    pub current: ArcSwap<CompiledRuntimeSnapshot>,
    pub metrics: Arc<MetricsRegistry>,
    pub readiness: AtomicBool,
    pub active_connections: AtomicU64,
    pub start_time: Instant,
}
```

Generation is read from:

```rust
state.current.load().generation
```

If retaining `SharedRoutingService`, make it load the router from `RuntimeState.current` or ensure both use the same atomic generation source. Do not keep two counters.

## Reload generation

Use one monotonic atomic allocator:

```rust
let next_generation = generation_counter.fetch_add(1, Ordering::SeqCst) + 1;
```

Compile candidate snapshot with that generation only after validation succeeds. If compilation fails, do not consume or publish the generation unless explicitly documented.

## Required tests

- admin status generation equals routing generation;
- route explanation generation equals status generation;
- successful reload increments exactly once;
- failed reload does not change current generation;
- concurrent reads never observe mismatched router/generation.

## Acceptance criteria

- one authoritative generation value exists.

---

# Workstream 3: Make health probes operate on router upstream objects

## Problem

Health currently runs, but scheduler eligibility is unaffected because probes update separate objects.

## Required integration

Build health targets from the snapshot’s shared registry:

```rust
pub struct HealthTarget {
    pub upstream: Arc<UpstreamRuntime>,
    pub probe: HealthProbe,
    pub config: HealthConfig,
}
```

Start manager with:

```rust
health_manager.start_targets(snapshot.health_plan.clone());
```

The scheduler must read the same `upstream.health` object modified by probes.

## Compile health configuration

Extend TOML/runtime model if needed:

```toml
[[upstreams]]
id = "proxy-a"
uri = "socks5://proxy-a:1080"

[upstreams.health]
mode = "tcp_connect"
interval = "30s"
timeout = "5s"
failures_to_unhealthy = 3
successes_to_healthy = 2
initial_state = "unknown"
```

Optional group-level defaults may be added, but per-upstream compilation is sufficient for closure.

Do not silently create probes for every upstream with hard-coded defaults unless documented. If health is omitted, choose and document one behavior:

- no active probe, health remains `Unknown` and compatibility policy allows it; or
- apply explicit global defaults.

## Reload behavior

On successful snapshot swap:

1. compare old and new health plans;
2. stop removed/changed targets;
3. retain unchanged probe tasks if manager supports reconciliation;
4. start added targets.

A simpler full restart is acceptable only if shared upstream objects are preserved and duplicate tasks cannot exist.

## Required binary-level tests

- start service with one healthy and one failing local upstream;
- verify `/upstreams` reports distinct states;
- verify routing excludes unhealthy upstream;
- recover endpoint and verify routing resumes selection;
- reload unchanged config and confirm health state remains;
- reload changed upstream and confirm state resets;
- repeated reload does not create duplicate probes.

## Acceptance criteria

- health state visibly changes scheduler behavior in a running service.

---

# Workstream 4: Fix listener identity and pre-bind listeners before readiness

## Problem A: wrong listener name

The runtime passes `local_addr.to_string()` into `ConnectionContext.listener`.

Fix:

```rust
let configured_listener_name = lcfg.name.clone();

ConnectionContext {
    source: Some(peer),
    listener: configured_listener_name.clone(),
}
```

Retain local address separately for logs and admin.

## Problem B: bind failures do not fail startup

Bind all required listeners before spawning accept loops.

Suggested prepared listener:

```rust
pub struct PreparedListener {
    pub config: CompiledListener,
    pub listener: TcpListener,
    pub local_addr: SocketAddr,
}
```

Startup sequence:

```rust
let mut prepared = Vec::new();
for config in snapshot.listeners.iter() {
    let listener = TcpListener::new(...).await?;
    let local_addr = listener.local_addr()?;
    prepared.push(PreparedListener { ... });
}
```

Only after every required listener binds successfully:

- start accept tasks;
- start admin;
- set readiness true.

On any bind failure:

- cancel already prepared listeners;
- stop health manager;
- return `RuntimeError::ListenerBind`;
- do not mark ready.

## Admin listener metadata

Build from actual prepared listeners:

```rust
pub struct ListenerInfo {
    pub name: String,
    pub configured_bind: String,
    pub local_addr: String,
    pub protocols: Vec<String>,
}
```

## Required tests

- listener-name matcher receives configured name;
- two listeners with different names route differently;
- occupied port causes `ServiceSupervisor::start` failure;
- readiness remains false on bind failure;
- admin status lists actual listeners and assigned ephemeral ports;
- zero-listener config rejected unless explicitly allowed.

## Acceptance criteria

- startup success implies all required listeners are bound.

---

# Workstream 5: Correct graceful shutdown token ordering

## Problem

One cancellation token currently stops listeners and active sessions simultaneously.

## Required tokens

```rust
pub struct ServiceSupervisor {
    listener_cancel: CancellationToken,
    connection_cancel: CancellationToken,
    health_cancel: CancellationToken,
    admin_cancel: CancellationToken,
    tasks: TaskTracker,
    connection_tasks: TaskTracker,
}
```

Connection tasks use `connection_cancel.child_token()`.
Listener loops use `listener_cancel.child_token()`.
Health uses `health_cancel`.

## Correct shutdown sequence

```text
1. readiness = false
2. listener_cancel.cancel()
3. stop accepting admin requests or leave health endpoint available by policy
4. health_cancel.cancel()
5. close listener task tracker
6. wait for active connections until grace deadline
7. if all drained: continue
8. if deadline reached: connection_cancel.cancel()
9. close connection task tracker
10. await all connection tasks
11. admin_cancel.cancel()
12. await all remaining tasks
```

Do not cancel connection token before the drain deadline.

## Suggested implementation

```rust
listener_cancel.cancel();
health_cancel.cancel();
listener_tasks.close();
listener_tasks.wait().await;

let drained = tokio::time::timeout(
    shutdown_grace,
    wait_until_zero(active_connections.clone()),
)
.await
.is_ok();

if !drained {
    connection_cancel.cancel();
}

connection_tasks.close();
connection_tasks.wait().await;
admin_cancel.cancel();
```

Use notification or task tracker completion rather than polling every 100 ms if practical.

## Required tests

- active tunnel remains usable during grace period;
- session that closes naturally is not cancelled;
- stuck session is cancelled after deadline;
- active count returns to zero;
- all connection tasks are joined;
- no new connections accepted after shutdown starts;
- readiness becomes false before drain;
- shutdown with zero connections completes immediately.

## Acceptance criteria

- “graceful shutdown” means drain first, cancel second.

---

# Workstream 6: Make admin consume live runtime state

## Problem

Admin currently receives a fixed `Arc<Router>` and empty listener/static data.

## Required state

```rust
pub struct AdminState {
    pub runtime: Arc<RuntimeState>,
    pub reload: Option<ReloadHandle>,
}
```

Avoid copying mutable runtime fields into admin state. Each request should load current snapshot:

```rust
let snapshot = state.runtime.current.load();
```

## Endpoint behavior

### `/-/ready`

- 200 with `ready` when runtime readiness is true;
- 503 with `not ready` when false.

### `/-/status`

Return:

- generation;
- uptime;
- readiness;
- active connections;
- actual listeners;
- upstream health summary.

### `/-/routes`

Read current snapshot router on every request.

### `/-/upstreams`

Read current shared upstream registry and expose:

- ID;
- health;
- enabled;
- active;
- in-flight;
- last probe metadata where available.

### `/-/config`

Return a redacted effective configuration summary from current snapshot.

### `/-/route-explain`

Use current snapshot and generation.

## Request limits

Add bounded request body handling for route explanation/reload:

```rust
const MAX_ADMIN_BODY: usize = 16 * 1024;
```

Reject larger bodies with 413.

## Route explanation context

Allow optional bounded fields:

```json
{
  "target": "example.com:443",
  "listener": "local",
  "protocol": "socks5",
  "source": "192.0.2.4:54321",
  "identity": "alice"
}
```

Identity is diagnostic input only; never return secrets.

## Required tests

- admin routes update immediately after reload;
- generation updates immediately after reload;
- upstream health/counters are live;
- readiness changes to 503 during shutdown;
- listener list is populated;
- oversized route-explain body returns 413;
- invalid source/identity fields return 400;
- config output contains no passwords.

## Acceptance criteria

- admin does not hold a stale router snapshot.

---

# Workstream 7: Define and implement honest reload semantics

## Decision required

Choose one of two closure-compatible approaches.

## Option A: Full Phase 2 hot reload

Reload:

- routing;
- upstream registry;
- health plan;
- timeouts for new sessions;
- PAC/static content;
- listener auth/protocol/limits where listener bind is unchanged;
- log filter;
- admin content.

Listener bind topology changes may still be restart-required.

## Option B: Explicit scoped reload

Support only:

- routing rules;
- upstreams/groups;
- health settings;
- PAC/static content;
- timeouts for new connections.

Reject listener/admin bind changes with:

```text
reload rejected: listener topology changed; restart required
```

This is acceptable for Phase 2 if documented clearly.

## Required reload transaction

1. load and validate full config;
2. compile candidate snapshot using previous snapshot for reconciliation;
3. classify unsupported changes;
4. prepare health/admin content;
5. atomically swap snapshot;
6. reconcile health tasks;
7. update metrics generation;
8. log change summary.

If any step before swap fails, retain old snapshot.

## Do not mutate old state during preparation

No old probes, admin data, or router state should be stopped until candidate compilation succeeds.

## Required tests

- valid routing reload changes new sessions;
- old active session continues on old selected route;
- invalid reload leaves old state intact;
- unsupported topology change returns restart-required;
- health state preserved for unchanged upstream;
- admin reflects new snapshot;
- generation increments exactly once;
- repeated reload has no task leak.

## Documentation wording

If Option B is chosen, README should say:

```text
- [x] Atomic routing/control-plane reload
- [ ] Listener topology hot reload
```

Do not use an unqualified “Configuration reload” checkbox.

---

# Workstream 8: Implement PAC/static runtime configuration or roll back claims

## Current inconsistency

PAC/static helper code exists, but the TOML model and runtime pass empty values.

Choose one path.

## Preferred path: implement configuration

Suggested TOML:

```toml
[admin.pac]
path = "/proxy.pac"
proxy = "PROXY 127.0.0.1:8080"
direct_fallback = true
direct_hosts = ["localhost"]
direct_suffixes = ["internal.example"]

[[admin.static]]
path = "/status.txt"
content_type = "text/plain; charset=utf-8"
body = "eggress online\n"
```

Optional file-backed content:

```toml
[[admin.static]]
path = "/policy.json"
content_type = "application/json"
file = "/etc/eggress/policy.json"
```

Validation:

- path begins with `/`;
- no duplicate paths;
- no collision with reserved admin endpoints;
- maximum body/file size;
- exactly one of `body` or `file`;
- safe PAC string escaping;
- no path traversal through request URL.

Compile PAC/static content into snapshot so reload updates atomically.

## Acceptable fallback path

If implementation is deferred:

- uncheck PAC serving and static HTTP endpoint;
- keep PAC generation checked only if library API is documented and tested;
- remove Phase 2 completion language claiming runtime serving;
- add explicit roadmap note for later integration.

## Required tests if implemented

- configured PAC served by running binary;
- static route served by running binary;
- HEAD behavior;
- path collision rejected;
- oversized body rejected;
- reload updates content;
- old content remains after invalid reload;
- PAC escaping snapshots.

## Acceptance criteria

- README matches executable behavior exactly.

---

# Workstream 9: Preserve direct-fallback metadata

## Problem

Direct fallback currently becomes a generic direct route and loses the selection reason.

## Required type change

```rust
pub enum SelectedRoute {
    Direct {
        decision: RouteDecision,
        selection_reason: SelectionReason,
    },
    Upstream { ... },
}
```

Use:

- `SelectionReason::Normal` for explicit direct route;
- `SelectionReason::DirectFallback` for fallback;
- `SelectionReason::UnhealthyFallback` for unhealthy upstream fallback.

Update:

- route descriptions;
- session reports;
- metrics labels;
- admin explanation;
- human-readable logs.

## Required tests

- explicit direct rule reports normal;
- group direct fallback reports direct-fallback;
- metrics distinguish them;
- route explanation displays fallback reason.

---

# Workstream 10: Refine failure categories

## Required changes

Add or use precise categories:

```rust
pub enum FailureCategory {
    Protocol,
    Authentication,
    HandshakeTimeout,
    Dns,
    ConnectionRefused,
    NetworkUnreachable,
    HostUnreachable,
    RouteTimeout,
    RouteHop,
    UpstreamAuthentication,
    PolicyDenied,
    Relay,
    Cancelled,
    Internal,
}
```

Mapping:

- `SessionOpenError::Hop` -> `RouteHop`;
- `io::ErrorKind::TimedOut` during route open -> `RouteTimeout`;
- cancellation -> `Cancelled`;
- unknown internal invariant -> `Internal`.

Do not classify route-opening failures as relay errors.

## Required tests

- hop failure category;
- route timeout category;
- relay reset category;
- cancellation category;
- policy denial category;
- metrics label values remain bounded.

---

# Workstream 11: Add true end-to-end integration tests

The README already acknowledges integration tests are still needed. This pass must add them.

## New test layout

```text
crates/eggress-runtime/tests/
├── startup.rs
├── routing.rs
├── health.rs
├── admin.rs
├── reload.rs
├── shutdown.rs
└── pac_static.rs
```

Use local ephemeral ports only.

## Required scenarios

### Startup

- full TOML config starts service;
- bind conflict fails startup;
- readiness becomes true only after binds succeed.

### Routing

- listener-name rule;
- source-CIDR rule;
- identity rule;
- round-robin distribution;
- least-connections with concurrent slow routes;
- direct fallback reason.

### Health

- failing upstream becomes unhealthy;
- scheduler stops selecting it;
- recovered upstream becomes eligible;
- reload preserves unchanged health.

### Admin

- status lists listeners;
- routes reflect current snapshot;
- upstreams reflect health and counters;
- metrics reflect real session;
- readiness returns 503 during shutdown.

### Reload

- valid reload changes routing;
- invalid reload preserves old routing;
- admin generation increments;
- unsupported topology change is explicit.

### Shutdown

- active tunnel drains during grace;
- stuck tunnel cancelled after grace;
- all tasks terminate.

### PAC/static

- execute if implemented;
- otherwise verify README boxes are unchecked.

## Test synchronization

- no arbitrary sleeps when a readiness signal is available;
- use admin readiness polling or explicit test hooks;
- all waits have deadlines;
- child processes or runtime handles always clean up.

## Acceptance criteria

- binary/runtime behavior, not just library functions, is covered.

---

# Workstream 12: Documentation and status reconciliation

## Immediate status

Set:

```text
Phase 2 final integration pass in progress
```

until all closure tests pass.

## README checkbox audit

Temporarily uncheck or annotate:

- active health checking;
- graceful shutdown;
- configuration reload;
- PAC serving;
- static HTTP endpoint;
- live route explanation if stale;
- listener-name rules if not yet fixed.

Re-check only in the same commit that adds integration tests.

## Required documentation updates

- runtime snapshot architecture;
- shared upstream registry;
- health configuration;
- reload scope;
- shutdown sequence;
- admin endpoint semantics;
- PAC/static configuration or deferral;
- route explanation context;
- failure categories.

Update:

- `README.md`;
- `docs/ARCHITECTURE.md`;
- `docs/ROADMAP.md`;
- `example-config.toml`;
- this plan’s completion record.

---

# Small-model execution sequence

Execute in this exact order unless compilation dependencies require a minor adjustment.

## Commit 1: Shared snapshot and upstream registry

- add `CompiledRuntimeSnapshot`;
- reconcile upstreams;
- build router groups from shared registry;
- add pointer-identity tests.

Do not change admin or reload yet.

## Commit 2: Health uses shared registry

- compile health plan;
- start probes on snapshot upstreams;
- add scheduler/health integration test.

## Commit 3: Listener identity and pre-bind startup

- pass configured listener name;
- pre-bind all listeners;
- fail startup on bind error;
- populate listener metadata.

## Commit 4: Unified generation and live admin state

- admin holds runtime state;
- remove stale router snapshot;
- connect readiness;
- add body-size limit;
- add generation tests.

## Commit 5: Correct shutdown sequencing

- split cancellation tokens;
- drain before forced cancellation;
- add shutdown integration tests.

## Commit 6: Atomic reload transaction

- compile candidate snapshot;
- classify unsupported changes;
- swap atomically;
- reconcile probes;
- add reload integration tests.

## Commit 7: PAC/static decision

Preferred:

- add TOML model;
- compile content;
- serve and reload;
- add tests.

Alternative:

- roll back README claims and document deferral.

Do not leave current inconsistency.

## Commit 8: Direct fallback metadata and failure categories

- preserve selection reason;
- refine failure mappings;
- update metrics/log tests.

## Commit 9: Complete end-to-end suite

- add remaining startup/routing/admin/health/reload tests;
- remove skipped or placeholder integration tests.

## Commit 10: Documentation and closure

- update README status/checklist;
- update architecture and roadmap;
- append completion record;
- verify CI.

---

# Code review checklist

## Shared state

- [ ] Does exactly one `Arc<UpstreamRuntime>` exist per upstream ID per snapshot?
- [ ] Do router, health, admin, and metrics share it?
- [ ] Is generation stored once?
- [ ] Does admin load current snapshot on every request?

## Startup/readiness

- [ ] Are listeners bound before readiness?
- [ ] Does any required bind failure abort startup?
- [ ] Does admin list actual listeners?
- [ ] Does `/-/ready` reflect runtime readiness?

## Shutdown

- [ ] Are listeners stopped before connection cancellation?
- [ ] Are sessions allowed to drain?
- [ ] Are stuck sessions cancelled after deadline?
- [ ] Are all tasks joined?

## Reload

- [ ] Is candidate config fully compiled before swap?
- [ ] Is old state untouched on failure?
- [ ] Are health tasks reconciled without duplication?
- [ ] Does admin immediately reflect new state?
- [ ] Are unsupported topology changes explicit?

## Documentation

- [ ] Are PAC/static claims accurate?
- [ ] Is reload scope explicit?
- [ ] Are all checked boxes backed by binary integration tests?

---

# Definition of done

The final Phase 2 integration pass is complete only when:

1. One compiled runtime snapshot owns router, upstream registry, listeners, admin content, and health plan.
2. Router, health manager, admin, metrics, and leases use the same upstream runtime objects.
3. Health transitions affect scheduler eligibility in a running service.
4. Configured listener names reach route matching.
5. Required listeners are bound before readiness and bind failure aborts startup.
6. Admin lists real listeners and uses current runtime state after reload.
7. One authoritative generation value is used everywhere.
8. `/-/ready` reflects startup and shutdown state.
9. Shutdown stops acceptance, drains sessions, then force-cancels only after the deadline.
10. All connection tasks are joined.
11. Reload is atomic, non-destructive on failure, and its supported scope is documented.
12. Health tasks reconcile correctly on reload.
13. PAC/static content is either fully configured and served or honestly marked incomplete.
14. Direct fallback retains explicit selection metadata.
15. Route-hop, timeout, cancellation, and relay failures have accurate categories.
16. Live route explanation uses current generation and current router.
17. Binary-level integration tests cover startup, routing, health, admin, reload, shutdown, and PAC/static policy.
18. README checkboxes correspond to integrated behavior, not isolated crate code.
19. All formatting, Clippy, unit, integration, audit, and interoperability checks pass.
20. No unsafe Rust, OpenSSL dependency, or native dependency is introduced.
21. Phase 3 begins only after these criteria are met.

## Completion record

```markdown
## Completion record

Implemented by commits:

- `d843f6f` — Fix post-audit gaps: G5 connection task tracking, G2 health probe reconciliation
- `7c6f4fb` — Phase 2 final integration pass: shared runtime snapshot, health config, graceful shutdown, atomic reload, PAC/static config, integration tests
- `0f9fd6b` — Remove dual generation counter from SharedRoutingService
- `eba1d38` — Move listener and admin config into CompiledRuntimeSnapshot
- `ae8b1e7` — Fix integration test timeouts: expose admin local addr, add AutoShutdown guard

All required checks passed on 2026-06-19.
```
