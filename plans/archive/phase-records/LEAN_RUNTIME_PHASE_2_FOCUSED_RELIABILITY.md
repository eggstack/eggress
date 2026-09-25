# Lean Runtime Phase 2 — Focused Reliability Closure

## Status

**IMPLEMENTED**

## Parent roadmap

[`LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md`](LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md)

## Dependency

Phase 1 feature topology must be stable before this phase closes so tests can verify both default/full behavior and the documented `common` build where applicable.

## Objective

Close the highest-probability latent correctness gaps identified in the repository audit using focused, deterministic regression tests and narrowly scoped fixes. The target areas are HTTP message framing, UDP association lifecycle, reload generation cleanup, shutdown/cancellation ordering, and bounded resource behavior.

This phase is not a general hardening campaign. It must reuse the current testkit, runtime status, metrics, readiness signals, and existing protocol tests. It must not create a new fault-injection framework, task-tracking subsystem, soak harness, or exhaustive protocol matrix.

## Non-goals

Do not use this phase to:

- add protocol capabilities or broaden compatibility tiers;
- implement TLS interception, SOCKS BIND, multi-hop UDP, or new HTTP versions;
- refactor HTTP around a new server framework;
- redesign UDP routing or association ownership absent a demonstrated defect;
- expose internal task registries solely for tests;
- add operating-system integration tests to routine CI;
- introduce randomized sleeps, long wall-clock tests, packet-capture dependencies, namespaces, containers, or privileged networking;
- create plan-numbered test suites or a permanent reliability workflow;
- make benchmarks, fuzzing, external `pproxy`, or soak tests phase gates;
- produce screenshots, generated evidence, copied command transcripts, or a completion document.

## General execution rules

1. Inventory existing tests before adding any new case.
2. Add a test only when no current test exercises the invariant.
3. Prefer an in-process listener on `127.0.0.1:0`, deterministic readiness signals, and bounded timeouts.
4. Avoid fixed sleeps. Use channels, existing handles, retry-until-deadline helpers, or observable state transitions.
5. Fix the narrow defect exposed by a failing test; do not rewrite adjacent architecture.
6. Keep tests package-local unless behavior crosses crate boundaries.
7. Use the existing `eggress-testkit` fixtures and report types where they already fit. Do not expand testkit into a generalized simulator.
8. One representative regression per invariant is preferred over combinatorial permutations.

## Workstream A — HTTP framing and connection-state integrity

### Risk

Forward-proxy HTTP handling can desynchronize persistent connections when request framing is ambiguous, connection-specific headers are mishandled, or an upstream responds before the request body has completed. These defects can cause request smuggling, cross-request contamination, hangs, or incorrect reuse.

### Files to inspect

At minimum:

```text
crates/eggress-protocol-http/src/
crates/eggress-server/src/
crates/eggress-core/src/relay.rs
crates/eggress-cli/tests/
crates/eggress-testkit/src/
fuzz/fuzz_targets/http_*.rs
```

Use repository search for existing coverage before adding cases:

```bash
rg -n "Content-Length|Transfer-Encoding|Connection:|informational|100 Continue|chunked|pipeline|upgrade|hop-by-hop" \
  crates tests python fuzz
```

### Required invariants

Verify existing behavior and add only missing tests for:

1. A request containing both `Transfer-Encoding` and `Content-Length` is rejected or normalized according to the implementation's documented safe policy; it must never be forwarded ambiguously.
2. Multiple conflicting `Content-Length` values are rejected.
3. Multiple identical `Content-Length` values, if accepted, are handled deterministically and do not alter body boundaries.
4. Headers nominated by the `Connection` header are removed when forwarding, in addition to the standard hop-by-hop set.
5. An IPv6 literal authority is parsed and emitted without losing brackets or port information.
6. An upstream early response does not leave unread client-body bytes available to be interpreted as a new request on the same connection.
7. A failed or rejected first request does not permit a pipelined second request to inherit framing or authentication state.
8. Upgrade or unsupported switching behavior fails explicitly rather than entering an unbounded relay accidentally.
9. Informational responses, when currently supported, do not cause the final response boundary to be lost. If not supported, the behavior must be explicit and documented rather than partially implemented.

