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
- UDP bind address
- Admin endpoint bind address

## Reload flow
1. Candidate snapshot compiled from new TOML
2. Unsupported topology changes rejected (listener count, bind addresses, names)
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
- Adding a field that should be hot-reloadable to listener config
- Not rejecting topology changes in the reload validator
- Forgetting to bridge new metrics into the Prometheus registry
