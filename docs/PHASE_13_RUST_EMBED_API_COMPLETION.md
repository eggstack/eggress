# Phase 13: Rust Embed API Stabilization — Completion Record

Phase 13 creates the Rust library surface that Python bindings (Phase 14) will wrap.
The goal is a deliberate embedded-service API, not exposing CLI internals or forcing
callers to manage sockets, async networking, routing, or runtime details.

## API summary

### Types

| Type | Description |
|------|-------------|
| `EggressConfig` | Parsed and validated TOML configuration |
| `EggressService` | Pre-start builder (async and blocking paths) |
| `EggressHandle` | Post-start handle for status, metrics, reload, shutdown |
| `BoundAddresses` | Discovered listener and admin addresses |
| `ListenerAddress` | Single listener name + socket address |
| `ServiceStatus` | Generation, readiness, uptime, active connections |
| `ReloadOutcome` | Result of a config reload attempt |
| `EggressError` | Stable error type with category labels |

### Methods

| Type | Methods |
|------|---------|
| `EggressConfig` | `from_toml_str`, `from_toml_file`, `source_toml` |
| `EggressService` | `new`, `from_toml_str`, `from_toml_file`, `start`, `start_blocking` |
| `EggressHandle` | `bound_addresses`, `status`, `metrics_text`, `reload_toml_str`, `reload_toml_file`, `shutdown`, `shutdown_blocking` |
| `BoundAddresses` | `listener` (lookup by name) |

## Runtime ownership model

- **Async path** (`start().await`): spawns a dedicated thread that runs
  `ServiceSupervisor::run()` with its own Tokio runtime. Readiness is
  signaled via a oneshot channel. The caller must be inside a Tokio context.
- **Blocking path** (`start_blocking()`): spawns a background thread that
  creates a Tokio runtime and runs the supervisor. Blocks until readiness.
  Suitable for Python/GIL-constrained threads.

## Bound address discovery

When config uses `127.0.0.1:0`, the OS assigns an ephemeral port. The handle
exposes actual bound addresses via `handle.bound_addresses()` with lookup
by listener name.

## Metrics and status

Metrics are available as Prometheus text without HTTP scraping:
`handle.metrics_text()` returns the full Prometheus exposition format.

Status provides generation, readiness, active connections, uptime, and
listener count without requiring the admin HTTP server.

## Reload

Hot-reload of routing, upstreams, groups, and health config via
`handle.reload_toml_str()` or `handle.reload_toml_file()`. Listener
bind changes are rejected (restart required).

## Error model

All errors implement `std::error::Error` and `Display`. Credentials are
never included in error messages. Variants are stable for PyO3 mapping.

## Tests

| File | Tests | Description |
|------|-------|-------------|
| `start_stop.rs` | 6 | Blocking/async start, multiple listeners, config errors |
| `proxy_traffic.rs` | 2 | SOCKS5 TCP echo, port-0 discovery |
| `reload.rs` | 5 | Generation increment, invalid config, bind rejection |
| `metrics_status.rs` | 3 | Prometheus counters, status fields, metrics after session |
| `error_redaction.rs` | 3 | No credentials in errors, error categories |

Total: 21 tests (including doc-tests), all passing.

## Documentation

- `docs/EMBED_API.md` — full API reference with examples
- `docs/ROADMAP.md` — Phase 13 added
- `README.md` — status line and capability checklist updated
- `AGENTS.md` — project structure, test commands, architecture facts updated
- `.skills/rust-proxy-dev/` — embed API section added
- `.skills/testing/` — embed API test section added

## Local verification

```bash
cargo fmt --all -- --check        # PASS
cargo check --workspace --all-targets  # PASS
cargo clippy --workspace --all-targets -- -D warnings  # PASS
cargo test -p eggress-embed       # 21/21 PASS
cargo test -p eggress-runtime --test startup --test reload --test shutdown  # PASS
```

## Limitations

- Requires a temp config file on disk (supervisor reads from path).
- `ServiceSupervisor::run()` creates its own Tokio runtime internally.
- Listener bind changes require a full restart (not reloadable).
- No logging initialization unless explicitly configured in TOML.

## Readiness for Phase 14

This API is designed for thin PyO3 wrappers:

- All public types are `Send + Sync`.
- No panics on normal user errors.
- Error variants are stable for Python exception mapping.
- Blocking path is suitable for Python's GIL-constrained threads.
- Config parsing happens at construction time (fail-fast).
