# Phase 2 Detailed Plan: Routing, Health, and Operations

## Objective

Phase 2 turns eggress from a functional Phase 1 proxy core into a long-running, policy-driven service with deterministic route selection, upstream groups, health-aware scheduling, validated TOML configuration, operational metrics, an administrative HTTP surface, PAC/static content serving, safe reload behavior, and route explanation tooling.

The implementation must preserve the Phase 1 invariants:

- protocol parsing remains separate from route selection;
- outbound targets remain unresolved until a connector requires resolution;
- protocol success replies are sent only after the selected route opens;
- ordinary HTTP forwarding and tunnels use the same route-opening layer;
- credentials remain redacted;
- no native TLS/OpenSSL dependency is introduced;
- no unsafe Rust is required for Phase 2;
- UDP remains out of scope until Phase 3.

## Phase 2 deliverables

Phase 2 includes:

- regex compatibility rules;
- exact-host, domain-suffix, CIDR, and port rules;
- direct, upstream, and reject actions;
- first-available scheduling;
- round-robin scheduling;
- random scheduling;
- least-connections scheduling;
- active connection accounting;
- health checking compatible with `pproxy` expectations;
- richer health state with hysteresis;
- validated TOML configuration;
- Prometheus-compatible metrics;
- JSON logs;
- local admin API;
- static HTTP endpoint;
- PAC generation and serving;
- graceful shutdown;
- safe configuration reload;
- route explanation command.

## Explicitly out of scope

Do not add:

- UDP routing or SOCKS5 UDP ASSOCIATE;
- TLS listeners or HTTPS proxy wrapping;
- Shadowsocks, Trojan, SSH, WebSocket, QUIC, HTTP/2, or HTTP/3;
- transparent proxying;
- reverse tunnels;
- system proxy mutation;
- arbitrary scripting or dynamic plugins;
- distributed control-plane state;
- persistent database storage for metrics or health state.

---

# Current architecture and required evolution

The current routing crate is a placeholder that always returns `RouteAction::Direct`. Phase 2 should make `eggress-routing` the authoritative routing and upstream-selection subsystem.

Current high-level flow:

```text
listener
  -> eggress-server accept
  -> route config: Direct or one fixed Chain
  -> open route
  -> reply and relay/forward
```

Target Phase 2 flow:

```text
listener
  -> eggress-server accept
  -> build RouteRequest from session metadata
  -> RouterSnapshot evaluates ordered rules
  -> RouteDecision
       - Direct
       - Reject
       - UpstreamGroup(group_id)
  -> Scheduler selects healthy upstream
  -> ActiveLease increments in-flight/active accounting
  -> RouteExecutor opens selected chain
  -> protocol reply and relay/forward
  -> ActiveLease drop decrements accounting
  -> metrics and health observations updated
```

The key design principle is that routing policy, upstream selection, route execution, and operational state remain separate.

---

# Proposed crate responsibilities

## `eggress-routing`

Own:

- rule AST;
- rule compilation;
- rule evaluation;
- route decisions;
- upstream groups;
- scheduler implementations;
- health state model;
- active connection counters;
- route explanation output;
- immutable runtime snapshots.

Do not own:

- socket opening;
- HTTP admin serving;
- CLI parsing;
- TOML file I/O;
- protocol replies;
- metrics exporter implementation.

## `eggress-config` new crate

Create:

```text
crates/eggress-config/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── file.rs
    ├── model.rs
    ├── validate.rs
    ├── compile.rs
    └── error.rs
```

Own:

- TOML schema;
- deserialization;
- source-aware validation;
- defaults;
- config versioning;
- conversion to typed runtime configuration;
- secret-redacted diagnostics;
- safe reload diff classification.

## `eggress-admin` new crate

Create:

```text
crates/eggress-admin/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── server.rs
    ├── routes.rs
    ├── metrics.rs
    ├── pac.rs
    ├── static_content.rs
    └── model.rs
```

Own:

- local admin HTTP server;
- health/readiness endpoints;
- metrics endpoint;
- route/upstream snapshots;
- PAC content;
- configured static responses;
- reload trigger endpoint only if explicitly enabled.

Prefer a pure Rust HTTP stack. Hyper is acceptable and already aligned with the roadmap. Avoid a heavy web framework unless the implementation becomes materially simpler.

## `eggress-server`

Evolve to depend on a routing service abstraction rather than `RouteConfig::Direct | Chain`.

Own:

- creation of `RouteRequest` from accepted session;
- invoking router;
- converting `Reject` into protocol-specific failure;
- requesting a route lease;
- executing the selected route;
- recording session completion.

## `eggress-cli`

Own:

- CLI arguments;
- loading TOML or compatibility CLI values;
- starting listeners;
- starting admin server;
- starting health manager;
- signal handling;
- reload orchestration;
- process exit status.

---

# Core runtime types

## Stable identifiers

