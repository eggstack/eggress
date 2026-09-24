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

## Native outbound connector (no listener)

`OutboundConnector` (implementation authority in `eggress-outbound`,
re-exported as `eggress_embed::outbound::*`) executes a compiled native
chain, a native TOML upstream, or a pproxy remote expression in-process via
`ChainExecutor`, without starting a local listener:

```rust
let connector = OutboundConnector::from_pproxy_uri(
    "socks5://127.0.0.1:1080__http://127.0.0.1:8080"
)?;

let (stream, info) = connector.connect_tcp("api.example.com", 443).await?;
assert_eq!(info.hop_count, 2);
```

Contract:

- `from_chain()` takes a compiled native `ProxyChainSpec` directly (no TOML,
  no pproxy, no server/runtime types) and rejects empty chains; `direct()`
  is the explicit no-hop alternative.
- `from_pproxy_uri()` accepts one pproxy remote expression, including
  canonical `__` multi-hop chains, preserving hop order.
- `from_toml()` supports native SSH upstreams when the `ssh` feature is
  enabled. Native SSH uses verified known-hosts behavior.
- `from_pproxy_uri()` supports pproxy-style SSH only when both `ssh` and
  `pproxy-compat` are enabled. It retains the explicit pproxy compatibility
  host-key policy; enabling `pproxy-compat` never weakens native TOML SSH.
- The connector owns the reusable SSH session cache for its lifetime, so
  callers do not construct `ChainExecutor` or SSH transport state themselves.
- The SSH facade contract is covered by a required-mode local OpenSSH
  regression: byte traversal, redacted fail-closed authentication failure,
  and native untrusted-host-key rejection. CI provisions `openssh-server`;
  local runs can use `EGRESS_REQUIRE_OPENSSH_TESTS=1` to make fixture setup
  and readiness failures fatal.
- The connector executes the chain in-process; it does not start a local
  listener, subprocess, or compatibility daemon.
- Protocol availability remains feature-gated (`ssh`, `quic`, and similar
  require their features).
- Unsupported chain semantics fail connector construction instead of being
  silently dropped or reordered.
- Malformed chained input returns a credential-redacted diagnostic.
- Ordinary `connect_tcp()` / `connect_tcp_timeout()` remain the compatibility
  API (`OutboundError::Runtime`). Detailed `connect_tcp_detailed()` /
  `connect_tcp_timeout_detailed()` share the same single execution and return
  `OutboundConnectError` with stable `kind()` / `stage()` / `hop_index()` /
  `protocol()` facts for embedding/routing policy. Kind/stage/hop/protocol
  are diagnostic facts, not retry recommendations; no direct fallback occurs.
  `HopConnect` vs `HopHandshake` distinguishes proxy transport failure from
  proxy-reported destination failure. Display/Debug are bounded and
  credential-safe; the outer deadline maps to `Timeout` / `Deadline`.
- TCP `OutboundInfo.local_addr` and `peer_addr` come from the socket actually
  connected. Direct routes describe the target socket; a chain normally
  describes hop 0. Hop-zero pooled SSH/H2 may return `None` because reuse can
  discard the candidate socket. H2 pools are scoped to the TLS client-config
  identity, while `H2PoolKey` alone omits trust policy. Explicit hop-zero
  `local_bind` disables SSH/H2 reuse, explicit insecure H2 is unpooled, and
  nested SSH/H2 is unpooled and retains hop-zero metadata. Unix and other
  non-TCP first hops may also return `None`; missing
  metadata does not fail a successful connect. `Some(addr)` always belongs to
  the physical transport carrying the returned stream.
  ALPN adaptation of a caller-supplied `Arc<rustls::ClientConfig>` (e.g.
  when an H2 hop adds the H2 ALPN list to a configured override) clones the
  existing `ClientConfig` via `ClientConfig::clone()` and only mutates
  `alpn_protocols`; trust roots, custom CA stores, mTLS client identity,
  custom verifier, and every other `ClientConfig` field are preserved.
  A caller-supplied `tls_override` combined with per-hop `insecure=true`
  is rejected explicitly (no silent substitution of Eggress's default
  insecure verifier); callers that need that combination must supply an
  explicit insecure override or remove `insecure=true`.
  The socket metadata correction shipped in immutable `v1.0.9`; the pooled
  transport policy corrective is being qualified for `1.0.10`.

### Listener-free UDP (`associate_udp`)

Fixed-target UDP without a listener, built over `eggress-udp` primitives:

```rust
let connector = OutboundConnector::from_pproxy_uri("direct://")?;
let assoc = connector.associate_udp("127.0.0.1", 53).await?;
assoc.send(b"ping").await?;
let mut buf = [0u8; 65535];
let n = assoc.recv(&mut buf).await?;
assoc.close();
```

- Direct and single-hop SOCKS5 upstream are supported over IPv4 and IPv6
  (direct binds `0.0.0.0:0` / `[::]:0` by destination family; SOCKS5 local
  bind is family-corrected to the relay); composed multi-hop
  and Shadowsocks UDP in this surface fail with `UnsupportedFeature`.
- `send_timeout` / `recv_timeout` bound one datagram; timeouts do not close
  the association.
- `close()` is idempotent; drop releases sockets/control tasks and decrements
  `active_udp_associations()`.
- Python does not expose UDP associations; Rust is the supported surface.

## Reload

