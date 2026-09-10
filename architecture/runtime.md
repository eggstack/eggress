# eggress-runtime — Supervisor, Snapshot Compilation, Reload, Shutdown

Process-level composition: binds listeners, owns shared state, compiles
config into the authoritative snapshot, runs signal handling, hot-reload,
health probes, reverse routing gate, and ordered shutdown.

## Module map

| File | Role |
|------|------|
| `src/supervisor.rs` | Orchestration facade: `ServiceSupervisor` public API (`start`/`start_from_config`/`start_from_config_with_compatibility` + deprecated legacy `start_from_config_with_options` shim/`run()`/`reload_config()`), `CompatibilityRuntimeHooks` + legacy `CompatibilityOptions` adapter + `SystemProxyRequest`, listener-prep dispatch, transport accept loops, admin/signal orchestration |
| `src/supervisor/startup.rs` | `init_supervisor()` (feature gates, bind pre-validation, metrics/UDP/health wiring, `RuntimeState` assembly), `resolve_udp_global_limit()`, `build_ssh_sessions(allow_insecure: bool)` |
| `src/supervisor/state.rs` | `RuntimeState` (snapshot, routing, session + runtime metrics, readiness, accounting, UDP registry, health, reverse state) + canonical `apply_compiled_config` transaction |
| `src/supervisor/reload.rs` | `ReloadResult`, `classify_listeners()` + `classify_reload_config()` (restart-required contract) |
| `src/supervisor/connection.rs` | `PreparedListener`/`PreparedQuicListener`, shared `wrap_tls_server()`, `build_connection_config()` (`ConnectionBuildParams`, `InboundSecurity`) |
| `src/supervisor/udp_runtime.rs` | `RuntimeUdpService` (`UdpService` impl), `make_udp_service()`, `compute_advertise_ip()`, `prepare_shadowsocks_udp_relay()` |
| `src/supervisor/operations.rs` | `RuntimeAdminListenerInfos` (`AdminSnapshotProvider` over the live snapshot) |
| `src/supervisor/accounting.rs` | `ListenerConnectionSlot` (per-listener limits), `ActiveConnectionGuard` (exactly-once global accounting), accept-error backoff |
| `src/supervisor/shutdown.rs` | `ShutdownPlan` + `shutdown_ordered()` (readiness false, listener stop, drain, admin last) |
| `src/snapshot.rs` | `CompiledRuntimeSnapshot { generation, upstreams, router, timeouts, listeners, admin, reverse_servers, reverse_clients }`; `compile_runtime_snapshot(config, previous)` reuses unchanged upstream `Arc`s via ptr-identity when chain+health are identical; increments generation monotonically |
| `src/reverse.rs` | `RouteEngineTargetResolver` gates reverse-client targets through `SharedRoutingService::decide()` with `transport=ReverseTcp`; routing is an authorization gate, not a redirect |
| `src/platform.rs` | `PlatformCapability`, `CapabilityStatus`, `check_capability[_with_overrides]()`, `platform_info()` |
| `src/error.rs` | `RuntimeError` — `Config`, `ListenerBind`, `AdminBind`, `RuntimeInit`, `Other` |

## Public API

| Symbol | Notes |
|--------|-------|
| `ServiceSupervisor::start(path)` | Load config from file, enable SIGHUP reload |
| `RuntimeState::apply_compiled_config(new_config)` | Canonical reload transaction: classify → snapshot build → publish snapshot/routing/admin → health restart → H2 clear → metrics (success *and* failure); preserves generation on reject/fail |
| `ServiceSupervisor::start_from_config(cfg, path)` | Config from memory; SIGHUP only if `path` is `Some`; passes no compatibility state (`None`) |
| `ServiceSupervisor::start_from_config_with_compatibility(cfg, path, hooks)` | Explicit pproxy compat path; `hooks: CompatibilityRuntimeHooks` built via `from_facade()` (auth reuse handle, `--sys` opt-in, SSH env decision) |
| `CompatibilityRuntimeHooks::from_legacy_options(options)` | Canonical legacy conversion; `auth_timeout`/`system_proxy`/`compatibility_mode` become narrow hooks, `debug`/`verbose_level` ignored (facade-owned logging) |
| `ServiceSupervisor::start_from_config_with_options(cfg, path, options)` | Deprecated legacy source-compatible shim; converts via `from_legacy_options` then delegates to the canonical path (`start_from_config` when empty, otherwise `start_from_config_with_compatibility`); emits one warning for non-default legacy logging fields |
| `ServiceSupervisor::run(&mut self)` | Blocking; owns signal loop and shutdown sequence |
| `ServiceSupervisor::reload_config(&mut self)` | Load-and-swap without blocking signal loop |
| `ServiceSupervisor::shutdown_token()` | Exposes master cancel for external callers |
| `RuntimeState::generation()` | Reads `snapshot.load().generation` |
| `compile_runtime_snapshot(rt, prev)` | `Result<CompiledRuntimeSnapshot, Box<dyn Error>>` |

## Compatibility ownership (Phase 3)

```
pproxy syntax/policy
      -> compatibility lowering / CLI startup decisions
      -> compiled native runtime config + narrow runtime-only hooks
      -> generic runtime/data plane
```

