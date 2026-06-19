# Phase 2 Corrective Integration Plan

## Purpose

This plan closes the gap between the current Phase 2 component implementations and a fully integrated, operable service.

The repository now contains substantial Phase 2 code:

- a compiled routing rule engine;
- upstream groups and four scheduler implementations;
- active/in-flight lease types;
- a health state machine and probe manager;
- TOML parsing and validation;
- Prometheus-compatible metrics;
- an admin HTTP server;
- PAC and static-content generation;
- route explanation;
- an upstream test command;
- `ArcSwap`-based router replacement;
- basic graceful shutdown.

However, several of these capabilities currently exist only as library-level implementations or isolated tests. The running `eggress` binary does not yet compose them into one coherent runtime. Some scheduling and lease semantics are also incorrect under real traffic.

The corrective pass must preserve the existing crate boundaries while introducing a proper runtime composition layer and completing operational integration. Do not begin Phase 3 UDP or TLS work until this plan is complete.

---

# Primary defects to resolve

1. Round-robin scheduler state is recreated per route selection, so runtime round-robin behaves like first-available.
2. Pending leases are converted to active before route establishment succeeds.
3. Upstream-group fallback policies are defined but ignored.
4. `connect_timeout` is configured but not applied around route establishment.
5. Real connection metadata is not propagated into routing requests:
   - source address;
   - listener name;
   - authenticated identity.
6. TOML listeners, timeouts, authentication, and limits do not control the running service.
7. Upstream IDs are derived from string length and are not unique.
8. Router construction silently discards invalid groups and rules via `filter_map`.
9. Active health checking is not started by the binary and health configuration is not compiled into runtime state.
10. Metrics, admin, PAC, and static-content services are not started by the binary.
11. Reload only swaps routing rules and does not reconcile health, admin, metrics metadata, or listener-affecting changes.
12. Graceful shutdown tracks only a global counter and cannot cancel or join active connection tasks after the deadline.
13. The upstream test command performs only a TCP connect to the first hop and does not test the configured proxy chain or requested target.
14. Route explanation does not reflect persistent scheduler state and is not tied to a live runtime generation.
15. TOML matcher support is narrower than the router matcher model.
16. README and roadmap currently mark several library-only capabilities as complete.

---

# Scope

## Included

- persistent scheduler state;
- correct pending-to-active lease lifecycle;
- group fallback semantics;
- route-open timeout enforcement;
- routing metadata propagation;
- stable unique upstream identifiers;
- strict router construction;
- runtime composition for listeners, health, metrics, admin, PAC, static routes, reload, and shutdown;
- full TOML-driven service startup;
- active health-probe lifecycle and reload reconciliation;
- supervised connection tasks;
- real chain-level upstream testing;
- live route explanation;
- expanded TOML matcher support;
- README and roadmap reconciliation;
- integration tests proving binary-level behavior.

## Excluded

- UDP;
- TLS listeners;
- HTTP/2 or HTTP/3;
- QUIC;
- system proxy mutation;
- transparent proxying;
- reverse tunnels;
- persistent storage;
- distributed control plane;
- plugin systems;
- new proxy protocols.

---

# Architectural target

The current CLI has become the de facto service runtime. Phase 2 corrective integration should introduce a dedicated runtime crate rather than continuing to grow `eggress-cli`.

Recommended crate:

```text
crates/eggress-runtime/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── build.rs
    ├── supervisor.rs
    ├── listener.rs
    ├── reload.rs
    ├── state.rs
    ├── shutdown.rs
    └── error.rs
```

Target dependency flow:

```text
eggress-cli
    -> eggress-runtime
        -> eggress-config
        -> eggress-routing
        -> eggress-server
        -> eggress-health/routing::health
        -> eggress-metrics
        -> eggress-admin
```

The CLI should be reduced to:

- argument parsing;
- config path selection;
- subcommand dispatch;
- logging bootstrap;
- invoking `Runtime::start()`;
- mapping runtime result to process exit code.

The runtime crate should own:

- validated runtime construction;
- listener startup;
- shared routing service;
- health manager;
- metrics registry;
- admin server;
- reload coordination;
- connection task tracking;
- graceful shutdown.

---

# Workstream 1: Correct scheduler ownership and persistence

## Problem

`Router::select` currently constructs a fresh scheduler object on each call. This resets round-robin state and prevents stateful scheduler behavior.

## Required design

Move scheduler state into `UpstreamGroup`.

```rust
pub struct UpstreamGroup {
    pub id: UpstreamGroupId,
    pub scheduler_kind: SchedulerKind,
    pub scheduler: Arc<dyn Scheduler>,
    pub members: Arc<[Arc<UpstreamRuntime>]>,
    pub fallback: GroupFallback,
}
```

Construct scheduler once:

```rust
impl UpstreamGroup {
    pub fn new(
        id: UpstreamGroupId,
        scheduler_kind: SchedulerKind,
        members: Arc<[Arc<UpstreamRuntime>]>,
        fallback: GroupFallback,
    ) -> Self {
        Self {
            id,
            scheduler_kind,
            scheduler: scheduler::resolve_scheduler(scheduler_kind),
            members,
            fallback,
        }
    }
}
```

