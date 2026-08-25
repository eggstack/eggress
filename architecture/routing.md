# eggress-routing -- Rules, Schedulers, Health, Route Selection

Policy engine: decides Direct / UpstreamGroup / Reject for each request,
selects a concrete upstream via schedulers, tracks health with hysteresis, and
bounds concurrency with lease accounting. Hot-reload safe via atomic snapshot
swap.

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | `MatchExpr` (17 variants + 4 composites), `CompiledRule`, `RouteRequest`, `RouteDecision`, `Router` (`decide`/`select`/`route`/`explain`), `RouteService` trait, `SharedRoutingService` (arc-swap), `SelectedRoute` + `SelectionReason`, `RouteError`, `CompatRegexRule`, host normalization |
| `src/upstream.rs` | `UpstreamRuntime` (chain, enabled flag, load counters, health cell, probe, config), `UpstreamGroup` (members + scheduler + fallback), `GroupFallback`, `validate_upstream_id` / `validate_group` |
| `src/scheduler.rs` | `SchedulerKind`: FirstAvailable, RoundRobin, Random, LeastConnections; `Scheduler` trait with `select` + `preview`; injectable `RandomIndex` for deterministic tests |
| `src/health.rs` | Six-state machine (`HealthState`), `HealthCell` (RwLock), `HealthConfig`, `HealthManager` (probe tasks + semaphore), `is_eligible`, `probe_tcp` |
| `src/lease.rs` | `PendingLease` (in-flight, RAII decrement on drop) and `ActiveLease` (active count, RAII decrement on drop) |

## Public API surface

| Type | Variants / Fields |
|---|---|
| `MatchExpr` | `Any`, `All(Vec)`, `AnyOf(Vec)`, `Not(Box)`, `HostExact`, `HostSuffix`, `HostRegex`, `DestinationCidr`, `DestinationPort` (`Exact`/`Range`/`Set`), `DestinationPortRegex`, `SourceCidr`, `SourcePort`, `Listener`, `Protocol`, `Identity`, `Transport`, `ReverseListener` |
| `TransportKind` | `Tcp`, `Udp`, `ReverseTcp` |
| `CompiledRule` | `id: RuleId`, `matcher: MatchExpr`, `action: RouteActionSpec` |
| `RouteActionSpec` | `Direct`, `UpstreamGroup(UpstreamGroupId)`, `Reject(RejectReason)` |
| `RouteDecision` | `Direct { rule }`, `UpstreamGroup { rule, group }`, `Reject { rule, reason }` |
| `RouteRequest<'a>` | `target`, `source: Option<SocketAddr>`, `listener`, `inbound_protocol`, `identity`, `transport` |
| `SelectedRoute` | `Direct { decision, selection_reason }`, `Upstream { decision, group, upstream, chain, pending_lease, selection_reason }` |
| `SelectionReason` | `Normal`, `DirectFallback`, `UnhealthyFallback` |
| `RouteError` | `Rejected { rule, reason }`, `NoEligibleUpstream(group)`, `UnknownGroup(group)` |

**Traits:** `RouteService` (methods: `decide`, `select`, `route` with default). `Router` and `SharedRoutingService` both implement it. `Router` holds `Vec<CompiledRule>`, `RouteActionSpec` default, `HashMap<UpstreamGroupId, Arc<UpstreamGroup>>`. `SharedRoutingService` wraps `ArcSwap<RoutingServiceInner>`.

**CompatRegexRule:** parses pproxy-style line rule files; each line compiled to `regex::Regex`, empty/comments skipped. Matches against `hostname:port` via a 320-byte stack buffer (heap fallback). `parse_file` returns 1-indexed line numbers on error.

## How it works

### Rule evaluation: first-match-wins + default

`Router::decide()` at `src/lib.rs:279-310` iterates `self.rules` in order. The first `CompiledRule` whose `matcher.matches(request)` returns `true` produces the `RouteDecision`. If no rule matches, the `default_action` applies with `RuleId("default")`.

### Selection flow

`Router::select()` at `src/lib.rs:538-612`:

1. **Direct** -- returns `SelectedRoute::Direct` immediately.
2. **Reject** -- returns `Err(RouteError::Rejected)`.
3. **UpstreamGroup** -- looks up the group, collects eligible members via `is_eligible()`, calls `scheduler.select()`. If candidates exist and the scheduler returns `Some`, wraps in `SelectedRoute::Upstream` with `SelectionReason::Normal` and a fresh `PendingLease`.

### GroupFallback semantics (when eligible candidates are empty)

| Fallback | Behavior | `SelectionReason` |
|---|---|---|
| `Reject` | `Err(RouteError::NoEligibleUpstream)` | n/a |
| `Direct` | `Ok(SelectedRoute::Direct)` | `DirectFallback` |
| `UseUnhealthy` | Filters to `is_enabled()` members (ignoring health), calls scheduler, falls back to `.first()`. If none, `Err(NoEligibleUpstream)` | `UnhealthyFallback` |

