# eggress-runtime — Supervisor, Snapshot Compilation, Reload, Shutdown

Process-level composition: binds listeners, owns shared state, compiles
config into the authoritative snapshot, runs signal handling, hot-reload,
health probes, reverse routing gate, and ordered shutdown.

## Module map

| File | Role |
|------|------|
| `src/supervisor.rs` | `ServiceSupervisor` (`start`/`start_from_config[_with_options]`/`run()`/`reload_config()`); `RuntimeState` (readiness, tokens, task trackers, UDP registry, reverse registry); `CompatibilityOptions`; `RuntimeAdminListenerInfos` (`AdminSnapshotProvider`); `classify_listeners`; `compute_advertise_ip`; `ListenerConnectionSlot` |
| `src/snapshot.rs` | `CompiledRuntimeSnapshot { generation, upstreams, router, health_config, listeners, admin, reverse_servers, reverse_clients }`; `compile_runtime_snapshot(config, previous)` reuses unchanged upstream `Arc`s via ptr-identity when chain+health are identical; increments generation monotonically |
| `src/reverse.rs` | `RouteEngineTargetResolver` gates reverse-client targets through `SharedRoutingService::decide()` with `transport=ReverseTcp`; routing is an authorization gate, not a redirect |
| `src/platform.rs` | `PlatformCapability`, `CapabilityStatus`, `check_capability[_with_overrides]()`, `platform_info()` |
| `src/error.rs` | `RuntimeError` — `Config`, `ListenerBind`, `AdminBind`, `RuntimeInit`, `Other` |

## Public API

| Symbol | Notes |
|--------|-------|
| `ServiceSupervisor::start(path)` | Load config from file, enable SIGHUP reload |
| `ServiceSupervisor::start_from_config(cfg, path)` | Config from memory; SIGHUP only if `path` is `Some` |
| `ServiceSupervisor::start_from_config_with_options(cfg, path, compat)` | Compatibility flags (pproxy compat, `--sys`, debug, verbosity) |
| `ServiceSupervisor::run(&mut self)` | Blocking; owns signal loop and shutdown sequence |
| `ServiceSupervisor::reload_config(&mut self)` | Load-and-swap without blocking signal loop |
| `ServiceSupervisor::shutdown_token()` | Exposes master cancel for external callers |
| `RuntimeState::generation()` | Reads `snapshot.load().generation` |
| `compile_runtime_snapshot(rt, prev)` | `Result<CompiledRuntimeSnapshot, Box<dyn Error>>` |

## Startup sequence

1. `init_with_config`: validates feature gates (rejects reverse/operations
   configs when features absent), parses listener bind addresses.
2. First `compile_runtime_snapshot(&rt_config, None)` produces generation 0.
3. `SharedRoutingService` built from `snapshot.router`.
4. `RuntimeState` constructed (readiness `false`, counters zeroed).
5. Five `CancellationToken`s: master, listener, connection, health, admin.
6. `HealthManager` started under `health_cancel` if upstreams exist.
7. Listeners bound (TCP, transparent, Unix, QUIC). Admin pre-bound before
   readiness so bind failures surface as startup errors.
8. Reverse servers/clients spawned (feature `reverse`), each with master
   cancel clone.
9. System proxy applied (`--sys` + `operations` feature) after bind but
   before accept loops; failure is a startup error.
10. `readiness.store(true)` — `/-/ready` returns 200.
11. Signal loop enters: `tokio::select!` over cancel, `ctrl_c`, SIGTERM,
    SIGHUP (reload only when `config_path` is `Some`).

## Shutdown ordering

Code at `src/supervisor.rs:2733-2786`:

| Step | Action | Effect |
|------|--------|--------|
| 1 | `readiness.store(false)` | `/-/ready` returns 503 |
| 2 | `listener_cancel.cancel()` | No new connections accepted |
| 3 | `health_cancel.cancel()` | Stop health probes (prevents false unhealthy marking) |
| 4 | `udp_registry.close_all().await` | Close all UDP association state |
| 5 | `udp_tasks.close(); timeout(grace, udp_tasks.wait())` | Drain UDP relays within `shutdown_grace` |
| 6 | `tasks.close(); tasks.wait().await` | Wait for listener accept loops to exit |
| 7 | Poll `active_connections` every 100ms until 0 or deadline; `connection_cancel.cancel()` on timeout | Grace drain, then force-cancel |
| 8 | `connection_tasks.close(); connection_tasks.wait().await` | Wait for connection tasks |
| 9 | `ssh_sessions.shutdown().await` (feature `ssh`) | Flush SSH state |
| 10 | `admin_cancel.cancel(); admin_tasks.close(); admin_tasks.wait().await` | Admin stops **last** — queryable through drain |
| 11 | `compatibility_system_proxy.restore()` | Revert OS proxy if `--sys` used |

Each concern uses its own `CancellationToken` or `TaskTracker`.

## Reload path

1. `load_and_compile(config_path)` — file I/O via `spawn_blocking`.
2. `classify_listeners` rejects topology changes (count, name, bind, UDP
   bind, transparent toggle, unix socket path/add/remove).
