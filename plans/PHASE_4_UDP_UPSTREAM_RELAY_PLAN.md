# Phase 4 Detailed Plan: UDP Upstream Relay Support

## Objective

Phase 4 extends the Phase 3 UDP foundation from **local SOCKS5 UDP ASSOCIATE + direct UDP forwarding** to **UDP relay through upstream proxies**, starting with SOCKS5 upstreams.

The goal is not to implement every UDP-capable proxy protocol. The first deliverable is a robust SOCKS5 UDP upstream path that integrates with existing Phase 2 routing, upstream groups, scheduler/lease semantics, metrics, admin visibility, reload/shutdown behavior, and Phase 3 UDP association lifecycle.

A client should be able to connect to an Eggress SOCKS5 listener, issue UDP ASSOCIATE, send SOCKS5 UDP datagrams to Eggress, and have Eggress relay those UDP payloads through a selected upstream SOCKS5 proxy using that upstream proxy's own UDP ASSOCIATE mechanism.

---

# Current baseline

The repository currently supports:

- SOCKS5 UDP ASSOCIATE server-side handling;
- direct UDP forwarding;
- SOCKS5 UDP datagram codec;
- per-listener UDP config;
- UDP bind/advertise behavior;
- association registry cleanup;
- idle association timeout;
- target-flow idle cleanup;
- runtime-tracked UDP relay tasks;
- UDP routing through `RouteService::route()`;
- direct fallback semantics;
- admin `/-/udp`;
- UDP metrics bridged into `/metrics`;
- explicit non-support for UDP upstream relay.

Phase 4 changes that final limitation for SOCKS5 upstreams only.

---

# Scope

## Included

- SOCKS5 UDP upstream client support;
- upstream UDP ASSOCIATE control connection management;
- upstream UDP relay socket management;
- mapping client target flows to upstream UDP association flows;
- upstream authentication reuse for SOCKS5 username/password chains;
- route selection and scheduler/lease semantics for UDP upstream selection;
- support for one-hop SOCKS5 UDP upstream initially;
- optional direct fallback behavior already supported by Phase 3;
- explicit rejection of unsupported upstream protocols and multi-hop UDP chains unless implemented;
- UDP upstream metrics and admin visibility;
- shutdown and idle cleanup for upstream UDP associations;
- reload behavior for existing and new UDP upstream flows;
- integration tests using a local Eggress or synthetic SOCKS5 UDP upstream server;
- docs and completion checklist updates.

## Excluded

Do not implement in this phase:

- HTTP CONNECT-UDP, MASQUE, HTTP/3, or QUIC;
- UDP over HTTP CONNECT;
- UDP through SOCKS4;
- UDP through Shadowsocks/Trojan/SSH;
- multi-hop UDP chains unless the one-hop SOCKS5 foundation is complete and tests are strong;
- transparent UDP proxying;
- SOCKS5 UDP fragmentation/reassembly;
- multicast/broadcast forwarding;
- kernel TPROXY or packet capture;
- UDP relay across mixed TCP-only chains;
- persistent storage of UDP upstream state.

---

# Required semantics

## Supported route result matrix

When a UDP datagram is routed:

| Route result | UDP behavior |
|---|---|
| `SelectedRoute::Direct { Normal }` | Send direct UDP as Phase 3 does today |
| `SelectedRoute::Direct { DirectFallback }` | Send direct UDP and record direct-fallback metric/log |
| `SelectedRoute::Upstream` with exactly one SOCKS5 hop | Relay through SOCKS5 UDP upstream |
| `SelectedRoute::Upstream` with HTTP hop | Drop as unsupported UDP upstream |
| `SelectedRoute::Upstream` with SOCKS4 hop | Drop as unsupported UDP upstream |
| `SelectedRoute::Upstream` with multi-hop chain | Initially drop as unsupported unless this phase explicitly implements multi-hop |
| `Reject` | Drop and record policy/drop metric |
| Route/open failure | Drop and record upstream failure metric |

Do not silently fall back to direct unless the selected route is a direct fallback selected by routing policy.

## Upstream lease semantics

UDP upstream selection must preserve Phase 2 accounting:

