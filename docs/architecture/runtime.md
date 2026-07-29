# eggress-runtime

`crates/eggress-runtime/`

Service supervisor and composition layer — the runtime lifecycle manager that ties all components together.

## Key Types

| Type | Description |
|---|---|
| `ServiceSupervisor` | Top-level runtime entry: start, run, graceful shutdown |
| `RuntimeState` | Shared state: snapshot, readiness, metrics, routing, connection tracking |
| `CompiledRuntimeSnapshot` | Single authoritative snapshot of the entire runtime |
| `RuntimeError` | Structured startup/shutdown errors (not panics) |

## Startup Sequence

```
ServiceSupervisor::run()
  1. Parse and validate config
  2. compile_runtime_snapshot()
     ├─ Build shared upstream registry
     ├─ Build router from rules
     ├─ Build health plan
     └─ Build admin snapshot
  3. Pre-bind listeners (before readiness)
  4. Start health probes
  5. Start admin server
  6. Set readiness = true
  7. Accept connections on listeners
```

## Reload (SIGHUP)

```
Signal: SIGHUP
  1. Parse new config
  2. Compile new snapshot
  3. Atomic swap via ArcSwap
  4. New connections use new snapshot
  5. Existing connections continue on old snapshot
  6. Listener topology NOT reloaded (requires restart)
```

Reloads are atomic — the new snapshot is compiled before swapping. If compilation fails, the old snapshot remains active.

## Shutdown Sequence

```
1. Readiness = false (reject new connections)
2. Stop listeners (stop accepting)
3. Drain existing connections (with timeout)
4. Force-cancel remaining connections
5. Stop admin server (stays up through drain for /-/ready, /metrics)
6. Stop health probes
7. Return
```

## Snapshot Compilation

`compile_runtime_snapshot()` builds:
- `Arc<UpstreamRuntime>` — shared upstream state (active connections, health)
- `Router` — compiled rules with first-match evaluation
- `HealthPlan` — probe configuration per upstream
- `AdminSnapshot` — PAC, static routes, listeners

The snapshot is the single source of truth shared by router, health manager, admin server, and metrics.

## Reverse Proxy Integration

`eggress-runtime/src/reverse.rs` bridges the routing engine to reverse clients:
- `RouteEngineTargetResolver` implements `TargetResolver`
- Routing decisions gate target resolution
- Reverse client/server lifecycles managed by the supervisor

## Dependencies

- `eggress-config` — configuration parsing
- `eggress-server` — connection handling
- `eggress-routing` — rule engine
- `eggress-metrics` — metrics recording
- `eggress-admin` — admin HTTP server
- `eggress-udp` — UDP association management
- `eggress-protocol-reverse` — reverse proxy
- `eggress-system-proxy` — system proxy inspection

See [overview.md](overview.md) for context.
