# Phase 3 Detailed Plan: UDP Foundation and SOCKS5 UDP ASSOCIATE

## Objective

Phase 3 adds the UDP foundation required for practical `pproxy` parity while preserving the Phase 1/2 architecture: explicit protocol parsing, shared routing snapshots, bounded resource use, structured observability, and safe runtime supervision.

The first UDP milestone must support:

- SOCKS5 UDP ASSOCIATE server behavior;
- direct UDP forwarding;
- UDP association lifecycle management;
- target-aware response demultiplexing;
- listener/session limits and idle expiry;
- UDP routing through the Phase 2 route engine;
- UDP metrics, admin visibility, and structured logs;
- TOML configuration for UDP behavior;
- interoperability tests against standard clients and Python `pproxy` where feasible.

UDP forwarding must be implemented defensively. UDP is connectionless, easily spoofed in some environments, and can amplify traffic if implemented carelessly. Phase 3 should therefore prioritize association ownership, target validation, bounded packet sizes, idle expiry, and response filtering over broad protocol coverage.

---

# Scope

## Included

- SOCKS5 UDP ASSOCIATE server command parsing;
- SOCKS5 UDP request/reply datagram codec;
- IPv4, IPv6, and domain-name targets in SOCKS5 UDP datagrams;
- direct UDP egress socket support;
- UDP association registry;
- association idle timeout;
- max associations per listener and process;
- max target mappings per association;
- target-aware upstream response demux;
- optional client address pinning for UDP packets;
- route-engine integration for UDP requests;
- reject-policy handling for UDP targets;
- UDP-specific metrics;
- admin visibility for active associations;
- graceful shutdown of UDP associations;
- TOML configuration for UDP enablement and limits;
- direct-mode UDP tests using local echo servers;
- SOCKS5 UDP integration tests;
- documentation and checklist updates.

## Excluded

Do not add in this phase:

- UDP relay through HTTP CONNECT;
- UDP relay through SOCKS4;
- Shadowsocks UDP;
- QUIC, HTTP/3, MASQUE, CONNECT-UDP;
- transparent UDP proxying;
- DNS policy engine beyond treating DNS as ordinary UDP payload;
- UDP over TLS;
- kernel-level packet capture or TPROXY;
- NAT traversal features;
- multicast or broadcast forwarding;
- UDP fragmentation/reassembly beyond SOCKS5 FRAG=0 handling;
- persistent storage of UDP state.

---

# Current architecture assumptions

The current runtime is Phase 2 complete:

- `eggress-runtime` owns a compiled runtime snapshot;
- `eggress-server` handles accepted TCP sessions;
- routing, metrics, admin, health, reload, and shutdown are integrated;
- listeners are configured from TOML;
- admin reads live snapshot data through an `AdminSnapshotProvider`;
- shutdown uses drain-first, cancel-after-deadline semantics.

Phase 3 must build on this rather than add a separate ad hoc UDP service path in the CLI.

Recommended high-level UDP flow:

```text
TCP SOCKS5 client
  -> method/auth negotiation
  -> UDP ASSOCIATE request
  -> server creates UdpAssociation
  -> server replies with UDP relay bind address
  -> client sends SOCKS5 UDP datagrams to relay address
  -> association decodes datagram and validates client ownership
  -> route engine decides Direct/Reject/UnsupportedUpstream
  -> direct UDP socket sends payload to target
  -> response from target is mapped back to client
  -> server encodes SOCKS5 UDP response datagram
  -> idle timeout or TCP control close tears down association
```

The TCP control connection lifetime owns the UDP association. If the control connection closes, the association closes. If the UDP association idles out, the TCP control connection should eventually close or return EOF behavior consistent with the server design.

---

# Crate and module layout

## New crate: `eggress-udp`

Create:

```text
crates/eggress-udp/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── assoc.rs
    ├── codec.rs
    ├── direct.rs
    ├── registry.rs
    ├── limits.rs
    ├── metrics.rs
    ├── error.rs
    └── testkit.rs
```

Responsibilities:

- UDP association state machine;
- SOCKS5 UDP datagram codec;
- direct UDP send/receive;
- association registry and limits;
- target mapping and response demux;
- UDP runtime task supervision primitives;
- test helpers.

Do not put UDP association orchestration in `eggress-cli`. The CLI/runtime should only configure and start UDP-capable listeners.

