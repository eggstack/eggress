# Phase 3 UDP Corrective Closure Plan

## Purpose

Phase 3 now has a real UDP foundation: SOCKS5 UDP ASSOCIATE is parsed and authenticated, a UDP relay address is returned, the SOCKS5 UDP datagram codec is implemented, direct UDP forwarding works against a local echo server, UDP packets pass through transport-aware routing, and basic UDP metrics/admin/docs exist.

The remaining work is a corrective closure pass. The current implementation is good enough to prove the design, but it still has lifecycle, configurability, task supervision, metrics, and documentation gaps that should be closed before moving to any next major phase.

This plan is tailored for smaller implementation models. Keep the commits small. Do not redesign the entire UDP subsystem. Do not add upstream UDP relay, QUIC/MASQUE, transparent UDP, or new protocols in this pass.

---

# Current gaps to close

1. UDP association registry slots are not clearly removed when an association closes.
2. Association idle timeout is not visible in the relay loop.
3. Target-flow idle cleanup is not visible in the relay loop.
4. UDP relay tasks are spawned with `tokio::spawn` and are not tracked by runtime task trackers.
5. UDP limits are hard-coded via `UdpLimits::default()` and not TOML-driven.
6. UDP relay bind address is hard-coded to `127.0.0.1:0`.
7. UDP advertised relay address is just the bound socket address and is not listener-aware.
8. UDP metrics are split between `eggress_udp::metrics::UdpMetrics` and `eggress_metrics::MetricsRegistry`; ensure `/metrics` exposes real UDP relay counters.
9. UDP routing calls `decide()` only and treats every `UpstreamGroup` as unsupported, so direct fallback is not honored.
10. UDP reload semantics are documented, but listener-level UDP config is too minimal to make those semantics meaningful.
11. Phase 3 completion docs overstate closure around idle timeout, registry cleanup, task waiting, and configurable limits.

---

# Non-goals

Do not implement:

- UDP through SOCKS5 upstreams;
- UDP through HTTP, SOCKS4, Shadowsocks, Trojan, SSH, QUIC, MASQUE, or CONNECT-UDP;
- UDP transparent proxying;
- fragmentation/reassembly beyond rejecting nonzero SOCKS5 FRAG;
- multicast/broadcast forwarding;
- DNS policy special cases beyond ordinary UDP target handling;
- system proxy mutation;
- unsafe Rust;
- OpenSSL or native dependencies.

---

# Desired final state

After this pass:

- every association is removed from the registry exactly once on close;
- active association gauges/counts return to zero;
- idle associations close automatically;
- idle target flows are evicted automatically;
- UDP relay tasks are tracked or joined through association/runtime handles;
- UDP bind/advertise/limits are configurable per listener;
- reload applies safe UDP config changes to new associations and rejects bind/topology changes explicitly;
- `/metrics` exposes real UDP association, packet, byte, drop, decode-error, and target-flow metrics;
- UDP direct fallback behavior is explicit and tested;
- docs accurately describe what is implemented and what remains intentionally unsupported.

---

# Workstream 1: Association lifecycle and registry cleanup

## Problem

`UdpAssociationRegistry::remove(id)` exists, but runtime/relay closure paths do not visibly call it. The relay loop calls `association.close()`, and `execute_udp_associate` cancels the handle, but the registry may retain a closed association. That will make `active_count()` stale and can exhaust association limits.

## Required design

Make registry ownership explicit. The component that creates an association must also arrange removal when the relay/control lifecycle ends.

Preferred option: include registry cleanup in the association handle created by runtime.

```rust
pub struct RuntimeUdpAssociationHandle {
    pub id: UdpAssociationId,
    pub relay_addr: SocketAddr,
    pub cancel: CancellationToken,
    pub done: JoinHandle<()>,
}
```

The relay task should run a cleanup block:

```rust
let assoc_id = assoc.id;
let registry = registry.clone();
let relay_task = task_tracker.spawn(async move {
    let result = udp_relay_loop(...).await;
    assoc.close();
    registry.remove(assoc_id).await;
    result
});
```

