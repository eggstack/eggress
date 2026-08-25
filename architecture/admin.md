# eggress-admin — Local Operational HTTP Server

Hyper-based HTTP API for operators: health/readiness, introspection JSON,
Prometheus scraping, PAC serving, static content, and route explanation
(dry-run routing decisions).

## Endpoints

| Path | Purpose |
|---|---|
| `GET /-/health` | Liveness (always ok when serving) |
| `GET /-/ready` | Readiness gate (503 during drain/shutdown) |
| `GET /-/status` | Version, generation, uptime, active connections, listeners |
| `GET /-/routes` | Compiled rule list + default action |
| `GET /-/upstreams` | Groups, members, health/eligibility/load, scheduler |
| `GET /-/config` | Config summary counts + listeners |
| `GET /-/udp` | Association/target-flow/upstream-flow gauges |
| `GET /-/reverse` | Reverse server registry state |
| `POST /-/route-explain` | Body `{target, listener, protocol, source?, identity?}` → `RouteExplanation` (dry run through the live router) |
| `GET /metrics` | Prometheus text format (if enabled) |
| configured PAC path | `application/x-ns-proxy-autoconfig` |
| configured static paths | Static content routes |

## Structure

| File | Role |
|---|---|
| `src/server.rs` | `AdminServer` accept loop (max 64 concurrent, 30s conn timeout), `AdminState`, `AdminSnapshotProvider` trait + `AdminSnapshot`, bearer/basic auth via constant-time compare |
| `src/routes.rs` | Router + handlers; body limit 16 KiB (413), identity ≤ 256 bytes |
| `src/pac.rs` | PAC generation with JS escaping (quotes, backslashes, line separators, C0) |
| `src/static_content.rs` | Static route serving |
| `src/reverse.rs` | `ReverseRegistry` shared with the runtime |

## Invariants

- Handlers fetch a fresh `AdminSnapshot` from the provider per request, so a
  reload is immediately visible without restarting admin.
- Non-loopback bind emits a warning; auth recommended (401 with
  `WWW-Authenticate: Bearer, Basic` otherwise).
- Readiness must flip false before connection drain (tested invariant).

## Review entry points

- Verify: `cargo test -p eggress-admin`