- selecting an upstream should create a pending lease or equivalent UDP lease;
- pending is converted to active only after the upstream UDP ASSOCIATE control path is established;
- active remains held while the upstream UDP flow/association is in use;
- active is released on flow idle cleanup, association close, or shutdown;
- failed upstream setup releases pending and records failure.

If reusing `PendingLease`/`ActiveLease`, make sure it remains correct for long-lived UDP target flows. If a UDP association can multiplex multiple target flows through one upstream, hold the active lease at the upstream association layer, not per packet.

---

# Workstream 1: Model UDP capability on upstream hops

## Problem

The current route layer can select upstream groups, but UDP code has no explicit way to know whether a selected chain can carry UDP.

## Required model

Add helper APIs close to `eggress-uri` or `eggress-routing`:

```rust
pub enum UdpRelayCapability {
    SupportedSocks5,
    UnsupportedProtocol { protocol: String },
    UnsupportedMultiHop,
}

pub fn udp_capability(chain: &ProxyChainSpec) -> UdpRelayCapability;
```

Initial rules:

- exactly one SOCKS5 hop: supported;
- any HTTP hop: unsupported;
- any SOCKS4 hop: unsupported;
- zero hops: direct, not upstream;
- more than one hop: unsupported for this phase unless later extended.

If the URI model represents protocols as enum variants, use those directly. Avoid string parsing where structured types exist.

## Required tests

- one SOCKS5 hop is supported;
- SOCKS5 with username/password is supported;
- HTTP hop unsupported;
- SOCKS4 hop unsupported;
- multi-hop chain unsupported;
- unsupported reason is stable for metrics/docs.

## Acceptance criteria

- UDP relay code never infers support from ad hoc string matching in the hot path.

---

# Workstream 2: SOCKS5 UDP upstream client handshake

## Required behavior

For a selected SOCKS5 upstream, Eggress must:

1. open a TCP control connection to the SOCKS5 upstream;
2. perform SOCKS5 method negotiation;
3. perform username/password authentication if configured;
4. send a UDP ASSOCIATE request;
5. parse the upstream SOCKS5 reply;
6. derive the upstream UDP relay address;
7. keep the upstream TCP control connection open while the upstream UDP association is active;
8. close/cancel the control connection when the UDP upstream flow is closed.

## API sketch

Add in `eggress-udp` or a new module `eggress-udp/src/upstream_socks5.rs`:

```rust
pub struct Socks5UdpUpstreamConfig {
    pub upstream_id: UpstreamId,
    pub hop: ProxyHopSpec,
    pub connect_timeout: Duration,
    pub udp_bind: SocketAddr,
}

pub struct Socks5UdpUpstreamAssociation {
    pub upstream_id: UpstreamId,
    pub relay_addr: SocketAddr,
    pub control_task: JoinHandle<()>,
    pub control_cancel: CancellationToken,
    pub udp_socket: Arc<UdpSocket>,
}

pub async fn open_socks5_udp_upstream(
    config: Socks5UdpUpstreamConfig,
    target_hint: Option<SocketAddr>,
) -> Result<Socks5UdpUpstreamAssociation, UdpUpstreamError>;
```

The `target_hint` is the client address field for UDP ASSOCIATE. For most upstreams, send `0.0.0.0:0` or `[::]:0` unless the upstream requires a specific address. Keep this configurable only if needed.

## Authentication

Reuse existing SOCKS5 client/auth code if present. If only server-side code exists, implement a small client handshake module with tests.

Required auth behavior:

- if hop has no credentials, offer `NO AUTH`;
- if hop has credentials, offer username/password and fail if upstream does not accept it;
- do not log passwords;
- bound username/password lengths according to SOCKS5 username/password subnegotiation limits.

## Upstream reply address handling

The upstream reply contains the upstream UDP relay endpoint. Use it as follows:

- If reply address is unspecified (`0.0.0.0` or `::`), substitute the TCP peer IP used to connect to the upstream, preserving the reply port.
- If reply address is loopback but upstream TCP peer is not loopback, treat as suspicious and either use as-is only if explicitly configured or fail with a clear error.
- Support IPv4, IPv6, and domain reply addresses if the upstream returns a domain.

## Required tests

Unit/synthetic server tests:

- no-auth handshake success;
- username/password success;
- username/password failure;
- upstream rejects all methods;
- UDP ASSOCIATE reply success;
- UDP ASSOCIATE reply failure code mapped to error;
- unspecified reply address substituted with upstream TCP peer;
- malformed reply rejected;
- timeout during handshake;
- control connection stays open until cancel.

## Acceptance criteria

- Eggress can establish a SOCKS5 UDP association with a local synthetic upstream.

---

# Workstream 3: Upstream UDP packet codec reuse

## Requirement

Do not create a second SOCKS5 UDP datagram codec. Use the existing Phase 3 codec for both inbound client datagrams and upstream relay datagrams.

For upstream send:

```rust
encode_socks5_udp_response_or_request(target, payload, out)
upstream_udp_socket.send_to(&out, upstream_relay_addr).await
```

The SOCKS5 UDP wire format is symmetric enough for request/reply encoding; rename functions if needed to avoid response-only naming confusion.

Recommended rename:

```rust
encode_socks5_udp_datagram(target, payload, out)
decode_socks5_udp_datagram(packet)
```

Keep backwards-compatible wrappers if many tests call old names.

## Required tests

- existing inbound codec tests still pass;
- upstream send uses same encoder;
- upstream response decode uses same decoder;
- zero-copy payload borrow behavior remains intact.

## Acceptance criteria

- exactly one SOCKS5 UDP datagram codec implementation exists.

---

# Workstream 4: Upstream target-flow model

## Current direct model

Phase 3 uses one local connected UDP socket per target flow.

## Required upstream model

For SOCKS5 UDP upstreams, each client association should maintain upstream UDP flow state. Keep it simple:

```rust
enum UdpFlowKind {
    Direct(DirectTargetFlow),
    Socks5Upstream(Socks5UdpTargetFlow),
}

pub struct Socks5UdpTargetFlow {
    pub target: SocksAddr,
    pub upstream_id: UpstreamId,
    pub upstream_relay_addr: SocketAddr,
    pub udp_socket: Arc<UdpSocket>,
    pub control_cancel: CancellationToken,
    pub control_task: JoinHandle<()>,
    pub lease: ActiveLease,
    pub last_activity: Instant,
    pub recv_task: JoinHandle<()>,
}
```

Initial implementation may create one upstream UDP association per target flow. This is simpler and safer.

Potential later optimization: one upstream UDP association per Eggress client UDP association and multiple target flows multiplexed through it. Do not implement that optimization until simple per-target upstream associations are correct.

## Per-target upstream association lifecycle

On first packet to target through a selected SOCKS5 upstream:

1. route target;
2. select upstream;
3. establish upstream TCP control + UDP relay;
4. convert pending lease to active;
5. create upstream UDP socket;
6. spawn upstream response receiver;
7. send encoded SOCKS5 UDP datagram to upstream relay.

On subsequent packets to same target with same selected upstream:

- reuse the flow if routing still selects equivalent upstream;
- or document that route selection is per-flow and not re-evaluated until flow idle expiry.

Recommended: route on first packet per flow; reuse until target-flow idle timeout. This avoids scheduler churn per datagram.

## Flow key

Current direct flow key likely uses target host/port. For upstream UDP, include upstream identity and action kind:

```rust
enum UdpFlowKey {
    Direct { target: SocksAddr },
    Socks5Upstream { target: SocksAddr, upstream_id: UpstreamId },
}
```

If the routing decision changes after reload, existing flows continue until idle expiry; new flows use new routing. Document this.

## Required tests

- first packet creates upstream flow;
- second packet to same target reuses flow;
- flow idle cleanup closes upstream control connection and releases lease;
- route reload affects new flow, not existing flow;
- direct and upstream flows to same target do not collide;
- target-flow limit counts both direct and upstream flows.

## Acceptance criteria

- upstream UDP flow lifecycle is explicit, bounded, and lease-safe.

---

# Workstream 5: Relay loop integration

## Current behavior

The relay loop handles selected direct routes and drops selected upstream routes as unsupported.

## Required behavior

Update the `SelectedRoute::Upstream` branch:

```rust
match selected {
    SelectedRoute::Direct { selection_reason, .. } => {
        send_direct(...).await;
    }
    SelectedRoute::Upstream { group, upstream, chain, pending_lease, .. } => {
        match udp_capability(&chain) {
            SupportedSocks5 => {
                send_via_socks5_upstream(..., pending_lease).await;
            }
            UnsupportedProtocol { protocol } => {
                drop(pending_lease);
                metrics.record_udp_unsupported_upstream(protocol);
                continue;
            }
            UnsupportedMultiHop => {
                drop(pending_lease);
                metrics.record_udp_unsupported_upstream("multi-hop");
                continue;
            }
        }
    }
}
```

Be explicit about pending lease drop on unsupported UDP upstream.

## Refactor recommendation

Split relay send handling:

```rust
async fn handle_client_datagram(
    request: Socks5UdpRequest<'_>,
    client_addr: SocketAddr,
    flows: &mut HashMap<UdpFlowKey, TargetFlowEntry>,
    config: &RelayConfig,
    response_tx: mpsc::UnboundedSender<ResponseMsg>,
) -> Result<(), UdpError>
```

Keep `udp_relay_loop` as orchestration only.

## Required tests

- selected SOCKS5 upstream path sends through upstream relay;
- unsupported HTTP upstream drops and releases pending lease;
- unsupported multi-hop drops and releases pending lease;
- direct fallback still works;
- route reject still drops;
- route errors record metrics.

## Acceptance criteria

- `SelectedRoute::Upstream` is no longer categorically unsupported; SOCKS5 one-hop works.

---

# Workstream 6: Synthetic SOCKS5 UDP upstream test server

## Need

Reliable tests should not depend on public internet or external tools. Add a local synthetic SOCKS5 UDP upstream server testkit.

## Test server behavior

Create in `eggress-udp::testkit` or runtime tests:

```rust
pub struct Socks5UdpTestServer {
    pub tcp_addr: SocketAddr,
    pub udp_addr: SocketAddr,
    pub received: mpsc::Receiver<TestUdpDatagram>,
    shutdown: CancellationToken,
}
```

The server should:

- accept TCP SOCKS5 method negotiation;
- optionally require username/password;
- accept UDP ASSOCIATE;
- reply with its UDP relay address;
- receive SOCKS5 UDP datagrams on UDP socket;
- decode target/payload;
- optionally echo back through SOCKS5 UDP response format;
- optionally inject malformed replies or failure reply codes.

## Modes

- no-auth success;
- username/password success;
- auth failure;
- UDP ASSOCIATE failure reply;
- malformed reply;
- slow handshake timeout;
- echo payload.

## Required tests

- direct client-to-testkit handshake test;
- Eggress upstream relay sends expected target and payload to testkit;
- testkit echo returns to original UDP client through Eggress;
- authenticated upstream path works;
- upstream auth failure is surfaced and metriced.

## Acceptance criteria

- UDP upstream behavior is covered without external services.

---

# Workstream 7: Metrics and admin updates

## Metrics to add

Existing UDP metrics cover direct relay. Add upstream-specific counters with bounded labels:

```text
egress_udp_upstream_associations_total{upstream_id,group_id,outcome}
egress_udp_upstream_associations_active{upstream_id,group_id}
egress_udp_upstream_packets_up_total{upstream_id,group_id}
egress_udp_upstream_packets_down_total{upstream_id,group_id}
egress_udp_upstream_bytes_up_total{upstream_id,group_id}
egress_udp_upstream_bytes_down_total{upstream_id,group_id}
egress_udp_upstream_failures_total{upstream_id,group_id,reason}
egress_udp_unsupported_upstream_total{protocol_or_reason}
```

Keep label values bounded:

- upstream ID;
- group ID;
- outcome/reason enum;
- no target host/IP;
- no client source;
- no username;
- no payload-derived labels.

## Admin `/-/udp`

Extend summary:

```json
{
  "associations_active": 1,
  "target_flows_active": 2,
  "upstream_flows_active": 1,
  "listeners": [...],
  "upstreams": [
    {
      "id": "socks-upstream-a",
      "udp_active": 1
    }
  ]
}
```

Do not expose client/target addresses by default.

## Required tests

- upstream association total increments;
- active upstream gauge returns to zero after flow idle cleanup;
- upstream packets/bytes increment on echo path;
- unsupported upstream counter increments for HTTP upstream rule;
- admin `/-/udp` includes upstream active count but no target/client address.

## Acceptance criteria

- operators can distinguish direct UDP from upstream-relayed UDP.

