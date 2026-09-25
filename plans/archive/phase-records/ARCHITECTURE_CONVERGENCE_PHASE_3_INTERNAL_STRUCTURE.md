# Architecture Convergence Phase 3 — Internal Structure and Ownership

## Status

**IMPLEMENTED**

## Closure record

- Implementation commit: `46945f4` (single commit on `main`, on top of
  `28ae0eb` which closed Phase 2).
- Verification at closure: `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace
  --locked` (2704 passed), `cargo check --manifest-path fuzz/Cargo.toml
  --bins`, maturin build + `pytest python/tests tests/compat` (2273
  passed). No external oracle/interop suites (no compatibility behavior
  changed). `fuzz/Cargo.lock` pins refreshed to 1.0.4 as a side effect of
  the required fuzz check.

### Final runtime module map (`crates/eggress-runtime/src/`)

`supervisor.rs` is now the orchestration facade (`ServiceSupervisor` public
API, `CompatibilityOptions`, listener-prep dispatch, transport accept loops,
admin/signal orchestration) plus `supervisor/{accounting, connection,
operations, reload, shutdown, startup, state, udp_runtime}.rs`:

| Module | Responsibility |
|---|---|
| `accounting.rs` | `ListenerConnectionSlot`, `ActiveConnectionGuard`, accept-error backoff |
| `connection.rs` | `PreparedListener`/`PreparedQuicListener`, shared `wrap_tls_server()`, `build_connection_config()` (`ConnectionBuildParams`, `InboundSecurity`) |
| `operations.rs` | `RuntimeAdminListenerInfos` over the live snapshot |
| `reload.rs` | `ReloadResult`, `classify_listeners()`, `classify_reload_config()` |
| `shutdown.rs` | `ShutdownPlan` + `shutdown_ordered()` (ordering preserved exactly) |
| `startup.rs` | `init_supervisor()`, `resolve_udp_global_limit()`, `build_ssh_sessions()` |
| `state.rs` | `RuntimeState` + canonical `apply_compiled_config` transaction |
| `udp_runtime.rs` | `RuntimeUdpService`, `make_udp_service()`, `compute_advertise_ip()`, Shadowsocks relay prep |

Duplicated TLS wrapping + `ConnectionConfig` assembly across
standard/transparent/Unix/QUIC paths is replaced by the shared helpers
(~250 lines removed from the accept paths; `supervisor.rs` 3811 -> ~2500
lines including tests).

### Metrics ownership (Model A — subsystem counters canonical)

Subsystem atomics own UDP relay/standalone, Shadowsocks, H2, and transparent
counters; the registry mirrors them by saturating-delta promotion at render
time (retained necessarily: `prometheus_client::Counter` is increment-only).
Prometheus `Family` objects stay canonical for labeled metrics (route
decisions, upstream open/failure, H2 streams). `SessionMetrics` narrowed to
6 session methods; new `RuntimeMetrics` trait (10 methods) owns
reload/generation/platform/unix/transparent/UDP-association/exposition.
Genuine duplicate removed: `RuntimeUdpService` no longer increments the
registry UDP total directly (it double-counted with the relay
subsystem+bridge path). Direct record methods remain as documented
unbridged fallbacks, never called alongside a bridge for the same event.
`eggress-metrics/src/` is now
`labels/registry/session/runtime/udp/shadowsocks/h2/render/tests` + facade.

### Canonical compatibility issue type

`eggress_pproxy_compat::issues::CompatIssue { severity, code, category,
feature, feature_id, tier, message, suggestion }` with
`IssueSeverity::{Warning, Unsupported, Info}`. `TranslationOutput`,
`CombinedTranslation`, and `NativeTranslation` store `issues: Vec<CompatIssue>`;
`warnings()`/`unsupported()`/`diagnostics()` are views. The two drifted
unsupported-feature tables in `diagnostics.rs` are unified into one
(canonical `classify_unsupported_feature`; code/tier helpers delegate), and
the missing `trojan-listener` arm was added. `translate.rs` is now
`translate/{entry, intermediates, model, rules, native, toml_render,
tests}` + facade.

