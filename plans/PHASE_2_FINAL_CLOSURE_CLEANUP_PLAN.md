# Phase 2 Final Closure Cleanup Plan

## Purpose

This plan closes the remaining precision and runtime-semantics issues after the Phase 2 final integration pass. The repository now has a credible Phase 2 runtime: shared compiled snapshot, pre-bound listeners, configured listener names, PAC/static admin wiring, route/health/admin integration tests, and tracked connection tasks.

The remaining work is intentionally small. Do not redesign the runtime. Do not start Phase 3 UDP until this cleanup is complete.

## Remaining issues to fix

1. `ServiceSupervisor::run()` panics on startup/runtime errors because it unwraps the async result with `.expect(...)`.
2. Bind-conflict behavior is not covered by a negative-path integration test.
3. Runtime generation is still duplicated between `CompiledRuntimeSnapshot.generation` and `RuntimeState.generation`.
4. `AdminState` still carries a stale `router` field alongside the live routing service.
5. PAC/static handlers appear to use startup-captured `Arc`s, so reload freshness is ambiguous or stale.
6. Admin route explanation accepts only target/listener/protocol and cannot explain source- or identity-based rules.
7. Health probe startup still passes `HealthConfig::default()` into `start_probes`; verify and enforce per-upstream compiled health config.
8. Admin is stopped before connection drain, so readiness/status/metrics are not available during the graceful-drain window.
9. README status text is internally inconsistent: it says Phase 2 corrective integration is complete while also saying integration tests are still needed.
10. Completion docs rely on code inspection for listener bind failure instead of an executable negative-path test.

## Non-goals

Do not add:

- UDP;
- TLS;
- new proxy protocols;
- new scheduler algorithms;
- persistent storage;
- admin authentication beyond existing policy;
- listener topology hot reload;
- system proxy integration.

---

# Workstream 1: Make supervisor run fallible instead of panicking

## Problem

The runtime block returns `Result<(), RuntimeError>` but `run()` ends with:

```rust
.expect("runtime error during startup")
```

This converts expected runtime failures, such as listener bind failure, into panics. Phase 2 should return structured runtime errors to the CLI.

## Required change

Change:

```rust
pub fn run(&mut self) {
    // ...
    rt.block_on(async move { ... }).expect("runtime error during startup");
}
```

to:

```rust
pub fn run(&mut self) -> Result<(), RuntimeError> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(RuntimeError::RuntimeInit)?;

    let result = rt.block_on(async move {
        // existing async body
        Ok::<(), RuntimeError>(())
    });

    // preserve post-run health extraction / cleanup if needed
    result
}
```

Add a convenience wrapper only if necessary:

```rust
pub fn run_or_exit(&mut self) {
    if let Err(error) = self.run() {
        tracing::error!(%error, "runtime failed");
        std::process::exit(1);
    }
}
```

Prefer updating the CLI to call `run()` and return a nonzero exit code on error.

## Required error variant

If not present, add:

```rust
pub enum RuntimeError {
    RuntimeInit(std::io::Error),
    ListenerBind { addr: String, source: std::io::Error },
    AdminBind { addr: String, source: std::io::Error },
    Config(String),
    // existing variants
}
```

## Required tests

- `run()` returns `Err(RuntimeError::ListenerBind { .. })` for a held listener port;
- no panic occurs;
- CLI-level test or unit test verifies nonzero exit behavior if a binary test is already available.

## Acceptance criteria

- no expected runtime startup error is surfaced as a panic.

---

# Workstream 2: Add bind-conflict integration test

## Problem

The completion record marks listener bind failure as verified by code inspection. This needs an executable test.

## Test pattern

Use a held OS listener on a dynamic port:

```rust
#[tokio::test]
async fn bind_conflict_aborts_startup_before_readiness() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = held.local_addr().unwrap();

    let config = format!(r#"
version = 1

[[listeners]]
name = "conflict"
bind = "{addr}"
protocols = ["http"]
"#);

    let file = write_config(&config);
    let path = file.path().to_str().unwrap();

    let mut supervisor = ServiceSupervisor::start(path).unwrap();
    let state = supervisor.state().clone();

    let result = tokio::task::spawn_blocking(move || supervisor.run())
        .await
        .unwrap();

    assert!(matches!(result, Err(RuntimeError::ListenerBind { .. })));
    assert!(!state.readiness.load(Ordering::Relaxed));

    drop(held);
}
```

