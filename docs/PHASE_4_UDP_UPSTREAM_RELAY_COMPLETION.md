# Phase 4 Completion Record: UDP Upstream Relay Support

## Date
June 2026

## Summary

Implemented one-hop SOCKS5 UDP upstream relay support. Clients can now send UDP
datagrams through an Eggress SOCKS5 listener that are relayed through a selected
upstream SOCKS5 proxy.

## Implemented Components

### UDP Capability Model
- `UdpRelayCapability` enum classifies proxy chains as supported/unsupported
- Single SOCKS5 hop: supported
- HTTP/SOCKS4 hops: unsupported with metrics
- Multi-hop chains: unsupported for this phase

### SOCKS5 Upstream Client
- Full SOCKS5 handshake: method negotiation, username/password auth, UDP ASSOCIATE
- Unspecified relay address substitution (0.0.0.0 -> TCP peer IP)
- Control connection kept alive while UDP association is active
- Error taxonomy with stable reason labels for metrics

### Flow Model
- `UdpFlowKind` enum distinguishes direct and upstream flows
- `UdpFlowKey` typed enum for flow map keys (Direct vs Socks5Upstream)
- Per-target upstream association with TCP control + UDP relay
- Route-on-first-packet-per-flow semantics
- Flow reuse until idle timeout

### Relay Integration
- `SelectedRoute::Upstream` branch handles SOCKS5 upstream selection
- `handle_client_datagram()` extracted for cleaner relay loop
- Pending lease dropped on unsupported chains
- Active lease held during upstream flow lifetime
- Cleanup on idle expiry, association close, and shutdown

### Metrics and Admin
- Upstream-specific Prometheus metrics (aggregate counters, per-upstream/group labels deferred)
- `/-/udp` endpoint extended with `upstream_flows_active`
- Bridge from in-memory `UdpMetrics` to Prometheus registry

### Codec Rename
- `encode_socks5_udp_response` → `encode_socks5_udp_datagram`
- `decode_socks5_udp_request` → `decode_socks5_udp_datagram`
- Backwards-compatible wrappers retained for old names

### Synthetic Test Server
- `Socks5UdpTestServer` with configurable modes:
  - No auth, username/password, EchoWithCredentials, auth failure, associate failure, echo
- Used for integration testing without external dependencies

## Files Modified

- `crates/eggress-udp/src/flow.rs` - new: flow model types and `UdpFlowKey`
- `crates/eggress-udp/src/udp_capability.rs` - new: capability classification
- `crates/eggress-udp/src/upstream_socks5.rs` - new: SOCKS5 client handshake
- `crates/eggress-udp/src/relay.rs` - updated: upstream flow handling, `handle_client_datagram()` refactor, `UdpFlowKey` usage
- `crates/eggress-udp/src/metrics.rs` - updated: upstream metrics
- `crates/eggress-udp/src/testkit.rs` - updated: synthetic test server with `EchoWithCredentials` mode
- `crates/eggress-protocol-socks/src/socks5/udp_codec.rs` - renamed: codec functions with backwards-compatible wrappers
- `crates/eggress-protocol-socks/src/socks5/server.rs` - added: `Hash` derive to `SocksAddr`
- `crates/eggress-metrics/src/lib.rs` - updated: Prometheus bridge
- `crates/eggress-admin/src/routes.rs` - updated: admin endpoint

## New Integration Tests

- `crates/eggress-udp/tests/socks5_upstream.rs` - 11 tests covering:
  - Echo through upstream (no-auth)
  - Authenticated upstream
  - Auth failure drops and records metric
  - Associate failure drops
  - HTTP upstream unsupported
  - Multi-hop chain unsupported
  - Idle cleanup releases flow
  - Upstream metrics tracking
  - Method negotiation stall returns Timeout
  - Auth stall returns Timeout
  - Associate stall returns Timeout

- `crates/eggress-runtime/tests/udp_upstream.rs` - 9 tests covering:
  - Shutdown closes UDP flows
  - Metrics expose UDP counters
  - Admin endpoint safe (no address leakage)
  - Direct fallback forwards direct
  - Full TOML-configured SOCKS5 upstream echo
  - Authenticated SOCKS5 upstream echo
  - HTTP upstream drops unsupported
  - Multi-hop upstream drops unsupported
  - Target-flow idle timeout releases upstream gauge

## Verification

All checks passed:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo check --workspace`
- `cargo deny check`

## Definition of Done Checklist

- [x] One-hop SOCKS5 upstream chains classified as UDP-capable
- [x] HTTP, SOCKS4, multi-hop explicitly unsupported with metrics
- [x] SOCKS5 UDP ASSOCIATE control connection established
- [x] Full upstream SOCKS5 handshake bounded by timeout (was only TCP connect)
- [x] Username/password/domain SOCKS5 lengths validated before encoding
- [x] Domain ATYP supported in upstream UDP ASSOCIATE replies
- [x] Upstream response target validated and forwarded correctly
- [x] Unspecified relay address substituted correctly
- [x] Client packet traverses: client -> Eggress -> upstream -> target -> upstream -> Eggress -> client
- [x] Upstream target flows bounded by target-flow limits
- [x] Upstream control connections close on target-flow idle expiry
- [x] Active upstream leases held while flows active, released on close
- [x] Runtime shutdown closes upstream UDP flows and waits for tasks
- [x] Reload semantics documented and tested
- [x] Full ServiceSupervisor runtime tests prove TOML-configured upstream relay (echo, auth, unsupported, idle timeout)
- [x] `/metrics` exposes upstream UDP counters with aggregate counters
- [x] `/-/udp` exposes safe upstream summary without client/target leakage
- [x] Unsupported upstream selections visible in logs/metrics
- [x] Docs state only one-hop SOCKS5 UDP upstream supported
- [x] All tests, lint, audit pass
- [x] No unsafe Rust, OpenSSL, or native dependencies introduced
- [x] Integration test files exist per plan (socks5_upstream.rs, udp_upstream.rs)
- [x] 20 integration test scenarios covered
- [x] Codec renamed with backwards-compatible wrappers
- [x] `handle_client_datagram()` extracted from relay loop
- [x] `UdpFlowKey` enum for typed flow map keys
- [x] TOML config example in architecture docs

## Final Closure Record

Phase 4 closure fixes addressed:

- Full upstream SOCKS5 handshake timeout (method negotiation, auth, UDP ASSOCIATE now bounded)
- SOCKS5 field length validation (username/password/domain lengths checked before u8 encoding)
- Domain ATYP support in upstream UDP ASSOCIATE reply parser (streaming and buffer-based)
- Upstream response target preserved from decoded datagram (with equivalence check)
- Aggregate upstream UDP metrics (per-upstream/group labels deferred to later observability pass)
- Full ServiceSupervisor runtime test for TOML-configured SOCKS5 UDP upstream echo
- Runtime tests for authenticated upstream, HTTP upstream drop, multi-hop drop, and target-flow idle timeout
- Handshake-stage timeout unit tests (method stall, auth stall, associate stall)
- Plan archival policy documented

All required checks passed on 2026-06-22.