### Final PyO3 module map (`crates/eggress-python/src/`)

`lib.rs` (registration only) + `errors` (exceptions + `map_error`) +
`service` (`PyEggressConfig/Service/Handle`) + `connection` (compat
`Connection` + counters) + `compat` (URI/diagnostics/translation/explain
surface) + `outbound` (connector/stream) + `system_proxy` + `runtime`
(shared Tokio runtime). Exported names, exception hierarchy, and abi3
metadata unchanged (49 symbols; 2273 Python tests pass).

### WS5 extractions

`eggress-config/src/validate/` (`composition`, `listeners`, `upstreams`,
`rules`, `core`, `security` + orchestrator + tests);
`eggress-server/src/accept/` (`handlers`, `forward`, `detect`, `prefixed` +
facade + tests); `eggress-server/src/execute/` (`hops` + facade + tests).
No new crates; no public API or behavior change.

### Deliberately rejected / deferred

- A common accept-loop trait across TCP/transparent/Unix/QUIC: the socket
  APIs differ (`TcpListener::accept` vs transparent inner accept vs Unix vs
  QUIC `run`/`accept_connection`); unification would add complexity for no
  behavior gain. Shared per-connection behavior (accounting/TLS/config/UDP)
  is unified instead.
- QUIC/H3 `ConnectionConfig` construction stays inline: the H3 path builds
  a `PendingTunnel` + `execute()` directly (no `serve_connection`), so it
  does not share the stream-listener shape.
- Prometheus `Counter` mirrors were not replaced by direct exposition:
  that would be a metrics schema redesign, explicitly out of scope.
- `H2ConnectionLabels` retained as a public (currently unused) label struct
  rather than removed, to avoid shrinking the public metrics surface in a
  maintenance phase.

## Baseline

- Repository: `eggstack/eggress`
- Roadmap: `plans/ARCHITECTURE_CONVERGENCE_ROADMAP.md`
- Dependency: Phase 2 canonical startup/reload/translation paths complete

## Objective

Reduce responsibility concentration and duplicate state ownership inside the largest implementation modules without changing crate topology or adding product features.

This phase is intentionally an internal-maintenance pass. The desired result is smaller review units, explicit ownership boundaries, and fewer places where the same state or diagnostic concept is represented independently.

The main targets are:

- `eggress-runtime/src/supervisor.rs`;
- `eggress-metrics/src/lib.rs`;
- `eggress-pproxy-compat/src/translate.rs` and diagnostics/warnings;
- `eggress-server/src/accept.rs` and `execute.rs` where responsibilities are clearly separable;
- `eggress-python/src/lib.rs`;
- config validation modules if extraction directly improves ownership clarity.

Do not split code merely to hit arbitrary file-size targets. Every module extraction must correspond to a coherent responsibility with a small, explicit interface.

---

## Workstream 1 — Decompose runtime supervision by lifecycle responsibility

### Problem

`eggress-runtime/src/supervisor.rs` currently owns a large share of the runtime lifecycle: startup preparation, listener construction, reload classification/application, health integration, admin publication, standard/transparent/Unix/QUIC accept loops, UDP relay preparation, compatibility system proxy handling, reverse server/client startup, connection task construction, shutdown, and assorted accounting helpers.

This concentration makes it difficult to verify invariants such as listener-state ownership, cancellation ordering, connection accounting, and reload publication.

### Target module boundaries

Use internal modules under `eggress-runtime`; do not add crates.

A suitable decomposition may resemble:

```text
runtime/
  supervisor.rs       orchestration facade + public ServiceSupervisor API
  startup.rs          compiled-config -> prepared runtime state
  listeners/
    mod.rs             common prepared-listener model and dispatch
    tcp.rs             standard TCP accept loop
    unix.rs            Unix socket accept loop
    transparent.rs     transparent listener accept loop
    quic.rs            QUIC/H3 listener lifecycle
  connection.rs       build ConnectionConfig + shared per-connection wrapper
  reload.rs           classification + canonical apply transaction
  udp_runtime.rs      runtime-side UDP service/relay wiring
  operations.rs       admin/system-proxy integration where feature-gated
  reverse_runtime.rs  existing reverse runtime adapter/startup wiring
  shutdown.rs         task tracker/cancellation/drain ordering if extraction helps
```

Exact names may differ. The important property is responsibility separation.

### Implementation tasks

1. Extract reload classification/application first because Phase 2 should already have established a canonical transaction.
2. Introduce a shared per-connection preparation function/struct for fields currently duplicated across standard, transparent, Unix, QUIC/H3 paths.
3. Extract TLS server wrapping into one helper used by every stream listener that supports the same TLS semantics.
4. Extract construction of `eggress_server::ConnectionConfig` into one helper where protocol-specific differences can be supplied explicitly.
5. Keep transport-specific accept mechanics in transport-specific modules; do not force Unix/transparent/QUIC into an artificial common trait if that adds more complexity than it removes.
6. Preserve task/cancellation ownership and shutdown ordering exactly unless a focused regression test proves an existing defect.
7. Preserve `ServiceSupervisor` public API to avoid downstream churn.
8. Update `architecture/runtime.md` when file/module ownership changes.

### Required tests

Existing runtime lifecycle, reload, transparent, Unix, QUIC/H3, UDP, admin, reverse, and shutdown tests should continue to exercise the extracted modules. Add new tests only where extraction exposes an invariant that previously lacked coverage.

Required explicit invariants:

- every accepted standard/Unix/transparent stream increments/decrements active connection accounting exactly once;
- connection limits continue to apply per listener;
- cancellation stops new accepts before connection drain/cancellation;
- TLS listener wrapping is behaviorally unchanged;
- generation/context values originate from the same active snapshot for new connections;
- feature-disabled builds do not gain unconditional dependencies through module movement.

### Acceptance criteria

- `supervisor.rs` is reduced to a readable orchestration facade rather than containing full implementations for every listener type;
- duplicated standard/Unix/transparent connection-config construction is materially reduced;
- no new public crate or generic plugin abstraction is introduced;
- existing lifecycle behavior remains unchanged.

---

## Workstream 2 — Establish one metrics ownership model

### Problem

Metrics are currently partly owned by subsystem-native atomics/counters (`UdpMetrics`, Shadowsocks metrics, H2 state, transparent counters) and partly duplicated into `MetricsRegistry`. Bridging code snapshots subsystem counters and computes deltas using `*_prev_*` state. At the same time, the `eggress_server::SessionMetrics` trait includes operations that are not session concerns, such as reload and platform-capability metrics and Prometheus rendering.

### Decision required before code movement

Choose one of these two bounded ownership models based on the least disruptive implementation:

**Model A — subsystem counters canonical:** protocol/runtime subsystems own counters; `eggress-metrics` reads/snapshots those counters directly when rendering/collecting, without maintaining duplicate cumulative mirrors where unnecessary.

**Model B — metrics sink canonical:** subsystems emit metric events through small narrowly scoped sink traits; `MetricsRegistry` owns the Prometheus counters directly.

Do not retain a hybrid for a metric family unless there is a documented reason, such as an external subsystem that cannot depend on the metrics crate.

### Preferred direction

Model A is likely lower risk for existing UDP/Shadowsocks atomics because those counters already have runtime consumers and can remain dependency-light. For session/runtime events that are naturally emitted at a single point, direct sink methods may remain appropriate. The result can therefore use more than one transport mechanism, but each metric family must have one canonical stored value.