### SharedRoutingService::route() atomicity

```rust
// src/lib.rs:671-675
fn route(&self, request: &RouteRequest) -> Result<SelectedRoute, RouteError> {
    let inner = self.inner.load();          // ONE snapshot
    let decision = inner.router.decide(request);
    inner.router.select(&decision, request)
}
```

`load()` returns an `Arc<RoutingServiceInner>`. Both `decide` and `select` operate on the same `Arc<Router>`, so a `swap()` during a config reload cannot interleave a new router mid-decision. Separate `decide()` + `select()` calls each do their own `load()` and may see different routers.

### Host normalization rules

`normalize_host_for_exact()` at `src/lib.rs:138-147`:
- Strips trailing `.` (e.g. `example.com.` -> `example.com`).
- If parseable as `IpAddr`, canonicalizes via `ip.to_string()` (lowercases IPv6 hex, collapses padding). So `FE80::1` equals `fe80::1` and `fe80:0:0:0:0:0:0:1`.
- Otherwise, `to_ascii_lowercase()`.

`HostSuffix` also strips trailing dots and lowercases before suffix comparison; it requires a label boundary (full match or `.` prefix in the suffix) so `notexample.com` does not match suffix `example.com`.

`HostRegex` tests against the raw host string (no normalization).

### Health state machine

Six states in `HealthState` (`src/health.rs:10-18`):

| Current state | On success | On failure |
|---|---|---|
| `Unknown` | Stay `Unknown` until `consecutive_successes >= successes_to_healthy`, then `Healthy` | `Suspect` (1 failure); `Unhealthy` at threshold |
| `Healthy` | Stay `Healthy` | `Suspect` (below threshold); `Unhealthy` at threshold |
| `Suspect` | `Healthy` | Stay `Suspect` (below threshold); `Unhealthy` at threshold |
| `Unhealthy` | `Recovering` | Stay `Unhealthy` |
| `Recovering` | Stay `Recovering` until `consecutive_successes >= successes_to_healthy`, then `Healthy` | `Unhealthy` (any failure) |
| `Disabled` | `Disabled` | `Disabled` |

Defaults (`HealthConfig::default()` at `src/health.rs:41-51`):
- `interval`: 30 s, `timeout`: 5 s.
- `failures_to_unhealthy`: 3, `successes_to_healthy`: 2.
- `initial_state`: `Unknown`.

Jitter: each probe delay is `interval +/- 20%` via signed `fastrand::f64()` multiplication (`src/health.rs:236-243`). Probe concurrency bounded by a 10-permit semaphore (`src/health.rs:216`).

### Eligibility

`is_eligible()` at `src/health.rs:189-200` returns `true` when `upstream.is_enabled()` AND state is `Unknown | Healthy | Suspect | Recovering`. `Unhealthy` and `Disabled` are excluded.

### Lease RAII lifecycle

`PendingLease::new()` at `src/lease.rs:18-24`: increments `in_flight` atomically. On drop (connection rejected or abandoned), `in_flight` is decremented.

`PendingLease::established()` at `src/lease.rs:26-34`: sets state to `Transferred`, decrements `in_flight`, increments `active`. Returns `ActiveLease`.

`ActiveLease::drop()` at `src/lease.rs:59-62`: decrements `active`.

This two-phase design means `in_flight` tracks route-selection-to-upstream-open latency while `active` tracks the live relay session. `UpstreamRuntime::current_load()` returns `active + in_flight`.

### Schedulers

| Kind | Strategy | Determinism |
|---|---|---|
| `FirstAvailable` | First candidate where `is_eligible()` | List-order dependent |
| `RoundRobin` | Atomic cursor, `compare_exchange` loop, skips ineligible | Concurrent-safe, skips disabled/unhealthy |
| `Random` | `fastrand::usize` start index, circular scan for eligible | Seeded via `RandomIndex` trait (production: `FastrandRandom`; tests: `DeterministicRandom`) |
| `LeastConnections` | `min_by_key(current_load())` | Tie-broken by iterator order |

### CompatRegexRule matching

`CompatRegexRule::matches()` at `src/lib.rs:716-724`: formats `hostname:port` into a stack buffer (320 bytes), falls back to `format!()` for oversized targets. The regex runs against this combined string, not the hostname alone.

## Error and failure model

| Error variant | Source | Semantics |
|---|---|---|
| `RouteError::Rejected { rule, reason }` | Policy matched with `Reject` action | Client sees structured reject (access denied, blocked, etc.) |
| `RouteError::NoEligibleUpstream(group)` | Fallback=Reject and no eligible members; or UseUnhealthy with no enabled members | Upstream group is exhausted or entirely down |
| `RouteError::UnknownGroup(group)` | Rule references an `UpstreamGroupId` not in `Router::groups` | Config error or stale snapshot |

`RouteError` is `thiserror::Error` with `Display` impl.

## Configuration and features

