# Routing Rules and Match Expressions

## When to use
Use when adding new routing rules, matchers, schedulers, or modifying route selection behavior.

## Architecture
- First-match-wins rule evaluation with configurable default action
- Rules are compiled into a `CompiledRule` AST
- `MatchExpr` supports recursive combinators: `all`, `any_of`, `not`
- Leaf matchers: host (exact/suffix/regex), CIDR, port range, port set, listener, protocol, identity, source, transport

## Adding a new matcher

1. Add the match variant to `MatchExpr` in `eggress-routing/src/rule.rs`
2. Add parsing in `eggress-config/src/model.rs` (TOML deserialization)
3. Add compilation in `eggress-config/src/compile.rs`
4. Add matching logic in `eggress-routing/src/rule.rs`
5. Add `route-explain` support in `eggress-admin/src/routes.rs`
6. Add tests

## Key types (`eggress-routing`)
- `RouteService` trait — pluggable routing backends
- `SharedRoutingService` — `ArcSwap`-backed implementation
- `RouteDecision` — rule match result
- `SelectedRoute` — Direct or Upstream with selection metadata
- `SelectionReason` — Normal, DirectFallback, UnhealthyFallback
- `PendingLease`/`ActiveLease` — connection accounting

## Adding a new scheduler
1. Implement the scheduler in `eggress-routing/src/scheduler/`
2. Register it in the scheduler factory
3. Add TOML string variant
4. Add unit tests with deterministic scenarios

## Testing
- `cargo test -p eggress-routing` — unit tests
- `cargo test -p eggress-runtime routing` — integration tests
- Route explanation: `/-/route-explain` endpoint

## Rule evaluation order
Rules are evaluated top-to-bottom. First match wins. If no rule matches, the default action applies (direct or reject, configurable).