### Implementation tasks

1. Inventory every metric family and record its canonical owner.
2. Separate interfaces by domain:
   - session events;
   - runtime/reload/platform events;
   - protocol subsystem snapshots;
   - rendering/export.
3. Narrow `SessionMetrics` to session/route/upstream/auth events actually required by `eggress-server`, or rename/split it if necessary while preserving dependency direction.
4. Move runtime-only metrics calls to a runtime-facing metrics interface owned outside the server session trait.
5. Remove `*_prev_*` delta mirrors where rendering can directly expose canonical monotonically increasing counters.
6. If Prometheus `Family` objects must remain canonical for labeled metrics, keep those families in the registry and have the subsystem emit events rather than also owning parallel labeled totals.
7. Split `eggress-metrics/src/lib.rs` into coherent internal modules such as registry setup, labels, session metrics, UDP bridge/export, Shadowsocks export, H2 export, and rendering.
8. Preserve metric names and labels unless a currently incorrect metric is explicitly documented. This phase is not a metrics schema redesign.
9. Update `architecture/metrics.md` with ownership rules.

### Required tests

- existing metric names and help/type output remain stable;
- cumulative counters do not double-count after repeated renders/snapshots;
- gauge values track current active state correctly;
- reload and platform metrics remain available without passing through session-only interfaces;
- repeated Prometheus renders are idempotent with respect to stored totals;
- feature-gated subsystem metrics disappear/compile cleanly when the relevant feature is disabled.

### Acceptance criteria

- every metric family has a documented canonical owner;
- unnecessary previous-snapshot/delta state is removed;
- `eggress-server` no longer owns runtime/admin rendering concerns through an overly broad session trait;
- Prometheus metric names remain compatible.

---

## Workstream 3 — Consolidate compatibility diagnostics into one typed issue model

### Problem

The compatibility crate currently has free-form diagnostic helper strings, a typed `DiagnosticCode`/`StructuredDiagnostic` model, and separate string-category `CompatWarning`/`UnsupportedFeature` types that are later converted back into structured diagnostics. This duplicates classification and makes documentation/JSON/human output easier to drift.

### Target design

Use one typed compatibility issue representation carrying:

- severity/disposition (`warning`, `unsupported`, `info` as needed);
- stable diagnostic code;
- optional capability/feature id;
- compatibility tier where relevant;
- redacted human-readable message;
- optional suggestion/remediation.

Human text, JSON output, warning collection, and CLI rendering should be views over that model.

### Implementation tasks

1. Define the canonical typed issue representation in the compatibility crate.
2. Convert parser/translator call sites to construct typed issues directly rather than string category tags.
3. Make `TranslationOutput` carry typed issues. Preserve compatibility helper accessors if public callers rely on separate warnings/unsupported collections, but implement them as views/filters rather than independently stored objects.
4. Fold platform diagnostic helpers into constructors/renderers over typed codes.
5. Keep stable diagnostic code spellings used by JSON/tests.
6. Ensure all messages use canonical redaction helpers from Phase 1.
7. Split `translate.rs` by semantic area only after the typed result boundary is stable, for example arguments/listener translation/upstream translation/rules/reverse/renderer.
8. Update `architecture/pproxy-compat.md` and active diagnostic documentation.

### Required tests

- JSON diagnostic code output remains stable;
- warning vs unsupported classification remains stable for representative inputs;
- existing suggestions remain present where part of tests/docs;
- no credential-bearing input leaks into any rendered issue;
- TOML renderer does not become responsible for semantic classification;
- capability/feature IDs map consistently to the active manifest where applicable.

### Acceptance criteria

- one stored typed model represents compatibility issues;
- free-form string category dispatch is eliminated from active semantic paths;
- human and JSON diagnostics are generated from the same model;
- `translate.rs` is decomposed into coherent semantic modules without changing compatibility behavior.

---

