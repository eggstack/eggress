# eggress-config -- TOML Schema, Validation, Compilation

Turns user TOML into a validated, compiled `RuntimeConfig`. This is the only
place the configuration surface is defined; CLI flags, embed API, and Python
bindings all funnel through it. Single compilation pass: anything invalid fails
before any socket is bound.

## Module map

| File | Role |
|---|---|
| `src/model.rs` | Serde types mirroring the TOML schema: ConfigFile, listeners, upstreams, groups, rules, timeouts, process, admin, reverse servers/clients |
| `src/lib.rs` | Public entry points (`load_and_validate`, `validate_and_compile_toml`, `_with_warnings` variants), integration tests |
| `src/compile.rs` | Validation -> compiled RuntimeConfig; resolves secrets, CLI-flag compatibility, default synthesis |
| `src/validate.rs` | Structural validation (duplicates, unknown refs, bad URIs/durations/regex/CIDR) and security warnings |
| `src/file.rs` | Bounded file loading (1 MB limit with TOCTOU guard) |
| `src/error.rs` | `ConfigError` and `ConfigWarning` types |

## Public API surface

### Entry points (`lib.rs`)

| Function | Returns | Notes |
|---|---|---|
| `load_and_validate(path)` | `Result<RuntimeConfig, ConfigError>` | File-based; no security warnings |
| `load_and_validate_with_warnings(path)` | `Result<(RuntimeConfig, Vec<ConfigWarning>), ConfigError>` | File-based; includes warnings |
| `validate_and_compile_toml(toml_str)` | `Result<RuntimeConfig, ConfigError>` | In-memory; used by compat layer |
| `validate_and_compile_toml_with_warnings(toml_str)` | `Result<(RuntimeConfig, Vec<ConfigWarning>), ConfigError>` | In-memory; includes warnings |

All paths share: TOML parse -> version check (must be 1 or absent) -> structural validate -> security warnings -> compile.

### ConfigFile schema (`model.rs`)

Top-level fields:

| Field | Type | Notes |
|---|---|---|
| `version` | `Option<u32>` | Must be 1 if present |
| `process` | `Option<ProcessConfig>` | log_format, log_level, shutdown_grace |
| `timeouts` | `Option<TimeoutConfig>` | handshake, connect (duration strings) |
| `listeners` | `Option<Vec<ListenerConfig>>` | Each requires name, bind, protocols |
| `upstreams` | `Option<Vec<UpstreamConfig>>` | Each requires id, uri |
| `upstream_groups` | `Option<Vec<UpstreamGroupConfig>>` | Each requires id, members |
| `rules` | `Option<Vec<RuleConfig>>` | Each requires id + matcher + action |
| `rules_file` | `Option<String>` | pproxy-format regex rules file |
| `routing` | `Option<RoutingConfig>` | default action |
| `admin` | `Option<AdminConfig>` | bind, enabled, metrics, auth, pac, static_content |
| `reverse_servers` | `Option<Vec<ReverseServerConfig>>` | Control channel acceptors |
| `reverse_clients` | `Option<Vec<ReverseClientConfig>>` | Control channel clients |

### ListenerConfig fields

| Field | Type | Notes |
|---|---|---|
| `name` | String | Unique across listeners |
| `bind` | String | Socket address |
| `protocols` | Vec<String> | Non-empty; validated against known protocol list |
| `connection_limit` | Option<u32> | Must be > 0 if set |
| `auth` | Option<AuthConfig> | type="password", username, password/password_env |
| `udp_enabled` / `udp` | Option | Legacy sugar or full UDP config |
| `tls` | Option<ListenerTlsConfig> | cert, key, alpn |
| `shadowsocks` / `ssr` / `trojan` | Option | Protocol-specific sections |
| `transparent` / `unix` | Option | Platform-specific listener modes |
| `fixed_target` / `local_bind` | Option | Bypass routing / outbound bind |

### RuleConfig / MatchExprConfig

