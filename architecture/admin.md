# eggress-admin — Local Operational HTTP Server

Hyper-based HTTP API for operators: health/readiness, introspection JSON,
Prometheus scraping, PAC serving, static content, route explanation (dry-run
routing decisions), UDP association status, and reverse server state.

## Module map

| File | Role |
|------|------|
| `src/lib.rs` | `AdminError` enum (Bind, Accept, Server); re-exports public types |
| `src/server.rs` | `AdminServer` accept loop, `AdminState`, `AdminSnapshotProvider` trait, `AdminSnapshot`, `StaticAdminSnapshot`, `ListenerInfo`, bearer/basic auth via constant-time compare, `MAX_ADMIN_CONNECTIONS = 64`, 30 s connection timeout |
| `src/routes.rs` | Router + all endpoint handlers; `MAX_ADMIN_BODY = 16 KiB`, `MAX_IDENTITY_LEN = 256`, streaming body collection via `collect_limited()` |
| `src/pac.rs` | PAC generation with `js_escape()` (quotes, backslashes, C0 controls, U+2028/U+2029 line separators) |
| `src/static_content.rs` | `serve_static()` — returns a `StaticRoute` body with its configured content type |
| `src/reverse.rs` | `ReverseRegistry` — `HashMap<ReverseServerId, ReverseServerEntry>` behind `RwLock`, snapshots `Arc<ReverseServerState>` per server |

## Public API surface

```rust
pub struct AdminServer { listener: TcpListener, cancel: CancellationToken }

pub struct AdminState {
    pub metrics: Arc<MetricsRegistry>,
    pub start_time: Instant,
    pub readiness: Arc<AtomicBool>,
    pub active_connections: Option<Arc<AtomicU64>>,
    pub provider: Arc<dyn AdminSnapshotProvider>,
    pub udp_registry: Arc<UdpAssociationRegistry>,
    pub reverse_registry: Arc<ReverseRegistry>,
    pub metrics_enabled: bool,
    pub auth: Option<AdminAuthConfig>,
}

pub trait AdminSnapshotProvider: Send + Sync + 'static {
    fn snapshot(&self) -> AdminSnapshot;
}

pub struct AdminSnapshot {
    pub generation: u64,
    pub router: Arc<Router>,
    pub pac: Option<PacConfig>,
    pub static_routes: Vec<StaticRoute>,
    pub listeners: Vec<ListenerInfo>,
}

pub struct StaticAdminSnapshot { pub snapshot: AdminSnapshot }
// Test helper; returns a fixed snapshot, does not reflect live reloads.
```

## Endpoints

| Path | Method | Purpose |
|------|--------|---------|
| `/-/health` | GET | Liveness — always `200 "ok"` when serving |
| `/-/ready` | GET | Readiness gate — `200 "ready"` or `503 "not ready"` |
| `/-/status` | GET | JSON: version, generation, uptime, active connections, listeners (with mode, capability_status, original_dst_support, unix_socket fields) |
| `/-/routes` | GET | JSON: compiled rules with IDs and actions, default action, rule count |
| `/-/upstreams` | GET | JSON: upstream groups with members, protocols, health, eligibility, scheduler, active/in_flight counts, tcp_connect/udp_associate capability |
| `/-/config` | GET | JSON: config summary counts + listener names |
| `/-/udp` | GET | JSON: association/target-flow/upstream-flow gauges, per-listener active associations |
| `/-/reverse` | GET | JSON: registered reverse servers with active_control, active_streams, pending_external, denied_bind, dropped counters |
| `/-/route-explain` | POST | JSON body `{target, listener, protocol, source?, identity?}` → `RouteExplanation` dry run through live router |
| `/metrics` | GET | Prometheus text format (`text/plain; version=0.0.4`); returns 404 if metrics disabled |
| configured PAC path | GET | `application/x-ns-proxy-autoconfig` PAC JavaScript |
| configured static paths | GET | Static content with configured content type |

## How it works — request handling pipeline

1. **Accept** — `AdminServer::run()` loops on `self.listener.accept()`. Each connection acquires a semaphore permit from a pool of 64 (`server.rs:96-105`). If no permit is available, the connection is dropped immediately.
2. **Auth check** — before dispatching, `authorized()` (`server.rs:30-59`) checks `AdminState.auth`:
   - **Bearer**: `Authorization: Bearer <token>` compared via `subtle::ConstantTimeEq`
   - **Basic**: `Authorization: Basic <base64>` decoded, split on `:`, CT-compared
   - On failure: `401` with `WWW-Authenticate: Bearer, Basic` header
3. **Timeout** — each HTTP/1.1 connection is wrapped in `tokio::time::timeout(Duration::from_secs(30), conn)` (`server.rs:135-148`).
4. **Snapshot** — handlers call `state.snapshot()` which delegates to `AdminSnapshotProvider::snapshot()`. A fresh snapshot is fetched per request, so reloads are immediately visible without restarting admin.
5. **Dispatch** — `handle_request()` (`routes.rs:15`) pattern-matches on path. Body-consuming endpoints (`/-/route-explain`) use `collect_limited()` to enforce the 16 KiB limit.
6. **Non-loopback warning** — `AdminServer::new()` logs a warning when bound to a non-loopback address (`server.rs:79-86`).

## Limits