If the server-facing `UdpAssociationHandle` cannot contain a `JoinHandle`, use a background cleanup task:

```rust
let assoc_for_cleanup = assoc.clone();
let registry_for_cleanup = registry.clone();
tokio::spawn(async move {
    assoc_for_cleanup.closed_notify.notified().await;
    registry_for_cleanup.remove(assoc_for_cleanup.id).await;
});
```

The first option is preferred because it enables task tracking.

## Idempotency

`remove(id)` should be safe to call more than once. Keep it idempotent.

## Required tests

- create association, close via TCP control close, registry active count returns to zero;
- close via runtime shutdown, registry active count returns to zero;
- close via idle timeout, registry active count returns to zero;
- double close does not panic and does not underflow metrics;
- after close/removal, global association limit slot is reusable in a runtime-level test.

## Acceptance criteria

- no closed association remains in `UdpAssociationRegistry` except through local `Arc`s held by in-flight cleanup.

---

# Workstream 2: Association idle timeout

## Problem

`UdpAssociation` tracks `last_activity`, but the relay loop only selects on UDP receive, response channel, and cancellation. There is no visible idle-timeout branch.

## Required behavior

An association closes after `limits.idle_timeout` without valid client or target activity.

Add a periodic tick to the relay loop:

```rust
let mut idle_tick = tokio::time::interval(limits.idle_check_interval());
idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

loop {
    tokio::select! {
        _ = idle_tick.tick() => {
            if association.last_activity().elapsed() >= config.limits.idle_timeout {
                tracing::debug!(association_id = ?association.id, "UDP association idle timeout");
                config.udp_metrics.record_association_timeout();
                break;
            }
        }
        // existing recv/response/cancel branches
    }
}
```

If `Instant::elapsed()` is awkward around mutex handling, expose:

```rust
pub fn is_idle(&self, now: Instant, timeout: Duration) -> bool;
```

## Activity definition

Call `association.touch()` for:

- valid client datagram after successful client-pin validation;
- target response sent back to client.

Do not update activity for rejected packets from the wrong client address.

Do update activity for well-formed packets that are dropped due to route policy? Recommended: yes, because the association is actively used by its owner. But record a drop metric.

## Required tests

Use short TOML/runtime limits:

- association closes after idle timeout without packets;
- repeated valid packets extend lifetime;
- rejected wrong-client packets do not extend lifetime;
- policy-rejected packets either extend or do not extend according to documented decision;
- registry cleanup occurs after idle timeout.

## Acceptance criteria

- idle timeout is enforced by runtime behavior, not only stored in config.

---

# Workstream 3: Target-flow idle cleanup

## Problem

Target flows are added to a `HashMap` and stay there until the association closes. This makes `max_targets_per_association` a lifetime cap rather than an active-flow cap.

## Required behavior

Each target flow should close after `limits.target_idle_timeout` without activity.

Add periodic cleanup:

```rust
fn reap_idle_flows(
    flows: &mut HashMap<String, TargetFlowEntry>,
    now: Instant,
    timeout: Duration,
    metrics: &UdpMetrics,
) {
    flows.retain(|_, entry| {
        let keep = now.duration_since(entry.flow.last_activity()) < timeout;
        if !keep {
            entry.recv_task.abort();
            metrics.record_target_flow_closed();
        }
        keep
    });
}
```

Make the task handle field non-underscore if it is intentionally used:

```rust
struct TargetFlowEntry {
    flow: UdpTargetFlow,
    recv_task: JoinHandle<()>,
}
```

## Flow activity

Update flow activity on:

- client payload sent to target;
- response received from target.

## Required tests

- flow is created on first packet;
- flow is reused for repeated packets to same target;
- flow is evicted after target idle timeout;
- after eviction, a new flow to same target can be created;
- target-flow count metric decrements after eviction;
- target limit slot is reusable after eviction.