If `ServiceSupervisor::start()` is changed to pre-bind listeners itself, assert the error there instead. The key requirement is that the failure is structured and readiness remains false.

## Avoid flakiness

- Bind `127.0.0.1:0` and keep the listener alive.
- Use the exact returned socket address in TOML.
- Drop the held listener at test end.
- Do not rely on sleeping.

## Acceptance criteria

- listener bind conflict is tested in CI;
- completion docs no longer cite code inspection only.

---

# Workstream 3: Remove duplicate generation state

## Problem

`CompiledRuntimeSnapshot` already stores `generation`, but `RuntimeState` also has `generation: Arc<AtomicU64>`. Admin reads the atomic rather than the current snapshot.

This duplicates authority and can drift.

## Required change

Remove:

```rust
pub generation: Arc<AtomicU64>
```

from `RuntimeState`.

Add helper:

```rust
impl RuntimeState {
    pub fn generation(&self) -> u64 {
        self.snapshot.load().generation
    }
}
```

Update admin status, config summary, route explanation, metrics update, and tests to read current snapshot generation.

On reload:

```rust
let new_generation = new_snapshot.generation;
state.snapshot.store(Arc::new(new_snapshot));
metrics.set_config_generation(new_generation);
```

Do not separately store generation in another atomic.

## SharedRoutingService consideration

If `SharedRoutingService` still has generation internally, remove it or ensure it is not used for admin/reporting. Ideally it should be reduced to an `ArcSwap<Arc<Router>>` or replaced by routing through `RuntimeState.snapshot`.

For this cleanup, it is acceptable to leave internal routing-service generation unused if deleting it is disruptive, but no external behavior should depend on it.

## Required tests

- status generation equals snapshot generation;
- route-explain generation equals snapshot generation;
- reload increments generation exactly once;
- failed reload leaves generation unchanged;
- no code reads a separate `RuntimeState.generation` field.

## Acceptance criteria

- there is one authoritative externally visible generation.

---

# Workstream 4: Remove stale router from admin state

## Problem

Admin receives both a startup-captured router and a live routing service. Handlers mostly use the live routing service, but the stale field remains a footgun.

## Required change

Remove from `AdminState`:

```rust
pub router: Option<Arc<Router>>
```

Retain only:

```rust
pub routing: Arc<SharedRoutingService>
```

or preferably:

```rust
pub runtime: Arc<RuntimeState>
```

Handlers should load live state per request.

## Required handler behavior

- `/-/routes` calls `state.routing.router()` or loads current snapshot.
- `/-/upstreams` calls the same live source.
- `/-/route-explain` calls the same live source.
- No admin endpoint uses startup-captured router data.

## Required tests

- reload changes a rule;
- `/-/routes` reflects the new rule;
- `/-/route-explain` reflects the new decision;
- grep/code test or compile failure ensures no `router:` field remains in `AdminState`.

## Acceptance criteria

- admin cannot accidentally report stale route state after reload.

---

# Workstream 5: Make PAC/static reload semantics explicit and correct

## Problem

Admin receives `static_routes` and `pac_config` as startup-captured `Arc`s. If reload changes PAC/static configuration, handlers may serve stale content.

## Preferred fix

Make admin read PAC/static from the current runtime snapshot on each request.

Recommended `AdminState` shape:

```rust
pub struct AdminState {
    pub runtime: Arc<RuntimeState>,
    pub metrics: Arc<MetricsRegistry>,
    pub readiness: Arc<AtomicBool>,
    // no startup-captured static_routes/pac_config
}
```

PAC handler:

```rust
let snapshot = state.runtime.snapshot.load();
let Some(admin) = snapshot.admin.as_ref() else { return 404; };
let Some(pac_config) = admin.pac.as_ref() else { return 404; };
let pac = generate_pac(pac_config);
```

Static handler:

```rust
let snapshot = state.runtime.snapshot.load();
if let Some(admin) = snapshot.admin.as_ref() {
    for route in &admin.static_content {
        if route.path == path {
            return serve_static(route);
        }
    }
}
```

If direct runtime dependency from admin crate to runtime crate would introduce a cycle, define a small admin-facing trait or snapshot provider interface in `eggress-admin`:

```rust
pub trait AdminSnapshotProvider: Send + Sync {
    fn snapshot(&self) -> AdminSnapshotView;
}
```

But avoid overengineering; a simple closure/data provider is enough.

## Alternative fix