## Existing crate changes

### `eggress-protocol-socks`

Add or expose:

- SOCKS5 UDP ASSOCIATE request support;
- SOCKS5 reply for UDP ASSOCIATE;
- SOCKS5 UDP datagram codec if not placed in `eggress-udp`.

The preferred split:

- `eggress-protocol-socks` owns wire-level SOCKS5 constants and command/reply parsing;
- `eggress-udp` owns association lifecycle and datagram relay.

### `eggress-server`

Extend accepted session model with UDP association:

```rust
pub enum AcceptedSession {
    Tunnel(PendingTunnel),
    HttpForward(PendingHttpForward),
    UdpAssociate(PendingUdpAssociate),
}
```

`eggress-server` should create the UDP association via a runtime-provided UDP service handle.

### `eggress-runtime`

Own:

- UDP relay bind configuration;
- UDP association registry;
- UDP task tracker;
- shutdown integration;
- metrics/admin hooks.

### `eggress-routing`

Extend `RouteRequest` or introduce `DatagramRouteRequest` with transport metadata:

```rust
pub enum TransportKind {
    Tcp,
    Udp,
}
```

or:

```rust
pub struct DatagramRouteRequest<'a> {
    pub target: &'a TargetAddr,
    pub source: Option<SocketAddr>,
    pub listener: &'a str,
    pub inbound_protocol: ProtocolId,
    pub identity: &'a ClientIdentity,
}
```

Do not overload TCP route semantics in a way that implies SOCKS/HTTP upstream UDP support exists.

---

# Workstream 1: SOCKS5 UDP protocol support

## Required behavior

SOCKS5 supports command `UDP ASSOCIATE` (`CMD = 0x03`). The client sends a normal SOCKS5 request over TCP after method/auth negotiation. The request target address in UDP ASSOCIATE often describes where the client expects to send UDP from, but many clients send `0.0.0.0:0`.

The server should:

1. complete method/auth negotiation using the existing SOCKS5 authentication path;
2. parse `CMD = 0x03`;
3. create a UDP association bound to a configured or ephemeral address;
4. reply with SOCKS5 success and the UDP relay bind address/port;
5. keep the TCP control connection open while the association is active;
6. close association when the TCP control connection closes or cancellation fires.

## Reply bind address

The UDP relay address returned to the client must be reachable from the client. For loopback listeners, `127.0.0.1` is acceptable. For non-loopback listeners, derive the reply address carefully:

- If config specifies `udp_advertise_addr`, use it.
- Else if listener is bound to a concrete non-unspecified address, use that IP and UDP relay port.
- Else use the TCP local address if non-unspecified.
- Else return `0.0.0.0` only if explicitly documented and tested with local clients.

Suggested config:

```toml
[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
advertise = "127.0.0.1"
idle_timeout = "60s"
max_associations = 1024
max_targets_per_association = 64
max_datagram_size = 65535
client_pin = true
```

## Parser changes

Extend SOCKS5 request representation:

```rust
pub enum Socks5Command {
    Connect,
    Bind,
    UdpAssociate,
}

pub struct Socks5Request {
    pub command: Socks5Command,
    pub target: TargetAddr,
}
```

Existing CONNECT handling should reject unsupported BIND with the current failure code and route UDP ASSOCIATE into the new pending session.

## Required tests

- parse UDP ASSOCIATE IPv4 request;
- parse UDP ASSOCIATE IPv6 request;
- parse UDP ASSOCIATE domain request;
- no-auth UDP ASSOCIATE success;
- username/password UDP ASSOCIATE success;
- wrong credentials reject before association creation;
- unsupported listener protocol rejects UDP ASSOCIATE if UDP disabled;
- BIND remains unsupported;
- CONNECT behavior unchanged.

## Acceptance criteria

- SOCKS5 UDP ASSOCIATE reaches `AcceptedSession::UdpAssociate` only after authentication succeeds.

---

# Workstream 2: SOCKS5 UDP datagram codec

## Datagram format

SOCKS5 UDP datagrams use:

```text
+----+------+------+----------+----------+----------+
|RSV | FRAG | ATYP | DST.ADDR | DST.PORT | DATA     |
+----+------+------+----------+----------+----------+
| 2  |  1   |  1   | variable |    2     | variable |
+----+------+------+----------+----------+----------+
```

Rules for Phase 3:

- `RSV` must be `0x0000`;
- `FRAG` must be `0x00`;
- reject or drop nonzero `FRAG`; do not implement fragmentation;
- support ATYP IPv4, IPv6, and domain;
- domain length must be nonzero and bounded by 255;
- require at least one byte of payload unless explicitly allowing zero-length UDP payloads;
- enforce configured maximum datagram size.

## Codec API

```rust
pub struct Socks5UdpPacket<'a> {
    pub target: TargetAddr,
    pub payload: &'a [u8],
}

pub enum UdpCodecError {
    TooShort,
    BadReserved,
    FragmentationUnsupported,
    UnknownAddressType,
    BadDomainLength,
    MissingPort,
    MissingPayload,
    PacketTooLarge,
}

pub fn decode_socks5_udp(
    packet: &[u8],
    limits: &UdpLimits,
) -> Result<Socks5UdpPacket<'_>, UdpCodecError>;

pub fn encode_socks5_udp(
    target: &TargetAddr,
    payload: &[u8],
    out: &mut Vec<u8>,
    limits: &UdpLimits,
) -> Result<(), UdpCodecError>;
```

## Allocation policy

- Decoding should borrow payload from the input buffer.
- Encoding may allocate into a caller-provided buffer.
- Do not allocate per packet beyond necessary domain target representation.
- Enforce output maximum size before send.

## Required tests

- decode IPv4 target;
- decode IPv6 target;
- decode domain target;
- encode/decode round trips;
- reject bad RSV;
- reject FRAG != 0;
- reject short packets at every boundary;
- reject unknown ATYP;
- reject zero domain length;
- enforce max packet size;
- preserve payload bytes exactly.

## Acceptance criteria

- datagram codec is deterministic, bounded, and independent of socket I/O.

---

# Workstream 3: UDP association model

## Association identity

Each UDP association should have:

```rust
pub struct UdpAssociationId(u64);
```

and metadata:

```rust
pub struct UdpAssociationMeta {
    pub id: UdpAssociationId,
    pub listener: ListenerName,
    pub client_tcp_peer: SocketAddr,
    pub client_udp_addr: Option<SocketAddr>,
    pub identity: ClientIdentity,
    pub created_at: Instant,
    pub last_activity: AtomicInstantOrMutex,
    pub generation: u64,
}
```

`client_udp_addr` starts as `None` and is set by the first valid UDP packet if client pinning is enabled.

## Ownership semantics

A UDP association is owned by the TCP control connection created by SOCKS5 UDP ASSOCIATE.

Close when:

- TCP control connection closes;
- idle timeout expires;
- listener/runtime shutdown begins;
- association exceeds limits;
- internal fatal socket error occurs.

## Association state

```rust
pub enum UdpAssociationState {
    Open,
    Closing,
    Closed,
}
```

## Runtime handle

```rust
pub struct UdpAssociationHandle {
    pub id: UdpAssociationId,
    pub relay_addr: SocketAddr,
    pub advertised_addr: SocketAddr,
    close: CancellationToken,
    done: JoinHandle<UdpAssociationReport>,
}
```

`eggress-server` needs enough information to send the SOCKS5 reply and then keep the TCP control connection alive until association close/cancel.

## Required tests

- association ID increments monotonically;
- first UDP packet pins client address;
- packet from different client address is dropped when pinning enabled;
- association closes when TCP control task cancels;
- association closes after idle timeout;
- association close releases registry slot;
- shutdown closes all associations.

## Acceptance criteria

- no UDP association can outlive its owner control connection or runtime shutdown.

---

# Workstream 4: Association registry and limits

## Registry

```rust
pub struct UdpAssociationRegistry {
    next_id: AtomicU64,
    associations: DashMap<UdpAssociationId, Arc<UdpAssociationRuntime>>,
    global_limit: usize,
}
```

If avoiding `dashmap`, use `tokio::sync::RwLock<HashMap<...>>`. The expected association count is bounded; simplicity is acceptable.

## Limits

```rust
pub struct UdpLimits {
    pub max_associations_global: usize,
    pub max_associations_per_listener: usize,
    pub max_targets_per_association: usize,
    pub max_datagram_size: usize,
    pub idle_timeout: Duration,
    pub client_pin: bool,
    pub target_idle_timeout: Duration,
}
```

## Target mappings

For each association, track target flows:

```rust
pub struct UdpTargetFlow {
    pub target: TargetAddr,
    pub socket: UdpSocket,
    pub last_activity: Instant,
    pub packets_up: AtomicU64,
    pub packets_down: AtomicU64,
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
}
```

Phase 3 direct mode can use one outbound UDP socket per target flow. This is simple and makes response demux reliable because replies arrive on the target-specific socket.

Later optimization can use shared sockets with explicit peer mapping.

## Limit behavior

- If global limit is reached, reply to UDP ASSOCIATE with SOCKS5 general failure or connection not allowed.
- If per-listener limit is reached, same.
- If target-flow limit is reached, drop new target packets and increment a metric.
- If datagram too large, drop and increment a metric.
- If idle timeout fires, close association.

## Required tests

- global association limit;
- per-listener association limit;
- target-flow limit;
- target idle cleanup;
- datagram size drop;
- registry slot released on close;
- metrics increment for drops/limit failures.

## Acceptance criteria

- all UDP state is bounded by configured limits.

---

# Workstream 5: Direct UDP forwarding

## Direct UDP semantics

For each decoded client packet:

1. validate association ownership;
2. decode SOCKS5 UDP packet;
3. route target using route engine;
4. if decision is `Direct`, get or create target flow;
5. send payload to target;
6. update activity and metrics.

For each target response:

1. receive from outbound socket;
2. verify source matches expected target endpoint where applicable;
3. encode SOCKS5 UDP response with original target address;
4. send to pinned client UDP address;
5. update metrics.

## Domain targets

Direct UDP with domain targets requires resolving the target to an IP endpoint before sending.

Phase 3 should define DNS behavior explicitly:

- Use Tokio DNS resolution via `ToSocketAddrs` for direct UDP domain targets.
- Cache resolved address per target flow for the flow lifetime.
- Do not perform DNS resolution during route-rule CIDR matching.
- DNS resolution failure drops the packet and records failure metric.

## Connected vs unconnected UDP sockets

Use connected UDP sockets per target flow when possible:

```rust
let socket = UdpSocket::bind(local_bind).await?;
socket.connect(resolved_target).await?;
socket.send(payload).await?;
let n = socket.recv(&mut buf).await?;
```

Connected UDP simplifies response filtering. If connected sockets behave unexpectedly on any target OS, use `send_to`/`recv_from` and explicit source checks.

## Required tests

- IPv4 UDP echo through SOCKS5 UDP relay;
- IPv6 UDP echo when available;
- domain target UDP echo using localhost name if reliable;
- response from unexpected address is ignored if using unconnected sockets;
- DNS failure increments metric and does not crash;
- multiple target flows in one association;
- target-flow idle expiry.

## Acceptance criteria

- direct UDP works through SOCKS5 UDP ASSOCIATE for IPv4 and domain targets, with IPv6 covered where platform permits.

---

# Workstream 6: Routing integration for UDP

## Route semantics

Phase 3 supports:

- `Direct` for UDP;
- `Reject` for UDP;
- upstream group only if selected upstream protocol supports UDP.

Initially, HTTP and SOCKS4 upstream chains do not support UDP. SOCKS5 UDP upstream support may be deferred to a later Phase 3 subphase unless explicitly implemented.

Recommended behavior:

```rust
pub enum DatagramRouteDecision {
    Direct { rule: RuleId },
    Reject { rule: RuleId, reason: RejectReason },
    UnsupportedUpstream { rule: RuleId, group: UpstreamGroupId },
}
```

If a rule selects an upstream group that has no UDP-capable path, the packet should be dropped and metric/log should record `unsupported_upstream_udp`.

Do not silently fall back to direct unless the configured group fallback says direct and policy permits it.

## Transport-aware rules

Add matcher support for transport if not already present:

```rust
MatchExpr::Transport(TransportKind::Udp)
```

TOML example:

```toml
[[rules]]
id = "dns-direct"
match = { all = [ { transport = "udp" }, { destination_port = 53 } ] }
action = { direct = true }

[[rules]]
id = "block-udp-private"
match = { all = [ { transport = "udp" }, { destination_cidr = "10.0.0.0/8" } ] }
action = { reject = "policy_denied" }
```

## Route request

Include transport:

```rust
pub struct RouteRequest<'a> {
    pub target: &'a TargetAddr,
    pub source: Option<SocketAddr>,
    pub listener: &'a str,
    pub inbound_protocol: ProtocolId,
    pub identity: &'a ClientIdentity,
    pub transport: TransportKind,
}
```