`HealthConfig` is configurable per-upstream:
- `interval`, `timeout`, `failures_to_unhealthy`, `successes_to_healthy`, `initial_state`.
- `UpstreamRuntime::with_health_config()` sets both the config and reinitializes the `HealthCell` with the configured `initial_state`.

`SchedulerKind` is set per-group and cannot change after construction.

`GroupFallback` is set per-group at construction.

No Cargo features gate routing functionality (all routing code is always compiled).

## Security notes

- `SourceCidr` / `SourcePort` match against `request.source` which is the peer socket address. Spoofable at the transport layer if the listener does not enforce real peer addresses (e.g. transparent proxy with netfilter).
- `CompatRegexRule` compiles user-supplied regex. Regexes are bounded by `regex::Regex` complexity limits (DFA size). No user-controlled regex can cause exponential backtracking due to the regex crate's guarantees.
- Credentials and secret-bearing URIs are redacted before logging; `RouteExplanation::chain` uses `RedactedUri::new()` (`src/lib.rs:397`).
- `validate_upstream_id()` at `src/upstream.rs:21-28` enforces `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$` to prevent label injection.

## Concurrency and lifecycle

- `SharedRoutingService` uses `arc_swap::ArcSwap` for lock-free snapshot reads. `swap()` and `swap_arc()` store new inner values; `load()` returns an `Arc` that can be held across the decision.
- `HealthCell` uses `RwLock<HealthSnapshot>` with poisoned-lock recovery (`unwrap_or_else(|e| e.into_inner())`).
- `HealthManager` spawns one task per upstream with a `JoinSet`; probes are bounded by a 10-permit semaphore. `stop_all()` aborts all tasks.
- All atomic counters use `Ordering::Relaxed` -- sufficient for approximate metrics and health checks.
- `Router` groups are wrapped in `Arc` for clone-safe sharing across connections.
- Reload swaps the entire `Router` atomically; listener topology is never hot-reloaded.

## Test coverage map

| Area | Files / tests | What is covered |
|---|---|---|
| MatchExpr | `src/lib.rs:796-1323` | 30+ tests: host exact/suffix/regex, CIDR IPv4/v6, port exact/range/set, source CIDR/port, listener, protocol, identity, composite All/AnyOf/Not, empty All/AnyOf |
| Router decide/select | `src/lib.rs:1155-1244` | first-match-wins, default action, upstream group, reject, accessor methods |
| Health state machine | `src/health.rs:306-508` | 15+ tests: every state transition, thread safety (100 threads), eligibility, timestamps, failure resets counter, Disabled terminal |
| Jitter | `src/health.rs:572-596` | 1000 iterations, validates +/- 20% range |
| Probe | `src/health.rs:536-560` | TCP probe success, failure, timeout |
| Schedulers | `src/lib.rs:1431-1456`, `src/scheduler.rs:230-312` | FirstAvailable order, disabled skip, RoundRobin, Random determinism, LeastConnections |
| Lease RAII | `src/lib.rs:1389-1429` | PendingLease decrement on drop, established->active, ActiveLease decrement on drop |
| CompatRegexRule | `src/lib.rs:1247-1294` | Parse valid/invalid, file parsing, line numbers, hostname:port matching |
| Properties | `tests/properties.rs` | Proptest-based property tests |
| Scheduler parity | `tests/scheduler_parity.rs` | Cross-scheduler behavioral equivalence |

## Reviewer gotchas

1. **`decide()` + `select()` is racy on `SharedRoutingService`.** Each method does its own `inner.load()`. A config reload between the two calls can cause `select()` to operate on a different `Router` than `decide()`. Always use `route()` in data-plane code.
2. **`is_eligible()` excludes `Recovering` from the unhealthy set.** Recovering upstreams are eligible for selection -- this is intentional to allow traffic to resume as soon as the upstream starts accepting connections.
3. **`UseUnhealthy` fallback ignores health but respects the `enabled` flag.** A manually disabled upstream is never selected, even under `UseUnhealthy`.
4. **RoundRobin cursor advances past ineligible members.** The `compare_exchange` loop skips disabled/unhealthy members but still advances the cursor, so the distribution is stable even when members toggle.
5. **`DestinationPortRegex` matches the decimal port as a string**, not as a number. `^80$` matches port 80 but `^0*80$` also matches (leading zeros are included in the decimal representation).
6. **`HostRegex` does NOT normalize the input.** Unlike `HostExact`/`HostSuffix`, the regex receives the raw hostname including case and trailing dots.
7. **Lease `PendingLease` must not be leaked.** If `established()` is never called and the `PendingLease` is dropped, `in_flight` decrements automatically via RAII. But `in_flight` being non-zero does not block selection (it only affects `current_load()` for LeastConnections).
8. **`CompatRegexRule` tests against `hostname:port`**, not just the hostname. Rules from pproxy files test both parts.

## See also

- [metrics.md](metrics.md) -- Prometheus registry, bridge mechanics, H2 atomics source
- [../docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) -- system-wide architecture
- [../docs/CI_STATUS.md](../docs/CI_STATUS.md) -- CI and verification policy