## Acceptance criteria

- `max_targets_per_association` bounds active flows, not lifetime history.

---

# Workstream 4: Track UDP relay tasks and wait on shutdown

## Problem

The runtime spawns relay loops with `tokio::spawn`, not via a runtime or UDP task tracker. Completion docs claim shutdown waits for UDP tasks, but the code should make that true.

## Required design

Add UDP task tracking to runtime state or service:

```rust
pub struct RuntimeState {
    // existing
    pub udp_tasks: TaskTracker,
}
```

or to `RuntimeUdpService`:

```rust
struct RuntimeUdpService {
    // existing
    udp_tasks: TaskTracker,
}
```

Spawn relay loops with:

```rust
udp_tasks.spawn(async move {
    let result = udp_relay_loop(...).await;
    registry.remove(assoc_id).await;
    if let Err(error) = result {
        tracing::debug!(%error, association_id = ?assoc_id, "UDP relay ended with error");
    }
});
```

Shutdown sequence should include:

```rust
state.udp_registry.close_all().await;
state.udp_tasks.close();
let _ = tokio::time::timeout(shutdown_grace, state.udp_tasks.wait()).await;
```

Do not close the tracker permanently if new associations can be created after reload. Close it only during process shutdown.

## Required tests

- active UDP relay task exits after TCP control close;
- active UDP relay task exits after runtime shutdown;
- shutdown waits for UDP task completion;
- no association remains active after shutdown;
- no UDP task leak after repeated association create/close.

## Acceptance criteria

- UDP relay tasks are supervised like TCP connection tasks.

---

# Workstream 5: TOML UDP listener configuration

## Problem

The current TOML model has only `udp_enabled: bool`. UDP limits and relay bind/advertise are hard-coded.

## Required TOML shape

Support nested listener UDP config while preserving backward compatibility with `udp_enabled = true` for now.

```toml
[[listeners]]
name = "socks-in"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
advertise = "127.0.0.1"
idle_timeout = "60s"
target_idle_timeout = "30s"
max_associations = 1024
max_targets_per_association = 64
max_datagram_size = 65535
client_pin = true
```

## Runtime config type

```rust
pub struct ListenerUdpConfig {
    pub enabled: bool,
    pub bind: SocketAddr,
    pub advertise: Option<IpAddr>,
    pub idle_timeout: Duration,
    pub target_idle_timeout: Duration,
    pub max_associations: usize,
    pub max_targets_per_association: usize,
    pub max_datagram_size: usize,
    pub client_pin: bool,
}
```

In compiled listener:

```rust
pub udp: Option<ListenerUdpConfig>
```

Retain `udp_enabled` only as compatibility sugar:

- if `udp_enabled = true` and `[listeners.udp]` absent, synthesize default UDP config;
- if both are present and disagree, reject config.

## Validation

- UDP config is allowed only when listener protocols include `socks5`;
- `max_datagram_size` must be between SOCKS5 header minimum and 65535;
- `max_associations` and `max_targets_per_association` must be > 0;
- idle timeouts must be positive;
- UDP bind must parse as socket address;
- advertise must parse as IP address;
- if TCP listener bind is non-loopback and UDP enabled without auth, warn or reject according to current security posture.

## Required tests

- nested UDP config parses;
- compatibility `udp_enabled = true` still works;
- nested config overrides defaults;
- conflicting `udp_enabled = false` with `udp.enabled = true` rejects;
- UDP config on non-SOCKS5 listener rejects;
- invalid durations/limits reject;
- compiled listener contains expected UDP limits.

## Acceptance criteria

- UDP behavior is operator-configurable per listener.

---

# Workstream 6: UDP relay bind and advertise behavior

## Problem

Every association binds `127.0.0.1:0` and advertises that address. This works for local tests but fails remote/non-loopback listeners.

## Required behavior

Use compiled listener UDP config for relay socket bind and reply address.

When creating an association, pass listener UDP config into the service:

```rust
udp_service.create_association(
    listener_name,
    client_tcp_peer,
    identity,
    generation,
    listener_udp_config,
)
```

or make one `RuntimeUdpService` per listener with baked config.

Preferred per-listener service:

```rust
struct RuntimeUdpService {
    listener: String,
    udp_config: ListenerUdpConfig,
    registry: Arc<UdpAssociationRegistry>,
    // ...
}
```

Bind:

```rust
let relay_socket = UdpSocket::bind(udp_config.bind).await?;
```

Advertise:

```rust
let local = relay_socket.local_addr()?;
let advertised_ip = udp_config
    .advertise
    .unwrap_or_else(|| derive_advertise_ip(tcp_listener_local_addr, local.ip()));
let advertised_addr = SocketAddr::new(advertised_ip, local.port());
```

Return advertised address in SOCKS5 reply, not necessarily the local bind address.

## Derivation rules

- If `advertise` is configured, use it.
- Else if UDP bind IP is not unspecified, use UDP bind IP.
- Else if TCP listener local IP is not unspecified, use TCP listener local IP.
- Else if TCP peer is loopback, use loopback matching address family.
- Else return a config error requiring explicit `advertise`.

Avoid advertising `0.0.0.0` to clients by default.

## Required tests

- default loopback UDP bind advertises 127.0.0.1;
- configured advertise IP appears in SOCKS5 reply;
- UDP bind port from config is used where nonzero;
- UDP bind conflict fails association or startup according to selected strategy;
- unspecified non-loopback case without advertise is rejected or documented;
- IPv6 advertise works where platform supports it.

## Acceptance criteria

- the SOCKS5 UDP relay address is usable by clients under the configured deployment model.

---

# Workstream 7: Metrics unification

## Problem

UDP relay currently records into `eggress_udp::metrics::UdpMetrics`, while the main Prometheus registry is `eggress_metrics::MetricsRegistry`. Ensure real relay packet/drop/decode/flow counts are visible in `/metrics`.

## Preferred approach

Move all runtime-visible UDP metric recording into `eggress_metrics::MetricsRegistry`.

Replace `RelayConfig` fields:

```rust
pub udp_metrics: Arc<UdpMetrics>
```

with:

```rust
pub metrics: Arc<eggress_metrics::MetricsRegistry>
```

Then record labels centrally:

```rust
metrics.record_udp_packet_up(listener, rule_id, action, bytes);
metrics.record_udp_packet_down(listener, rule_id, action, bytes);
metrics.record_udp_drop(listener, reason);
metrics.record_udp_decode_error(listener, kind);
metrics.record_udp_target_flow_created(listener);
metrics.record_udp_target_flow_closed(listener);
```

If keeping `UdpMetrics` internally, bridge it into `MetricsRegistry::render_prometheus()` and prove that `/metrics` exposes it. But avoid two metric sources unless there is a compelling reason.

## Required metric labels

Allowed:

- listener;
- rule ID or `unknown`;
- action: direct, reject, unsupported_upstream, drop;
- reason;
- decode error kind.

Forbidden:

- target host/IP;
- source IP;
- username;
- payload content;
- arbitrary error string.

## Required tests

- UDP echo increments `/metrics` packet and byte counters;
- decode error increments `/metrics` decode error counter;
- reject rule increments `/metrics` drop counter;
- target-flow creation/closure visible;
- active association gauge returns to zero after close;
- rendered metrics do not contain target address or client source address.

## Acceptance criteria

- `/metrics`, not only internal structs, reflects live UDP relay behavior.

---

# Workstream 8: UDP routing fallback semantics

## Problem

The relay calls `RouteService::decide()` and treats any `UpstreamGroup` decision as unsupported. That is correct for upstream UDP support, but it bypasses group fallback semantics such as direct fallback.

## Required decision

Choose one of these explicit semantics:

### Preferred

For UDP, evaluate full route selection through `route()` and then handle:

- `SelectedRoute::Direct { selection_reason: Normal }` => direct UDP;
- `SelectedRoute::Direct { selection_reason: DirectFallback }` => direct UDP with fallback metric;
- `SelectedRoute::Upstream { .. }` => unsupported upstream UDP drop;
- rejected route => drop with policy metric.

This preserves Phase 2 fallback behavior.

### Acceptable minimum

Document that UDP routing uses rule decisions only and does not honor upstream-group direct fallback. If this option is chosen, README and completion docs must say so explicitly.

Preferred implementation:

```rust
let selected = match config.routing.route(&route_request) {
    Ok(selected) => selected,
    Err(RouteError::Rejected { rule, .. }) => { drop_policy(rule); continue; }
    Err(_) => { drop_no_route(); continue; }
};

match selected {
    SelectedRoute::Direct { selection_reason, decision } => { ... direct ... }
    SelectedRoute::Upstream { group, upstream, .. } => { drop_unsupported_upstream(...); }
}
```

Be careful: `route()` may create a pending lease for upstream selection. If UDP upstreams are unsupported, do not leak or unnecessarily increment counters. If `route()` creates leases, either:

- add a UDP-specific `route_datagram()` that applies fallback but does not lease upstreams; or
- ensure dropping `SelectedRoute::Upstream` immediately releases pending lease.

## Required tests

- UDP direct rule forwards;
- UDP reject rule drops;
- UDP upstream group with fallback direct forwards and records direct-fallback reason;
- UDP upstream group with fallback reject drops;
- UDP upstream group with use-unhealthy still drops as unsupported if selected upstream is UDP-unsupported;
- no upstream active/in-flight counter leak for unsupported UDP upstream decision.

## Acceptance criteria

- UDP fallback behavior is explicit, tested, and documented.

---

# Workstream 9: Admin UDP endpoint correctness

## Required behavior

`GET /-/udp` should report live data from the same registry used by runtime associations.

Minimum JSON fields:

```json
{
  "associations_active": 0,
  "target_flows_active": 0,
  "listeners": [
    {
      "name": "socks-in",
      "udp_enabled": true,
      "active_associations": 0
    }
  ]
}
```

Do not expose client IPs or target IPs by default.

## Required tests

- before association: active count 0;
- during active association: active count 1;
- after TCP close: active count 0;
- per-listener counts distinguish listeners;
- target-flow active count reflects flow creation and cleanup;
- endpoint does not include client source or target address.

## Acceptance criteria

- admin UDP visibility is live and privacy-preserving.

---

# Workstream 10: Documentation and completion status correction

## Problem

`docs/PHASE_3_COMPLETION.md` currently marks several items complete that are not fully supported by the inspected code.

## Immediate doc correction

Until this plan is complete, change status to:

```text
Phase 3 UDP corrective closure in progress
```

Update `docs/PHASE_3_COMPLETION.md` or add a note:

```markdown
## Corrective closure status

The UDP foundation is implemented, but final closure is pending registry cleanup, idle-timeout enforcement, TOML UDP limits/bind/advertise configuration, and metrics unification.
```

## Final docs after implementation

Update:

- `README.md` UDP checklist;
- `docs/ARCHITECTURE.md` UDP section;
- `docs/PHASE_3_COMPLETION.md`;
- example config showing nested `[listeners.udp]`;
- metrics reference if present.

## Required accuracy

Do not mark as complete unless backed by code and tests:

- idle timeout;
- target-flow cleanup;
- association registry cleanup;
- configurable limits;
- relay bind/advertise;
- task waiting;
- `/metrics` visibility.

## Acceptance criteria

- Phase 3 docs match executable behavior.

---

# Recommended commit sequence

## Commit 1: Association cleanup and task tracking

- Add UDP task tracker.
- Ensure relay task removes registry entry on exit.
- Ensure TCP control close leads to registry cleanup.
- Add tests for active count returning to zero.

## Commit 2: Idle timeout and target-flow reaping

- Add association idle timeout branch.
- Add target-flow idle cleanup.
- Add tests for timeout and target slot reuse.