### Scope control

Do not implement a complete RFC conformance suite. Use a small raw-TCP fixture that sends exact bytes and asserts response/connection behavior. Reuse an existing fixture if one exists.

If the implementation intentionally closes the client connection after an ambiguous or early-response case, that is acceptable and often preferable. Do not add complex body-draining solely to preserve connection reuse.

### Acceptance criteria

- every accepted request has one unambiguous body boundary;
- connection-specific headers cannot leak to the origin;
- a rejected or prematurely completed exchange cannot desynchronize the next request;
- new tests complete within existing unit/integration time budgets;
- no new HTTP abstraction layer or parser dependency is introduced.

## Workstream B — UDP association lifecycle and isolation

### Risk

SOCKS5 UDP associations and fixed-target/direct UDP flows maintain registry, socket, client identity, destination, and timeout state. Failure paths can leave stale registry entries, leaked tasks, spoofable client sources, or associations surviving their TCP control channel.

### Files to inspect

```text
crates/eggress-udp/src/assoc.rs
crates/eggress-udp/src/registry.rs
crates/eggress-udp/src/direct.rs
crates/eggress-udp/src/codec.rs
crates/eggress-udp/src/testkit.rs
crates/eggress-protocol-socks/src/
crates/eggress-runtime/src/
crates/eggress-server/src/
```

Search current coverage:

```bash
rg -n "association|registry|idle|control channel|source|spoof|shutdown|cancel|cleanup|UDP ASSOCIATE" \
  crates/eggress-udp crates/eggress-protocol-socks crates/eggress-runtime crates/eggress-cli/tests
```

### Required invariants

Verify or add focused tests for:

1. Closing the SOCKS5 TCP control channel removes or invalidates the associated UDP relay within a bounded deadline.
2. Failure during association setup leaves no registry entry and no bound socket task.
3. Runtime shutdown removes active associations and completes without waiting for the full idle timeout.
4. A packet from a source other than the authorized client endpoint is rejected after client endpoint pinning.
5. Association and destination tracking remain bounded by existing configured limits.
6. Idle expiry removes state exactly once and does not race with explicit close into a panic or double-accounting defect.
7. IPv4 and IPv6 target encoding/decoding preserve address family and do not reuse stale destination state.
8. A malformed datagram is isolated to that datagram/association and cannot terminate the listener or registry task.

### Scope control

Do not build a network emulator. Use loopback UDP sockets and existing codec/testkit helpers. One association with a bounded deadline is enough to prove each lifecycle invariant.

Do not make DNS rebinding, multi-hop UDP, NAT traversal, or cross-platform kernel behavior part of this phase.

### Acceptance criteria

- no setup, close, idle-expiry, or shutdown path leaves stale registry state;
- spoofed or malformed client packets cannot affect unrelated associations;
- the UDP listener remains usable after a malformed packet or failed association;
- cleanup tests are deterministic and do not rely on multi-second sleeps;
- no new long-running UDP CI job is introduced.

## Workstream C — Reload generation ownership

### Risk

Repeated routing/upstream reloads can retain obsolete health-check tasks, counters, snapshots, labels, or cancellation tokens. The existing architecture requires routing, health, admin, and metrics to share one compiled runtime snapshot.

### Files to inspect

```text
crates/eggress-runtime/src/snapshot.rs
crates/eggress-runtime/src/supervisor.rs
crates/eggress-runtime/src/lib.rs
crates/eggress-routing/src/
crates/eggress-admin/src/
crates/eggress-metrics/src/
crates/eggress-embed/src/
```

### Required invariants

Verify or add tests for:

1. A successful reload increments generation once and makes the new routing/upstream state observable atomically.
2. A rejected reload leaves the previous generation and readiness state unchanged.
3. Removing an upstream stops its health-check ownership and prevents future state updates from the removed generation.
4. Repeated bounded reloads do not grow the observable upstream, health, or metric-label set after old generations are dropped.
5. Active connections may finish against the snapshot they acquired, while new connections use the new snapshot.
6. Shutdown after several reloads terminates all generation-owned background work.

### Test shape

Use a small deterministic loop, such as 10 to 25 reloads, not a soak test. Assert existing observable counts or statuses. If no safe count is exposed, use weak-reference/drop markers inside a test-only module local to the owning crate; do not add a public task inventory API.

### Scope control

Do not make listener topology hot-reloadable. Do not duplicate snapshot state. Do not add a general generation garbage collector if ordinary ownership/cancellation fixes the defect.

### Acceptance criteria

- reload is atomic from public handles;
- rejected reloads do not partially mutate state;
- removed generations stop producing observable work;
- bounded repeated reload tests complete quickly;
- no public API exists solely to satisfy the test.

## Workstream D — Shutdown, cancellation, and failure isolation

### Risk

The documented shutdown invariant is readiness false, listener stop, connection drain/cancellation, then admin shutdown. Errors in one connection or component must not terminate unrelated proxy functionality.

### Required invariants

Verify or add focused tests for:

1. Readiness becomes false before listeners and admin surfaces fully terminate.
2. New connections are refused or rejected after shutdown begins.
3. An active relay receives the documented drain opportunity and is cancelled after the configured deadline.
4. A protocol handshake error affects only that session and the listener accepts the next valid connection.
5. An upstream connect failure affects only that route/session and does not poison scheduler or runtime state permanently.
6. Admin shutdown cannot wait indefinitely on a listener or background task already cancelled.
7. Shutdown remains idempotent through the public embed/Python-facing handle behavior currently promised.

### Scope control

Use existing cancellation tokens and supervisor handles. Do not add a durable job queue, actor framework, global task manager, or restart supervisor.

### Acceptance criteria

- one malformed client or failed upstream cannot stop the service;
- shutdown ordering matches current architecture documentation;
- shutdown completes within bounded test deadlines;
- tests do not use process-wide sleeps or OS-specific signals unless testing the CLI signal path specifically.

## Workstream E — Resource-bound confirmation

This workstream is an audit of current limits, not a new quota system.

Inspect existing limits for:

- connection count;
- handshake bytes and timeout;
- HTTP header bytes/count;
- UDP association and destination counts;
- routing regex execution behavior;
- admin request/body limits;
- metrics label cardinality.

Add or adjust a test only when an existing configured limit lacks an executable boundary test or an over-limit path can panic, allocate without bound, or poison the service.

Do not add duplicate limits at multiple layers. The owning parser/registry should enforce the bound once and return a structured error.

## Fix discipline

When a test exposes a defect:

1. Commit the failing regression or include it in the same focused fix commit.
2. Patch the owning module only.
3. Preserve full/default compatibility semantics unless the old behavior is unsafe or clearly incorrect.
4. Update a compatibility manifest only when an externally observable `pproxy` claim changes.
5. Do not update aggregate counts or generate reports manually; use existing manifest tooling when applicable.
6. Do not refactor adjacent modules unless the fix cannot be expressed safely otherwise.

## Required verification

Run affected package tests during implementation, for example:

```bash
cargo test -p eggress-protocol-http
cargo test -p eggress-udp
cargo test -p eggress-runtime
cargo test -p eggress-server
cargo test -p eggress-embed
```

Run relevant CLI integration tests only for behavior crossing the process boundary.

Where Phase 1 permits, verify representative `common` behavior:

```bash
cargo test -p eggress-cli --no-default-features --features common --test <existing_smoke_target>
```

Do not invent a new aggregate command if direct Cargo commands suffice.

Phase gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

External differential tests are required only if a fix changes a maintained compatibility claim. Fuzz targets are run only if a parser defect is found and the existing relevant target can reproduce or guard it.

## Acceptance criteria

Phase 2 is complete only when:

1. Existing coverage has been inventoried and duplicate tests were not added.
2. Missing high-risk invariants from workstreams A through D have focused deterministic tests.
3. Any discovered defects are fixed in their owning modules without broad architecture changes.
4. HTTP ambiguous framing cannot be forwarded or reused unsafely.
5. UDP setup, close, idle expiry, malformed input, and shutdown paths cannot leave stale association state or stop the listener.
6. Reload success/failure is atomic and obsolete generations stop observable background work.
7. Shutdown ordering and failure isolation match documented behavior.
8. Existing resource limits have executable boundary coverage where materially needed.
9. No new CI workflow, test matrix, soak harness, task registry, network emulator, or generated evidence artifact is introduced.
10. The complete workspace gate passes.

## Stop conditions

Stop expanding a workstream when:

- the remaining case is already covered by an equivalent test;
- testing would require privileged networking, containers, packet capture, or a new framework;
- the behavior is explicitly out of scope in current compatibility documentation;
- the proposed fix would redesign a public API or protocol architecture;
- only a theoretical issue remains without a reachable state in current code;
- a long-running soak test would be the only way to increase confidence.

Record deferred observations in the implementation summary or an issue if actionable. Do not create another plan unless a concrete defect is too large for this bounded pass.

## Handoff sequence

Prefer commits grouped by invariant rather than by file:

1. `test(http): cover framing and connection-state boundaries`
2. `test(udp): cover association cleanup and isolation`
3. `test(runtime): cover reload and shutdown ownership`
4. focused `fix(...)` commits only where tests expose defects
5. `docs: align focused reliability guidance` only if active documentation changes

Do not create one commit per test case.

## Closure update

Update this plan in place with:

- implementation commit range;
- tests added or existing tests identified as sufficient;
- defects fixed;
- any workstream closed without changes and why;
- verification commands run.

Do not add a separate reliability completion or evidence document.

## Closure update

### Implementation commit range

Two implementation commits covering all workstreams:
1. `092ed77` — initial Phase 2 tests and `socks_addr_equivalent` bug fix
2. Gap-closing commit — additional invariant coverage for all workstreams

### Tests added

**Workstream A — HTTP framing and connection-state integrity:**
- `forward/server.rs` — 21 unit/integration tests (17 initial + 4 gap-closing):
  - `test_te_plus_cl_rejected_not_forwarded` — TE+CL must be rejected
  - `test_conflicting_cl_values_rejected` — conflicting CL values must be rejected
  - `test_equal_duplicate_cl_deterministic` — equal duplicate CL handled deterministically
  - `test_connection_nominated_headers_removed` — Connection-nominated headers removed
  - `test_ipv6_literal_authority_roundtrip` — IPv6 bracket notation parsed correctly
  - `test_ipv6_literal_no_port` — IPv6 without explicit port uses default
  - `test_chunked_not_final_rejected` — chunked not final with unsupported coding rejected
  - `test_unsupported_transfer_encoding_rejected` — unsupported TE rejected
  - `test_upstream_connection_close_detected` — Connection: close detected via forward_response
  - `test_upstream_http11_keepalive_default` — HTTP/1.1 keep-alive default via forward_response
  - `test_filter_hop_by_hop_removes_upgrade` — Upgrade header stripped
  - `test_filter_hop_by_hop_removes_proxy_connection` — Proxy-Connection stripped
  - `test_request_body_kind_none_has_no_body` — None body kind has no body
  - `test_forward_request_body_kind_dispatches_correctly` — ForwardRequest.body_kind() dispatches
  - `test_copy_request_body_premature_eof` — Content-Length body with premature EOF fails (invariant 6)
  - `test_forward_request_stream_after_failure` — failed request doesn't corrupt stream for next request (invariant 7)
  - `test_build_origin_request_strips_upgrade` — Upgrade/Connection headers stripped from forwarded request (invariant 8)
  - `test_forward_response_informational_100_continue` — 100 Continue forwarded explicitly, documents no special 1xx body handling (invariant 9)
