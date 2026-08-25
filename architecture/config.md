# eggress-config — TOML Schema, Validation, Compilation

Turns user TOML into a validated, compiled `RuntimeConfig`. This is the only
place the configuration surface is defined; CLI flags, embed API, and Python
bindings all funnel through it.

## Module map

| File | Role |
|---|---|
| `src/model.rs` | Serde types mirroring the TOML schema (listeners, upstreams, groups, rules, timeouts, process, admin, reverse) |
| `src/lib.rs` | Public entry points, leaf matcher definitions (host, port range/set, CIDR, listener, protocol, identity), recursive matchers (`all` / `any_of` / `not`) |
| `src/compile.rs` | Validation → compiled `RuntimeConfig`; resolves secret sources; CLI-flag compatibility compilation |
| `src/validate.rs` | Structural validation (duplicate IDs, unknown group/rule references, bad URIs/durations/regex/CIDR) and security warnings (non-loopback binds without auth, zero durations) |
| `src/file.rs` | File loading helpers |
| `src/error.rs` | Structured config errors |

## Key behaviors

- Secrets resolve from inline values, environment variables, or files at
  compile time; compiled output carries resolved values only.
- Compilation is total: anything invalid fails before any socket is bound.
- Security warnings are generated during validation (e.g., auth-less listener
  on non-loopback bind); Shadowsocks-suppressed warnings handled explicitly.
- The legacy `udp_enabled` flag is synthesized into `[listeners.udp]` unless
  both are present and disagree (then rejected).

## Interactions

- `RuntimeConfig` → `eggress-runtime::compile_runtime_snapshot()` builds the
  live snapshot ([runtime.md](runtime.md)).
- `eggress-admin` reads `AdminConfig` for PAC/static content.
- pproxy translation emits TOML that must validate against this schema.

## Review entry points

- `example-config.toml` at repo root demonstrates every section.
- Verify: `cargo test -p eggress-config` (80+ inline tests),
  `cargo test -p eggress-config --test fuzz_smoke`.
