# Phase 2 Gap Closure Plan

Closes 3 remaining "Definition of done" criteria and fills the completion record.

## Workstreams

### WS 1: Snapshot refactor (Criterion 1)

**Goal**: `CompiledRuntimeSnapshot` owns listeners, admin content (PAC/static), and health config — not just router + upstreams.

**Changes to `crates/eggress-runtime/src/snapshot.rs`**:
1. Add `listeners: Vec<CompiledListener>` and `admin: Option<CompiledAdmin>` to `CompiledRuntimeSnapshot`
2. `CompiledListener` stores: `name`, `bind`, `protocols`, `auth`, `handshake_timeout`, `connection_limit`
3. `CompiledAdmin` stores: `bind`, `pac_config`, `static_routes`, `enabled`
4. `compile_runtime_snapshot()` compiles these from `RuntimeConfig`

**Changes to `crates/eggress-runtime/src/supervisor.rs`**:
1. `ServiceSupervisor::start()`: build snapshot, extract listeners from it, bind them, build admin state from it
2. `reload_config()`: build new snapshot, atomically swap, admin content comes from snapshot
3. Remove duplicated listener/admin compilation logic from `run()`

---

### WS 2: Remove dual generation counter (Criterion 7)

**Goal**: `RuntimeState.generation` is the single authoritative generation counter.

**Changes to `crates/eggress-routing/src/lib.rs`**:
1. Remove `generation` field from `RoutingServiceInner`
2. Remove `pub fn generation(&self)` from `SharedRoutingService`
3. Remove generation increment from `swap()` and `swap_arc()`
4. Simplify `new()` and `new_arc()`
5. Remove/update 4 tests that assert on `service.generation()`

---

### WS 3: Integration test — health behavioral (Criterion 17)

**New file**: `crates/eggress-runtime/tests/health.rs`

Tests: health_affects_routing, reload_preserves_health_state, upstream_status_reports_healthy_and_unhealthy

---

### WS 4: Integration test — PAC/static (Criterion 17)

**New file**: `crates/eggress-runtime/tests/pac_static.rs`

Tests: pac_endpoint_serves_valid_javascript, static_content_serves_configured_files, pac_not_configured_returns_404

---

### WS 5: Integration test — shutdown behavioral (Criterion 17)

**Replace**: `crates/eggress-runtime/tests/shutdown.rs` (replace unit tests with behavioral tests)

Tests: graceful_shutdown_drains_connections, shutdown_force_cancels_after_deadline, shutdown_stops_accepting_new_connections, shutdown_cleans_up_all_tasks

---

### WS 6: Fill completion record

Fill in the plan file's completion record with actual commit SHAs and date.