Rules support two matcher styles:
1. **Legacy** (single field): `host_exact`, `host_suffix`, `host_regex`, `destination_port`, `destination_port_regex`, `any` -- exactly one allowed per rule
2. **Recursive** (`match` field): `MatchExprConfig` with `all`, `any_of`, `not` composites and leaf matchers

Leaf matcher fields (in `MatchExprConfig::Leaf`):

| Field | Type | Notes |
|---|---|---|
| `host_exact` | String | Exact hostname match |
| `host_suffix` | String | Suffix match |
| `host_regex` | String | Regex match (compiled at validate time) |
| `destination_port` | u16 | Exact port |
| `destination_port_regex` | String | Port regex |
| `destination_port_range` | Vec<u16> | Exactly 2 elements [start, end]; start <= end enforced |
| `destination_port_set` | Vec<u16> | Non-empty set |
| `destination_cidr` | String | CIDR notation (parsed via ipnet) |
| `source_cidr` | String | CIDR notation |
| `source_port` | u16 | Exact source port |
| `listener` | String | Listener name match |
| `protocol` | String | Protocol name match |
| `identity` | String | Client identity match |
| `transport` | String | "tcp", "udp", or "reverse_tcp" |
| `reverse_listener` | String | Reverse listener name match |

Composite matchers enforce depth limit (10) and node count limit (100).

### RuntimeConfig (`compile.rs`)

Compiled output with all defaults resolved: process config (log defaults: text/info/30s), timeout config (10s/30s), compiled listeners, upstreams (parsed chains + health + h2), groups (scheduler + members + fallback), compiled rules, default action (Direct), admin config (127.0.0.1:9090 default), reverse server/client configs.

## How it works (control flow)

```
TOML string
  -> toml::from_str()           [model.rs types]
  -> version check              [must be 1 or absent]
  -> validate_config()          [validate.rs: structural checks]
  -> validate_config_security() [validate.rs: non-fatal warnings]
  -> compile_config()           [compile.rs: resolve defaults, secrets, URIs]
  -> RuntimeConfig
```

### Secret resolution (`compile.rs:resolve_password`)

Secrets are resolved at compile time from three sources:
1. **Inline**: `password = "secret"` -- used directly
2. **Environment**: `password_env = "MY_SECRET"` -- `std::env::var()` at compile time
3. **File**: TLS cert/key read via `std::fs::read()` at compile time

Resolution rules:
- If `password_env` is set, the environment variable is read; missing var = error
- If both `password` and `password_env` are set for admin auth, it is an error
- Resolved value replaces the source in the compiled output; no references remain

### Legacy `udp_enabled` synthesis (`compile.rs`)

| `udp_enabled` | `[listeners.udp]` | Result |
|---|---|---|
| None | None | No UDP config |
| None | Present | Uses udp section |
| true | None | Synthesizes defaults (requires socks5 protocol) |
| true | Present | Merges: udp section fields override defaults |
| false | None | No UDP config |
| false | Present + enabled=true | **Rejected** (conflict) |
| false | Present + enabled=false | Uses disabled udp config |

### UDP transport validation (`validate.rs`)

When any listener has UDP enabled:
- Upstream chains are checked via `classify_upstream_chain()`
- HTTP, SOCKS4, Trojan upstreams with UDP listener = error
- Multi-hop chains where not all hops are Socks5/Shadowsocks = error
- Default route to non-UDP group with UDP listener = error
- Rules with `transport = "udp"` matching non-UDP upstreams = error

### Composition matrix validation

`validate_config_composition()` loads `docs/parity/composition_matrix.toml` and
warns (not errors) when listener protocols have no composition cell for the
upstream's TCP/UDP capability. Opt-in and path-relative.

## Error & failure model

### ConfigError

| Variant | When |
|---|---|
| `Io(String)` | File read failure or size exceeded |
| `Parse(toml::de::Error)` | Invalid TOML syntax |
| `Validation { path, message }` | Structural/semantic error with location |
| `UnsupportedVersion(u32)` | Version != 1 |

