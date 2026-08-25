# eggress-embed — Stable In-Process Rust API

The embedding contract: start/control/reload/stop a full proxy service
inside your own process, plus a direct outbound connector that skips
listeners entirely. Designed as the binding target for PyO3.

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | `EggressConfig`, `EggressService`, `EggressHandle`, redaction logic |
| `src/outbound.rs` | `OutboundConnector` for chain execution without listeners |
| `src/error.rs` | `EggressError` enum (7 variants, PyO3-mappable) |

## Public API surface

### EggressConfig (`src/lib.rs:64`)

| Method | Line | Description |
|---|---|---|
| `from_toml_str(input)` | :70 | Parse, version-check, validate, compile; stores source TOML |
| `from_toml_file(path)` | :96 | Read file then delegate to `from_toml_str` |
| `source_toml()` | :104 | Return raw TOML text |
| `to_redacted_toml()` | :113 | TOML with credentials replaced by `****` / `****:****@` |

Validation chain: `toml::from_str` → version check (must be 1 or absent)
→ `validate_config()` → `compile_config()`.

### EggressService (`src/lib.rs:127`)

| Method | Line | Description |
|---|---|---|
| `new(config)` | :133 | Wrap a validated config |
| `from_toml_str(input)` | :138 | Convenience: parse + new |
| `from_toml_file(path)` | :143 | Convenience: file parse + new |
| `start()` async | :152 | Start inside caller's Tokio runtime |
| `start_blocking()` | :233 | Start on dedicated OS threads |
| `start_blocking_with_compatibility_options()` | :313 | pproxy-compat entry point (feature-gated) |

### EggressHandle (`src/lib.rs:419`)

| Method | Line | Description |
|---|---|---|
| `bound_addresses()` | :430 | `BoundAddresses` with listener + admin addrs |
| `status()` | :458 | `ServiceStatus`: generation, readiness, connections, uptime, listeners |
| `metrics_text()` | :498 | Prometheus metrics text |
| `reload_toml_str(input)` | :506 | Hot-reload routing/upstream; rejects listener topology changes |
| `reload_toml_file(path)` | :586 | File-based reload |
| `shutdown()` async | :594 | Cancel token + join runtime |
| `shutdown_blocking()` | :614 | Blocking shutdown |

### OutboundConnector (`src/outbound.rs:55`)

| Method | Line | Description |
|---|---|---|
| `from_toml(config_toml)` | :63 | Compile config, require at least one upstream |
| `from_pproxy_uri(uri)` | :106 | pproxy URI → translated TOML → connector (feature-gated) |
| `connect_tcp(host, port)` | :133 | Execute chain, return `(BoxStream, OutboundInfo)` |
| `connect_tcp_timeout(host, port, timeout)` | :188 | Wraps `connect_tcp` in `tokio::time::timeout` |
| `associate_udp(target_host, target_port)` | :206 | Returns error — not yet implemented |
| `upstream_count()` | :219 | Number of configured upstreams |
| `validate_outbound_config(toml)` | :228 | Static validation, returns hop count |

## How it works

### Async path (`start()`)

1. Writes config to a temp file via `write_temp_config()` (:867) — on Unix,
   file is created with mode `0o600` (:877-883).
2. Spawns `tokio::task::spawn_blocking` which calls
   `ServiceSupervisor::start(&config_path)`.
3. Inside that blocking task, spawns a dedicated OS thread
   `"eggress-embed-rt"` (:171) that runs `sup.run()`.
4. Polls `state.readiness` every 5ms for up to 30 seconds (:176-191).
5. Returns `EggressHandle` with `RuntimeState`, `CancellationToken`, and
   the Tokio task join handle.

### Blocking path (`start_blocking()`)

1. Spawns outer OS thread `"eggress-embed-rt"` (:238) for startup.
2. Inside that thread, spawns inner OS thread `"eggress-embed-run"` (:252-254)
   that owns `ServiceSupervisor::run()`.
3. Sends `(state, token, run_handle, config_path)` through a
   `sync_channel(1)`.
4. Returns `EggressHandle` with the run thread's `JoinHandle`.

### Reload semantics

`reload_toml_str()` (:506-583):

1. Acquires `reload_mutex` (prevents concurrent reloads).
2. Parses, validates, compiles new config.
3. Compares listener topology (count, names, bind addresses). Changes
   require a full restart — returns error.
4. Builds new `CompiledRuntimeSnapshot` via `compile_runtime_snapshot()`.
5. Publishes new snapshot via `store()`, swaps router via `swap_arc()`.
6. Returns `ReloadOutcome::Applied { generation, upstreams }`.