If changing the existing type is too broad, introduce `DatagramRouteRequest` and conversion helpers.

## Required tests

- UDP direct rule matches;
- UDP reject rule drops packet;
- TCP-only rule does not match UDP if transport matcher is present;
- UDP route report includes rule ID;
- unsupported upstream group increments metric;
- direct fallback behavior is explicit.

## Acceptance criteria

- UDP packets do not bypass Phase 2 policy.

---

# Workstream 7: Runtime integration

## Runtime services

Add to `eggress-runtime`:

```rust
pub struct UdpRuntime {
    registry: Arc<UdpAssociationRegistry>,
    metrics: Arc<MetricsRegistry>,
    routing: Arc<SharedRoutingService>,
    snapshot: Arc<ArcSwap<CompiledRuntimeSnapshot>>,
    cancel: CancellationToken,
    tasks: TaskTracker,
}
```

The runtime must pass a UDP service handle into `eggress-server::ConnectionConfig`:

```rust
pub struct ConnectionConfig {
    // existing fields
    pub udp: Option<Arc<UdpService>>,
}
```

If a SOCKS5 UDP ASSOCIATE request arrives when UDP is disabled, return a protocol-correct failure.

## Listener config

TOML should allow UDP per listener:

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
max_associations = 1024
max_targets_per_association = 64
max_datagram_size = 65535
client_pin = true
```

UDP config must be rejected for listeners that do not include `socks5` until other UDP-capable protocols exist.

## Shutdown

Shutdown sequence extension:

1. readiness false;
2. stop TCP listeners;
3. stop health;
4. stop accepting new UDP associations;
5. drain TCP sessions;
6. cancel remaining UDP associations after deadline or when owner TCP closes;
7. wait UDP tasks;
8. stop admin.

## Reload

Phase 3 can choose scoped reload:

- UDP limits may apply to new associations only;
- UDP bind changes are restart-required;
- UDP advertise address changes are restart-required if socket bind changes, otherwise may update new replies only;
- route changes apply immediately to future UDP packets if routing is evaluated per packet.

Document exact behavior.

## Required tests

- UDP disabled returns SOCKS failure;
- UDP enabled returns relay address;
- UDP bind conflict fails startup;
- runtime shutdown closes association;
- reload route change affects subsequent UDP packets;
- UDP listener topology change rejected as restart-required.

## Acceptance criteria

- UDP is a first-class runtime service, not a protocol-local background task.

---

# Workstream 8: Metrics and admin visibility

## Metrics

Add bounded-cardinality metrics:

```text
egress_udp_associations_active
egress_udp_associations_total
egress_udp_association_failures_total{reason}
egress_udp_packets_up_total{listener,rule,action}
egress_udp_packets_down_total{listener,rule,action}
egress_udp_bytes_up_total{listener,rule,action}
egress_udp_bytes_down_total{listener,rule,action}
egress_udp_dropped_packets_total{reason}
egress_udp_target_flows_active
egress_udp_target_flows_total
egress_udp_decode_errors_total{kind}
```

Allowed labels:

- listener;
- rule ID;
- action;
- drop reason;
- decode error kind.

Do not label by:

- target host;
- target IP;
- source IP;
- username;
- payload content.

## Admin

Extend `/ -/status` or `/ -/upstreams` only if appropriate, but add a dedicated endpoint:

```text
GET /-/udp
```

Return:

```json
{
  "associations_active": 3,
  "associations_total": 18,
  "target_flows_active": 7,
  "listeners": [
    {
      "name": "socks-in",
      "udp_enabled": true,
      "active_associations": 3
    }
  ]
}
```

Optionally include per-association details only when a debug flag is enabled. Avoid exposing client IPs unless admin config explicitly allows peer disclosure.

## Required tests

- metrics increment for successful UDP relay;
- decode errors increment;
- policy drops increment;
- active gauges return to zero after association close;
- admin `/ -/udp` reflects active association count;
- no target/source/user labels appear in metrics output.

## Acceptance criteria

- UDP behavior is visible operationally without high-cardinality leakage.

---

# Workstream 9: Security and abuse controls

UDP requires explicit anti-abuse controls.

## Required controls

- client address pinning enabled by default;
- nonzero FRAG dropped;
- datagram size bounded;
- association count bounded;
- target-flow count bounded;
- target idle expiry;
- global association idle expiry;
- response source validation;
- no broadcast or multicast forwarding by default;
- optional private-network reject rule examples in docs;
- no amplification from unauthenticated clients when listener auth is configured;
- metrics for drops and limit hits.

## Broadcast/multicast policy

Reject by default:

- IPv4 broadcast addresses;
- IPv4 multicast `224.0.0.0/4`;
- IPv6 multicast `ff00::/8`;
- unspecified destination addresses;
- port 0.

Config may later allow this explicitly, but Phase 3 should keep defaults strict.

## Amplification note

A SOCKS5 UDP relay can be used as an amplifier if open to the internet. Document:

- bind to loopback by default;
- use authentication for non-loopback listeners;
- configure CIDR reject rules;
- keep association limits low by default.

## Required tests

- multicast target rejected;
- broadcast target rejected;
- unspecified target rejected;
- port 0 rejected;
- packet from unpinned client dropped;
- open listener without auth emits warning or config error if non-loopback.

## Acceptance criteria

- UDP defaults are safe for local use and explicit for broader exposure.

---

# Workstream 10: Tests and interoperability

## Unit tests

- SOCKS5 UDP codec;
- association registry limits;
- target-flow lifecycle;
- route decision mapping;
- datagram security policy;
- metrics label generation.

## Integration tests

Add:

```text
crates/eggress-runtime/tests/udp_socks5.rs
crates/eggress-runtime/tests/udp_limits.rs
crates/eggress-runtime/tests/udp_routing.rs
crates/eggress-runtime/tests/udp_admin_metrics.rs
```

Scenarios:

- SOCKS5 UDP ASSOCIATE returns usable UDP relay address;
- UDP echo through direct route;
- two concurrent associations do not cross packets;
- two target flows under one association;
- idle timeout closes association;
- shutdown closes association;
- route reject drops packet and increments metric;
- listener UDP disabled rejects command;
- datagram decode errors are counted;
- target limit enforced.

## External interoperability

Use Python `pproxy` as an oracle where feasible. If `pproxy` UDP behavior is difficult to drive directly, document the limitation and use a known SOCKS5 UDP-capable client/helper.

Potential local helpers:

- a small Rust test client using the SOCKS5 UDP codec;
- `dig` through SOCKS5 is not standard without a wrapper, so avoid public DNS dependency;
- local UDP echo server is preferred.

External tests must not require internet.

## Deterministic testing

- bind all sockets to `127.0.0.1:0`;
- use readiness channels, not sleeps;
- idle timeout tests use short configured durations and bounded polling;
- avoid assuming IPv6 availability unless guarded;
- avoid relying on UDP packet ordering beyond single-flow echo cases.

## Acceptance criteria

- UDP behavior is covered at codec, association, runtime, routing, metrics, and shutdown levels.

---

# Workstream 11: Documentation and README checklist

## README updates

When implemented, update SOCKS5 checklist:

```markdown
- [x] SOCKS5 UDP ASSOCIATE server
- [x] SOCKS5 UDP ASSOCIATE client-side datagram codec
```

Add UDP section:

```markdown
### UDP

