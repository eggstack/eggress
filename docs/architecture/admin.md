# eggress-admin

`crates/eggress-admin/`

Local admin HTTP server providing operational endpoints for health, metrics, routing inspection, PAC serving, and static content.

## Endpoints

| Endpoint | Method | Description |
|---|---|---|
| `/-/ready` | GET | Readiness check (200 if ready, 503 if not) |
| `/-/health` | GET | Liveness check (always 200) |
| `/-/status` | GET | Runtime status: generation, uptime, active connections |
| `/-/routes` | GET | Current routing rules and upstream groups |
| `/-/upstreams` | GET | Upstream health and connection counts |
| `/-/config` | GET | Current configuration (redacted secrets) |
| `/-/udp` | GET | UDP association status |
| `/-/reverse` | GET | Reverse proxy control connection status |
| `/metrics` | GET | Prometheus metrics exposition |
| `/proxy.pac` | GET | PAC (Proxy Auto-Configuration) file |
| `/*` | GET | Static content serving |

## Key Types

| Type | Description |
|---|---|
| `AdminServer` | HTTP server binding and request handler |
| `AdminState` | Server state: readiness, generation, metrics |
| `AdminSnapshot` | Live data view: generation, router, PAC, static routes, listeners |
| `AdminSnapshotProvider` | Trait for providing live snapshots per request |
| `StaticAdminSnapshot` | Fixed snapshot for tests |

## Authentication

Configure `[admin.auth]` with either `bearer_token` (or
`bearer_token_env`) or `basic_auth.user` plus `password`/`password_env`.
Configured credentials are checked on every endpoint, including health and
readiness. Non-loopback admin binds require authentication; loopback binds
may remain unauthenticated for local probes.

## Snapshot Provider

The runtime implements `AdminSnapshotProvider`, so admin handlers see live data from the current `CompiledRuntimeSnapshot` on every request. Reloads take effect without restarting the admin server.

## Route Explanation

`/-/routes` supports optional query parameters for route explanation:
- `target` — destination to explain routing for
- `source` — source socket address
- `identity` — client username

Returns detailed explanation of which rule matched and why.

## PAC Generation

Serves a PAC file based on configured listeners and upstreams. PAC is regenerated from the current snapshot on each request.

## Body Size Limits

Route explanation and config endpoints enforce body size limits to prevent abuse.

## Dependencies

- `eggress-routing` — route explanation
- `eggress-metrics` — Prometheus rendering
- `eggress-udp` — UDP status
- `eggress-protocol-reverse` — reverse proxy registry

See [overview.md](overview.md) for context.