`resolve_scheduler` should return `Arc<dyn Scheduler>` rather than `Box<dyn Scheduler>` if the group is shared.

```rust
pub fn resolve_scheduler(kind: SchedulerKind) -> Arc<dyn Scheduler> {
    match kind {
        SchedulerKind::FirstAvailable => Arc::new(FirstAvailableScheduler),
        SchedulerKind::RoundRobin => Arc::new(RoundRobinScheduler::new()),
        SchedulerKind::Random => Arc::new(RandomScheduler::new()),
        SchedulerKind::LeastConnections => Arc::new(LeastConnectionsScheduler),
    }
}
```

Then selection becomes:

```rust
let selected = upstream_group
    .scheduler
    .select(upstream_group, &candidates, request)
    .ok_or_else(|| RouteError::NoEligibleUpstream(group.clone()))?;
```

## Random scheduler testability

Give `RandomScheduler` an injectable or seedable RNG source in tests.

A simple implementation may hold an atomic xorshift state or use a trait:

```rust
pub trait RandomIndex: Send + Sync {
    fn index(&self, upper: usize) -> usize;
}
```

Production may use `fastrand`; tests should use a deterministic implementation.

## Required tests

- round-robin group selects A, B, C, A across separate calls;
- state persists across `Arc<Router>` clones;
- explanation does not mutate cursor;
- reload creates new scheduler state only when group identity/config changes;
- unchanged group state may be preserved when practical;
- random scheduler only chooses eligible members;
- seeded random sequence is deterministic;
- least-connections tie-breaking remains stable.

## Acceptance criteria

- no scheduler object is created in `Router::select`;
- round-robin passes a binary-level integration test with multiple connections;
- scheduler state is owned by the group or runtime snapshot.

---

# Workstream 2: Correct lease lifecycle

## Problem

`PendingLease::established()` is currently called during route selection, before the route opens.

Correct lifecycle:

```text
select upstream
-> increment in_flight
-> attempt route open
-> on success: decrement in_flight, increment active
-> run session
-> on session completion: decrement active
```

## Required type changes

Change selected route:

```rust
pub enum SelectedRoute {
    Direct {
        decision: RouteDecision,
    },
    Upstream {
        decision: RouteDecision,
        group: UpstreamGroupId,
        upstream: UpstreamId,
        chain: Arc<ProxyChainSpec>,
        pending_lease: PendingLease,
    },
}
```

`PendingLease` must not expose a path that can be called twice.

```rust
pub struct PendingLease {
    upstream: Arc<UpstreamRuntime>,
    armed: bool,
}

impl PendingLease {
    pub fn new(upstream: Arc<UpstreamRuntime>) -> Self {
        upstream.in_flight.fetch_add(1, Ordering::Relaxed);
        Self { upstream, armed: true }
    }

    pub fn establish(mut self) -> ActiveLease {
        self.upstream.in_flight.fetch_sub(1, Ordering::Relaxed);
        self.upstream.active.fetch_add(1, Ordering::Relaxed);
        self.armed = false;
        ActiveLease {
            upstream: self.upstream.clone(),
        }
    }
}

impl Drop for PendingLease {
    fn drop(&mut self) {
        if self.armed {
            self.upstream.in_flight.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
```

Route opening should return the connected stream plus active lease:

```rust
pub struct OpenedRoute {
    pub stream: BoxStream,
    pub metadata: RouteMetadata,
    pub active_lease: Option<ActiveLease>,
}
```

Example:

```rust
match selected {
    SelectedRoute::Upstream {
        chain,
        pending_lease,
        ..
    } => {
        let stream = timeout(connect_timeout, executor.execute(&chain.hops, target)).await??;
        let active_lease = pending_lease.establish();
        Ok(OpenedRoute {
            stream,
            metadata,
            active_lease: Some(active_lease),
        })
    }
    // ...
}
```

The active lease must remain alive for the entire relay or HTTP-forward exchange.

## Required tests

- in-flight increments immediately after selection;
- failed route open returns in-flight to zero;
- connect timeout returns in-flight to zero;
- successful open moves count from in-flight to active;
- active decrements after tunnel completion;
- active decrements after HTTP-forward completion;
- active decrements on client cancellation;
- no underflow under repeated failure;
- least-connections sees concurrent pending attempts.

## Acceptance criteria

- no active count is incremented before route establishment;
- counters are correct under success, error, timeout, and cancellation.

---

# Workstream 3: Implement group fallback behavior

## Problem

`GroupFallback` is currently ignored.

## Required semantics

### Reject

No eligible upstream produces a route error and protocol-specific policy failure.

### Direct

No eligible upstream produces a direct route.

### UseUnhealthy

If no healthy/eligible upstream exists, select from enabled but unhealthy members using the group scheduler.

Do not include disabled members.

## Recommended route metadata

Record fallback use explicitly:

```rust
pub enum SelectionReason {
    Normal,
    DirectFallback,
    UnhealthyFallback,
}
```

Include this in `SelectedRoute`, session reports, logs, metrics, and route explanation.

## Selection example