Replace numeric aliases where practical with opaque newtypes:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UpstreamId(std::sync::Arc<str>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UpstreamGroupId(std::sync::Arc<str>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleId(std::sync::Arc<str>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ListenerName(std::sync::Arc<str>);
```

Human-readable stable IDs are preferable for logs, config errors, metrics labels, and admin output.

If changing public Phase 1 aliases is disruptive, add these inside `eggress-routing` and defer core alias cleanup.

## Route request

```rust
pub struct RouteRequest<'a> {
    pub target: &'a TargetAddr,
    pub source: Option<std::net::SocketAddr>,
    pub listener: &'a ListenerName,
    pub inbound_protocol: ProtocolId,
    pub identity: &'a ClientIdentity,
}
```

Phase 2 rules must be able to match target host/port and optionally listener, protocol, source CIDR, and authenticated identity. The roadmap requires host/CIDR/port rules; the additional fields should be supported in the data model even if advanced matchers arrive later in the phase.

## Route decision

```rust
pub enum RouteDecision {
    Direct {
        rule: RuleId,
    },
    UpstreamGroup {
        rule: RuleId,
        group: UpstreamGroupId,
    },
    Reject {
        rule: RuleId,
        reason: RejectReason,
    },
}
```

Always include the rule that produced the decision. The default route should have an explicit synthetic rule ID such as `default`.

## Selected route

```rust
pub enum SelectedRoute {
    Direct {
        decision: RouteDecisionMeta,
    },
    Upstream {
        decision: RouteDecisionMeta,
        group: UpstreamGroupId,
        upstream: UpstreamId,
        chain: std::sync::Arc<ProxyChainSpec>,
        lease: ActiveLease,
    },
}
```

`ActiveLease` should decrement in-flight/active counters on drop, including early errors and cancellation.

---

# Workstream 1: Finish the Phase 1 routing cleanup first

Before implementing Phase 2 policy, perform one compact cleanup commit for known residual Phase 1 issues:

1. enforce `max_decoded_body` for Content-Length bodies;
2. reject repeated `chunked` transfer codings;
3. remove duplicate connection-completion logging if both library and CLI emit it;
4. retain protocol identity on authentication failure;
5. refine failure categories for policy denial, route-hop failure, and I/O timeout;
6. preserve configured detector order or document fixed order until the generalized dispatcher lands.

This should be a single bounded pre-Phase-2 commit, not a new phase.

Acceptance:

- all existing tests pass;
- no README regression;
- no new feature scope.

---

# Workstream 2: Rule model and matching engine

## Rule representation

Use a compiled rule model rather than evaluating raw TOML or regex strings on each connection.

```rust
pub struct CompiledRule {
    pub id: RuleId,
    pub matcher: MatchExpr,
    pub action: RouteActionSpec,
}

pub enum MatchExpr {
    Any,
    All(Vec<MatchExpr>),
    AnyOf(Vec<MatchExpr>),
    Not(Box<MatchExpr>),
    HostExact(std::sync::Arc<str>),
    HostSuffix(std::sync::Arc<str>),
    HostRegex(regex::Regex),
    DestinationCidr(ipnet::IpNet),
    DestinationPort(PortMatcher),
    SourceCidr(ipnet::IpNet),
    Listener(ListenerName),
    Protocol(ProtocolId),
    Identity(std::sync::Arc<str>),
}

pub enum PortMatcher {
    Exact(u16),
    Range { start: u16, end: u16 },
    Set(std::sync::Arc<[u16]>),
}

pub enum RouteActionSpec {
    Direct,
    UpstreamGroup(UpstreamGroupId),
    Reject(RejectReason),
}
```

Use `ipnet` or an equivalent pure Rust CIDR crate.

## Matching semantics

Rules are evaluated in declaration order. First matching rule wins.

Domain handling:

- exact host matching is case-insensitive for DNS names;
- trim one terminal dot for comparison;
- do not lowercase IP literals;
- suffix `.example.com` should match `a.example.com` and optionally `example.com` only if explicitly documented;
- regex matching uses the original normalized hostname;
- CIDR applies only to IP literals in Phase 2 unless an explicit DNS-resolution policy is later added.

Do not resolve domain targets solely to evaluate CIDR rules. Premature resolution would break remote-DNS semantics.

## Regex compatibility rules

Implement a compatibility parser for `pproxy`-style regex files.

Recommended semantics:

- one non-empty rule per line;
- lines beginning with `#` are comments;
- regex is matched against hostname and decimal port string to preserve compatibility behavior;
- invalid regex is a configuration error identifying file and line;
- an associated upstream or action is configured outside the rule file.

Example config:

```toml
[[rules]]
id = "legacy-proxy-list"
compat_regex_file = "/etc/eggress/proxy.rules"
action = { upstream_group = "internet" }
```

Compile the file at startup and on reload. Never read it on the connection path.

## Default route

Configuration must require or synthesize one default action:

```toml
[routing]
default = "direct"
```

Valid defaults:

- direct;
- reject;
- upstream group.

## Tests

Add table-driven tests for:

- exact domain case normalization;
- terminal dot behavior;
- suffix matches and nonmatches;
- regex matches;
- IPv4 CIDR;
- IPv6 CIDR;
- exact port;
- port range boundaries;
- first-match wins;
- reject action;
- direct action;
- upstream-group action;
- domain target not resolved for CIDR;
- regex compatibility file comments and invalid lines.

Add property tests for suffix and CIDR matching where useful.

Acceptance:

- no allocation-heavy parsing occurs on the hot path;
- rule order is deterministic;
- every decision records the matching rule ID;
- compatibility fixtures match Python `pproxy` behavior.

---

# Workstream 3: Upstream groups and immutable runtime snapshots

## Runtime snapshot

Use immutable snapshots behind `Arc`:

```rust
pub struct RoutingSnapshot {
    pub generation: u64,
    pub rules: std::sync::Arc<[CompiledRule]>,
    pub groups: std::collections::HashMap<UpstreamGroupId, std::sync::Arc<UpstreamGroup>>,
    pub default_action: RouteActionSpec,
}
```

The live service holds:

```rust
pub struct RoutingService {
    current: arc_swap::ArcSwap<RoutingSnapshot>,
}
```

`arc-swap` is a pure Rust dependency and is appropriate for lock-free read-mostly snapshot replacement. If avoiding the dependency, use `RwLock<Arc<...>>`, but do not hold a lock across route opening.

## Upstream runtime

```rust
pub struct UpstreamRuntime {
    pub id: UpstreamId,
    pub chain: std::sync::Arc<ProxyChainSpec>,
    pub enabled: std::sync::atomic::AtomicBool,
    pub active: std::sync::atomic::AtomicU64,
    pub in_flight: std::sync::atomic::AtomicU64,
    pub totals: UpstreamTotals,
    pub health: HealthCell,
}
```

Prefer immutable identity/config plus atomic runtime state.

## Group configuration

```rust
pub struct UpstreamGroup {
    pub id: UpstreamGroupId,
    pub scheduler: SchedulerKind,
    pub members: std::sync::Arc<[std::sync::Arc<UpstreamRuntime>]>,
    pub fallback: GroupFallback,
}

pub enum GroupFallback {
    Reject,
    Direct,
    UseUnhealthy,
}
```

`UseUnhealthy` should be explicit and documented. Default should not silently send traffic through unhealthy upstreams.

## Validation

Reject:

- duplicate group IDs;
- duplicate upstream IDs;
- empty groups;
- unknown upstream references;
- unsupported chain protocols;
- groups whose fallback references themselves;
- route rules referencing unknown groups;
- duplicate listener names;
- invalid scheduler names.

Acceptance:

- route reads require no global mutable lock;
- reload swaps one validated snapshot atomically;
- existing sessions retain their old selected upstream safely;
- removed upstreams drain naturally through retained `Arc`s.

---

# Workstream 4: Scheduler implementations

Define:

```rust
pub trait Scheduler: Send + Sync {
    fn select(
        &self,
        group: &UpstreamGroup,
        candidates: &[std::sync::Arc<UpstreamRuntime>],
        request: &RouteRequest<'_>,
    ) -> Option<std::sync::Arc<UpstreamRuntime>>;
}
```

Schedulers must only select among eligible candidates determined by enabled and health policy.

## First available

Semantics:

- preserve group member order;
- choose the first eligible member;
- compatibility default for `pproxy`-style behavior.

## Round robin

Use an `AtomicU64` cursor:

```rust
let start = cursor.fetch_add(1, Ordering::Relaxed) as usize;
for offset in 0..members.len() {
    let index = (start + offset) % members.len();
    if eligible(&members[index]) {
        return Some(members[index].clone());
    }
}
```

Do not mutate or rotate the member vector.

## Random

Use a pure Rust RNG. Prefer `fastrand` or `rand` with minimal features.

To make tests deterministic, inject a seedable selection source or isolate candidate permutation behind a test implementation.

## Least connections

Compare:

```text
active + in_flight
```

not only established active connections.

Use deterministic tie breaking by member order or upstream ID.

Increment `in_flight` before route establishment. Convert to active on success. Decrement correctly on failure, cancellation, and lease drop.

Suggested lease lifecycle:

```rust
pub struct PendingLease {
    upstream: Arc<UpstreamRuntime>,
    state: LeaseState,
}

impl PendingLease {
    pub fn established(mut self) -> ActiveLease {
        self.upstream.in_flight.fetch_sub(1, Ordering::Relaxed);
        self.upstream.active.fetch_add(1, Ordering::Relaxed);
        self.state = LeaseState::Transferred;
        ActiveLease { upstream: self.upstream.clone() }
    }
}

impl Drop for PendingLease {
    fn drop(&mut self) {
        if self.state == LeaseState::Pending {
            self.upstream.in_flight.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl Drop for ActiveLease {
    fn drop(&mut self) {
        self.upstream.active.fetch_sub(1, Ordering::Relaxed);
    }
}
```

## Scheduler tests

- first available preserves order;
- round robin sequence is deterministic;
- round robin skips unhealthy members;
- random only selects eligible members;
- seeded random test covers every candidate;
- least connections chooses minimum `active + in_flight`;
- deterministic tie break;
- no counter underflow;
- pending lease decrements on connect failure;
- active lease decrements on normal completion and cancellation;
- high-contention test with many concurrent selections.

Acceptance:

- schedulers are deterministic under test;
- no scheduler locks the group for the session lifetime;
- counters are accurate under failures and cancellation.

---

# Workstream 5: Health state and active probing

## Health model

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Unknown,
    Healthy,
    Suspect,
    Unhealthy,
    Recovering,
    Disabled,
}

pub struct HealthSnapshot {
    pub state: HealthState,
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
    pub last_checked_at: Option<std::time::SystemTime>,
    pub last_success_at: Option<std::time::SystemTime>,
    pub last_failure_at: Option<std::time::SystemTime>,
    pub last_latency: Option<std::time::Duration>,
    pub last_error: Option<HealthErrorKind>,
}
```

Store snapshots in an `RwLock` or atomically replace an `Arc<HealthSnapshot>`. Health checks are low-frequency, so simplicity is preferable to over-optimization.

## Probe types

Phase 2 should support:

```rust
pub enum HealthProbe {
    TcpConnect {
        timeout: Duration,
    },
    ProxyConnect {
        target: TargetAddr,
        timeout: Duration,
    },
}
```

Compatibility mode may use transport reachability only. Production mode should prefer an end-to-end proxy handshake to a configured probe target.

Do not send public internet probes by default. Require an explicit target for end-to-end checks.

## Hysteresis

Configurable thresholds:

```toml
[health]
interval = "30s"
timeout = "5s"
failures_to_unhealthy = 3
successes_to_healthy = 2
initial_state = "unknown"
```

Suggested transitions:

```text
Unknown + success -> Healthy
Unknown + failure -> Suspect
Healthy + failure below threshold -> Suspect
Suspect + success -> Healthy
Suspect + threshold failure -> Unhealthy
Unhealthy + success -> Recovering
Recovering + enough success -> Healthy
Recovering + failure -> Unhealthy
Disabled -> Disabled
```

## Eligibility policy

Default:

- Healthy: eligible;
- Recovering: eligible only after threshold or if configured;
- Unknown: eligible in compatibility mode, otherwise configurable;
- Suspect: eligible but deprioritized or excluded by policy;
- Unhealthy: excluded;
- Disabled: excluded.

For Phase 2 simplicity, use a clear boolean policy per state before adding weighted preference.

## Health manager

```rust
pub struct HealthManager {
    cancel: CancellationToken,
    tasks: JoinSet<()>,
}
```

Requirements:

- one cancellable loop per upstream or a central scheduler;
- bounded concurrent probes via semaphore;
- jitter to avoid synchronized bursts;
- immediate initial probe optional;
- graceful shutdown;
- reload starts probes for added upstreams and cancels removed ones;
- probe errors are normalized and redacted.

## Passive observations

Record route-open success/failure separately from active health checks.

Do not immediately mark an upstream unhealthy on one client-specific target failure. Distinguish:

- failure to connect to the upstream proxy itself;
- upstream authentication failure;
- failure from upstream to arbitrary destination;
- protocol negotiation failure.

Only upstream-reachability and authentication failures should directly affect upstream health by default.

## Health tests

Use paused Tokio time and fake probes:

- state transition table;
- threshold boundaries;
- recovery hysteresis;
- cancellation;
- reload add/remove;
- probe semaphore bound;
- deterministic jitter injection;
- no duplicate probe loops after reload;
- disabled upstream never probed;
- metrics reflect current state.

Acceptance:

- state changes are deterministic;
- no arbitrary sleeps in unit tests;
- scheduler eligibility uses health state;
- active checks do not block routing threads.

---

# Workstream 6: Route service integration with `eggress-server`

## Replace fixed route config

Current:

```rust
pub enum RouteConfig {
    Direct,
    Chain(ProxyChainSpec),
}
```

Target:

```rust
pub struct ConnectionConfig {
    pub routing: Arc<dyn RouteService>,
    pub listener: ListenerName,
    pub handshake_timeout: Duration,
    pub protocols: Arc<[ProtocolId]>,
    pub authentication: InboundAuthentication,
}
```

Trait:

```rust
pub trait RouteService: Send + Sync {
    fn decide(&self, request: &RouteRequest<'_>) -> Result<RouteDecision, RouteError>;

    fn select(
        &self,
        decision: &RouteDecision,
        request: &RouteRequest<'_>,
    ) -> Result<SelectedRoute, RouteError>;
}
```

A simpler combined `route()` method is acceptable, but explanation tooling benefits from keeping decision and selection conceptually distinct.

## Session metadata

Accepted sessions should retain:

- inbound protocol;
- authenticated identity;
- source peer address;
- listener name;
- target.

Authentication should return `ClientIdentity::Username` on success rather than discard identity.

## Reject mapping

Map route rejection to protocol-specific failures:

- HTTP CONNECT: 403;
- ordinary HTTP: 403;
- SOCKS5: REP `0x02` connection not allowed by ruleset;
- SOCKS4: request rejected.

Log the matched rule ID and reject reason.

## Selected upstream execution

`eggress-server` should receive a selected chain plus lease and pass it to route execution.

Do not rebuild handler registries for every connection. Build a reusable `RouteExecutor` once and share it.

Suggested:

```rust
pub struct RouteExecutor {
    direct: DirectConnector,
    chain: ChainExecutor,
    connect_timeout: Duration,
}
```

## Route timeout

Add explicit route/open timeout separate from handshake timeout:

```toml
[timeouts]
handshake = "10s"
connect = "10s"
```

Apply connect timeout around the entire selected route opening. Per-hop timeouts can be added later if not already supported.

## Tests

- direct rule opens direct route;
- upstream rule selects group member;
- reject rule emits protocol-correct failure;
- identity/listener/protocol matchers work;
- least-connections accounting covers tunnel and ordinary HTTP sessions;
- lease drops on success, failure, and cancellation;
- matched rule/upstream appears in report;
- route-open timeout produces `RouteTimeout`.

Acceptance:

- no fixed chain lives directly in `ConnectionConfig`;
- every session passes through router;
- reports include rule ID, group ID, and upstream ID where applicable.

---

# Workstream 7: TOML configuration

## Configuration goals

The TOML format should support the complete Phase 2 runtime while preserving CLI compatibility.

CLI URI flags remain valid. They compile into the same typed runtime model as TOML.

## Suggested schema

```toml
version = 1

[process]
log_format = "pretty"
log_level = "info"
shutdown_grace = "10s"

[timeouts]
handshake = "10s"
connect = "10s"

[[listeners]]
name = "local"
bind = "127.0.0.1:8080"
protocols = ["http", "socks4", "socks5"]
connection_limit = 4096

[listeners.auth]
type = "username_password"
username = "user"
password_env = "EGRESS_PROXY_PASSWORD"

[[upstreams]]
id = "proxy-a"
uri = "socks5://user:pass@proxy-a.example:1080"

[[upstreams]]
id = "proxy-b"
uri = "http://proxy-b.example:8080"

[[upstream_groups]]
id = "internet"
scheduler = "least_connections"
members = ["proxy-a", "proxy-b"]
fallback = "direct"

[upstream_groups.health]
probe = "tcp_connect"
interval = "30s"
timeout = "5s"
failures_to_unhealthy = 3
successes_to_healthy = 2

[[rules]]
id = "block-private-admin"
match = { host_exact = "admin.internal.example" }
action = { reject = "access_denied" }

[[rules]]
id = "proxy-example"
match = { any = [
  { host_suffix = ".example.com" },
  { destination_port = 443 }
] }
action = { upstream_group = "internet" }

[routing]
default = "direct"

[admin]
bind = "127.0.0.1:9090"
enabled = true
metrics = true

[[admin.static]]
path = "/status.txt"
content_type = "text/plain; charset=utf-8"
body = "eggress online\n"

[admin.pac]
path = "/proxy.pac"
proxy = "PROXY 127.0.0.1:8080"
direct_fallback = true
```

## Secret sources

Support:

- inline secret for compatibility;
- environment variable reference;
- file reference with strict size limits.

Model:

```rust
pub enum SecretSource {
    Inline(SecretString),
    Environment(String),
    File(PathBuf),
}
```

Do not implement external secret managers in Phase 2.

## Duration parsing

Use a pure Rust duration parser such as `humantime` or a small project-local parser.

## Validation errors

Errors must include:

- config path;
- logical field path;
- invalid value category;
- referenced unknown ID;
- no plaintext secrets.

Example:

```text
configuration error at upstream_groups[0].members[1]: unknown upstream `proxy-c`
```

## CLI precedence

Recommended:

1. built-in defaults;
2. TOML file;
3. explicit CLI overrides.

Do not merge repeated URI flags ambiguously with configured listeners/upstreams without documented behavior.

A safe initial rule:

- `--config` supplies full service configuration;
- compatibility `-l`/`-r` mode is used when `--config` is absent;
- reject mixed mode except for log-level and admin-disable overrides.

## Commands

```text
egress --config /etc/eggress/eggress.toml
egress config validate --config file.toml
egress config print-effective --config file.toml
egress config example
```

Effective config output must redact secrets.

## Tests

- minimal config;
- full config;
- unknown fields rejected or explicitly allowed policy;
- duplicate IDs;
- invalid URI;
- invalid duration;
- bad CIDR;
- bad regex;
- unknown group/upstream references;
- secret redaction;
- env secret resolution;
- missing env secret;
- safe file secret limits;
- compatibility CLI compilation.

Acceptance:

- all runtime state is produced from one validated typed model;
- no raw TOML values reach the data plane;
- invalid configs fail before binding listeners.

---

# Workstream 8: Configuration reload

## Reload trigger

Support:

- Unix SIGHUP;
- optional admin endpoint `POST /-/reload` when enabled;
- Windows reload via admin endpoint or documented console mechanism.

## Reload model

Parse, resolve secrets, validate, and compile a complete new runtime configuration before changing live state.

```text
read file
-> parse TOML
-> resolve included files/rule files/secrets
-> validate
-> compile routing snapshot
-> classify changes
-> atomically apply safe changes
```

Never partially apply an invalid configuration.

## Safe reload scope

Phase 2 safe reload:

- routing rules;
- upstream groups;
- scheduler settings;
- health settings;
- admin static/PAC content;
- log level;
- metrics metadata.

Listener bind-address or protocol changes are more complex. Choose one of:

### Preferred

Implement listener reconciliation:

- start new listeners first;
- if all new listeners bind successfully, swap configuration;
- stop removed listeners gracefully;
- retain unchanged listeners.

### Acceptable Phase 2 minimum

Classify listener topology changes as restart-required and reject the reload with a clear message while allowing routing-only changes.

Document whichever behavior is chosen.

## Generation tracking

Every successful reload increments generation:

```rust
pub struct RuntimeGeneration(pub u64);
```

Expose generation in logs, admin status, metrics, and route explanation.

## Health reconciliation

- unchanged upstream ID and equivalent chain retains health state/counters;
- changed chain under same ID should reset health after warning;
- removed upstream probe tasks cancel;
- added upstream probe tasks start;
- group membership changes apply atomically.

## Tests

- valid routing reload;
- invalid reload leaves old snapshot active;
- generation increments once;
- concurrent connections use old or new complete snapshot, never partial state;
- removed upstream drains existing session;
- health task reconciliation;
- repeated reload does not leak tasks;
- restart-required change reports clearly.

Acceptance:

- no data-plane lock is held during file parsing;
- reload is atomic;
- invalid reload is non-destructive.

---

# Workstream 9: Metrics subsystem

## Metric naming

Use a stable prefix:

```text
egress_connections_active
egress_connections_total
egress_connection_failures_total
egress_bytes_upstream_total
egress_bytes_downstream_total
egress_route_decisions_total
egress_upstream_active
egress_upstream_in_flight
egress_upstream_connect_attempts_total
egress_upstream_connect_failures_total
egress_upstream_health_state
egress_upstream_probe_duration_seconds
egress_config_generation
egress_reload_total
egress_reload_failures_total
```

## Labels

Keep cardinality bounded.

Allowed:

- listener;
- protocol;
- outcome/failure category;
- rule ID;
- upstream group ID;
- upstream ID;
- scheduler;
- health state.

Do not label by:

- target hostname;
- source IP;
- username;
- full route URI;
- arbitrary error string.

## Implementation

Prefer `prometheus-client` for a pure Rust exporter and explicit registry control, or `metrics` plus a Prometheus exporter if already aligned with project style.

The admin crate should expose `/metrics` in Prometheus text format.

## Counter integration

Instrument:

- acceptance outcome;
- route decision;
- scheduler selection;
- route opening;
- relay/forward completion;
- health probe;
- reload;
- admin requests.

Avoid per-byte atomic increments. Update counters from final session reports and relay summaries.

## Tests

- metric names and labels stable;
- no secret or target labels;
- counters increment once;
- active gauges return to zero;
- health state is one-hot or documented numeric representation;
- scrape output parses;
- reload generation changes;
- concurrency tests avoid double counting.

Acceptance:

- `/metrics` is scrapeable;
- metric cardinality is bounded;
- metrics do not expose secrets or user destinations.

---

# Workstream 10: JSON logs

## Logging formats

CLI/TOML should support:

```text
pretty
compact
json
```

Use `tracing-subscriber` JSON formatting.

## Required structured fields

Connection completion:

- connection ID;
- listener;
- protocol;
- source address where enabled;
- identity type, not raw password;
- target may be logged according to current policy;
- rule ID;
- route action;
- upstream group;
- upstream ID;
- outcome;
- failure category;
- byte counts;
- duration;
- config generation.

Health event:

- upstream ID;
- prior state;
- next state;
- probe type;
- latency;
- normalized failure.

Reload event:

- prior generation;
- next generation;
- success/failure;
- change summary;
- no secrets.

## Reloadable log filter

Use `tracing_subscriber::reload` to allow log-level reload without restarting.

## Tests

Capture JSON output and verify:

- valid JSON per line;
- required keys;
- no credentials;
- no authorization headers;
- generation included;
- normalized failure values.

Acceptance:

- JSON logs are machine-readable and documented;
- secret-redaction tests cover all config and runtime paths.

---

# Workstream 11: Local admin API

## Security posture

Admin binds to loopback by default.

Reject non-loopback bind unless explicitly allowed:

```toml
[admin]
bind = "127.0.0.1:9090"
allow_non_loopback = false
```

Phase 2 may omit admin authentication if loopback-only is enforced. If non-loopback is allowed, require authentication or clearly reject the configuration.

## Endpoints

```text
GET /-/health       process liveness
GET /-/ready        readiness: listeners active and config valid
GET /-/status       JSON runtime summary
GET /-/routes       redacted rules and defaults
GET /-/upstreams    upstream health/counters
GET /-/config       redacted effective config
GET /metrics        Prometheus metrics
POST /-/reload      optional reload trigger
GET <PAC path>      PAC content
GET <static path>   configured static content
```

## Status response

```json
{
  "version": "0.1.0",
  "generation": 4,
  "uptime_seconds": 1234,
  "listeners": 2,
  "active_connections": 14,
  "upstreams": {
    "healthy": 2,
    "suspect": 0,
    "unhealthy": 1
  }
}
```

## Readiness semantics

Ready when:

- initial config compiled;
- required listeners bound;
- routing snapshot installed;
- admin itself does not determine readiness;
- health may be unknown without making process unready unless configured otherwise.

## Method and body limits

- only GET except reload POST;
- bounded request head;
- reload body empty;
- reject unsupported methods with 405;
- connection timeout;
- no request body buffering.

## Tests

- loopback default;
- non-loopback validation;
- liveness/readiness;
- redacted config;
- upstream health output;
- reload enabled/disabled;
- method restrictions;
- request-size limits;
- concurrent scrape/status requests;
- graceful shutdown.

Acceptance:

- admin API cannot expose credentials;
- endpoints return stable content types and status codes;
- admin failure does not corrupt data-plane routing state.

---

# Workstream 12: Static endpoint and PAC serving

## Static content

Config:

```toml
[[admin.static]]
path = "/robots.txt"
content_type = "text/plain; charset=utf-8"
body = "User-agent: *\nDisallow: /\n"
```

Validation:

- path begins with `/`;
- no duplicate paths;
- maximum body size, e.g. 1 MiB;
- explicit content type;
- GET and HEAD only;
- immutable content per snapshot.

Optional file-backed static content may be included if reload reads and bounds the file.

## PAC generation

Support generated PAC from a constrained model rather than arbitrary code initially.

```rust
pub struct PacConfig {
    pub path: String,
    pub proxy_directive: String,
    pub direct_fallback: bool,
    pub direct_hosts: Vec<String>,
    pub direct_suffixes: Vec<String>,
}
```

Generated example:

```javascript
function FindProxyForURL(url, host) {
  if (isPlainHostName(host)) return "DIRECT";
  if (dnsDomainIs(host, ".internal.example")) return "DIRECT";
  return "PROXY 127.0.0.1:8080; DIRECT";
}
```

PAC output must escape strings safely. Do not insert unescaped user configuration into JavaScript.

Allow a fully static PAC body as an alternative for compatibility.

## Tests

- content type;
- HEAD behavior;
- path collisions;
- generated PAC syntax snapshot;
- escaping of quotes/backslashes;
- direct fallback on/off;
- reload updates PAC atomically;
- maximum static body limit.

Acceptance:

- PAC and static content share the admin server;
- no arbitrary file access from URL paths;
- config reload safely replaces content.

---

# Workstream 13: Graceful shutdown

## Shutdown sequence

1. receive SIGINT/SIGTERM or service stop;
2. mark readiness false;
3. stop accepting new data-plane connections;
4. stop accepting new admin requests;
5. cancel health probe loops;
6. allow active sessions to drain until grace deadline;
7. terminate remaining sessions after deadline;
8. flush logs and exit.

## Supervisor

Create a top-level runtime supervisor:

```rust
pub struct ServiceSupervisor {
    cancel: CancellationToken,
    listeners: JoinSet<Result<(), ServiceError>>,
    health: HealthManager,
    admin: Option<AdminHandle>,
    sessions: SessionRegistry,
}
```

Do not detach connection tasks without tracking them.

## Session registry

At minimum:

- active count;
- cancellable child token;
- JoinSet or task tracker;
- drain wait with deadline.

`tokio-util::task::TaskTracker` is suitable if already available and stable for the selected version.

## Exit behavior

- clean signal shutdown exits 0;
- fatal listener failure exits nonzero unless other configured listeners may continue by policy;
- invalid initial config exits nonzero;
- failed reload does not exit;
- panic in critical control-plane task should be surfaced.

## Tests

- listener stops accepting after cancellation;
- active tunnel drains within grace period;
- stuck tunnel cancelled after deadline;
- health tasks terminate;
- admin terminates;
- no task leak under repeated start/stop tests;
- readiness becomes false before drain;
- Windows-compatible shutdown abstraction compiles.

Acceptance:

- long-running service shuts down predictably;
- all major task groups are supervised.

---

# Workstream 14: Route explanation command

## Commands

```text
egress route explain example.com:443
egress route explain --config file.toml example.com:443
egress route explain --listener local --protocol socks5 example.com:443
egress route explain --source 192.0.2.5 --identity alice example.com:443
egress route explain --json example.com:443
```

## Output

Human-readable:

```text
Target: example.com:443
Listener: local
Protocol: socks5
Matched rule: proxy-example
Action: upstream group internet
Scheduler: least-connections
Eligible upstreams:
  proxy-a  healthy  active=2  in_flight=0
  proxy-b  unhealthy  active=0  in_flight=0
Selected upstream: proxy-a
Chain: socks5://proxy-a.example:1080
Config generation: 4
```

Redact credentials in chain output.

JSON output should use a stable serializable explanation model.

## Determinism

Explanation should not mutate round-robin cursor or active counters by default.

Provide scheduler preview behavior:

- first available and least connections can preview exactly;
- random reports eligible candidates unless a seed is supplied;
- round robin reports next candidate without consuming cursor, or clearly marks preview.

## Tests

- every rule matcher explains match/no-match;
- redaction;
- rejected route;
- direct route;
- unhealthy exclusion;
- no scheduler state mutation;
- JSON schema snapshots.

Acceptance:

- operators can understand why a route was selected without enabling debug logs.

---

# Workstream 15: Compatibility CLI integration

## Preserve current mode

Existing commands such as:

```text
egress -l http+socks4+socks5://:8080
egress -r socks5://proxy:1080
```

must continue to work.

Compile them into a synthetic runtime config:

- one listener named `cli-0`;
- one upstream per `-r` chain or one group depending on compatibility semantics;
- first-available scheduler;
- compatibility regex rules from existing CLI flags;
- direct fallback consistent with Python `pproxy` behavior.

## Phase 2 CLI flags

Implement or preserve equivalents for:

```text
-b RULE_FILE
-a HEALTH_INTERVAL
-s first|rr|random|least
--pac PATH
--get PATH=CONTENT_OR_FILE
--test TARGET
```

Map them into the same typed config used by TOML.

Do not create a separate runtime path for CLI mode.

## Upstream test command

```text
egress upstream test
egress upstream test --id proxy-a
egress upstream test --target example.com:443
```

Return per-upstream:

- reachability;
- handshake success;
- latency;
- normalized failure;
- no secret output.

Acceptance:

- compatibility CLI and TOML use the same router, schedulers, and health implementation;
- no behavior fork develops.

---

# Workstream 16: Testing strategy

## Unit tests

- matcher normalization;
- rule order;
- scheduler algorithms;
- lease accounting;
- health transitions;
- config validation;
- secret redaction;
- PAC generation;
- metric labels;
- reload diff classification.

## Integration tests

- listener -> route rule -> selected upstream -> echo origin;
- direct/reject/upstream actions for HTTP CONNECT, ordinary HTTP, SOCKS4, SOCKS5;
- all scheduler modes;
- health exclusion and recovery;
- config reload during active sessions;
- admin API and metrics;
- graceful shutdown;
- CLI compatibility mode.

## Differential compatibility

Extend Python `pproxy` fixtures for:

- regex route files;
- first-available behavior;
- round-robin behavior where observable;
- health-check fallback semantics;
- PAC/static serving where applicable.

Do not overfit undocumented behavior without recording the chosen compatibility target.

## Concurrency tests

- least-connections under concurrent route opens;
- round-robin atomic cursor;
- reload during heavy route evaluation;
- health transitions during selection;
- no active-counter underflow;
- removed upstream draining.

Use Loom only if a specific atomic invariant proves difficult to test normally; do not require Loom broadly in Phase 2.

## Soak test

Add an ignored or scheduled test that:

- runs multiple local origins;
- rotates health state;
- repeatedly reloads routing snapshots;
- maintains concurrent proxy sessions;
- verifies counters return to zero;
- checks task count and memory do not grow unbounded.

---

# Workstream 17: Performance and resource constraints

## Hot-path goals

Route evaluation should:

- use immutable compiled rules;
- avoid file I/O;
- avoid regex compilation;
- avoid DNS resolution unless route execution requires it;
- avoid global locks;
- allocate minimally.

## Benchmarks

Add benchmarks for:

- exact host matching over 10, 100, 1,000 rules;
- suffix matching;
- regex rule sets;
- CIDR lookup;
- scheduler selection under 2, 10, 100 upstreams;
- snapshot swap under concurrent readers;
- metrics update overhead;
- route explanation separate from hot path.

## Resource limits

- maximum rules;
- maximum upstreams;
- maximum groups;
- maximum regex length;
- maximum regex-file size;
- maximum static content size;
- maximum admin request head;
- maximum concurrent health probes;
- bounded reload file size.

Defaults can be generous but must be finite.

---

# Workstream 18: Security requirements

Phase 2 introduces a control plane. Required protections:

- loopback-only admin default;
- no secret values in admin responses;
- no unbounded config or rule files;
- regex engine must be Rust `regex` without catastrophic backtracking;
- safe PAC escaping;
- bounded metrics labels;
- reload authorization if exposed over HTTP;
- no path traversal for static content;
- secret file size and permission warnings;
- upstream URIs redacted everywhere;
- config validation before socket binding;
- policy reject occurs before route opening;
- private-network egress policy remains future work unless implemented as explicit CIDR reject rules.

Add threat-model notes to `docs/SECURITY_MODEL.md` or create it if absent.

---

# Milestone sequence

## Milestone 2.0: Residual Phase 1 cleanup

Deliver:

- Content-Length limit consistency;
- repeated chunked rejection;
- logging de-duplication;
- protocol-aware auth failures;
- failure-category cleanup;
- detector-order decision.

Exit:

- Phase 1 remains green.

## Milestone 2.1: Routing rule engine

Deliver:

- typed rule/action model;
- exact, suffix, regex, CIDR, and port matchers;
- first-match semantics;
- default action;
- compatibility regex parser;
- route explanation model skeleton.

Exit:

- deterministic rule fixture suite passes.

## Milestone 2.2: Upstream groups and schedulers

Deliver:

- runtime upstream/group state;
- first, round-robin, random, least-connections;
- active/in-flight leases;
- scheduler tests.

Exit:

- all schedulers deterministic under tests and counters correct.

## Milestone 2.3: Server routing integration

Deliver:

- `RouteService` integration;
- direct/upstream/reject execution;
- protocol-correct reject replies;
- selected rule/upstream in reports;
- connect timeout.

Exit:

- all Phase 1 protocols route through policy engine.

## Milestone 2.4: Health management

Deliver:

- health state model;
- active probes;
- hysteresis;
- scheduler eligibility;
- health lifecycle and reload reconciliation.

Exit:

- unhealthy upstreams excluded and recover deterministically.

## Milestone 2.5: TOML configuration

Deliver:

- `eggress-config` crate;
- versioned schema;
- validation;
- secret sources;
- CLI compatibility compilation;
- config commands.

Exit:

- invalid config fails before binding; effective config is redacted.

## Milestone 2.6: Metrics and JSON logging

Deliver:

- metrics registry;
- bounded labels;
- JSON logs;
- reloadable log filter;
- tests.

Exit:

- metrics scrape and JSON log fixtures pass.

## Milestone 2.7: Admin API, PAC, and static content

Deliver:

- loopback admin server;
- status/routes/upstreams/config endpoints;
- metrics;
- PAC;
- static content;
- request limits.

Exit:

- admin API is redacted, bounded, and documented.

## Milestone 2.8: Reload and graceful shutdown

Deliver:

- atomic reload;
- generation tracking;
- health reconciliation;
- signal handling;
- supervised drain.

Exit:

- invalid reload is non-destructive and shutdown tests pass.

## Milestone 2.9: Route explanation and upstream testing

Deliver:

- human and JSON explain output;
- upstream test command;
- no scheduler mutation during explain.

Exit:

- route decisions are operator-auditable.

## Milestone 2.10: Phase closure

Deliver:

- README checklist update;
- roadmap update;
- compatibility report;
- soak results;
- security review;
- green CI matrix.

---

# Recommended commit sequence for a smaller model

1. `routing: add typed IDs, rule AST, and matchers`
2. `routing: add ordered router decisions and default action`
3. `routing: add upstream runtime and group validation`
4. `routing: implement first and round-robin schedulers`
5. `routing: implement random and least-connections schedulers`
6. `routing: add pending and active lease accounting`
7. `server: integrate RouteService and reject responses`
8. `server: add connect timeout and route metadata reports`
9. `health: add state machine and fake probe tests`
10. `health: add active probe manager and scheduler eligibility`
11. `config: add TOML schema and validation`
12. `config: compile compatibility CLI into runtime config`
13. `config: add secret resolution and redacted effective output`
14. `ops: add metrics registry and instrumentation`
15. `ops: add JSON logs and reloadable filter`
16. `admin: add health, ready, status, routes, upstreams`
17. `admin: add metrics, static content, and PAC`
18. `reload: add atomic routing reload and generation tracking`
19. `runtime: add graceful shutdown and task supervision`
20. `cli: add route explain and upstream test commands`
21. `docs: close Phase 2 checklist and compatibility report`

Each commit should compile and keep existing tests green. Avoid one giant Phase 2 commit.

---

# README checklist additions/updates

Check only with implementation, tests, documentation, and applicable interoperability evidence.

## Routing and scheduling

- [ ] Direct routes
- [ ] Ordered upstream routes
- [ ] Regex compatibility rules
- [ ] Exact-host rules
- [ ] Domain-suffix rules
- [ ] CIDR rules
- [ ] Port rules
- [ ] Reject rules
- [ ] First-available scheduling
- [ ] Round-robin scheduling
- [ ] Random scheduling
- [ ] Least-connections scheduling
- [ ] Active health checking
- [ ] Health hysteresis
- [ ] Direct fallback
- [ ] Route explanation command

## Administration and operations

- [ ] TOML configuration
- [ ] Configuration validation
- [ ] Configuration reload
- [ ] JSON logs
- [ ] Per-upstream metrics
- [ ] Prometheus endpoint
- [ ] Local admin API
- [ ] PAC generation
- [ ] PAC serving
- [ ] Static HTTP endpoint
- [ ] Upstream test command

Add explicit operational limitations if listener topology reload is restart-required.

---

# Phase 2 exit criteria

Phase 2 is complete only when:

1. every accepted TCP session is evaluated by the routing engine;
2. exact, suffix, regex, CIDR, and port rules are implemented and tested;
3. direct, reject, and upstream-group actions work for HTTP CONNECT, ordinary HTTP, SOCKS4, and SOCKS5;
4. first-available, round-robin, random, and least-connections schedulers pass deterministic tests;
5. active and in-flight counters cannot leak or underflow;
6. health probes and hysteresis exclude and recover upstreams correctly;
7. a validated TOML config can define listeners, upstreams, groups, rules, health, admin, PAC, and static endpoints;
8. compatibility CLI mode compiles into the same runtime model;
9. invalid reloads leave the previous generation active;
10. safe reloads swap atomically and reconcile health tasks;
11. graceful shutdown stops acceptance, drains sessions, and terminates at the deadline;
12. Prometheus metrics expose bounded-cardinality routing, upstream, health, session, and reload data;
13. JSON logs are structured and secret-free;
14. admin endpoints are loopback-safe, bounded, and redacted;
15. PAC and static content serving are tested and reload safely;
16. route explanation and upstream testing are available in human and machine-readable form;
17. differential compatibility fixtures cover regex routing and scheduling behavior where observable;
18. long-running restart, reload, and shutdown tests pass;
19. all cross-platform checks, Clippy, formatting, deny, audit, and interoperability jobs are green;
20. no native dependency, OpenSSL dependency, or unsafe code is introduced.

## Completion record

When Phase 2 is implemented, append:

```markdown
## Completion record

Implemented by commits:

- `<sha>` — routing rules and schedulers
- `<sha>` — health and runtime integration
- `<sha>` — TOML configuration and reload
- `<sha>` — metrics, admin, PAC, and static endpoints
- `<sha>` — graceful shutdown and final closure

All required checks passed on `<date>`.
```