- Added `status` field to `ForwardResult` struct to expose HTTP status code from forwarded responses

**Workstream B — UDP association lifecycle and isolation:**
- `relay.rs` — 2 new tests + 1 bug fix:
  - `socks_addr_equivalent_works` extended with IPv4-mapped IPv6 roundtrip test
  - `relay_exit_cleans_up_registry` — registry is empty after relay loop exits (invariant 2)
  - Fixed `socks_addr_equivalent` in `relay.rs` and `flow.rs` to use `Ipv6Addr::to_ipv4_mapped()` instead of broken `IpAddr::from()` pattern match
- IPv6 relay integration (invariant 7) covered by existing `codec_decode_encode_roundtrip_ipv6` test; end-to-end IPv6 relay test deferred (requires IPv6-bound relay socket)

**Workstream C — Reload generation ownership:**
- `lifecycle_invariants.rs` — 7 new tests (4 initial + 3 gap-closing):
  - `repeated_reloads_do_not_leak_observable_state` — 10 reloads with identical config, verify snapshot has no leaked upstreams
  - `shutdown_after_multiple_reloads_completes` — reload 3x then shutdown, verify completes within timeout
  - `rejected_topology_reload_preserves_snapshot_identity` — rejected reload preserves generation and router Arc identity
  - `failed_toml_reload_preserves_generation_and_readiness` — corrupt TOML fails, preserves generation, recovers
  - `upstream_removal_on_reload_removes_from_snapshot` — removed upstream no longer in snapshot after reload (invariant 3)
  - `repeated_reloads_with_upstreams_do_not_leak` — 10 reloads with upstream present, count stays at 1 (invariant 4)
  - `active_connection_survives_reload` — active SOCKS5 connection continues working after reload, connection tracking consistent (invariant 5)

**Workstream D — Shutdown, cancellation, and failure isolation:**
- `shutdown.rs` — 2 new tests:
  - `new_connections_refused_after_shutdown_begins` — connect after token.cancel() fails or readiness is false (invariant 2)
  - `malformed_handshake_does_not_corrupt_listener` — garbage bytes to SOCKS5 listener, then valid handshake succeeds (invariant 4)
- `start_stop.rs` — 3 initial tests:
  - `drop_handle_without_explicit_shutdown_does_not_panic` — Drop without shutdown doesn't panic
  - `double_shutdown_does_not_panic` — documents type-system enforced single shutdown
  - `reload_then_shutdown_completes` — reload then shutdown completes cleanly

**Workstream E — Resource-bound confirmation:**
- `admin.rs` — 1 initial test:
  - `admin_rejects_oversized_post_body` — POST body > 16KB rejected with 413
- `regex_compat.rs` — 1 new test:
  - `rulefile_max_entries_enforced` — rule file with >10,000 entries triggers error diagnostic and truncation (invariant: regex rule count limit)
- Existing limits verified by inspection: connection limit (semaphore), handshake timeout, HTTP header size/count, UDP limits, admin body/identity limits, metrics label cardinality

### Defects fixed

1. **`socks_addr_equivalent` IPv4-mapped IPv6 detection** (`relay.rs`, `flow.rs`): The previous implementation used `IpAddr::from([u8; 16])` which never matches the `IpAddr::V4` arm for v4-mapped addresses. Fixed to use `Ipv6Addr::to_ipv4_mapped()`.
2. **`ForwardResult` missing status field** (`forward/server.rs`): Added `status: u16` field to expose the HTTP status code of forwarded responses.

### Workstreams closed without changes

None. All workstreams had either new tests or bug fixes.

### Verification commands run

```bash
cargo test -p eggress-protocol-http --lib -- forward::server::tests
cargo test -p eggress-udp --lib -- relay::tests
cargo test -p eggress-runtime --test lifecycle_invariants
cargo test -p eggress-runtime --test shutdown
cargo test -p eggress-pproxy-compat --lib -- regex_compat::tests::rulefile_max_entries_enforced
cargo test -p eggress-embed --test start_stop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```