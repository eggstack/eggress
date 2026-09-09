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
| `from_toml_str(input)` | `src/lib.rs` | Parse, version-check, validate, compile once via shared `parse_validate_compile`; stores compiled `RuntimeConfig` + ancillary source TOML |
| `from_compiled(compiled, source)` | `src/lib.rs` | Construct from native `RuntimeConfig` (pproxy direct path); startup uses only `compiled` |
| `compiled()` / `into_compiled()` | `src/lib.rs` | Borrow/consume the canonical compiled handoff |
| `from_toml_file(path)` | :96 | Read file then delegate to `from_toml_str` |
| `source_toml()` | :104 | Return raw TOML text |
| `to_redacted_toml()` | :113 | TOML with secrets replaced by `****` and URI userinfo by `****@` |

Validation chain (single shared boundary `parse_validate_compile`): `toml::from_str` → version check (must be 1 or absent) → `validate_config()` → `compile_config()`. `OutboundConnector::from_toml` / `validate_outbound_config` and `reload_toml_str` reuse the same boundary; reload maps failures to `Reload` + metrics.

### EggressService (`src/lib.rs:127`)

| Method | Line | Description |
|---|---|---|
| `new(config)` | :133 | Wrap a validated config |
| `from_toml_str(input)` | :138 | Convenience: parse + new |
| `from_toml_file(path)` | :143 | Convenience: file parse + new |
| `start()` async | `src/lib.rs` | In-memory `start_from_config` (no temp file) inside caller's Tokio runtime |
| `start_blocking()` | `src/lib.rs` | In-memory `start_from_config` (no temp file); single `eggress-embed-run` thread |
| `start_blocking_with_compatibility_options()` | `src/lib.rs` | Same `startup_in_memory` core with explicit `CompatibilityOptions`; only options differ |

### EggressHandle (`src/lib.rs:419`)

| Method | Line | Description |
|---|---|---|
| `bound_addresses()` | :430 | `BoundAddresses` with listener + admin addrs |
| `status()` | :458 | `ServiceStatus`: generation, readiness, connections, uptime, listeners |
| `metrics_text()` | :498 | Prometheus metrics text |
| `reload_toml_str(input)` | :520 | Hot-reload routing/upstream/groups/health; rejects startup-captured listener changes |
| `reload_toml_file(path)` | :584 | File-based reload |
| `shutdown()` async | :594 | Cancel token + join runtime |
| `shutdown_blocking()` | :614 | Blocking shutdown |

### OutboundConnector (`src/outbound.rs:55`)

| Method | Line | Description |
|---|---|---|
| `from_toml(config_toml)` | `src/outbound.rs` | Parse/validate/compile via shared `parse_validate_compile`, then require at least one upstream + non-empty chain (outbound-only checks) |
| `from_pproxy_uri(uri)` | `src/outbound.rs` | Full pproxy `__` chain → `compile_chain_to_native` (typed `PproxyChain` → native `ProxyChainSpec`, no TOML string) → minimal `RuntimeConfig` → connector (fail-closed, redacted errors) |
| `connect_tcp(host, port)` | :133 | Execute chain, return `(BoxStream, OutboundInfo)` |
| `connect_tcp_timeout(host, port, timeout)` | :188 | Wraps `connect_tcp` in `tokio::time::timeout` |
| `associate_udp(target_host, target_port)` | `src/outbound.rs` | Listener-free fixed-target UDP: direct or single-hop SOCKS5 via `eggress-udp` primitives, no hidden listener |
| `associate_udp_timeout(host, port, timeout)` | `src/outbound.rs` | Same with establishment timeout |
| `active_udp_associations()` | `src/outbound.rs` | Live listener-free UDP count (increment on create, decrement on close/drop) |
| `upstream_count()` | :219 | Number of configured upstreams |
| `validate_outbound_config(toml)` | :228 | Static validation, returns hop count |

### Outbound UDP (`associate_udp`)

Fixed-target connected semantics over existing UDP primitives, no hidden
listener:

- Direct: family-aware wildcard bind (`0.0.0.0:0` for IPv4, `[::]:0`
  for IPv6, selected from the resolved destination) + `connect(resolved
  target)` for `direct://` connectors. Never loopback-bound.
- Single-hop SOCKS5: `open_socks5_udp_upstream()` (TCP control + UDP
  ASSOCIATE handshake) with SOCKS5 datagram encode/decode per send/recv.
  The caller passes an unspecified hint (`0.0.0.0:0`, or `[::]:0` for
  IPv6-literal proxy hosts); the primitive family-corrects the effective
  bind against the negotiated relay (`effective_udp_bind`), so IPv6
  relays use `[::]:0` instead of failing on IPv4 loopback.
- Target validation via `validate_standalone_target(allow_private_egress=true)`
  (multicast/broadcast/unspecified/port-zero rejected; private/loopback allowed
  because the caller explicitly selected the destination) plus
  `validate_datagram_size(65535)`.
- Unsupported chains (HTTP, multi-hop, composed, Shadowsocks UDP in this
  surface) fail with `UnsupportedFeature`, never silent direct fallback.
- `UdpAssociation::send/recv/send_timeout/recv_timeout/close/wait_closed`,
  `is_closed`, `local_addr`, `target`, `relay_addr` (SOCKS5 only). Close is
  idempotent; drop decrements `active_udp_associations()` exactly once and
  aborts the SOCKS5 control keepalive.