---

# Workstream 8: Reload and shutdown semantics

## Reload

Existing UDP config reload semantics mostly apply. Add upstream-specific semantics:

- existing UDP upstream flows keep their selected upstream until target-flow idle expiry or association close;
- new target flows use the latest routing snapshot;
- if upstream config changes, existing flow continues with old control connection, because it owns its socket/control state;
- removed upstream is unavailable for new flows but old flows can drain until idle timeout;
- health/scheduler state for new flows follows current Phase 2 routing snapshot.

Document this clearly.

## Shutdown

Runtime shutdown must close:

- client UDP association relay task;
- all direct target-flow recv tasks;
- all upstream target-flow recv tasks;
- upstream SOCKS5 TCP control tasks;
- upstream UDP sockets;
- active leases.

Flow cleanup must not depend on process exit.

## Required tests

- shutdown with active upstream UDP flow releases lease and active counters;
- target-flow idle timeout closes upstream control connection;
- reload changing route from upstream to direct affects new flow after old flow expiry;
- removed upstream does not break existing flow immediately but no new flows select it;
- forced shutdown cancels upstream control task.

## Acceptance criteria

- upstream UDP state is lifecycle-safe across reload and shutdown.

---

# Workstream 9: Error taxonomy

## Add UDP upstream errors

```rust
pub enum UdpUpstreamError {
    UnsupportedProtocol,
    UnsupportedMultiHop,
    TcpConnect,
    SocksMethodRejected,
    SocksAuthFailed,
    SocksAssociateRejected(u8),
    MalformedSocksReply,
    UdpRelayAddressInvalid,
    Timeout,
    Io,
}
```

Map to metrics reason strings:

- `unsupported_protocol`;
- `unsupported_multi_hop`;
- `tcp_connect`;
- `method_rejected`;
- `auth_failed`;
- `associate_rejected`;
- `malformed_reply`;
- `bad_relay_addr`;
- `timeout`;
- `io`.

Do not use arbitrary error strings as metric labels.

## Required tests

- every error maps to a stable reason;
- metrics labels are bounded;
- logs include useful context without credentials.

## Acceptance criteria

- UDP upstream failures are diagnosable without high-cardinality metrics.

---

# Workstream 10: Documentation and examples

## README updates

Update UDP checklist:

```markdown
- [x] SOCKS5 UDP ASSOCIATE server
- [x] Direct UDP forwarding
- [x] UDP through one-hop SOCKS5 upstream
- [ ] UDP through multi-hop proxy chains
- [ ] UDP through HTTP/MASQUE/CONNECT-UDP
- [ ] UDP through Shadowsocks/Trojan
```

## Config example

Add an example:

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
max_associations = 512
max_targets_per_association = 32
max_datagram_size = 65535
client_pin = true

[[upstreams]]
id = "socks-upstream"
uri = "socks5://user:pass@127.0.0.1:19080"

[[upstream_groups]]
id = "udp-egress"
scheduler = "first-available"
members = ["socks-upstream"]
fallback = "reject"

[[rules]]
id = "udp-via-socks"
upstream_group = "udp-egress"