Reload configuration without restarting the process. Only routing, upstreams,
groups, and health config are hot-swapped. Listener changes (bind, protocols,
auth, TLS, Shadowsocks/Trojan, connection limits/targets, UDP settings,
transparent/unix config) are rejected (restart required).

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

The handle owns exactly one of two mutually exclusive thread models.
Both paths share `startup_in_memory`; only compatibility hooks differ
(`None` native vs `Some` compatibility).

**Async path** (`start()`):
- A Tokio blocking-pool thread runs in-memory startup and then blocks on
  the run thread join for the lifetime of the service.
- A dedicated OS thread (`"eggress-embed-run"`) owns `ServiceSupervisor::run()`.
- `_runtime_task` wraps the blocking task's JoinHandle as a Tokio task.

**Blocking path** (`start_blocking()`):
- Startup runs in the caller thread; a single OS thread
  (`"eggress-embed-run"`) owns `ServiceSupervisor::run()`.
- `_run_handle` holds that thread's JoinHandle directly.

No temporary config file is created; the supervisor starts from the
in-memory compiled `RuntimeConfig` and SIGHUP reload is disabled
(`config_path=None`).

### Compatibility startup

- `start_blocking_with_compatibility_options(CompatibilityOptions)` —
  deprecated legacy source-compatible facade; converts immediately via
  `CompatibilityRuntimeHooks::from_legacy_options` and shares
  `startup_in_memory` (empty maps to native `None`).
- `start_blocking_with_compatibility_hooks(CompatibilityRuntimeHooks)` —
  preferred typed path for maintained Rust/Python callers; same core with
  explicit `Some(hooks)`.

`-d`/`-v` never enter the supervisor; logging is facade-owned via
`PproxyArgs::default_log_level()` with `RUST_LOG` precedence.

### Shutdown behavior

- **`shutdown()`** (async) and **`shutdown_blocking()`** perform orderly
  shutdown: cancel token → join supervisor. These are
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

- Startup is in-memory from the compiled `RuntimeConfig`; no temp config file
  is written and none is required.
- `ServiceSupervisor::run()` creates its own Tokio runtime internally.
- Listener bind changes require a full restart (not reloadable).
- No logging initialization unless explicitly configured in TOML.
- Reverse/backward proxying is supported through the embed API via the same
  TOML config model (`[[reverse_servers]]` and `[[reverse_clients]]` sections).
  No special embed API changes needed — the supervisor handles reverse
  spawning internally, just like forward proxy listeners.

## Feature groups

The embed crate supports the same feature groups as the CLI and runtime:

| Feature | Contents |
|---------|----------|
| `common` | HTTP/SOCKS core, TLS transport, UDP, raw |
| `extended` | Shadowsocks, Trojan, WebSocket (upstream-only) |
| `operations` | System proxy inspection |
| `reverse` | Reverse/backward proxy control-channel |
| `pproxy-compat` | pproxy URI translation and compatibility binary |
| `ssh` | Native/TOML SSH upstream transport; does not activate `pproxy-compat` |
| `quic` | QUIC/H3 transport and protocol |
| `full` | `common` + `extended` + `operations` + `reverse` + `pproxy-compat` + `pproxy-legacy` (default; SSH and QUIC remain opt-in) |

A lean build excludes optional protocol families and operational integrations:

```toml
[dependencies]
eggress-embed = { path = "crates/eggress-embed", default-features = false, features = ["common"] }
```

The `from_pproxy_uri` method requires the `pproxy-compat` feature. Pproxy-style
SSH through that constructor requires both `ssh` and `pproxy-compat`.

The corrected SSH outbound facade is available beginning with `v1.0.7`.
Downstream applications pinned to older releases should not remove an SSH
fallback until they upgrade to `v1.0.7` or newer.

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
| `EggressService` | `new`, `from_toml_str`, `from_toml_file`, `start`, `start_blocking`, `start_blocking_with_compatibility_options` (deprecated legacy facade), `start_blocking_with_compatibility_hooks` (preferred) |
| `EggressHandle` | `bound_addresses`, `status`, `metrics_text`, `reload_toml_str`, `reload_toml_file`, `shutdown`, `shutdown_blocking` |
| `BoundAddresses` | `listener` (lookup by name) |
| `ServiceStatus` | `generation`, `readiness`, `active_connections`, `uptime_secs`, `listener_count`, `listeners`, `udp_associations_active`, `upstream_count` |
| `ListenerStatus` | `name`, `bind`, `local_addr`, `protocols`, `udp_enabled` |
| `ReloadOutcome` | `Applied { generation, upstreams }` |
| `OutboundConnector` | `from_chain`, `direct`, `from_toml`, `from_pproxy_uri`, `connect_tcp`, `connect_tcp_detailed`, `connect_tcp_timeout`, `connect_tcp_timeout_detailed`, `upstream_count`, `hop_count` |
| `OutboundInfo` | `local_addr`, `peer_addr`, `hop_count` |
| `OutboundConnectError` | `kind`, `stage`, `hop_index`, `protocol` |
| `OutboundConnectErrorKind` | `Timeout`, `Dns`, `ConnectionRefused`, `NetworkUnreachable`, `HostUnreachable`, `Authentication`, `Tls`, `Protocol`, `Policy`, `Other` |
| `OutboundConnectStage` | `DirectConnect`, `HopConnect`, `HopHandshake`, `Deadline` |
| `EggressError` | `Config`, `Runtime`, `Startup`, `Reload`, `Shutdown`, `UnsupportedFeature`, `Internal` |