- [x] SOCKS5 UDP ASSOCIATE server
- [x] SOCKS5 UDP datagram codec
- [x] Direct UDP forwarding
- [x] UDP association lifecycle
- [x] UDP idle timeout
- [x] UDP association limits
- [x] UDP target-flow limits
- [x] UDP metrics
- [x] UDP admin visibility
- [ ] UDP through SOCKS5 upstream
- [ ] UDP through Shadowsocks
- [ ] Transparent UDP proxying
```

## Architecture docs

Document:

- UDP association ownership by TCP control connection;
- datagram codec boundaries;
- target-flow model;
- routing per datagram;
- direct-only limitation;
- security defaults;
- reload limitations.

## Config docs

Add examples for:

- enabling UDP on a SOCKS5 listener;
- DNS UDP direct route;
- rejecting private/multicast ranges;
- limiting associations;
- admin metrics visibility.

## Completion doc

Create or update:

```text
docs/PHASE_3_COMPLETION.md
```

Only mark Phase 3 complete when all exit criteria below pass.

---

# Recommended implementation sequence

## Commit 1: SOCKS5 UDP ASSOCIATE parser and session variant

- Add `Socks5Command::UdpAssociate`.
- Add pending UDP association session type.
- Preserve CONNECT behavior.
- Add parser/auth tests.

## Commit 2: SOCKS5 UDP datagram codec

- Add decode/encode functions.
- Add codec error types and exhaustive tests.

## Commit 3: UDP association registry and limits

- Add `eggress-udp` crate.
- Add association IDs, registry, limit checks, idle metadata.
- Add unit tests.

## Commit 4: Direct UDP target flow

- Add outbound UDP socket per target flow.
- Add local UDP echo integration test at crate level.

## Commit 5: Runtime SOCKS5 UDP service integration

- Add UDP config model.
- Create association on UDP ASSOCIATE.
- Send SOCKS5 UDP ASSOCIATE success reply.
- Keep TCP control connection alive.
- Add basic end-to-end UDP echo through SOCKS5 listener.

## Commit 6: UDP routing integration

- Add transport-aware route request.
- Route every UDP datagram.
- Implement direct/reject/unsupported-upstream handling.
- Add UDP routing tests.

## Commit 7: Metrics and admin visibility

- Add UDP metrics.
- Add admin `/ -/udp` endpoint or equivalent `/ -/udp` without the space.
- Add tests for counters and admin output.

## Commit 8: Security controls

- Enforce client pinning, datagram bounds, target restrictions, target-flow limits.
- Add drop metrics and tests.

## Commit 9: Shutdown/reload integration

- Ensure associations close on control close, idle timeout, runtime shutdown.
- Document/reject UDP bind reload changes.
- Add shutdown/reload tests.

## Commit 10: Interoperability, docs, and closure

- Add external/local interoperability tests.
- Update README, architecture docs, config examples.
- Add completion doc and final check record.

---

# Required negative tests

- UDP ASSOCIATE when listener UDP disabled;
- UDP ASSOCIATE on unauthenticated non-loopback listener if policy forbids it;
- bad RSV;
- FRAG nonzero;
- unknown ATYP;
- short address;
- missing port;
- packet too large;
- packet from unpinned client;
- target-flow limit exceeded;
- association limit exceeded;
- multicast target;
- broadcast target;
- unspecified target;
- port zero;
- route reject;
- route selects unsupported upstream;
- idle association cleanup;
- TCP control close cleanup;
- runtime shutdown cleanup.

---

# CI and verification commands

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Add any UDP external interoperability tests to CI with no public internet dependency.

If Python `pproxy` UDP tests are added:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_pproxy_udp
```