`from_pproxy_uri()` parses via `parse_pproxy_chain()`, preserving every `__`
hop in source order, then calls `compile_chain_to_native()` (validation +
`build_chain_config_uri` → `parse_proxy_chain`, no TOML). Only a single
`direct` hop takes the direct fast path; multi-hop `direct`, backward (`+in`),
or unsupported roles fail closed with redacted errors. Execution reuses
`ChainExecutor` with no listener.

## How it works

### Async path (`start()`)

1. Consumes `EggressConfig::into_compiled()` (validated once, no filesystem).
2. Spawns `tokio::task::spawn_blocking` which calls shared
   `startup_in_memory(rt_config, CompatibilityOptions::default())` →
   `ServiceSupervisor::start_from_config_with_options(rt_config, None, _)`.
3. Inside, spawns a single OS thread `"eggress-embed-run"` that owns
   `sup.run()`.
4. Polls `state.readiness` every 5ms for up to 30 seconds; on timeout cancels
   and joins before returning startup error (no leaked thread).
5. Sends `(state, token)` via oneshot, then blocks on run-thread join for
   service lifetime. Returns `EggressHandle` with `_config_path=None`
   (SIGHUP disabled; in-memory service pretends no config file).

### Blocking path (`start_blocking()`)

1. Calls the same shared `startup_in_memory(rt_config, options)` directly in
   the caller thread (no outer thread, no channel).
2. `startup_in_memory` creates the supervisor from memory, spawns
   `"eggress-embed-run"`, waits readiness (cancel+join on timeout).
3. Returns `EggressHandle` with the run thread's `JoinHandle` and
   `_config_path=None`.

Native and compatibility startup share `startup_in_memory`; only
`CompatibilityOptions` differ.

### Reload semantics

`reload_toml_str()` / `reload_compiled()` delegate to the canonical
`RuntimeState::apply_compiled_config()` transaction (see `runtime.md`).
File (`reload_toml_file`), string, and native entry points differ only in how
the new `RuntimeConfig` is obtained (file read vs string
`parse_validate_compile` vs already-compiled). The transaction owns
classification, snapshot build, snapshot/routing/admin publication, health
restart, H2 pool clear, and metrics (`set_config_generation` + `record_reload`
on success *and* failure). Rejected/failed reloads preserve generation.
`reload_compiled()` is the native entry point for direct pproxy compilation
(no TOML string).

### Drop behavior

`Drop for EggressHandle`:
- Cancels the shutdown token.
- Blocking path: joins run thread directly.
- Async path: creates a throwaway `tokio::runtime::Runtime`, awaits task
  with a 5-second timeout.
- Clears `_config_path` (always `None`; no temp file exists).

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
| `common` | HTTP/SOCKS core, TLS transport, UDP, raw |
| `extended` | Adds Shadowsocks, Trojan, WebSocket |
| `pproxy-compat` | pproxy URI translation (`from_pproxy_uri`) and compatibility options |
| `operations` | System proxy integration |
| `reverse` | Reverse proxy control channel |
| `ssh` | SSH transport passthrough to runtime |
| `quic` | QUIC/H3 config support |
| `legacy-crypto` | Legacy Shadowsocks ciphers |

## Security notes

- No temp config file exists (in-memory startup); plaintext credentials never
  touch the filesystem via embed startup. Retained source TOML (ancillary
  display state) stays in memory only.
- `to_redacted_toml()` walks the TOML tree generically (:788-827):
  - Keys matching `REDACTED_SECRET_KEYS` (`password`, `password_env`,
    `secret`, `secret_ref`, `token`, `api_key`, `apikey`, `credentials`)
    have their string values replaced with `****`.
  - Strings containing `://` are passed through the canonical tolerant
    redactor `eggress_uri::redact_proxy_uri()`, which strips `user:pass@`
    and `user@` userinfo for any scheme (last unbracketed `@` wins, so
    passwords containing `@` stay covered) and emits `scheme://****@host`.
    There is no embed-local scheme whitelist.
- Outbound error paths (`outbound.rs`, feature `pproxy-compat`) parse hops
  via `eggress_pproxy_compat` and fall back to a scheme-agnostic
  last-`@`-outside-brackets scrubber that additionally masks `#` auth
  fragments; over-redaction is preferred to leakage there.

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
- `listener_addr_*` helpers
- `from_toml_str_validates_once_and_compiles_in_memory`
- `blocking_start_succeeds_from_in_memory_config_without_tempfile`
  (asserts `_config_path=None` + no `eggress-embed-*.toml` created)
- `startup_does_not_require_writable_temp_dir`
- `async_start_succeeds_from_in_memory_config`

## Reviewer gotchas

- `start()` requires an active Tokio runtime context; calling it outside
  one produces a runtime error, not a compile error.
- The 30-second readiness timeout (:177, :270) is a hard wall — if the
  service doesn't become ready in time, the handle is not returned.
- `reload_toml_str` rejects ANY listener topology change (count, name, or
  bind). Only routing rules, upstreams, and health state can be hot-reloaded.
- `OutboundConnector::associate_udp()` supports direct + single-hop SOCKS5;
  composed/Shadowsocks UDP in this surface fail with `UnsupportedFeature`.
  Python does not expose UDP associations; Rust is the supported surface.
- No temp file exists; `EggressHandle._config_path` is always `None`.
  In-memory services never pretend to have a config file (SIGHUP disabled).

## See also

- [cli.md](cli.md) — CLI alternative to embed API
- [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) — SSH/QUIC/H3 transport features

## Review entry points

- `cargo test -p eggress-embed --test reload`
- `cargo test -p eggress-embed --test error_redaction`
- `cargo test -p eggress-embed --test start_stop`
