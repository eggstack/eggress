# Embed API Reference

Stable Rust API for embedding eggress as a library in another Rust process.

This crate wraps the internal runtime, config, and server infrastructure behind
a minimal, binding-friendly surface. Python bindings (PyO3) in later phases will
wrap this API.

## Overview

The `eggress-embed` crate provides:

- **`EggressConfig`** — parse and validate TOML configuration
- **`EggressService`** — pre-start builder
- **`EggressHandle`** — post-start handle for status, metrics, reload, and shutdown
- **`BoundAddresses`** — discovered listener and admin addresses
- **`ListenerStatus`** — detailed per-listener status (name, bind, protocols, UDP)
- **`ServiceStatus`** — generation, readiness, uptime, connections, UDP associations, upstreams
- **`ReloadOutcome`** — result of a config reload attempt
- **`EggressError`** — stable error type for PyO3 mapping

## Blocking usage

```rust
use eggress_embed::{EggressService, EggressConfig};

let config = EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "socks"
    bind = "127.0.0.1:0"
    protocols = ["socks5"]
"#).unwrap();

let handle = EggressService::new(config).start_blocking().unwrap();

// Discover bound addresses (port-0)
let addrs = handle.bound_addresses();
let socks_addr = addrs.listener("socks").unwrap();
println!("SOCKS5 listening on {socks_addr}");

// Check status
let status = handle.status();
println!("generation: {}, readiness: {}", status.generation, status.readiness);

// Get Prometheus metrics
let metrics = handle.metrics_text().unwrap();
assert!(metrics.contains("eggress_connections_total"));

// Shutdown
handle.shutdown_blocking().unwrap();
```

## Async usage

```rust
# tokio_test::block_on(async {
use eggress_embed::{EggressService, EggressConfig};

let config = EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "http"
    bind = "127.0.0.1:0"
    protocols = ["http"]
"#).unwrap();

let handle = EggressService::new(config).start().await.unwrap();

let status = handle.status();
println!("generation: {}", status.generation);

handle.shutdown().await.unwrap();
# });
```

## Port-0 binding

When config uses `127.0.0.1:0`, the OS assigns an ephemeral port. The handle
exposes the actual bound address:

```rust
# let config = eggress_embed::EggressConfig::from_toml_str(r#"
# version = 1
# [[listeners]]
# name = "test"
# bind = "127.0.0.1:0"
# protocols = ["socks5"]
# "#).unwrap();
# let handle = eggress_embed::EggressService::new(config).start_blocking().unwrap();
let addrs = handle.bound_addresses();
let addr = addrs.listener("test").unwrap();
assert!(addr.port() > 0);
# handle.shutdown_blocking().unwrap();
```

## Redacted config output

`EggressConfig::to_redacted_toml()` returns the TOML source with credentials
replaced by placeholders. Suitable for logging or display:

```rust
let config = eggress_embed::EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "socks"
    bind = "127.0.0.1:0"
    protocols = ["socks5"]

    [listeners.auth]
    type = "password"
    username = "admin"
    password = "super_secret_123"
"#).unwrap();

let redacted = config.to_redacted_toml().unwrap();
assert!(!redacted.contains("super_secret_123"));
assert!(redacted.contains("****"));
// Username is not a secret, remains visible
assert!(redacted.contains("admin"));
```

Upstream URI credentials are also redacted:
`socks5://user:pass@host:port` → `socks5://****:****@host:port`.

## Reload

Reload configuration without restarting the process. Only routing, upstreams,
groups, and health config are hot-swapped. Listener bind changes are rejected
(restart required).

```rust
# let config = eggress_embed::EggressConfig::from_toml_str(r#"
# version = 1
# [[listeners]]
# name = "http"
# bind = "127.0.0.1:0"
# protocols = ["http"]
# "#).unwrap();
# let handle = eggress_embed::EggressService::new(config).start_blocking().unwrap();
let new_config = r#"
version = 1

[[listeners]]
name = "http"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;

match handle.reload_toml_str(new_config) {
    Ok(eggress_embed::ReloadOutcome::Applied { generation, upstreams }) => {
        println!("reloaded: generation={generation}, upstreams={upstreams}");
    }
    Err(e) => {
        eprintln!("reload failed: {e}");
    }
}
# handle.shutdown_blocking().unwrap();
```

## Metrics and status