Validation errors include a `path` string (e.g. `"listeners[1].tls.cert"`) for precise location in the config file.

### ConfigWarning

Non-fatal warnings emitted during `validate_config_security()`:
- Non-loopback listener bind without auth (and not shadowsocks/ssr/trojan)
- Non-loopback admin bind without auth
- Non-loopback reverse server control_bind without auth

## Configuration/features

- `toml` for parsing; `regex` for host/port regex compilation; `ipnet` for CIDR; `rcgen` (test only)
- Duration parsing: ns, us/micro-s, ms, s, m, h, d -- with overflow checking
- File size limit: 1 MB (`MAX_CONFIG_SIZE`) with TOCTOU guard (read one extra byte after stat)
- No `unsafe` code

## Security notes

- File size limit: 1 MB with TOCTOU guard (`file.take(MAX_CONFIG_SIZE + 1)`)
- Secrets resolved at compile time -- no secret-bearing strings remain in references
- Security warnings for non-loopback binds without auth (not errors, operator review required)
- TLS cert/key validated at compile time via `TlsServerConfigBuilder`
- Trojan requires TLS -- validated at structural level
- UDP transport validation prevents routing UDP through non-UDP-capable upstreams
- Match expression depth (10) and node count (100) limits prevent resource exhaustion

## Concurrency & lifecycle

- Entirely synchronous -- no async in any code path
- TLS cert/key are read from disk at compile time, not at listener bind time
- `RuntimeConfig` is `Clone` and can be shared across threads via `Arc`
- No interior mutability; all state is immutable after compilation

## Test coverage map

| Module | Test count | Key coverage |
|---|---|---|
| `lib.rs` | ~80 | Minimal config, full config, all sections, invalid TOML, unsupported version, invalid duration/URI, duplicate names/IDs, unknown references, combined legacy matchers rejected, recursive matchers (all/any_of/not/nested), leaf matchers (port range/set/identity/CIDR/regex), health config (all/partial/defaults/invalid), PAC/static content config, UDP config (nested/legacy synthesis/conflict/SOCKS5 requirement/transport validation), TLS listener config |
| `validate.rs` | ~15 | Zero durations rejected, loopback detection, security warnings (listener/admin/reverse), non-loopback without auth warned, loopback not warned, authed listener not warned |
| `compile.rs` | ~12 | Process/admin defaults, protocol compilation, reject reasons, matcher compilation, UDP mode compilation, health compilation, H2 config, reverse server/client compilation |
| **Total** | **107** | |

## Reviewer gotchas

- `validate_config` returns `Result<(), Vec<ConfigError>>` -- multiple errors collected in one pass; caller joins them into a single `ConfigError::Validation`.
- `validate_config_security` runs only after structural validation succeeds; warnings do not prevent loading.
- Legacy matcher fields and the recursive `match` field are mutually exclusive on a rule.
- `udp_enabled = true` with `[listeners.udp]` merges (udp section overrides defaults). Only `false` + `enabled = true` is a conflict.
- Combined legacy matchers (e.g. `host_exact` + `destination_port`) are rejected at validation time.
- `rules_file` routes all rules to a single upstream group; errors if multiple groups exist.
- `resolve_password` uses `std::env::var()` at compile time. Empty env var resolves to `Some("")` which is later rejected by auth compiler.
- QUIC/HTTP3 listeners require TLS; HTTP3 must be sole protocol; raw QUIC needs an application protocol.

## See also

- [overview.md](overview.md) -- system architecture
- [core.md](core.md) -- foundation types consumed by RuntimeConfig
- [uri.md](uri.md) -- upstream URI parsing used during compilation
- [routing.md](routing.md) -- CompiledRule and MatchExpr types
- [runtime.md](runtime.md) -- compile_runtime_snapshot consumes RuntimeConfig
- [server.md](server.md) -- listener/connection orchestration from config