If dynamic PAC/static reload is intentionally unsupported, then:

- document this explicitly;
- reject PAC/static changes on reload as restart-required;
- keep README wording precise.

Preferred path is live snapshot reads because routing reload already swaps the whole snapshot.

## Required tests

- initial PAC path serves configured PAC;
- initial static path serves configured body;
- reload changes PAC content and old admin server serves new content;
- reload changes static body and old admin server serves new content;
- invalid reload preserves old PAC/static content;
- reserved path collision remains rejected.

## Acceptance criteria

- PAC/static serving behavior matches README and completion docs.

---

# Workstream 6: Extend route explanation context

## Problem

`/-/route-explain` only accepts `target`, `listener`, and `protocol`. It always uses `source: None` and `identity: Anonymous`, so it cannot explain source- or identity-based rules.

## Required request schema

Support optional fields:

```json
{
  "target": "example.com:443",
  "listener": "local",
  "protocol": "socks5",
  "source": "192.0.2.10:54321",
  "identity": "alice"
}
```

Rules:

- `source` is optional and must parse as `SocketAddr` if present;
- `identity` is optional and maps to `ClientIdentity::Username(identity)`;
- empty identity is rejected;
- identity length is bounded, e.g. 256 bytes;
- no password field is accepted.

Suggested parser:

```rust
let source = match body.get("source").and_then(|v| v.as_str()) {
    Some(raw) => Some(raw.parse::<SocketAddr>().map_err(...)?),
    None => None,
};

let identity_buf;
let identity = match body.get("identity").and_then(|v| v.as_str()) {
    Some(raw) if raw.is_empty() || raw.len() > 256 => return 400,
    Some(raw) => {
        identity_buf = ClientIdentity::Username(raw.to_string());
        &identity_buf
    }
    None => &ClientIdentity::Anonymous,
};
```

## Required tests

- source-CIDR rule explanation changes when source changes;
- identity rule explanation changes when identity changes;
- invalid source returns 400;
- oversized identity returns 400;
- identity is never treated as a credential secret in output.

## Acceptance criteria

- route explanation can debug every matcher class exposed by TOML.

---

# Workstream 7: Verify and simplify health config use

## Problem

Supervisor calls `HealthManager::start_probes(&upstream_runtimes, &HealthConfig::default())`. This is ambiguous: either the manager ignores the passed config and uses each upstream’s compiled config, or per-upstream health settings are not actually applied.

## Required investigation

Inspect `HealthManager::start_probes`.

If it uses the passed `HealthConfig`, change it to use each upstream’s own config:

```rust
pub fn start_probes(&mut self, upstreams: &[Arc<UpstreamRuntime>]) {
    for upstream in upstreams {
        let config = upstream.health_config.clone();
        let probe = upstream.health_probe.clone();
        // spawn with config
    }
}
```

If it already uses the upstream config, remove the unused parameter for clarity:

```rust
hm.start_probes(&upstream_runtimes);
```

## Required tests

Use accelerated per-upstream health settings:

```toml
[upstreams.health]
interval = "50ms"
timeout = "50ms"
failures_to_unhealthy = 1
successes_to_healthy = 1
```

Tests:

- a failing upstream becomes unhealthy according to configured threshold, not default threshold;
- different upstreams can use different thresholds;
- reload changes health interval/threshold and the manager uses the new values;
- health config equality controls upstream runtime reuse correctly.

## Acceptance criteria

- no call site passes `HealthConfig::default()` unless it is truly the configured global default.

---

# Workstream 8: Decide admin availability during drain

## Current behavior

Shutdown stops admin before draining active connections. This is acceptable but means `/-/ready` and metrics are unavailable during drain.

## Preferred behavior

Keep admin alive until after connection drain.

Shutdown sequence:

```text
readiness=false
listener_cancel.cancel()
health_cancel.cancel()
wait listener tasks
wait active connections until deadline
if timeout: connection_cancel.cancel()
wait connection tasks
admin_cancel.cancel()
wait admin tasks
```

This allows:

- `/-/ready` returns 503 during drain;
- `/metrics` can show active sessions draining;
- `/-/status` remains useful.

## Acceptable alternative

Document current behavior explicitly:

```text
Admin endpoints shut down before connection draining begins. Readiness false is visible in-process but not served over HTTP during drain.
```

Given the presence of admin readiness tests, preferred behavior is to keep admin available.

## Required tests for preferred behavior

