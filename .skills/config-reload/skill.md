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

## Reload flow
1. Candidate snapshot compiled from new TOML
2. Startup-captured listener changes rejected (any field above); only
   routing/upstream/group/health/PAC changes pass classification
3. Router swapped atomically via `ArcSwap`
4. Snapshot swapped via `Arc<ArcSwap<CompiledRuntimeSnapshot>>`
5. Health tasks stopped and restarted from new snapshot
6. Old state untouched on failure

## Key types
- `CompiledRuntimeSnapshot` — single authoritative runtime snapshot
- `RuntimeState` — shared state with snapshot, readiness, generation
- `ArcSwap<Router>` — lock-free router reads
- `AdminSnapshotProvider` — trait for admin to read live snapshot

## Adding a new config field
1. Add to TOML schema in `eggress-config/src/model.rs`
2. Add validation in `eggress-config/src/validate.rs`
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
