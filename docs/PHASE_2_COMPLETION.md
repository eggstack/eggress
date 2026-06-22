# Phase 2 Integration Pass — Completion Record

## Summary

The Phase 2 integration pass addressed all remaining architectural gaps between
isolated crate implementations and a working end-to-end proxy service. A
follow-up final-closure cleanup pass then removed the last precision and
runtime-semantics issues (fallible supervisor, single generation source, live
admin reads, PAC/static reload freshness, route-explain context, admin-during-
drain, and a documented list of Phase 2 operational limitations). The work
spans 15+ commits and is validated by 583 unit and integration tests.

## Commits

| Commit | Description |
|--------|-------------|
| `d843f6f` | Fix post-audit gaps: G5 connection task tracking, G2 health probe reconciliation |
| `7c6f4fb` | Phase 2 final integration pass: shared runtime snapshot, health config, graceful shutdown, atomic reload, PAC/static config, integration tests |
| `0f9fd6b` | Remove dual generation counter from SharedRoutingService |
| `eba1d38` | Move listener and admin config into CompiledRuntimeSnapshot |
| `ae8b1e7` | Fix integration test timeouts: expose admin local addr, add AutoShutdown guard |
| `e97f8fe` | Add shutdown_force_cancels_after_deadline test, complete example-config.toml |
| `bf07e3f` | Add missing integration tests, fix health probe config, fill completion record |
| `456dd3c` | Complete Phase 2 closure: update completion record, add completion doc |
| `7a75ccb` | Add Phase 2 final closure cleanup plan |
| `0b44f31` | Make supervisor run fallible and add bind-conflict test |
| `a91857c` | Single generation source via RuntimeState helper |
| `e650e06` | Make admin read live snapshot data and remove stale router |
| `4a5265f` | Keep admin alive during connection drain and extend route-explain |

## Architecture Changes

### Shared Runtime Snapshot

`CompiledRuntimeSnapshot` is the single authoritative source for the runtime.
It owns:

- Router (compiled rule AST)
- Upstream registry (`Arc<UpstreamRuntime>` per ID)
- Listener configs (pre-bound before readiness)
- Admin config (PAC, static content)
- Health config (per-upstream probe settings)
- Generation counter (the only authoritative generation value)

Router, health manager, admin, and metrics share the same `Arc<UpstreamRuntime>`
objects. Pointer-identity tests verify this.

`RuntimeState` exposes `generation()` which returns `self.snapshot.load().generation`.
No duplicate `Arc<AtomicU64>` exists on `RuntimeState` or `AdminState`; the
admin server reads the snapshot per request through an `AdminSnapshotProvider`
implemented on the runtime.

### Service Supervisor Lifecycle

```
start() → build snapshot → pre-bind listeners → set readiness
run()   → spawn accept loops → start health probes → start admin → wait for shutdown
shutdown →
  1. readiness = false
  2. listener_cancel.cancel(); wait for listener tasks
  3. drain active connections until shutdown_grace
  4. force-cancel connections if grace exceeded; wait for connection tasks
  5. admin_cancel.cancel(); wait for admin tasks
```

`ServiceSupervisor::run()` returns `Result<(), RuntimeError>`. Listener bind
failures, tokio runtime init errors, and reload-time failures are all
structured rather than panics. The CLI propagates errors with a nonzero exit
code.

### Live Admin Reads

`AdminState` no longer carries a startup-captured router, PAC config, or
static content list. Instead, the runtime implements `AdminSnapshotProvider`
which produces an `AdminSnapshot { generation, router, pac, static_routes,
listeners }` from the current `CompiledRuntimeSnapshot` on every request. A
`StaticAdminSnapshot` helper is provided for tests.

This means:
- PAC content is reloaded live
- Static content is reloaded live
- Route explanation reflects the reloaded router and generation
- Listener info combines `snapshot.listeners` (config) with the bound
  `listener_addrs` populated after the listener bind step

### Atomic Config Reload

1. Candidate snapshot compiled from new TOML
2. Unsupported topology changes rejected (listener count, bind addresses, names)
3. Router swapped atomically via `ArcSwap`
4. Snapshot swapped via `Arc<ArcSwap<CompiledRuntimeSnapshot>>`
5. Health tasks stopped and restarted from new snapshot
6. Old state untouched on failure

### Per-Upstream Health Config

Each upstream can have its own health probe settings in TOML. The
`HealthManager::start_probes` API takes only the upstreams list; per-upstream
`HealthConfig` is used exclusively and the previous `HealthConfig::default()`
parameter has been removed.

```toml
[[upstreams]]
id = "proxy-us"
uri = "socks5://proxy-us:1080"

  [upstreams.health]
  mode = "tcp_connect"
  interval = "15s"
  timeout = "3s"
  failures_to_unhealthy = 3
  successes_to_healthy = 2
```

### Route Explanation

`/-/route-explain` accepts optional `source` (SocketAddr) and `identity`
(Username, 1-256 bytes, non-empty) fields. Invalid source → 400; empty or
oversized identity → 400. Identity is not echoed back as a credential secret.