```rust
let healthy: Vec<_> = group
    .members
    .iter()
    .filter(|m| health::is_eligible(m))
    .cloned()
    .collect();

if !healthy.is_empty() {
    return select_group_member(group, healthy, SelectionReason::Normal);
}

match group.fallback {
    GroupFallback::Reject => Err(RouteError::NoEligibleUpstream(group.id.clone())),
    GroupFallback::Direct => Ok(SelectedRoute::DirectFallback { ... }),
    GroupFallback::UseUnhealthy => {
        let enabled: Vec<_> = group
            .members
            .iter()
            .filter(|m| m.is_enabled())
            .cloned()
            .collect();
        select_group_member(group, enabled, SelectionReason::UnhealthyFallback)
    }
}
```

## Required tests

- reject fallback returns policy failure;
- direct fallback opens target directly;
- unhealthy fallback selects unhealthy enabled member;
- disabled members are never selected;
- fallback appears in session report;
- fallback metric increments once;
- route explanation clearly indicates fallback behavior.

## Acceptance criteria

- all three configured fallback modes are operational in the binary.

---

# Workstream 4: Apply route-open timeout

## Problem

`connect_timeout` is present but unused.

## Required implementation

Apply timeout around the complete direct or chained route opening operation:

```rust
let result = tokio::time::timeout(config.connect_timeout, async {
    match selected {
        SelectedRoute::Direct { .. } => DirectConnector.connect(target).await,
        SelectedRoute::Upstream { chain, .. } => {
            executor.execute(&chain.hops, target).await
        }
    }
})
.await;
```

Normalize elapsed timeout as `SessionOpenError::Timeout` and `FailureCategory::RouteTimeout`.

The timeout should include:

- DNS resolution performed by connector;
- TCP connect;
- upstream protocol handshake;
- all chain hops;
- final target establishment.

It should not include the subsequent relay lifetime.

## Required tests

Use a controlled connector or black-hole test server:

- direct route timeout;
- HTTP-upstream handshake timeout;
- SOCKS-upstream handshake timeout;
- multi-hop timeout;
- in-flight lease cleanup on timeout;
- correct HTTP 504 mapping where applicable;
- correct SOCKS failure code;
- successful route within deadline.

## Acceptance criteria

- `connect_timeout` is used exactly once at the route-open boundary;
- no nested timeout accidentally shortens relay lifetime.

---

# Workstream 5: Propagate real routing context

## Problem

Routing currently receives empty listener name, no source address, and anonymous identity.

## Required connection context

Add:

```rust
#[derive(Clone)]
pub struct ConnectionContext {
    pub source: Option<SocketAddr>,
    pub listener: ListenerName,
}
```

Extend `ConnectionConfig`:

```rust
pub struct ConnectionConfig {
    pub routing: Arc<dyn RouteService>,
    pub route_executor: Arc<RouteExecutor>,
    pub context: ConnectionContext,
    pub handshake_timeout: Duration,
    pub connect_timeout: Duration,
    pub protocols: Arc<[ProtocolId]>,
    pub authentication: InboundAuthentication,
}
```

## Preserve authenticated identity

Authentication should return identity with the accepted session.

Recommended envelope:

```rust
pub struct AcceptedConnection {
    pub identity: ClientIdentity,
    pub session: AcceptedSession,
}
```

`accept()` returns:

```rust
Result<AcceptedConnection, AcceptError>
```

For unauthenticated connections:

```rust
ClientIdentity::Anonymous
```

For authenticated HTTP/SOCKS5:

```rust
ClientIdentity::Username(username)
```

Do not retain passwords.

## Route request construction

```rust
let request = RouteRequest {
    target: &pending.target,
    source: config.context.source,
    listener: config.context.listener.as_ref(),
    inbound_protocol: protocol,
    identity: &accepted.identity,
};
```

## Required tests

- source-CIDR rule matches real client address;
- listener-name rule differentiates two listeners;
- HTTP Basic username rule matches authenticated identity;
- SOCKS5 username rule matches authenticated identity;
- anonymous identity does not match username rule;
- password never appears in route request, logs, admin, or metrics.

## Acceptance criteria

- all router matcher fields have a path from real runtime data.

---

# Workstream 6: Replace unstable upstream IDs

## Problem

Upstream IDs are currently generated from textual ID length, causing collisions.

## Required type

Prefer a textual opaque ID:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct UpstreamId(Arc<str>);
```

Move or redefine the current numeric `UpstreamId` if necessary. If a breaking migration is too broad, introduce `RuntimeUpstreamId` inside `eggress-routing` and use it for all Phase 2 state.

## Requirements

- ID equals validated configuration ID;
- stable across reload when textual ID is unchanged;
- used in logs, metrics, admin, explanation, and selection reports;
- no collision based on length or hash truncation;
- redaction not needed for ID, but IDs must not contain secrets.

## Validation

Restrict IDs to a safe bounded form:

```text
[A-Za-z0-9][A-Za-z0-9._-]{0,63}
```

This supports safe metric labels and logs.

## Required tests

- `proxy-a` and `proxy-b` remain distinct;
- duplicate ID rejected;
- stable ID across reload;
- metrics distinguish upstreams;
- group duplicate-member validation uses textual identity.

## Acceptance criteria

- no ID is synthesized from string length.

---

# Workstream 7: Make router construction strict and reusable

## Problem

The CLI currently rebuilds runtime routing structures and silently drops invalid entries with `filter_map`.

## Required ownership

Move router construction into `eggress-config::compile` or `eggress-runtime::build`.

Preferred API:

```rust
pub struct CompiledRuntime {
    pub config: Arc<EffectiveConfig>,
    pub router: Router,
    pub upstreams: HashMap<UpstreamId, Arc<UpstreamRuntime>>,
    pub health_plan: HealthPlan,
    pub listeners: Vec<CompiledListener>,
    pub admin: Option<CompiledAdmin>,
}

