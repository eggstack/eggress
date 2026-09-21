# eggress-embed — Stable In-Process Rust API

The embedding contract: start/control/reload/stop a full proxy service
inside your own process, plus a direct outbound connector that skips
listeners entirely. Designed as the binding target for PyO3.

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | `EggressConfig`, `EggressService`, `EggressHandle`, redaction logic |
| `src/outbound.rs` | Compatibility facade: `pub use eggress_outbound::*` (implementation authority in `eggress-outbound`) |
| `src/error.rs` | `EggressError` enum (7 variants, PyO3-mappable) |

## Public API surface

### EggressConfig (`src/lib.rs:69`)

| Method | Line | Description |
|---|---|---|
| `from_toml_str(input)` | `src/lib.rs` | Delegate parse/version/validate/compile to canonical `eggress-config`; store compiled `RuntimeConfig` + ancillary source TOML |
| `from_compiled(compiled, source)` | `src/lib.rs` | Construct from native `RuntimeConfig` (pproxy direct path); startup uses only `compiled` |
| `compiled()` / `into_compiled()` | `src/lib.rs` | Borrow/consume the canonical compiled handoff |
| `from_toml_file(path)` | :143 | Read file then delegate to `from_toml_str` |
| `source_toml()` | :154 | Return raw TOML text |
| `to_redacted_toml()` | :163 | TOML with secrets replaced by `****` and URI userinfo by `****@` |

`eggress-config::validate_and_compile_toml()` is the single parse/version/
validation/compilation authority. The embed facade's private adapter preserves
its established error messages; `OutboundConnector` owns only outbound-specific
post-compilation checks, and reload maps failures to `Reload` + metrics.

### EggressService (`src/lib.rs:177`)

| Method | Line | Description |
|---|---|---|
| `new(config)` | :183 | Wrap a validated config |
| `from_toml_str(input)` | :188 | Convenience: parse + new |
| `from_toml_file(path)` | :193 | Convenience: file parse + new |
| `start()` async | `src/lib.rs` | In-memory `start_from_config` (no temp file) inside caller's Tokio runtime |
| `start_blocking()` | `src/lib.rs` | In-memory `start_from_config` (no temp file); single `eggress-embed-run` thread |
| `start_blocking_with_compatibility_options(options)` | `src/lib.rs` | Deprecated legacy source-compatible facade (`CompatibilityOptions`); converts via `from_legacy_options` then shares `startup_in_memory` (empty maps to `None`) |
| `start_blocking_with_compatibility_hooks(hooks)` | `src/lib.rs` | Preferred typed path with explicit `CompatibilityRuntimeHooks`; same `startup_in_memory` core (`Some` compat) |

### EggressHandle (`src/lib.rs:422`)

| Method | Line | Description |
|---|---|---|
| `bound_addresses()` | :433 | `BoundAddresses` with listener + admin addrs |
| `status()` | :461 | `ServiceStatus`: generation, readiness, connections, uptime, listeners |
| `metrics_text()` | :501 | Prometheus metrics text |
| `reload_toml_str(input)` | :515 | Hot-reload routing/upstream/groups/health; rejects startup-captured listener changes |
| `reload_toml_file(path)` | :572 | File-based reload |
| `shutdown()` async | :601 | Cancel token + join runtime |
| `shutdown_blocking()` | :619 | Blocking shutdown |

### OutboundConnector (`eggress_embed::outbound::*`, authority in `eggress-outbound`)

Compatibility facade over `eggress-outbound::OutboundConnector`
(`src/outbound.rs` is `pub use eggress_outbound::*`). Downstream
`use eggress_embed::outbound::{OutboundConnector, OutboundConnectErrorKind}`
keeps working; the implementation, hop registry, executor factory, and
classifier live in [outbound.md](outbound.md).