- create active long-lived session;
- trigger shutdown;
- while drain is in progress, query `/-/ready` and get 503;
- query `/metrics` and see active connection gauge/counter state;
- after drain/cancel, admin stops.

## Acceptance criteria

- operator-facing shutdown semantics are either implemented or explicitly documented.

---

# Workstream 9: Fix README and completion docs

## Required updates

- Remove “Integration tests still needed” from README if integration tests are now present.
- Add a note that listener topology reload is restart-required if that remains the chosen reload scope.
- Update `docs/PHASE_2_COMPLETION.md` after bind-conflict test exists.
- Remove code-inspection-only evidence for criterion 5.
- Record the cleanup commit(s) in the Phase 2 completion document and final integration plan.

## README status suggestion

```text
Status: Phase 2 complete — policy-driven routing with rule engine, upstream groups, health-aware scheduling, TOML configuration, metrics, admin API, PAC/static serving, scoped atomic reload, route explanation, and runtime supervisor.
```

If admin is not kept alive during drain, document:

```markdown
### Phase 2 operational limitations

- Listener topology changes require restart.
- Admin server is stopped during shutdown before active connection drain.
```

If preferred admin-during-drain behavior is implemented, omit the second limitation.

## Acceptance criteria

- docs match actual runtime behavior;
- no checklist item depends on “code inspection only.”

---

# Recommended commit sequence

## Commit 1: Fallible supervisor run and bind-conflict test

- Change `run()` to return `Result<(), RuntimeError>`.
- Update CLI or call sites.
- Add bind-conflict integration test.
- Update completion doc criterion 5 evidence.

## Commit 2: Single generation source and remove stale admin router

- Remove `RuntimeState.generation`.
- Make admin read `snapshot.load().generation`.
- Remove `router` field from `AdminState`.
- Update status/routes/upstreams/explain handlers.
- Add reload-generation test if needed.

## Commit 3: PAC/static live snapshot reads

- Make PAC/static handlers read current snapshot, or classify PAC/static reload as restart-required.
- Add reload tests for PAC/static behavior.

## Commit 4: Route explanation context and health config cleanup

- Add optional `source` and `identity` fields.
- Clean `HealthManager::start_probes` API or verify per-upstream config use.
- Add source/identity explanation tests and health-config threshold tests.

## Commit 5: Shutdown admin semantics and documentation closure

- Either keep admin alive through drain and test it, or document current behavior.
- Fix README status text.
- Update docs/PHASE_2_COMPLETION.md and plan completion record.
- Run final checks.

---

# Final verification commands

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Also run the external interoperability tests under the same environment used by CI:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_pproxy
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_curl
```

If the actual test names differ, update this plan and docs with the correct commands.

---

# Definition of done

This cleanup is complete only when:

1. `ServiceSupervisor::run()` returns structured errors instead of panicking.
2. Bind conflicts are covered by an executable integration test.
3. Admin/status/explain generation is read from the current snapshot, not a duplicate atomic.
4. `AdminState` no longer carries a stale startup router snapshot.
5. PAC/static reload behavior is implemented or explicitly documented as restart-required.
6. Route explanation supports optional source and identity fields.
7. Health probes demonstrably use compiled per-upstream health config.
8. Admin availability during drain is implemented or documented.
9. README no longer says integration tests are needed if they exist.
10. Completion docs no longer rely on code-inspection-only evidence for core runtime behavior.
11. All workspace tests, lint, audit, and interoperability checks pass.
12. No new unsafe code, OpenSSL dependency, or native dependency is introduced.

## Completion record

When complete, append:

```markdown
## Completion record

Implemented by commits:

- `<sha>` — fallible supervisor run and bind-conflict test
- `<sha>` — single generation source and admin live-state cleanup
- `<sha>` — PAC/static reload semantics and route explanation context
- `<sha>` — health config verification and shutdown/docs closure

All required checks passed on `<date>`.
```

---

# Completion record

Implemented by commits:

- `0b44f31` — fallible supervisor run and bind-conflict test
- `a91857c` — single generation source and admin live-state cleanup
- `e650e06` — PAC/static reload semantics and route explanation context
- `4a5265f` — health config verification and shutdown/docs closure
- `a5e09ad` — documentation closure (README, AGENTS, ARCHITECTURE, PHASE_2_COMPLETION)
- `25d173b` — WS7 required tests (different thresholds, reload changes config, health config ARC reuse)

All required checks passed on 2026-06-22.