If the exact test name differs, update this plan and documentation.

---

# Phase 3 exit criteria

Phase 3 is complete only when:

1. SOCKS5 UDP ASSOCIATE is parsed and authenticated correctly.
2. Server replies with a usable UDP relay address.
3. SOCKS5 UDP datagram codec supports IPv4, IPv6, and domain targets.
4. Nonzero FRAG is rejected or dropped with metrics.
5. Direct UDP forwarding works through a local SOCKS5 UDP association.
6. Association lifecycle is owned by the TCP control connection.
7. Idle timeout closes inactive associations.
8. Global and per-listener association limits are enforced.
9. Target-flow limits are enforced.
10. Client address pinning is enabled by default and tested.
11. Broadcast, multicast, unspecified targets, and port zero are rejected by default.
12. UDP datagrams are routed through the Phase 2 routing engine.
13. Reject rules drop UDP packets and increment metrics.
14. Unsupported upstream UDP paths are explicit and metriced.
15. UDP metrics expose bounded-cardinality counters and gauges.
16. Admin exposes active UDP association summary.
17. Runtime shutdown closes UDP associations and waits for UDP tasks.
18. Reload semantics for UDP config are explicit and tested.
19. README and architecture docs accurately describe UDP limitations.
20. All workspace tests, lint, audit, and applicable interoperability checks pass.
21. No unsafe Rust, OpenSSL dependency, or native dependency is introduced.

## Completion record

When complete, append:

```markdown
## Completion record

Implemented by commits:

- `<sha>` — SOCKS5 UDP ASSOCIATE and datagram codec
- `<sha>` — UDP associations, registry, direct forwarding
- `<sha>` — runtime routing, metrics, admin, shutdown integration
- `<sha>` — security limits and negative tests
- `<sha>` — interoperability and documentation closure

All required checks passed on `<date>`.
```