| Method | Description |
|---|---|
| `from_chain(chain)` | Native constructor from a compiled `ProxyChainSpec` (no TOML/pproxy); rejects empty chains |
| `direct()` | Explicit direct connector |
| `from_toml(config_toml)` | Parse/validate/compile via canonical `eggress-config`, then require at least one upstream + non-empty chain |
| `from_pproxy_uri(uri)` | Full pproxy `__` chain → `compile_chain_to_native` (no TOML string) → stored chain (fail-closed, redacted errors) |
| `connect_tcp(host, port)` | Compatibility surface: failures stay `OutboundError::Runtime` |
| `connect_tcp_detailed(host, port)` | Opt-in typed surface returning `OutboundConnectError` |
| `connect_tcp_timeout(host, port, timeout)` | Compatibility timeout: outer deadline stays `Runtime("connection timed out")` |
| `connect_tcp_timeout_detailed(host, port, timeout)` | Typed timeout: outer deadline is `Timeout`/`Deadline` |
| `associate_udp(target_host, target_port)` | Listener-free fixed-target UDP: direct or single-hop SOCKS5, no hidden listener |
| `associate_udp_timeout(host, port, timeout)` | Same with establishment timeout |
| `active_udp_associations()` | Live listener-free UDP count |
| `upstream_count()` / `hop_count()` | Configured upstreams / chain hops (0 for direct) |
| `validate_outbound_config(toml)` | Static validation, returns hop count |

### Outbound UDP (`associate_udp`, authority in `eggress-outbound`)

Fixed-target connected semantics over existing UDP primitives, no hidden
listener (see [outbound.md](outbound.md) for the full contract):

- Direct: family-aware wildcard bind + `connect(resolved target)` for
  `direct://` connectors. Never loopback-bound.
- Single-hop SOCKS5: `open_socks5_udp_upstream()` with SOCKS5 datagram
  encode/decode per send/recv.
- Unsupported chains fail with `UnsupportedFeature`, never silent direct
  fallback.

`from_pproxy_uri()` parses via `parse_pproxy_chain()`, preserving every `__`
hop in source order, then calls `compile_chain_to_native()` (validation +
`build_chain_config_uri` → `parse_proxy_chain`, no TOML). Only a single
`direct` hop takes the direct fast path; multi-hop `direct`, backward (`+in`),
or unsupported roles fail closed with redacted errors. The connector holds an
`OutboundRoute` (direct vs compiled chain + upstream count), never a full
`RuntimeConfig`, and owns the executor's SSH session state for its full
lifetime: native/TOML/`from_chain` construction uses the verified
`SshSessionCache::new()` policy, while `from_pproxy_uri()` uses
`new_compatibility()` only when both `ssh` and `pproxy-compat` are enabled.
Direct mode does not allocate SSH state.

The embed SSH regression provisions a temporary local OpenSSH daemon and
exercises byte traversal, fail-closed redacted authentication failure, and
native untrusted-host-key rejection. CI installs `openssh-server` and sets
`EGRESS_REQUIRE_OPENSSH_TESTS=1`; with that variable set, missing tools and
all fixture setup/readiness failures are fatal. Optional local runs may skip
only when `sshd` or `ssh-keygen` is genuinely unavailable.

## How it works

### Async path (`start()`)

1. Consumes `EggressConfig::into_compiled()` (validated once, no filesystem).
2. Spawns `tokio::task::spawn_blocking` which calls shared
   `startup_in_memory(rt_config, None)` →
   `ServiceSupervisor::start_from_config(rt_config, None)`.
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
compatibility hooks differ (`None` native vs `Some(CompatibilityRuntimeHooks)`
for `--auth` reuse / `--sys`). `-d`/`-v` never enter the supervisor. The legacy
`CompatibilityOptions` facade exists only for source compatibility and converts
immediately; new Rust/Python code uses
`start_blocking_with_compatibility_hooks`.

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

### Typed outbound errors (`OutboundConnectError`, authority in `eggress-outbound`)

Ordinary `connect_tcp()` / `connect_tcp_timeout()` remain the simple
compatibility API (`OutboundError::Runtime`). Detailed
`connect_tcp_detailed()` / `connect_tcp_timeout_detailed()` share one
private `connect_tcp_inner()` with the legacy methods and return
`OutboundConnectError` with stable `kind()` / `stage()` /
`hop_index()` / `protocol()` facts (see [outbound.md](outbound.md)).

- Kinds: `Timeout`, `Dns`, `ConnectionRefused`, `NetworkUnreachable`,
  `HostUnreachable`, `Authentication`, `Tls`, `Protocol`, `Policy`, `Other`.
- Stages: `DirectConnect`, `HopConnect`, `HopHandshake`, `Deadline`.
- Kind/stage/hop/protocol are diagnostic facts, not retry recommendations;
  callers own retry/backoff policy and no direct fallback occurs.
- `Display`/`Debug` carry only kind/stage/hop/protocol facts, never
  credentials, URIs, or config snippets; there is no public `source()`
  chain. The shared classifier lives in `eggress-outbound::classify`
  (`#[doc(hidden)]`, type-based only) and also backs
  `From<ChainError> for SessionOpenError`, which no longer flattens typed
  handshake failures to strings.

