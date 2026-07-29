# eggress-routing

`crates/eggress-routing/`

Policy-driven routing engine with rule matching, upstream selection, health tracking, and schedulers.

## Key Types

| Type | Description |
|---|---|
| `MatchExpr` | Composite matcher: `All`, `AnyOf`, `Not`, host, port, CIDR, protocol, identity, transport, listener |
| `CompiledRule` | First-match-wins rule with match expression and action (upstream group, direct, reject) |
| `Router` | Evaluates rules against a `RouteRequest`, returns `RouteDecision` |
| `RouteService` | Trait for pluggable routing backends |
| `SharedRoutingService` | `ArcSwap`-backed `RouteService` for atomic config reload |
| `RouteRequest` | Input to routing: target, protocol, transport, client identity, listener |
| `RouteDecision` | Output: `Match(rule_id, action)` or `Default(action)` |
| `SelectedRoute` | Final selection: `Direct { reason }` or `Upstream { group, upstream, lease }` |
| `SelectionReason` | `Normal`, `DirectFallback`, `UnhealthyFallback` |

## Rule Evaluation

Rules are evaluated in order (first-match-wins). A rule's `MatchExpr` is a recursive tree:

```toml
[[rules]]
id = "dns-to-direct"
upstream_group = "direct"

[rules.match]
all = [
  { destination_port = 53 },
  { protocol = "socks5" }
]
```

Supported matchers:
- `host_exact`, `host_suffix`, `host_regex` — destination hostname matching
- `destination_port`, `source_port` — port matching (range or set)
- `source_cidr` — source IP CIDR matching
- `protocol` — inbound protocol (http, socks4, socks5, etc.)
- `identity` — client identity (username)
- `transport` — tcp or udp
- `listener` — listener ID

## Upstream Selection

### Schedulers

| Scheduler | Behavior |
|---|---|
| `FirstAvailable` | First healthy upstream in list |
| `RoundRobin` | Cycle through healthy upstreams |
| `Random` | Random healthy upstream |
| `LeastConnections` | Fewest active connections |

### Lease Accounting

- `PendingLease` — created when route is selected, before connection completes
- `ActiveLease` — promoted when connection is established
- Tracked per-upstream for active connection counts

## Health State Machine

```
Unknown → Healthy ↔ Suspect → Unhealthy → Recovering → Healthy
                                     ↓
                                  Disabled
```

- Active TCP health probes with configurable intervals and jitter
- Hysteresis: transitions require consecutive successes/failures
- `Disabled` is a manual operator state

## Route Explanation

`Router.explain()` returns a `RouteExplanation` showing which rule matched, why, and what the fallback path was. Used by the admin API and CLI `route-explain` command.

## Dependencies

- `eggress-core` — `TargetAddr`, `ProtocolId`, `SessionContext`
- `eggress-uri` — URI parsing for pproxy-compatible rule syntax

See [overview.md](overview.md) for context.