pub fn compile_runtime(file: ConfigFile) -> Result<CompiledRuntime, ConfigError>;
```

No second validation pass should be required in CLI.

## Strict failure rules

Return an error for:

- unknown group reference;
- unknown upstream member;
- empty compiled group;
- duplicate upstream ID;
- duplicate group ID;
- invalid fallback;
- rule action referencing missing group;
- unsupported protocol in chain;
- missing listener protocol;
- unresolved secret;
- invalid admin bind policy.

Do not silently skip anything.

## Required tests

- every previously filtered case fails compilation;
- error contains field path and referenced ID;
- no secret values appear in error;
- compiled router contains exact expected rule/group count.

## Acceptance criteria

- `eggress-cli` does not construct `Router` manually;
- invalid policy cannot disappear silently.

---

# Workstream 8: Introduce the integrated runtime supervisor

## Runtime state

```rust
pub struct RuntimeState {
    pub generation: AtomicU64,
    pub routing: Arc<SharedRoutingService>,
    pub metrics: Arc<MetricsRegistry>,
    pub readiness: Arc<AtomicBool>,
    pub effective_config: arc_swap::ArcSwap<EffectiveConfig>,
    pub upstreams: arc_swap::ArcSwap<UpstreamRegistry>,
}
```

## Supervisor

```rust
pub struct ServiceSupervisor {
    cancel: CancellationToken,
    connection_cancel: CancellationToken,
    listeners: ListenerManager,
    health: HealthManager,
    admin: Option<AdminHandle>,
    tasks: TaskTracker,
    state: Arc<RuntimeState>,
}
```

`tokio_util::task::TaskTracker` is preferred for connection tasks.

## Startup order

1. parse and compile full configuration;
2. initialize metrics;
3. construct routing and upstream state;
4. start health manager;
5. bind all required listeners;
6. start admin server;
7. mark readiness true;
8. begin signal loop.

If any required listener or admin bind fails, shut down already-started tasks and exit nonzero.

## Failure policy

- invalid initial config: exit nonzero;
- required listener failure: exit nonzero;
- admin failure: configurable fatal/nonfatal, default fatal if enabled;
- health probe task failure: log and restart or mark degraded;
- reload failure: retain old generation and continue.

## Acceptance criteria

- CLI main becomes a thin wrapper;
- one runtime object owns all long-lived services.

---

# Workstream 9: Make TOML configuration authoritative

## Problem

TOML currently influences routing but not listener startup or service settings.

## Required behavior

When `--config` is supplied:

- listeners come from TOML;
- listener names come from TOML;
- bind addresses come from TOML;
- protocol lists come from TOML;
- authentication comes from TOML;
- connection limits come from TOML;
- handshake/connect timeouts come from TOML;
- admin comes from TOML;
- health comes from TOML;
- PAC/static content comes from TOML;
- process log/shutdown settings come from TOML where supported.

Do not also apply `-l` and `-r` silently. Either reject mixed mode or implement documented override semantics.

Recommended rule:

```text
--config mode and compatibility URI mode are mutually exclusive,
except for --log-format and --log-level overrides.
```

## Compiled listener

```rust
pub struct CompiledListener {
    pub name: ListenerName,
    pub bind: SocketAddr,
    pub protocols: Arc<[ProtocolId]>,
    pub authentication: InboundAuthentication,
    pub connection_limit: usize,
    pub handshake_timeout: Duration,
    pub connect_timeout: Duration,
}
```

## Listener manager

Start one task per compiled listener. Pass real listener name and peer address into `ConnectionContext`.

## Required tests

- TOML listener bind is actually used;
- TOML protocol restrictions are enforced;
- TOML auth is enforced;
- TOML connection limit is enforced;
- TOML timeouts are applied;
- multiple listeners route differently by listener-name matcher;
- compatibility CLI mode still works without config;
- mixed config/URI mode is rejected clearly.

## Acceptance criteria

- no hard-coded 30-second timeouts or 1024 connection limit remain in runtime listener startup.

---

# Workstream 10: Integrate active health management

## Configuration compilation

Extend compiled upstream configuration:

```rust
pub struct CompiledUpstream {
    pub id: UpstreamId,
    pub chain: ProxyChainSpec,
    pub health: Option<CompiledHealthProbe>,
}

