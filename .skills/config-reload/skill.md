# Config Reload and Atomic Swaps

## When to use
Use when modifying configuration schema, TOML parsing, hot-reload behavior, or the supervisor lifecycle.

## What is hot-reloadable (SIGHUP)
- Routing rules and rule engine
- Upstream definitions and groups
- Health probe configuration
- PAC and static content

## What is NOT hot-reloadable (requires restart)
- Listener topology (count, names, bind addresses)
- Listener behavior captured at startup: protocols, auth material, TLS
  material, Shadowsocks/Trojan config, `connection_limit`, `fixed_target`,
  `local_bind`, `reuse_port`
- All UDP listener settings (bind, mode, limits, timeouts, advertise, ...)
- Transparent/unix listener configuration
- Timeout configuration
- Admin endpoint bind address
- Reverse endpoint topology (servers/clients spawned once at startup)

## Reload flow (one canonical transaction)

`RuntimeState::apply_compiled_config(new_config)` owns all mutation.
File (`ServiceSupervisor::reload_config`), SIGHUP, and embed
(`reload_toml_str` / `reload_toml_file` / `reload_compiled`) differ only in
how `new_config` is obtained.

1. Classify via `classify_reload_config` on the live snapshot (reject any
   field above); only routing/upstream/group/health/PAC pass
2. `compile_runtime_snapshot(new, Some(prev))` (Arc reuse on identical
   chain+health); failure preserves generation + records `record_reload(false)`
3. Snapshot `store()` before `routing.swap_arc()`, then admin publish,
   `restart_health_probes()`, `H2_POOL_REGISTRY.clear()`,
   `set_config_generation` + `record_reload(true)`
4. Supervisor updates stored `rt_config` only on `Applied` (snapshot is
   authoritative for next classification)

## Startup (in-memory, no temp file)

`EggressConfig` stores compiled `RuntimeConfig` (canonical) + ancillary source
TOML. `EggressService::start/start_blocking` consume `into_compiled()` via
shared `startup_in_memory(rt, options)` → `start_from_config_with_options(rt,
None, _)`; `_config_path=None`, SIGHUP disabled. Native and compat startup
share the core; only `CompatibilityOptions` differ.

## Key types
- `CompiledRuntimeSnapshot` — single authoritative runtime snapshot
- `RuntimeState` — shared state with snapshot, readiness, generation
- `ArcSwap<Router>` — lock-free router reads
- `AdminSnapshotProvider` — trait for admin to read live snapshot

## Adding a new config field
1. Add to TOML schema in `eggress-config/src/model.rs`
2. Add validation in `eggress-config/src/validate/` (pick the owning submodule: `listeners`, `upstreams`, `rules`, `core`, `security`, `composition`; orchestration stays in `validate/mod.rs`)
3. Add compilation to runtime types in `eggress-config/src/compile.rs`
4. If hot-reloadable: ensure it's in `CompiledRuntimeSnapshot`
5. If NOT hot-reloadable: add topology validation rejection
6. Update example-config.toml

## Verification
- `cargo test -p eggress-config` — config parsing/validation
- `cargo test -p eggress-runtime reload` — reload integration tests
- Check that new fields have validation tests
- Check that reload tests cover the new field behavior

## Common mistakes
- Adding a listener field without a classification review: every
  startup-captured field must be rejected in `classify_listeners`
  (explicit comparison groups, not whole-struct equality)
- Not rejecting topology changes in the reload validator
- Claiming a setting is hot-reloadable when accept loops clone it at startup
- Forgetting to bridge new metrics into the Prometheus registry