## Configuration / features

| Feature | Description |
|---|---|
| `full` (default) | `common`+`extended`+`operations`+`reverse`+`pproxy-compat`+`pproxy-legacy` |
| `common` | HTTP/SOCKS core, TLS transport, UDP, raw |
| `extended` | Adds Shadowsocks, Trojan, WebSocket |
| `pproxy-compat` | pproxy URI translation (`from_pproxy_uri`) and compatibility options |
| `pproxy-legacy` | Bounded SSR TCP framing + six built-in plugins (in default `full`; forwards to runtime+outbound) |
| `operations` | Runtime operations gate (`eggress-runtime/operations`: admin snapshot provider wiring) |
| `reverse` | Reverse proxy control channel |
| `ssh` | Native/TOML SSH upstream transport; does not activate `pproxy-compat` |
| `quic` | QUIC/H3 config support |
| `legacy-crypto` | Legacy Shadowsocks ciphers |

## Security notes

- No temp config file exists (in-memory startup); plaintext credentials never
  touch the filesystem via embed startup. Retained source TOML (ancillary
  display state) stays in memory only.
- `to_redacted_toml()` walks the TOML tree generically (:788-827):
  - Keys matching `REDACTED_SECRET_KEYS` (`password`, `password_env`,
    `secret`, `secret_ref`, `token`, `api_key`, `apikey`, `credentials`,
    `bearer`, `bearer_token`, `bearer_token_env`)
    have their string values replaced with `****`.
  - Strings containing `://` are passed through the canonical tolerant
    redactor `eggress_uri::redact_proxy_uri()`, which strips `user:pass@`
    and `user@` userinfo for any scheme (last unbracketed `@` wins, so
    passwords containing `@` stay covered) and emits `scheme://****@host`.
    There is no embed-local scheme whitelist.
- Outbound error paths (in `eggress-outbound`, feature `pproxy-compat`) parse hops
  via `eggress_pproxy_compat` and fall back to a scheme-agnostic
  last-`@`-outside-brackets scrubber that additionally masks `#` auth
  fragments; over-redaction is preferred to leakage there.

## Concurrency & lifecycle

- `reload_mutex` (:428) is a `std::sync::Mutex` — serializes reload
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
| `tests/reload_convergence.rs` | Reload convergence |
| `tests/ssh.rs` | OpenSSH-gated SSH outbound regression (`EGRESS_REQUIRE_OPENSSH_TESTS=1`) |
| `tests/metrics_status.rs` | Prometheus metrics rendering, service status |
| `tests/proxy_traffic.rs` | End-to-end proxy traffic through embed handle |
| `tests/error_redaction.rs` | Credential redaction in errors, `to_redacted_toml`, category labels |
| `tests/outbound_detailed.rs` | Typed `connect_tcp_detailed` matrix via the re-export facade (direct/HTTP/SOCKS/TLS, hop provenance, deadline, legacy compat, redaction, reuse/cancel) |
| `tests/public_api.rs` | Public API surface guard |

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
- The 30-second readiness timeout (:375) is a hard wall — if the
  service doesn't become ready in time, the handle is not returned.
- `reload_toml_str` rejects ANY listener topology change (count, name, or
  bind). Only routing rules, upstreams, and health state can be hot-reloaded.
- `OutboundConnector` construction/execution coverage lives in
  `eggress-outbound` (`from_chain`, `direct`, TOML, pproxy, UDP); embed
  tests prove the `eggress_embed::outbound::*` re-export compiles and
  behaves, plus full-service lifecycle.
- `OutboundConnector::associate_udp()` supports direct + single-hop SOCKS5;
  composed/Shadowsocks UDP in this surface fail with `UnsupportedFeature`.
  Python does not expose UDP associations; Rust is the supported surface.
- No temp file exists; `EggressHandle._config_path` is always `None`.
  In-memory services never pretend to have a config file (SIGHUP disabled).
- The corrected SSH outbound facade is available beginning with `v1.0.7`;
  downstream fallback removal requires `v1.0.7` or newer.

## See also

- [cli.md](cli.md) — CLI alternative to embed API
- [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) — SSH/QUIC/H3 transport features

## Review entry points

- `cargo test -p eggress-embed --test reload`
- `cargo test -p eggress-embed --test error_redaction`
- `cargo test -p eggress-embed --test start_stop`
