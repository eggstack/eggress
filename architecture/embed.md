# eggress-embed — Stable In-Process Rust API

The embedding contract: start/control/reload/stop a full service inside your
own process, plus a direct outbound connector that skips listeners entirely.
Designed as the binding target for PyO3.

## API surface

| Item | File | Notes |
|---|---|---|
| `EggressConfig::from_toml_str/from_toml_file/to_redacted_toml` | `src/lib.rs` | Validation happens here; redaction for diagnostics |
| `EggressService::start()` async / `start_blocking()` (+ `_with_compatibility_options`) | `src/lib.rs` | Both return `EggressHandle` |
| `EggressHandle::{bound_addresses, status, metrics_text, reload_toml_str, shutdown, shutdown_blocking}` | `src/lib.rs` | Port-0 discovery supported; shutdown idempotent |
| `OutboundConnector::from_toml/from_pproxy_uri` → `connect_tcp[_timeout]` | `src/outbound.rs` | Chain connect WITHOUT any local listener; also exposes UDP association handle |

## Thread model (review carefully when touching lifecycle)

- Async path: startup on the caller's Tokio blocking pool; supervisor runs on
  a dedicated OS thread `"eggress-embed-rt"`.
- Blocking path: outer `"eggress-embed-rt"` thread does startup, inner
  `"eggress-embed-run"` thread owns `ServiceSupervisor::run()`.
- Drop cancels tokens best-effort with bounded join.

## Error handling

`EggressError` variants are stable and redacted (`error_redaction.rs` tests
prove credentials never escape in error strings) — this surface is mirrored
1:1 by the Python exception hierarchy.

## Review entry points

- Tests: `start_stop.rs`, `reload.rs`, `metrics_status.rs`,
  `proxy_traffic.rs`, `error_redaction.rs`.
- Verify: `cargo test -p eggress-embed --test reload`