## Commit 3: TOML UDP config model

- Add nested `[listeners.udp]` model.
- Compile to `ListenerUdpConfig`.
- Preserve compatibility with `udp_enabled = true`.
- Add validation and config tests.

## Commit 4: UDP bind/advertise behavior

- Bind relay sockets according to listener UDP config.
- Advertise configured/derived usable address.
- Add loopback and configured-advertise tests.

## Commit 5: Metrics unification

- Route UDP relay metrics into `eggress_metrics::MetricsRegistry` or bridge clearly.
- Add `/metrics` tests for live UDP packet/drop/flow counters.

## Commit 6: UDP routing fallback semantics

- Decide and implement fallback semantics.
- Add direct-fallback/unsupported-upstream tests.
- Ensure no lease/counter leaks.

## Commit 7: Admin UDP endpoint hardening

- Ensure live registry counts.
- Add target-flow count if practical.
- Add privacy tests.

## Commit 8: Documentation and closure record

- Correct README and completion docs.
- Add example config.
- Run final checks.

---

# Required regression tests

Add tests under:

```text
crates/eggress-runtime/tests/udp.rs
crates/eggress-udp/tests/udp_integration.rs
crates/eggress-config/tests/udp_config.rs
```

Required scenarios:

1. Association removed from registry after TCP close.
2. Association removed after idle timeout.
3. Global association limit slot reusable after close.
4. Target-flow slot reusable after target idle timeout.
5. Nested UDP TOML config compiles to expected limits.
6. `udp_enabled = true` compatibility still works.
7. UDP bind/advertise config appears in SOCKS5 reply.
8. Non-loopback/unspecified advertise behavior is rejected or documented.
9. UDP echo increments `/metrics` packet and byte counters.
10. Decode error increments `/metrics` decode error counter.
11. Reject route increments drop counter.
12. UDP direct fallback behavior matches documented semantics.
13. Admin `/-/udp` active count returns to zero after close.
14. Runtime shutdown waits for UDP relay task completion.

Avoid arbitrary sleeps where possible. Use readiness polling, short configured timeouts, and bounded polling with clear deadlines.

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

Run UDP-focused tests explicitly:

```bash
cargo test -p eggress-udp
cargo test -p eggress-runtime udp
cargo test -p eggress-config udp
```

Run existing external interoperability gates if available:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_pproxy
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_curl
```

If UDP-specific pproxy interoperability is added, document and run:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_pproxy_udp
```

---

# Definition of done

Phase 3 UDP corrective closure is complete only when:

1. UDP association registry entries are removed on every close path.
2. Active association counts return to zero after TCP close, idle timeout, and shutdown.
3. Association idle timeout is enforced by the relay loop.
4. Target-flow idle timeout is enforced and frees target slots.
5. UDP relay tasks are tracked and waited on during shutdown.
6. UDP listener limits are TOML-configurable.
7. UDP relay bind address is TOML-configurable.
8. UDP relay advertised address is TOML-configurable or safely derived.
9. Existing `udp_enabled = true` compatibility remains supported or is migrated with documentation.
10. `/metrics` exposes live UDP packet, byte, drop, decode-error, association, and target-flow metrics.
11. UDP routing fallback behavior is explicit and tested.
12. Admin `/-/udp` reports live registry state without exposing client/target addresses by default.
13. Reload semantics for UDP config are precise and tested.
14. Phase 3 completion docs no longer overstate unsupported behavior.
15. All workspace tests, lint, audit, and applicable interoperability checks pass.
16. No unsafe Rust, OpenSSL dependency, or native dependency is introduced.

## Completion record

When complete, append:

```markdown
## Completion record

Implemented by commits:

- `<sha>` — association cleanup, idle timeout, target-flow reaping
- `<sha>` — UDP TOML config, bind, and advertise behavior
- `<sha>` — metrics/admin/routing fallback integration
- `<sha>` — docs, tests, and final closure

All required checks passed on `<date>`.
```