Metrics are available as Prometheus text without HTTP scraping:

```rust
# let config = eggress_embed::EggressConfig::from_toml_str(r#"
# version = 1
# [[listeners]]
# name = "test"
# bind = "127.0.0.1:0"
# protocols = ["socks5"]
# "#).unwrap();
# let handle = eggress_embed::EggressService::new(config).start_blocking().unwrap();
let metrics = handle.metrics_text().unwrap();
// Contains: eggress_connections_active, eggress_connections_total, etc.
assert!(metrics.contains("eggress_connections_total"));

let status = handle.status();
assert!(status.readiness);
assert_eq!(status.generation, 0);
assert_eq!(status.udp_associations_active, 0);
assert_eq!(status.upstream_count, 0);
assert_eq!(status.listeners.len(), 1);
# handle.shutdown_blocking().unwrap();
```

## Lifecycle and shutdown

### Thread ownership model

The handle owns exactly one of two mutually exclusive thread models:

**Async path** (`start()`):
- A Tokio blocking-pool thread runs the startup sequence and then blocks on
  `run_result.join()` for the lifetime of the service.
- A dedicated OS thread (`"eggress-embed-rt"`) owns `ServiceSupervisor::run()`.
- `_runtime_task` wraps the blocking task's JoinHandle as a Tokio task.

**Blocking path** (`start_blocking()`):
- An outer OS thread (`"eggress-embed-rt"`) handles startup, sends results
  through a channel, and terminates.
- An inner OS thread (`"eggress-embed-run"`) owns `ServiceSupervisor::run()`.
- `_run_handle` holds the inner thread's JoinHandle directly.

No extra orchestration thread remains after startup in either path.

### Shutdown behavior

- **`shutdown()`** (async) and **`shutdown_blocking()`** perform orderly
  shutdown: cancel token → join supervisor → clean temp config. These are
  idempotent (second call is a no-op).
- **Dropping `EggressHandle`** cancels the shutdown token and performs a
  best-effort join with a 5-second timeout on the async path. Explicit
  `shutdown()` or `shutdown_blocking()` is preferred for guaranteed teardown.
- The service is deterministic: no background threads leak after shutdown.

## Error model

All errors implement `std::error::Error` and `Display`. Credentials are never
included in error messages.

| Variant | Meaning |
|---------|---------|
| `Config` | TOML parsing or validation error |
| `Runtime` | Tokio runtime initialization error |
| `Startup` | Listener bind or readiness timeout |
| `Reload` | Config reload parse, validation, or topology rejection |
| `Shutdown` | Runtime shutdown error |
| `UnsupportedFeature` | Feature not supported by the embed API |
| `Internal` | Unexpected internal error |

Use `error.category()` to get a short label for programmatic matching.

## Limitations

- The embed API requires a temp config file on disk (supervisor reads from path).
- `ServiceSupervisor::run()` creates its own Tokio runtime internally.
- Listener bind changes require a full restart (not reloadable).
- No logging initialization unless explicitly configured in TOML.

## Python-binding readiness

This API is designed for thin PyO3 wrappers:

- All public types are `Send + Sync`.
- No panics on normal user errors (all fallible operations return `Result`).
- Error variants are stable for mapping to Python exception types.
- Blocking path (`start_blocking`, `shutdown_blocking`) is suitable for
  Python's GIL-constrained threads.

## API summary

| Type | Methods |
|------|---------|
| `EggressConfig` | `from_toml_str`, `from_toml_file`, `source_toml`, `to_redacted_toml` |
| `EggressService` | `new`, `from_toml_str`, `from_toml_file`, `start`, `start_blocking` |
| `EggressHandle` | `bound_addresses`, `status`, `metrics_text`, `reload_toml_str`, `reload_toml_file`, `shutdown`, `shutdown_blocking` |
| `BoundAddresses` | `listener` (lookup by name) |
| `ServiceStatus` | `generation`, `readiness`, `active_connections`, `uptime_secs`, `listener_count`, `listeners`, `udp_associations_active`, `upstream_count` |
| `ListenerStatus` | `name`, `bind`, `local_addr`, `protocols`, `udp_enabled` |
| `ReloadOutcome` | `Applied { generation, upstreams }` |
| `EggressError` | `Config`, `Runtime`, `Startup`, `Reload`, `Shutdown`, `UnsupportedFeature`, `Internal` |