pub enum CompiledHealthProbe {
    TcpConnect {
        target: SocketAddr,
        config: HealthConfig,
    },
    ProxyConnect {
        target: TargetAddr,
        config: HealthConfig,
    },
}
```

At minimum, TCP connect should be fully integrated. Proxy-connect probing may be added in the same pass if it reuses `RouteExecutor` cleanly.

## Runtime startup

- attach probe configuration to each upstream;
- start `HealthManager` from compiled plan;
- expose health state to scheduler and admin;
- update health metrics on transitions;
- cancel probes on shutdown.

## Reload reconciliation

Use textual upstream ID plus chain/probe equivalence.

Rules:

- unchanged ID + unchanged chain/probe: preserve runtime object, health, counters, scheduler references;
- unchanged ID + changed chain/probe: create new runtime object and reset health;
- removed ID: stop probe, retain old object only through active session `Arc`s;
- added ID: create state and start probe.

Do not rebuild every upstream unconditionally.

## Passive observations

Add optional passive health observation only for failures attributable to the upstream itself:

- TCP connect to upstream failed;
- upstream authentication failed;
- upstream protocol handshake malformed.

Do not mark an upstream unhealthy because a destination refused connection.

## Required tests

- configured probe actually runs in binary-level test;
- healthy transition after threshold;
- unhealthy exclusion affects scheduler;
- recovery restores eligibility;
- reload preserves unchanged health;
- removed probe task stops;
- no duplicate probe loops after repeated reload;
- health metrics/admin update.

## Acceptance criteria

- `Active health checking` means probes run in the actual service process.

---

# Workstream 11: Integrate metrics into the data path

## Runtime registry

Create one `Arc<MetricsRegistry>` at startup and pass it to:

- listener manager;
- server session completion hook;
- router selection;
- health manager;
- reload coordinator;
- admin server.

## Session recording

Prefer a callback or reporter interface rather than coupling `eggress-server` directly to Prometheus types.

```rust
pub trait SessionObserver: Send + Sync {
    fn connection_started(&self, meta: &ConnectionMeta);
    fn connection_finished(&self, report: &SessionReport);
}
```

A no-op observer preserves embeddability.

## Required metrics

At minimum:

- active connections;
- total connections;
- failures by category;
- bytes up/down;
- route decisions by rule/action;
- upstream selections;
- upstream active/in-flight;
- upstream connect attempts/failures;
- health state/probe results;
- config generation;
- reload success/failure.

## Cardinality controls

Never label by:

- target host;
- source IP;
- username;
- raw URI;
- error string.

## Required tests

- running binary exposes metrics through admin;
- one completed session increments exactly once;
- active gauge returns to zero;
- round-robin selections attributed to correct upstream IDs;
- reload generation updates;
- no secret or target label appears.

## Acceptance criteria

- metrics are not merely crate-local tests; they reflect real proxy traffic.

---

# Workstream 12: Integrate admin, PAC, and static content

## Admin state

Current admin state must be expanded to reference live runtime state.

```rust
pub struct AdminState {
    pub metrics: Arc<MetricsRegistry>,
    pub runtime: Arc<RuntimeState>,
    pub reload: Option<ReloadHandle>,
}
```

Static routes and PAC should come from `RuntimeState.effective_config`, not fixed startup-only vectors.

## Required endpoints

- `GET /-/health`: process alive;
- `GET /-/ready`: readiness flag;
- `GET /-/status`: generation, uptime, listeners, active connections, health summary;
- `GET /-/routes`: compiled routing rules and default action;
- `GET /-/upstreams`: live upstream IDs, health, active, in-flight, last probe;
- `GET /-/config`: redacted effective configuration;
- `GET /metrics`: live Prometheus output;
- `POST /-/reload`: optional and protected by loopback/admin policy;
- configured PAC path;
- configured static paths.

The current `/-/routes` endpoint must not return static HTTP routes.

## Bind security

- default loopback only;
- reject non-loopback unless explicitly allowed;
- if non-loopback is allowed, require admin authentication or keep reload disabled;
- validate request method and bounded request head.

## Required tests

- admin starts from TOML;
- readiness changes false during shutdown;
- routes endpoint reflects router rules;
- upstream endpoint reflects live health and counters;
- config endpoint redacts secrets;
- PAC/static content update after reload;
- metrics endpoint reflects real session;
- reload endpoint disabled by default;
- non-loopback unsafe configuration rejected.

## Acceptance criteria

- all admin/PAC/static README checkboxes correspond to behavior in the shipped binary.

---

# Workstream 13: Implement coherent reload semantics

## Reload transaction

```text
read file
-> parse
-> resolve secrets
-> validate
-> compile complete candidate runtime
-> diff current vs candidate
-> prepare new listeners/admin/health resources
-> atomically swap safe state
-> retire removed resources
```

Invalid reload must leave old runtime unchanged.

## Change classes

### Hot-reloadable

- routing rules;
- group membership;
- scheduler settings;
- fallback settings;
- upstream chains;
- health settings;
- PAC/static content;
- log filter;
- admin response data;
- timeouts for new connections.

### Listener topology

Choose and document one implementation:

#### Preferred

Reconcile listeners:

1. bind new/changed listeners first;
2. if all binds succeed, publish new generation;
3. stop removed listeners;
4. retain unchanged listeners.

#### Acceptable temporary restriction

Reject reload if listener topology changes and report `restart required`.

Do not silently ignore listener changes.

### Admin bind changes

Treat similarly to listener topology. Either reconcile or reject as restart-required.

## Snapshot consistency defect to avoid

`decide()` and `select()` currently load `ArcSwap` separately. A reload between calls can produce a decision from generation N and selection from generation N+1.

Fix by routing through one loaded snapshot:

```rust
pub struct RoutePlan {
    snapshot: Arc<RoutingSnapshot>,
    decision: RouteDecision,
}