| Old `CompatibilityOptions` field | Disposition | Rationale |
|---|---|---|
| `compatibility_mode: bool` | Removed (generic); replaced by `allow_insecure_ssh_host_keys: bool` in hooks | Generic mode branched unrelated behavior; SSH policy is now a narrow typed bool resolved by the facade from `EGRESS_SSH_INSECURE_HOST_KEYS` via `ssh_insecure_acknowledged()` |
| `auth_timeout: Option<Duration>` | Removed; replaced by `auth_reuse: Option<Arc<AuthReuseCache>>` pre-built by the facade | Supervisor no longer interprets pproxy arg semantics; it threads the typed inbound-auth handle. Cache stays monotonic/bounded (4096)/process-local per `eggress-server::accept` |
| `system_proxy: bool` | Narrowed to `system_proxy: Option<SystemProxyRequest>` | Explicit post-bind opt-in; `None` (native) never mutates OS state; apply/rollback stays idempotent and lifecycle-safe |
| `debug: bool` | Removed | Pre-start adapter policy; CLI resolves `-d` via `default_log_level()` before construction with `RUST_LOG` precedence; extra failure log deleted, generic `connection completed` remains |
| `verbose_level: u8` | Removed | Presentation policy separated from log-level selection; extra `pproxy connection event` / `traffic stats` logs deleted (no event bus introduced) |

Native startup passes `None`; compatibility passes `Some(hooks)`.

### Closure legacy shim (source compatibility, not state)

```
CompatibilityOptions / start_from_config_with_options
    = legacy public source-compatible facade
    -> converts immediately to CompatibilityRuntimeHooks

CompatibilityRuntimeHooks / start_from_config_with_compatibility
    = canonical typed runtime path

ServiceSupervisor internal state
    = Option<CompatibilityRuntimeHooks>, never CompatibilityOptions
```

`CompatibilityOptions` restores the pre-Phase-3 field names/types
(`compatibility_mode`, `auth_timeout`, `system_proxy`, `debug`,
`verbose_level`) as a DTO only. `from_legacy_options()` maps `Some(timeout)`
to a bounded `AuthReuseCache` (`None` stays `None`; the CLI 30-day default
stays facade-owned via `effective_auth_timeout()`), `system_proxy` to the
narrow post-bind hook, and `compatibility_mode` to `allow_insecure_ssh_host_keys`
only when `EGRESS_SSH_INSECURE_HOST_KEYS` is explicitly acknowledged
(non-compat stays secure even with the variable set). `debug`/`verbose_level`
never re-enter supervisor state; the shim emits at most one warning that
logging is facade-owned via `PproxyArgs::default_log_level()` with `RUST_LOG`
precedence.

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
9. System proxy applied only when `Some(SystemProxyRequest)` (`--sys` +
   `operations` feature) after bind but before accept loops; failure is a
   startup error. Transport accept-loop orchestration is otherwise unchanged
   (no generic listener framework introduced).
10. `readiness.store(true)` — `/-/ready` returns 200.
11. Signal loop enters: `tokio::select!` over cancel, `ctrl_c`, SIGTERM,
    SIGHUP (reload only when `config_path` is `Some`).

## Shutdown ordering

Ordering implemented by `shutdown_ordered()` (`src/supervisor/shutdown.rs`):

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

## Reload path (one canonical transaction)

`RuntimeState::apply_compiled_config(&new_config)` owns all state mutation.
`reload_config()` (file), SIGHUP handling, and embed
`reload_toml_str` / `reload_compiled` / `reload_toml_file` differ only in how
`new_config` is obtained (file `load_and_compile` vs string
`parse_validate_compile` vs already-compiled).

1. Classify via `classify_reload_config` on the live snapshot (not stored
   `rt_config`): rejects count, name, bind, `reuse_port`, protocols, auth,
   TLS, Shadowsocks/Trojan, `connection_limit`/`fixed_target`/`local_bind`,
   all UDP settings, transparent/unix. Routing, upstream/group, health,
   PAC/static pass through. Records `record_reload(false)` on reject.
2. `compile_runtime_snapshot(&new_config, Some(prev))` — Arc reuse when
   `old.chain == new.chain && old.health_config == new.health`. Records
   `record_reload(false)` on snapshot-build failure; old snapshot stays live.
3. **Snapshot before router swap**: `snapshot.store()` then
   `routing.swap_arc()`, then `publish_admin_snapshot` (operations),
   `restart_health_probes()`, `H2_POOL_REGISTRY.clear()`,
   `set_config_generation(gen)` + `record_reload(true)`.
4. Supervisor `reload_config()` updates stored `rt_config` only on `Applied`
   (snapshot remains authoritative for next classification).

SIGHUP uses the same transaction (no duplicated classify/snapshot/publish/
health/metrics code). Embed string/file reloads record parse failures as
`record_reload(false)` before the transaction so file/embed metrics agree.

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

**Hot-reloadable:** rules, groups, upstreams, health config, PAC/static.

**Restart required:** listener bindings, `reuse_port`, protocols, auth, TLS,
Shadowsocks/Trojan config, `connection_limit`, `fixed_target`, `local_bind`,
all UDP listener settings, transparent/unix config, log level/format,
shutdown grace, timeouts, admin bind. Reverse endpoint topology
(servers/clients) is likewise startup-captured — reload swaps the snapshot
and routing, but reverse accept/reconnect tasks are spawned once at startup
(see Phase 2 control-plane convergence); reverse target authorization still
follows current routing.

## Concurrency

- Five `CancellationToken`s separate concerns; master propagates to reverse
  tasks.
- `TaskTracker`s: listener tasks, connection tasks, admin tasks,
  `state.udp_tasks`.
- `ArcSwap<CompiledRuntimeSnapshot>`: lock-free snapshot reads.
- `SharedRoutingService::swap_arc`: atomic routing table replacement.
- `AtomicBool` readiness, `AtomicU64` counters for connections and
  transparent metrics.
- `ListenerConnectionSlot` (`supervisor/accounting.rs`) uses `fetch_update` (CAS) for lock-free
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
- Snapshot published **before** router swap (`supervisor/state.rs`, `apply_compiled_config`).
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
