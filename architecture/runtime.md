# eggress-runtime — Supervisor, Snapshot Compilation, Reload, Shutdown

Process-level composition: binds listeners, owns shared state, compiles config
into the authoritative snapshot, runs signal handling, hot-reload, health
probes, metrics wiring, reverse proxy integration, and ordered shutdown.

## Module map

| File | Role |
|---|---|
| `src/supervisor.rs` | `ServiceSupervisor` (`start_from_config[_with_options]`, `run()`, `reload_config()`), `RuntimeState` (readiness flag, generation, cancellation tokens, task trackers, UDP registry, reverse registry, admin addr), `CompatibilityOptions`, `RuntimeAdminListenerInfos` (`AdminSnapshotProvider` so admin reads live snapshot per request), listener binding incl. Unix/transparent/TLS variants, signal loop (SIGTERM/SIGINT shutdown, SIGHUP reload only when file-backed) |
| `src/snapshot.rs` | `CompiledRuntimeSnapshot { generation, upstreams: HashMap<String, Arc<UpstreamRuntime>>, router, health_config, listeners, admin, reverse_servers, reverse_clients }`; `compile_runtime_snapshot(config, previous)` reuses unchanged upstream `Arc`s across reloads (identity preserved when chain+health config unchanged) and increments generation |
| `src/reverse.rs` | `RouteEngineTargetResolver`: gates reverse-client targets through `SharedRoutingService::decide()` with `transport = ReverseTcp` — routing is an authorization gate, not a redirect |
| `src/platform.rs` | Platform capability checks surfaced to listeners/diagnostics |
| `src/error.rs` | `RuntimeError` — startup failures are structured errors, never panics |

## Shutdown ordering (enforced sequence)

1. Readiness flips false (`/-/ready` starts failing).
2. Listener stop (no new connections).
3. UDP tasks: `udp_tasks.close()` then drain within `shutdown_grace`.
4. Connection tasks: close + grace drain, then force-cancel remaining.
5. Admin server stops LAST — `/metrics`, `/-/status`, `/-/ready` stay queryable
   through the drain.

Separate `CancellationToken`s exist for listeners vs. connections vs. health vs.
admin to make this ordering expressible.

## Reload semantics

Hot-swappable: rules, default action, groups, upstream definitions, health
config, PAC/static content. NOT hot-swappable: listener topology (binds,
protocols, TLS), UDP bind/advertise. Compile candidate first; on failure the
old snapshot stays live and `reload_failures_total` increments.

## Review entry points

- Integration suites live in `tests/`: `lifecycle_invariants.rs`,
  `shutdown.rs`, `reload.rs`, `startup.rs`, `observability.rs`,
  `retry_fallback.rs`, `multihop_tcp.rs`, `upstream_protocols.rs`,
  `security_invariants.rs`, `reverse_*`, `udp*.rs`, and more.
- Verify: `cargo test -p eggress-runtime reload` (focused),
  `cargo test -p eggress-runtime` (full).