## Definition of Done — Checklist

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | CompiledRuntimeSnapshot owns router, upstream registry, listeners, admin, health plan | Pass | `snapshot.rs:10-17` |
| 2 | Router, health, admin, metrics share same upstream Arc objects | Pass | `snapshot.rs:180-202` (ptr_eq test) |
| 3 | Health transitions affect scheduler eligibility | Pass | `tests/health.rs` (health_affects_routing) |
| 4 | Configured listener names reach route matching | Pass | `tests/routing.rs:32-51` |
| 5 | Listeners bound before readiness; bind failure aborts startup | Pass | `tests/startup.rs:bind_conflict_aborts_startup_before_readiness` (negative-path covered) |
| 6 | Admin lists real listeners, uses current state after reload | Pass | `tests/admin.rs:status_lists_listeners`, `tests/admin.rs:admin_routes_reflect_reload` |
| 7 | One authoritative generation value | Pass | `snapshot.rs:100-103`; `RuntimeState::generation()` reads snapshot |
| 8 | `/-/ready` reflects startup and shutdown state | Pass | `tests/startup.rs:readiness_starts_false_before_run`, `tests/shutdown.rs:readiness_transitions_to_false_on_shutdown`, `tests/shutdown.rs:admin_responds_during_shutdown_drain` |
| 9 | Shutdown stops acceptance, drains, force-cancels after deadline | Pass | `tests/shutdown.rs:shutdown_drains_active_connections`, `tests/shutdown.rs:shutdown_force_cancels_after_deadline` |
| 10 | All connection tasks joined | Pass | `supervisor.rs` (TaskTracker) |
| 11 | Reload atomic, non-destructive, scope documented | Pass | `tests/reload.rs` (6 tests); `README.md` Phase 2 limitations |
| 12 | Health tasks reconcile on reload | Pass | `tests/health.rs:reload_preserves_health_state` |
| 13 | PAC/static fully configured and served | Pass | `tests/pac_static.rs` (6 tests including reload-freshness) |
| 14 | Direct fallback retains selection metadata | Pass | `tests/routing.rs:direct_fallback_reason_preserved` |
| 15 | Failure categories accurate | Pass | `server/src/lib.rs` (failure category tests) |
| 16 | Live route explanation uses current generation and router | Pass | `admin/src/routes.rs:/-/route-explain`; `tests/admin.rs:admin_route_explain_reflects_reload` |
| 17 | Binary-level integration tests for all subsystems | Pass | `tests/{startup,routing,health,admin,reload,shutdown,pac_static}.rs` |
| 18 | README checkboxes match integrated behavior | Pass | All checked items backed by integration tests |
| 19 | Fallible supervisor: bind and runtime init errors are structured, not panics | Pass | `supervisor.rs:run() -> Result<(), RuntimeError>`; CLI returns nonzero on error; `tests/startup.rs:bind_conflict_aborts_startup_before_readiness` |
| 20 | Admin PAC, static content, and route-explain serve live data after reload | Pass | `tests/pac_static.rs:pac_reload_serves_new_content`, `static_content_reload_serves_new_body`, `tests/admin.rs:admin_routes_reflect_reload`, `admin_route_explain_reflects_reload` |
| 21 | Route explanation supports source and identity matchers | Pass | `tests/admin.rs:route_explain_source_field_changes_decision`, `route_explain_identity_field_changes_decision`, `route_explain_invalid_source_returns_400` |
| 22 | Admin stays available during connection drain | Pass | `tests/shutdown.rs:admin_responds_during_shutdown_drain`, `admin_metrics_visible_during_drain` |
| 23 | All formatting, Clippy, unit, integration checks pass | Pass | clippy clean, fmt clean, 583 tests pass |
| 24 | No unsafe, no OpenSSL, no native dependency | Pass | `unsafe_code = "forbid"` workspace-wide |
| 25 | Phase 3 blocked until criteria met | Pass | This document |

## Test Inventory

| Category | File | Tests |
|----------|------|-------|
| Startup | `tests/startup.rs` | 6 |
| Routing | `tests/routing.rs` | 7 |
| Health | `tests/health.rs` | 5 |
| Admin | `tests/admin.rs` | 15 |
| Reload | `tests/reload.rs` | 6 |
| Shutdown | `tests/shutdown.rs` | 9 |
| PAC/Static | `tests/pac_static.rs` | 6 |

Plus the eggress-admin, eggress-routing, eggress-server, eggress-config,
eggress-protocol-http, eggress-protocol-socks, eggress-testkit, and
eggress-cli unit test suites. Total workspace: 583 tests.

## Notes

### Phase 2 operational limitations

- Listener topology changes (count, names, bind addresses) require restart;
  routing, upstreams, health config, and admin content are hot-reloadable.
- All other runtime state is reloaded atomically on SIGHUP without dropping
  connections; admin endpoints reflect the reloaded state on the next request.

### Completion date

All checks passed on 2026-06-22.
