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
- `eggress-transport-tls` — TLS transport
- `eggress-protocol-shadowsocks` — Shadowsocks AEAD relay (optional, `extended` feature)
- `eggress-protocol-reverse` — reverse proxy (optional, `reverse` feature)
- `eggress-uri` — URI parsing

## Feature Gates

The runtime crate uses feature flags to conditionally compile optional protocol and operational integrations:

- **`extended`**: Enables Shadowsocks metrics initialization, UDP relay, and chain executor handler registration. Gates `eggress-protocol-shadowsocks` and forwards to `eggress-server/extended` and `eggress-metrics/extended`.
- **`reverse`**: Enables reverse server/client spawning in the supervisor. Gates `eggress-protocol-reverse`.
- **`operations`**: Enables system proxy inspection. Gates `eggress-system-proxy`.

Admin and metrics remain required dependencies because they are tightly coupled to the runtime snapshot invariant. The `RuntimeState` struct conditionally includes `shadowsocks_metrics` and `reverse_registry`/`reverse_metrics` fields based on these features.

Under a lean build (`--no-default-features --features common`), the runtime provides only HTTP/SOCKS core proxying with direct TCP/UDP and TLS transport. Extended protocol URIs, reverse proxy configs, and system proxy commands fail with structured diagnostics.

The `extended` group also enables the internal `eggress-udp/shadowsocks`
feature. Without it, UDP route selection returns the existing unsupported
capability path; it does not fall back to direct UDP. Admin and metrics remain
compiled in both groups because they participate in the shared runtime
snapshot.

See [overview.md](overview.md) for context.