pub trait RouteService {
    fn plan(&self, request: &RouteRequest<'_>) -> Result<RoutePlan, RouteError>;
}
```

Then selection uses the same snapshot:

```rust
impl RoutePlan {
    pub fn select(self, request: &RouteRequest<'_>) -> Result<SelectedRoute, RouteError>;
}
```

Or use a single `route(request)` call.

## Generation

Use one monotonic atomic generation source owned by runtime, not an `AtomicU64` embedded separately in each swapped object.

```rust
let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
```

## Required tests

- invalid reload leaves old generation active;
- decision and selection never cross snapshots;
- concurrent routing during reload sees complete old or new configuration;
- unchanged health state preserved;
- changed upstream reset;
- PAC/static content atomically updated;
- listener change either reconciles or returns restart-required;
- repeated reload does not leak tasks;
- generation strictly increases.

## Acceptance criteria

- reload semantics are explicit and operationally coherent.

---

# Workstream 14: Supervise connections and implement forced shutdown

## Problem

Current shutdown only polls a global count and cannot cancel or join connection tasks.

## Required task model

Use `TaskTracker` plus child cancellation token:

```rust
pub struct ConnectionSupervisor {
    tracker: TaskTracker,
    cancel: CancellationToken,
    active: Arc<AtomicU64>,
}
```

Spawn:

```rust
let child_cancel = self.cancel.child_token();
self.tracker.spawn(async move {
    let _guard = ActiveConnectionGuard::new(active);
    serve_connection_with_cancel(stream, config, child_cancel).await
});
```

Server relay should observe cancellation. If relay currently has no cancellation-aware API, wrap it:

```rust
tokio::select! {
    result = relay(client, upstream) => result,
    _ = cancel.cancelled() => cancelled_report(),
}
```

## Shutdown order

1. readiness false;
2. stop accepting new connections;
3. stop admin acceptance;
4. stop health probes;
5. close task tracker;
6. wait for connections until grace deadline;
7. cancel connection token;
8. await tracker completion;
9. exit.

## Required tests

- new connections rejected after shutdown begins;
- active session drains normally before deadline;
- stuck session cancelled after deadline;
- active counters return to zero;
- all tasks joined;
- health/admin/listeners stop;
- readiness false before drain;
- repeated start/shutdown leaves no leaked tasks.

## Acceptance criteria

- no detached connection tasks remain after runtime shutdown.

---

# Workstream 15: Make upstream testing real

## Problem

The current command only TCP-connects to the first hop. `--target` is informational only.

## Required modes

```text
egress upstream test --config file.toml
    --id proxy-a
    --target example.com:443
    --mode tcp|proxy
    --json
```

### TCP mode

- tests reachability of first hop;
- explicitly labels result `tcp_reachable`;
- does not claim target reachability.

### Proxy mode

- executes the configured chain to the supplied target using the shared `RouteExecutor`;
- performs upstream authentication and all chain handshakes;
- reports successful target establishment;
- closes immediately after connection.

Default should be `proxy` when a target is supplied.

## Result model

```rust
#[derive(Serialize)]
pub struct UpstreamTestResult {
    pub upstream_id: UpstreamId,
    pub mode: TestMode,
    pub target: Option<String>,
    pub success: bool,
    pub latency_ms: u64,
    pub failure: Option<FailureCategory>,
    pub failed_hop: Option<usize>,
}
```

Do not print raw credentials or full unredacted chain.

## Required tests

- TCP mode success/failure;
- SOCKS5 proxy mode reaches local target;
- HTTP CONNECT proxy mode reaches local target;
- authenticated upstream test;
- multi-hop chain;
- target failure distinguished from proxy failure;
- timeout;
- JSON schema;
- no secrets.

## Acceptance criteria

- README “upstream test command” means actual proxy-path validation.

---

# Workstream 16: Make route explanation reflect live runtime

## Offline explanation

`egress route explain --config file target` may continue to compile a snapshot offline.

It must clearly label:

```text
Mode: offline
Generation: not-live
```

## Live explanation

Add admin-backed or local control command:

```text
egress route explain --admin http://127.0.0.1:9090 target
```

or a local Unix socket later. For Phase 2, an admin JSON endpoint is sufficient:

```text
POST /-/route-explain
```

with bounded JSON body.

## Scheduler preview

Do not mutate scheduler state.

- first-available: exact preview;
- least-connections: exact preview from counters;
- round-robin: expose next candidate through a nonmutating `peek` method;
- random: report eligible candidates and `selection: nondeterministic`, unless a seed is supplied.

Extend scheduler trait:

```rust
pub trait Scheduler {
    fn select(...);
    fn preview(...)-> SchedulerPreview;
}
```

## Required tests

- live explanation uses actual generation;
- round-robin preview does not consume cursor;
- random preview is not falsely deterministic;
- fallback shown;
- unhealthy exclusion shown;
- credentials redacted.

## Acceptance criteria

- explanation never presents a fabricated scheduler selection as authoritative.

---

# Workstream 17: Expand TOML matcher coverage

## Required schema support

Expose the implemented router model:

- exact host;
- suffix;
- regex;
- destination CIDR;
- exact port;
- port range;
- port set;
- source CIDR;
- listener;
- protocol;
- identity;
- all;
- any-of;
- not.

Recommended recursive schema:

```toml
[[rules]]
id = "proxy-secure-example"
action = { upstream_group = "internet" }

[rules.match]
all = [
  { host_suffix = "example.com" },
  { any_of = [
      { destination_port = 443 },
      { destination_port_range = [8443, 8499] }
  ]},
  { not = { source_cidr = "10.0.0.0/8" } }
]
```

Use an internally tagged or untagged Serde enum only if error messages remain intelligible. Otherwise define a validated intermediary AST.

## Limits

- maximum expression depth;
- maximum nodes per rule;
- maximum regex length;
- maximum port-set size.

## Compatibility regex files

Wire the existing compatibility parser into TOML and CLI options. File errors must include line number.

## Required tests

- each matcher round-trips through TOML;
- composite expressions;
- excessive depth rejected;
- invalid CIDR;
- invalid port range;
- unknown protocol;
- identity routing with authenticated client;
- compatibility regex file.

## Acceptance criteria

- README only checks matchers configurable through supported user-facing configuration.

---

# Workstream 18: README and roadmap correction during implementation

## Immediate status correction

At the beginning of this corrective pass, change Phase 2 status to:

```text
Phase 2 corrective integration in progress
```

Temporarily uncheck or annotate capabilities that are currently library-only:

- round-robin scheduling;
- least-connections scheduling;
- active health checking;
- direct fallback;
- TOML configuration as full service config;
- configuration reload;
- per-upstream metrics;
- Prometheus endpoint;
- local admin API;
- PAC serving;
- static HTTP endpoint;
- upstream test command.

Do not leave them checked based solely on unit-test existence.

## Re-check policy

A feature may be checked only after:

1. binary integration exists;
2. configuration path exists;
3. integration tests exercise it;
4. documentation describes behavior and limitations;
5. applicable CI passes.

## Final documentation

Update:

- `README.md`;
- `docs/ROADMAP.md`;
- `docs/ARCHITECTURE.md`;
- config example;
- admin API documentation;
- metrics reference;
- reload limitations;
- upstream test semantics.

---

# Recommended execution sequence

Use small commits. Do not repeat the previous single-commit Phase 2 implementation pattern.

## Commit 1: Correct scheduler ownership

- persistent scheduler in group;
- round-robin integration tests;
- deterministic random tests;
- route explanation preview API skeleton.

## Commit 2: Correct lease lifecycle and fallback

- pending lease in selected route;
- activation after successful connect;
- fallback semantics;
- counter tests.

## Commit 3: Apply connect timeout and snapshot-consistent route planning

- single snapshot route plan;
- timeout around route opening;
- failure mapping tests.

## Commit 4: Stable upstream IDs and strict runtime compilation

- textual IDs;
- remove `filter_map` policy loss;
- compile full router/upstream registry in config/runtime crate.

## Commit 5: Propagate source, listener, and identity

- accepted connection envelope;
- connection context;
- real matcher integration tests.

## Commit 6: Add `eggress-runtime` supervisor

- runtime state;
- listener manager;
- task tracker;
- thin CLI integration.

## Commit 7: Make TOML listeners/timeouts/auth authoritative

- full config mode startup;
- mixed-mode rejection;
- listener tests.

## Commit 8: Integrate health manager

- compiled health plan;
- startup/shutdown;
- health-aware selection;
- metrics hooks.

## Commit 9: Integrate metrics and session observer

- one registry;
- real data-path metrics;
- bounded labels.

## Commit 10: Integrate admin/PAC/static services

- live runtime state endpoints;
- redacted config;
- metrics endpoint;
- PAC/static from config.

## Commit 11: Implement coherent reload

- complete candidate compilation;
- atomic route snapshot;
- health reconciliation;
- PAC/static update;
- listener topology policy.

## Commit 12: Supervised graceful shutdown

- tracked connection tasks;
- forced cancellation after grace;
- readiness transitions.

## Commit 13: Real upstream test command

- TCP and proxy modes;
- shared route executor;
- JSON output.

## Commit 14: Live route explanation

- scheduler preview;
- live generation;
- admin endpoint or live control path.

## Commit 15: Expand TOML matchers

- CIDR/source/listener/protocol/identity/composites;
- limits and tests.

## Commit 16: Documentation and Phase 2 closure

- README reconciliation;
- roadmap completion record;
- architecture diagrams;
- final CI verification.

---

# Required binary-level integration matrix

| Capability | Required proof |
|---|---|
| Round robin | Four real connections distribute A, B, C, A |
| Least connections | Concurrent slow opens prefer less-loaded upstream |
| Direct fallback | Unhealthy group routes directly |
| Use-unhealthy fallback | Unhealthy enabled upstream selected only when configured |
| Connect timeout | Black-hole route fails within configured bound |
| Listener rule | Two TOML listeners select different routes |
| Source CIDR rule | Real peer address affects decision |
| Identity rule | Authenticated HTTP/SOCKS5 username affects route |
| Health exclusion | Probe failure removes upstream from selection |
| Health recovery | Probe success restores upstream |
| Metrics | Real proxy session changes scrape output |
| Admin routes | Endpoint reflects compiled routing rules |
| Admin upstreams | Endpoint reflects health and counters |
| PAC/static | Configured paths served by running binary |
| Reload | Rule change takes effect without restart |
| Invalid reload | Old route remains active |
| Graceful shutdown | Active session drains or is cancelled at deadline |
| Upstream test | Proxy handshake reaches configured target |
| Route explain | Live generation and scheduler state reported |

---

# Required negative tests

- duplicate textual upstream ID;
- unknown group member;
- empty group;
- unknown group in rule;
- scheduler state reset regression;
- lease underflow;
- route timeout lease leak;
- disabled member selected through unhealthy fallback;
- auth identity lost before routing;
- TOML listener ignored in config mode;
- mixed CLI/config mode ambiguity;
- admin non-loopback without explicit permission;
- reload with invalid listener bind;
- reload crossing decision/select snapshots;
- duplicate health probe loop after reload;
- metrics target-label cardinality leak;
- admin secret disclosure;
- shutdown leaves detached connection task;
- upstream test reports target success after only TCP reachability;
- random explanation claims deterministic selection.

---

# CI requirements

All existing checks remain required:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Add or retain CI integration jobs for:

- TOML-configured binary startup;
- routing/scheduler integration;
- health integration with local fake endpoints;
- admin/metrics/PAC integration;
- reload integration;
- graceful shutdown;
- external `pproxy`/curl interoperability.

No integration test may depend on public internet availability.

Use dynamic ports and deterministic readiness polling.

---

# Definition of done

Phase 2 corrective integration is complete only when all of the following are true:

1. Scheduler state persists across route selections.
2. Round-robin works under real binary traffic.
3. Pending and active lease counters reflect actual connection lifecycle.
4. Group fallback modes are implemented and observable.
5. Route opening obeys configured timeout.
6. Routing receives real source, listener, protocol, and identity context.
7. Upstream IDs are unique, stable, and textual.
8. Runtime compilation fails on invalid references rather than dropping policy.
9. TOML configuration fully controls listeners, authentication, limits, timeouts, routing, health, admin, PAC, and static content.
10. Health probes run in the actual service and affect scheduling.
11. Metrics are populated by real proxy traffic.
12. Admin endpoints expose live, redacted runtime state.
13. PAC and static content are served by the running binary.
14. Reload is atomic, snapshot-consistent, and non-destructive on failure.
15. Listener/admin topology reload behavior is explicit.
16. Connection tasks are tracked, drained, and force-cancelled after deadline.
17. Upstream testing validates the configured proxy path to the requested target.
18. Route explanation accurately distinguishes offline and live state and does not mutate scheduler state.
19. TOML exposes the documented routing matcher set.
20. README and roadmap checkboxes match integrated binary behavior.
21. All cross-platform, security, lint, unit, integration, and interoperability checks are green.
22. No native dependency, OpenSSL dependency, or unsafe code is introduced.
23. Phase 3 work begins only after these closure criteria are met.

## Completion record

When complete, append:

```markdown
## Completion record

Implemented by commits:

- `<sha>` — persistent schedulers, leases, fallback, route timeout
- `<sha>` — runtime supervisor and authoritative TOML startup
- `<sha>` — health, metrics, admin, PAC, and static integration
- `<sha>` — atomic reload and graceful shutdown
- `<sha>` — upstream testing, live route explanation, matcher completion
- `<sha>` — documentation and final closure

All required checks passed on `<date>`.
```

## Completion record

Implemented across multiple commits:

- Persistent schedulers in UpstreamGroup with Arc<dyn Scheduler>
- Correct PendingLease→ActiveLease lifecycle after route open
- Group fallback semantics (Reject, Direct, UseUnhealthy) with group scheduler
- Route-open timeout enforcement with tokio::time::timeout
- Real routing context propagation (source, listener, identity)
- Stable UpstreamId as Arc<str> with validation
- Strict router construction (no filter_map, error on unknown references)
- eggress-runtime crate with ServiceSupervisor, TaskTracker, graceful shutdown
- TOML-driven listeners, timeouts, authentication, connection limits
- Active health probes attached to upstream runtimes
- SessionMetrics trait with record_session_start/record_session/record_route_decision
- Admin endpoints: /-/route-explain, /-/upstreams (live health), /-/config (redacted), /-/status (active connections)
- Coherent reload with upstream runtime preservation and health reconciliation
- Per-connection CancellationToken with forced cancellation after deadline
- Upstream test with proxy mode, mode/failure/failed_hop fields
- Route explanation with offline/online mode labeling
- Recursive TOML match expressions (all, any_of, not) with expanded leaf matchers
- Injectable RNG for deterministic random scheduler tests
- UseUnhealthy fallback uses group scheduler
- SelectionReason propagated to SessionReport
- Config example file (example-config.toml)
- validate_upstream_id enforced at config compilation time
- Mixed CLI/config mode rejection
- Documentation updates (README, ROADMAP, ARCHITECTURE, AGENTS.md)
