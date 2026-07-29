# eggress-embed

`crates/eggress-embed/`

Stable Rust in-process embedding API for running an eggress proxy without the CLI.

## Key Types

| Type | Description |
|---|---|
| `EggressConfig` | Parsed and validated configuration |
| `EggressService` | Pre-start builder (async or blocking) |
| `EggressHandle` | Running proxy handle |
| `EggressError` | Error type for all embed operations |

## Usage

### Async (within Tokio runtime)

```rust
let config = EggressConfig::from_toml_str(toml)?;
let handle = EggressService::new(config).start().await?;

// Inspect
let addrs = handle.bound_addresses();
let status = handle.status();

// Reload
handle.reload_toml_str(new_toml).await?;

// Shutdown
handle.shutdown().await;
```

### Blocking

```rust
let config = EggressConfig::from_toml_str(toml)?;
let handle = EggressService::new(config).start_blocking()?;

// Same operations available
handle.shutdown_blocking();
```

## Handle Operations

| Method | Description |
|---|---|
| `bound_addresses()` | Discover listener ports (supports port-0) |
| `status()` | Generation, readiness, uptime, active connections |
| `metrics_text()` | Prometheus metrics without HTTP |
| `reload_toml_str()` | Hot-reload routing/upstreams |
| `reload_toml_file()` | Hot-reload from file |
| `shutdown()` / `shutdown_blocking()` | Graceful shutdown (idempotent) |

## Thread Model

- **Async path**: Tokio blocking-pool thread + dedicated OS thread
- **Blocking path**: Outer startup thread + inner run thread

Both paths create a dedicated Tokio runtime on a separate OS thread.

## Native OutboundConnector

`eggress-embed::outbound` provides `OutboundConnector` for native Rust outbound connections:
- `OutboundConnector::from_toml(toml)` — create from TOML
- `OutboundConnector::from_pproxy_uri(uri)` — create from pproxy URI
- `connector.connect_tcp(target)` — connect to TCP target

## Dependencies

- `eggress-config` — configuration parsing
- `eggress-runtime` — runtime lifecycle

See [overview.md](overview.md) for context.