| Constant | Value | Location |
|----------|-------|----------|
| `MAX_ADMIN_CONNECTIONS` | 64 | `server.rs:23` |
| Connection timeout | 30 s | `server.rs:136` |
| `MAX_ADMIN_BODY` | 16 KiB | `routes.rs:12` |
| `MAX_IDENTITY_LEN` | 256 bytes | `routes.rs:13` |

## PAC generation

`generate_pac()` (`pac.rs:26`) produces a `FindProxyForURL` JavaScript function. `js_escape()` (`pac.rs:3`) handles:
- Backslashes → `\\`
- Double quotes → `\"`
- Newlines/CR/tab → `\n`, `\r`, `\t`
- C0 controls (below U+0020) → `\u00xx`
- U+2028 / U+2029 line terminators → `\u2028` / `\u2029` (JavaScript string literals break on these even though they are not ASCII controls)

Hosts and suffixes are sorted alphabetically before emission. Direct fallback appends `; DIRECT` to the return value.

## ReverseRegistry

Thread-safe registry (`RwLock<HashMap<ReverseServerId, ReverseServerEntry>>`). Runtime inserts entries at startup; admin reads snapshots atomically. Each entry holds `Arc<ReverseServerState>` with atomic counters for active_control, active_streams, pending_external, denied_bind, dropped_stream_limit, dropped_pending_limit. Uses `std::sync::RwLock` (not Tokio) because `snapshot()` is fast and non-async.

## Invariants

- **Snapshot freshness**: handlers fetch a fresh `AdminSnapshot` from the provider per request, so config reloads are immediately visible.
- **Non-loopback warning**: binding to a non-loopback address emits a tracing warning; auth is recommended (401 with `WWW-Authenticate: Bearer, Basic` otherwise).
- **Readiness flips before drain**: readiness must become `false` before connection drain begins (tested invariant: `lib.rs:404-418`).
- **Auth constant-time**: both Bearer token and Basic username/password comparisons use `subtle::ConstantTimeEq` to prevent timing side-channels (`server.rs:36,54`).
- **Auth per-request**: auth is checked inside the service_fn closure per request, not per-connection, so keep-alive connections are still gated.
- **Body limit streaming**: `collect_limited()` rejects bodies exceeding 16 KiB chunk-by-chunk, avoiding unbounded memory allocation (`routes.rs:394-424`).
- **Identity validation**: empty identity rejected; identity over 256 bytes rejected; non-`None` identity wrapped as `ClientIdentity::Username` (`routes.rs:318-334`).

## Security notes

- Admin is an operator surface; exposing it to the network without auth leaks topology, metrics, and routing state. Prefer loopback bind or configure auth.
- Auth is checked per-request (not per-connection), so keep-alive connections are still gated.
- Bearer and Basic auth are both supported; configure one or the other via `AdminAuthConfig`.
- The 16 KiB body limit prevents memory-exhaustion from oversized POST bodies.
- The 64-connection semaphore prevents trivial DoS via connection flooding.
- The 30 s connection timeout prevents slow-loris-style resource exhaustion.

## Test coverage map

| Area | Tests |
|------|-------|
| Health | Returns 200 "ok" |
| Readiness | Returns 200 "ready" when true, 503 "not ready" when false |
| Readiness drain | Becomes false before drain (atomic store then re-query) |
| Status | Valid JSON with version, generation, uptime |
| Metrics | Prometheus format with expected metric names (`eggress_connections_active`, `eggress_connections_total`) |
| Auth rejection | Configured auth returns 401 with "unauthorized" body |
| Routes | JSON with rules, default_action, rule_count |
| Upstreams | JSON array of groups |
| Reverse | Empty when no servers; reports registered server state (active_control, active_streams, denied_bind, dropped counters) |
| PAC | Generated when configured, 404 when not; escaping of quotes, backslashes, line separators, C0 controls; host sorting; no-fallback mode |
| Static content | Correct content type and body |
| Unknown path | 404 "not found" |

Run: `cargo test -p eggress-admin`

## Reviewer gotchas

1. The `/-/route-explain` endpoint is POST-only; GET returns 405.
2. `metrics_enabled: false` causes `/metrics` to return 404, not an error.
3. `ListenerInfo` fields are `#[serde(skip_serializing_if = "Option::is_none")]`, so optional fields are absent from JSON when `None`.
4. PAC generation sorts `direct_hosts` and `direct_suffixes` independently; the PAC output order is deterministic.
5. `StaticAdminSnapshot` is a test helper that returns a fixed snapshot — it does not reflect live reloads.
6. `ReverseRegistry` uses `std::sync::RwLock` (not Tokio), which is fine because `snapshot()` is fast and non-async.
7. Auth checking happens inside the service_fn closure, not at the accept layer, so an unauthenticated request still holds a connection slot for the duration of the 30 s timeout.
8. `AdminState::generation()` (`server.rs:212-214`) calls `provider.snapshot().generation` — a convenience method that allocates a full snapshot just to read one field.

## See also

- [runtime.md](runtime.md) — supervisor lifecycle, readiness, admin startup
- [config.md](config.md) — TOML schema including admin bind/auth
- [metrics.md](metrics.md) — MetricsRegistry and Prometheus rendering
- [routing.md](routing.md) — compiled router exposed at `/-/routes` and `/-/route-explain`
