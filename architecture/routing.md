# eggress-routing — Rules, Schedulers, Health, Route Selection

Policy engine: decides Direct / UpstreamGroup / Reject for each request,
selects a concrete upstream via schedulers, tracks health with hysteresis, and
bounds concurrency with lease accounting. Hot-reload safe via atomic snapshot
swap.

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | `MatchExpr` (Any/All/AnyOf/Not composites + HostExact/Suffix/Regex, Destination/Source CIDR & port, Listener, Protocol, Identity, Transport, ReverseListener), `CompiledRule`, `RouteRequest`, `RouteDecision`, `Router` (`decide`/`select`/`route`/`explain`), `RouteService` trait, `SharedRoutingService` (arc-swap), `SelectedRoute` + `SelectionReason` (Normal/DirectFallback/UnhealthyFallback), `RouteError`, `CompatRegexRule` (pproxy-style line rule files) |
| `src/upstream.rs` | `UpstreamRuntime` (chain, enabled, load counters, health cell, probe), `UpstreamGroup` (members + scheduler + fallback), `GroupFallback` (Reject/Direct/UseUnhealthy) |
| `src/scheduler.rs` | `SchedulerKind`: FirstAvailable, RoundRobin, Random, LeastConnections; injectable random source for deterministic tests |
| `src/health.rs` | Six-state machine: Unknown → Healthy ⇄ Suspect ⇄ Unhealthy → Recovering → Healthy; `Disabled` terminal. Defaults: 3 consecutive failures to mark unhealthy, 2 successes to recover; probe interval with ±20% jitter. Eligible states: everything except Unhealthy/Disabled |
| `src/lease.rs` | `PendingLease` (in-flight, RAII decrement on drop) → `established()` → `ActiveLease` (active count) |

## Key invariants

- `SharedRoutingService::route()` loads ONE snapshot for decide+select;
  calling `decide()`/`select()` separately across a reload boundary is racy by
  design — always go through `route()` in data-plane code.
- First-match-wins rule evaluation; default action when nothing matches.
- Group fallback semantics: Reject → error; Direct → `SelectionReason::DirectFallback`;
  UseUnhealthy → picks from all enabled members.
- Host matching normalizes case/trailing dots but preserves IPv6 canonical form.

## Interactions

- `eggress-server::open_route()` calls `route()`, converts `PendingLease` to
  `ActiveLease` after successful upstream open.
- `eggress-runtime` swaps snapshots on reload and starts/stops
  `HealthManager` probes.
- `eggress-admin` renders rules/groups/explanations from the same `Router`.

## Review entry points

- Property tests: `tests/properties.rs` (proptest), `tests/scheduler_parity.rs`.
- Verify: `cargo test -p eggress-routing`