[rules.match]
all = [
  { transport = "udp" },
  { destination_port = 53 }
]
```

## Architecture docs

Document:

- one-hop SOCKS5 UDP upstream support;
- per-target upstream association model;
- route-on-first-packet-per-flow semantics;
- flow reuse until idle expiry;
- unsupported multi-hop rationale;
- metric/admin behavior;
- reload/shutdown lifecycle.

## Completion doc

Create or update:

```text
docs/PHASE_4_UDP_UPSTREAM_RELAY_COMPLETION.md
```

Do not mark complete until tests below pass.

---

# Required tests

## Unit tests

- UDP capability classification;
- SOCKS5 upstream handshake no-auth;
- SOCKS5 upstream handshake username/password;
- upstream UDP reply address substitution;
- upstream error mapping;
- flow-key separation between direct/upstream.

## Integration tests

Add:

```text
crates/eggress-runtime/tests/udp_upstream.rs
crates/eggress-udp/tests/socks5_upstream.rs
```

Scenarios:

1. Local synthetic SOCKS5 UDP upstream echoes payload through Eggress.
2. Authenticated SOCKS5 upstream works.
3. Upstream auth failure drops packet and records failure.
4. HTTP upstream selected for UDP increments unsupported counter and drops.
5. Multi-hop chain selected for UDP increments unsupported counter and drops.
6. Direct fallback still forwards direct.
7. Upstream target-flow idle cleanup releases active lease.
8. TCP control close closes upstream UDP flow.
9. Runtime shutdown closes upstream control task and releases lease.
10. Reload route change affects new flow after idle expiry.
11. `/metrics` exposes upstream UDP counters without target/client labels.
12. `/-/udp` shows upstream active count without target/client addresses.

## Interoperability tests

If feasible, use Python `pproxy` as the upstream SOCKS5 UDP server. If not feasible, document why and rely on the synthetic test server.

No test may depend on public internet.

---

# Recommended commit sequence

## Commit 1: UDP upstream capability model

- Add `UdpRelayCapability` helper.
- Add unit tests for supported/unsupported chains.

## Commit 2: SOCKS5 UDP upstream client handshake

- Add upstream client module.
- Add synthetic server tests for no-auth/auth/failure.

## Commit 3: Upstream flow model

- Introduce `UdpFlowKind` and `Socks5UdpTargetFlow`.
- Keep direct flow behavior unchanged.
- Add flow-key tests.

## Commit 4: Relay integration for one-hop SOCKS5 upstream

- Update `SelectedRoute::Upstream` branch.
- Send packets through upstream relay.
- Receive upstream responses and forward to client.
- Add local echo integration test.

## Commit 5: Lease, lifecycle, reload, shutdown hardening

- Hold active leases for upstream UDP flows.
- Release on idle/close/shutdown.
- Add shutdown/reload tests.

## Commit 6: Metrics/admin integration

- Add upstream UDP metrics.
- Extend `/-/udp` safely.
- Add metrics/admin tests.

## Commit 7: Negative cases and unsupported chains

- HTTP upstream unsupported.
- SOCKS4 upstream unsupported.
- Multi-hop unsupported.
- Auth failure and malformed replies.

## Commit 8: Docs and completion record

- README checklist.
- Config examples.
- Architecture docs.
- Completion doc.
- Final verification.

---

# Verification commands

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Run focused tests:

```bash
cargo test -p eggress-udp socks5_upstream
cargo test -p eggress-runtime udp_upstream
cargo test -p eggress-runtime udp
```

If external UDP upstream interop is added:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_pproxy_udp_upstream
```

---

# Definition of done

Phase 4 UDP upstream relay support is complete only when:

1. One-hop SOCKS5 upstream chains are classified as UDP-capable.
2. HTTP, SOCKS4, and multi-hop chains are explicitly unsupported for UDP with metrics.
3. Eggress can establish a SOCKS5 UDP ASSOCIATE control connection to an upstream.
4. Upstream username/password auth works and failures are handled without credential leakage.
5. Upstream UDP relay address handling is correct, including unspecified-address substitution.
6. A client UDP packet can traverse: client -> Eggress UDP relay -> SOCKS5 upstream UDP relay -> target echo server -> upstream -> Eggress -> client.
7. Upstream UDP target flows are bounded by existing target-flow limits.
8. Upstream control connections close on target-flow idle expiry.
9. Active upstream leases are held while flows are active and released on close.
10. Runtime shutdown closes upstream UDP flows and waits for tasks.
11. Reload semantics for existing vs new upstream UDP flows are documented and tested.
12. `/metrics` exposes upstream UDP counters with bounded labels.
13. `/-/udp` exposes safe upstream UDP summary without client/target leakage.
14. Unsupported upstream UDP selections are visible in logs/metrics.
15. Docs clearly state that only one-hop SOCKS5 UDP upstream is supported.
16. All tests, lint, audit, and applicable interop checks pass.
17. No unsafe Rust, OpenSSL dependency, or native dependency is introduced.

## Completion record

When complete, append:

```markdown
## Completion record

Implemented by commits:

- `<sha>` — UDP upstream capability model and SOCKS5 client handshake
- `<sha>` — upstream flow model and relay integration
- `<sha>` — lifecycle, lease, reload, and shutdown hardening
- `<sha>` — metrics, admin, negative tests, and docs

All required checks passed on `<date>`.
```