### Drop behavior

`Drop for EggressHandle` (:635-662):
- Cancels the shutdown token.
- Blocking path: joins run thread directly.
- Async path: creates a throwaway `tokio::runtime::Runtime`, awaits task
  with a 5-second timeout.
- Removes temp config file.

## Error & failure model

`EggressError` (`src/error.rs:6`):

| Variant | Label | Meaning |
|---|---|---|
| `Config(String)` | `config` | Parse/validation/compile error |
| `Runtime(String)` | `runtime` | Connection or runtime error |
| `Startup(String)` | `startup` | Service failed to start |
| `Reload(String)` | `reload` | Config reload rejected/failed |
| `Shutdown(String)` | `shutdown` | Shutdown error |
| `UnsupportedFeature { feature, message }` | `unsupported_feature` | Feature not available |
| `Internal(String)` | `internal` | Should not occur |

All variants carry redacted string messages. `category()` (:43-53) returns
a stable `&'static str` label for each variant.

## Configuration / features

| Feature | Description |
|---|---|
| `full` (default) | `common`+`extended`+`operations`+`reverse`+`pproxy-compat`+`pproxy-legacy` |
| `common` | HTTP, SOCKS4/5, Shadowsocks, Trojan |
| `extended` | WebSocket, raw, H2 |
| `pproxy-compat` | Enables `start_blocking_with_compatibility_options` |
| `operations` | System proxy integration |
| `reverse` | Reverse proxy control channel |
| `ssh` | SSH transport passthrough to runtime |
| `quic` | QUIC/H3 config support |
| `legacy-crypto` | Legacy Shadowsocks ciphers |

## Security notes

- Temp config file is `0o600` on Unix (:877-883) since TOML may carry
  plaintext upstream credentials.
- `to_redacted_toml()` walks the TOML tree generically (:777-805):
  - Keys matching `REDACTED_SECRET_KEYS` (`password`, `password_env`,
    `secret`, `secret_ref`, `token`, `api_key`, `apikey`, `credentials`)
    have their string values replaced with `****`.
  - Strings matching `looks_like_proxy_uri()` (scheme in eggress-supported
    set) are passed through `redact_uri()` which replaces `user:pass@`
    with `****:****@`.
- `redact_uri()` (:841-861) finds the LAST unbracketed `@` after the
  scheme to handle passwords containing `@`.

## Concurrency & lifecycle

- `reload_mutex` (:425) is a `std::sync::Mutex` — serializes reload
  attempts. Poisoned mutex is recovered via `into_inner()`.
- `state.snapshot` is an `ArcSwap` — readers see a consistent snapshot
  without blocking writers.
- `state.readiness` is `AtomicBool` — polled with `Ordering::Acquire`.
- `state.active_connections` is `AtomicU64` — `Ordering::Relaxed` reads.

## Test coverage

| Test file | What it exercises |
|---|---|
| `tests/start_stop.rs` | Start + bound_addresses + shutdown lifecycle |
| `tests/reload.rs` | Hot-reload, listener topology rejection |
| `tests/metrics_status.rs` | Prometheus metrics rendering, service status |
| `tests/proxy_traffic.rs` | End-to-end proxy traffic through embed handle |
| `tests/error_redaction.rs` | Credential redaction in errors, `to_redacted_toml`, category labels |

Inline tests (`src/lib.rs`):
- `listener_addr_prefers_bound_address` (:900)
- `listener_addr_falls_back_to_configured_bind` (:909)
- `listener_addr_uses_default_for_invalid_configured_bind` (:919)
- `temp_config_file_is_owner_only` (Unix, :928)

## Reviewer gotchas

- `start()` requires an active Tokio runtime context; calling it outside
  one produces a runtime error, not a compile error.
- The 30-second readiness timeout (:177, :270) is a hard wall — if the
  service doesn't become ready in time, the handle is not returned.
- `reload_toml_str` rejects ANY listener topology change (count, name, or
  bind). Only routing rules, upstreams, and health state can be hot-reloaded.
- `OutboundConnector::associate_udp()` always returns an error (:210-215)
  — this is an unimplemented stub.
- `write_temp_config` uses a monatomic counter (:869) for unique filenames;
  two embed instances in the same process never collide.

## See also

- [cli.md](cli.md) — CLI alternative to embed API
- [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) — SSH/QUIC/H3 transport features

## Review entry points

- `cargo test -p eggress-embed --test reload`
- `cargo test -p eggress-embed --test error_redaction`
- `cargo test -p eggress-embed --test start_stop`