3. `compile_runtime_snapshot(&new_config, prev_ref)` — Arc reuse when
   `old.chain == new.chain && old.health_config == new.health`.
4. `self.rt_config = new_rt_config` — kept in sync before snapshot publish.
5. **Snapshot before router swap**: `snapshot.store(new_snapshot)` then
   `routing.swap_arc(router)`. Readers seeing new generation see matching
   router.
6. Health probes restarted from new upstreams.

On failure, old snapshot stays live; `reload_failures_total` increments.

## Arc identity reuse rules

`compile_runtime_snapshot` reuses `Arc<UpstreamRuntime>` when upstream ID
matches and `upstream_runtime_compatible` (chain + health config identical)
holds. Changed upstreams get fresh Arcs. Groups and router share Arc
clones, so health probing and routing see identical upstream state. Partial
upstream changes preserve Arc identity for unchanged siblings.

## Reverse routing gate

`RouteEngineTargetResolver` (`src/reverse.rs:72-101`) builds a synthetic
`RouteRequest` with `transport=ReverseTcp` on each reconnection:

| Router decision | `TargetResolution` |
|----------------|-------------------|
| `Direct` or `UpstreamGroup` | `Connect { host, port }` — allowed, always to configured target |
| `Reject` | `Reject { reason }` — refused |

## Error model

`RuntimeError` variants: `Config(String)`, `ListenerBind { addr, source }`,
`AdminBind { addr, source }`, `RuntimeInit(io::Error)`, `Other(String)`.
Startup failures are structured `Result` errors, never panics.

## Feature gates

| Gate | Runtime effect |
|------|---------------|
| `operations` | Admin server, `RuntimeAdminListenerInfos`, system-proxy dep |
| `reverse` | Reverse server/client spawning, `reverse_registry` (implies `operations`) |
| `extended` | Shadowsocks metrics, `pproxy-legacy`, Shadowsocks UDP relay |
| `ssh` | `SshSessionCache`, SSH session shutdown |
| `quic` | QUIC/HTTP3 listener binding |

**Hot-reloadable:** rules, groups, upstreams, health config, PAC/static,
listener metadata, reverse config.

**Restart required:** listener bindings, log level/format, shutdown grace,
timeouts, admin bind, UDP bind/advertise, transparent toggle, unix socket
path.

## Concurrency

- Five `CancellationToken`s separate concerns; master propagates to reverse
  tasks.
- `TaskTracker`s: listener tasks, connection tasks, admin tasks,
  `state.udp_tasks`.
- `ArcSwap<CompiledRuntimeSnapshot>`: lock-free snapshot reads.
- `SharedRoutingService::swap_arc`: atomic routing table replacement.
- `AtomicBool` readiness, `AtomicU64` counters for connections and
  transparent metrics.
- `ListenerConnectionSlot` uses `fetch_update` (CAS) for lock-free
  per-listener connection limiting.

## Test coverage

| File | Behavior |
|------|----------|
| `lifecycle_invariants.rs` | Startup/shutdown/reload ordering |
| `shutdown.rs` | Graceful shutdown, drain, force-cancel |
| `reload.rs` | Reload success/rejection/failure, Arc reuse |
| `startup.rs` | Config validation, listener bind |
| `observability.rs` | Metrics, admin, readiness |
| `retry_fallback.rs` | Upstream retry and fallback |
| `multihop_tcp.rs` | Multi-hop chain relay |
| `upstream_protocols.rs` | Protocol detection |
| `security_invariants.rs` | Auth enforcement |
| `reverse_interop.rs` / `reverse_runtime.rs` / `reverse_soak.rs` | Reverse proxy |
| `routing.rs` | Rule matching |
| `scheduler_runtime.rs` / `load.rs` | Scheduling, distribution |
| `admin.rs` / `health.rs` | Admin endpoints, health probes |
| `pac_static.rs` | PAC file serving |
| `tls.rs` / `transparent.rs` / `unix_socket.rs` | Listener variants |
| `shadowsocks_tcp.rs` / `shadowsocks_udp.rs` | Shadowsocks |
| `trojan.rs` | Trojan protocol |
| `udp.rs` / `udp_upstream.rs` | UDP associations |
| `performance_smoke.rs` | Performance |

## Reviewer gotchas

- `reload_config()` is synchronous (called from the async signal loop via
  blocking path); config I/O is blocking.
- Snapshot published **before** router swap (`supervisor.rs:2862-2864`).
- `health_cancel` cancelled at step 3, not step 2, to avoid false
  unhealthy marking during drain.
- Admin stops **last** (step 10) so `/metrics` is queryable during drain.
- `RuntimeAdminListenerInfos` reads `ArcSwap` per request — no stale data.

## See also

- [overview.md](overview.md)
- [admin.md](admin.md)
- [routing.md](routing.md)
- [udp.md](udp.md)
- [protocols-reverse.md](protocols-reverse.md)
- [server.md](server.md)
- [config.md](config.md)