## Workstream 4 — Decompose PyO3 binding implementation by public surface

### Problem

The Rust Python binding implementation places service/config, outbound connector/stream, compatibility `Connection`, error mapping, system proxy, runtime ownership, and module registration in one large source file.

### Target module boundaries

Possible internal layout:

```text
eggress-python/src/
  lib.rs              module registration only
  errors.rs           Python exception declarations + mapping
  service.rs          PyEggressConfig/Service/Handle
  outbound.rs         PyOutboundConnector/Stream
  connection.rs       compatibility Connection object/state
  system_proxy.rs     apply/restore wrapper
  runtime.rs          shared Python outbound Tokio runtime helper
```

Do not move Python-level policy out of `python/eggress` into Rust merely to reduce source size.

### Implementation tasks

1. Move exception declarations/mapping first so every module imports one canonical error mapper.
2. Move shared runtime initialization into an internal runtime helper.
3. Separate native service lifecycle and outbound APIs.
4. Keep pproxy-compatibility `Connection` behavior isolated from native service/outbound classes.
5. Keep module registration and exported names unchanged.
6. Preserve abi3 compatibility and package metadata.
7. Update `architecture/python-bindings.md`.

### Required tests

- compiled module exports exactly the expected public symbols;
- Python exception inheritance/mapping remains stable;
- service, outbound, connection, and system-proxy smoke tests pass;
- wheel build and abi3 smoke behavior remain unchanged.

### Acceptance criteria

- `lib.rs` becomes primarily module registration/re-export glue;
- binding domains can be reviewed independently;
- no public Python import path changes.

---

## Workstream 5 — Focused server/config decomposition where it removes duplication

This workstream is intentionally secondary. Only perform extractions that directly reduce duplicated logic or clarify a frequently changed responsibility.

Candidate extractions:

- `eggress-server::accept`: authentication reuse/cache, protocol detection, protocol-specific accept handlers, shared prefixed stream;
- `eggress-server::execute`: route opening/metadata, tunnel execution, HTTP-forward execution, UDP-associate execution, chain-executor construction;
- `eggress-config::validate`: composition validation, listener validation, upstream/group/rule validation.

Constraints:

1. Do not rewrite stable protocol parsers solely to make files shorter.
2. Preserve error types and public exports where practical.
3. Prefer private modules and re-exports to downstream API churn.
4. Stop once responsibilities are clear; file-size reduction is not itself an acceptance criterion.

### Acceptance criteria

- any extracted module has one coherent responsibility and tests remain colocated or discoverable;
- no new cross-crate dependency cycle or public abstraction is introduced;
- code movement does not obscure protocol-specific behavior behind generic machinery.

---

## Phase verification

At phase closure run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-runtime
cargo test -p eggress-metrics
cargo test -p eggress-server
cargo test -p eggress-config
cargo test -p eggress-pproxy-compat
cargo test -p eggress-python
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Run the Python smoke suite after PyO3 source movement.

No external oracle/interoperability suite is required unless behavior changes rather than code ownership.

## Phase acceptance criteria

Phase 3 is complete only when:

- runtime listener/reload/UDP/operations/reverse responsibilities are split into coherent internal modules;
- duplicated connection preparation among listener types is materially reduced;
- metrics ownership is explicitly documented and duplicate delta mirroring is reduced;
- runtime-only metrics concerns are not exposed through an unnecessarily broad session interface;
- compatibility diagnostics use one typed stored model;
- pproxy translation and PyO3 binding implementation are decomposed by semantic/public surface;
- no new crates are added solely for decomposition;
- no public feature expansion occurs;
- workspace and Python tests pass with unchanged external behavior.

## Closure record

When implemented, update this file in place with:

- implementation commit range;
- final runtime module map;
- chosen metrics ownership model and any retained exceptions;
- canonical compatibility issue type;
- final PyO3 module map;
- any candidate decomposition deliberately rejected because it increased complexity.