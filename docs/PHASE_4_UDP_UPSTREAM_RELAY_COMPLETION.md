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
- Per-target upstream association with TCP control + UDP relay
- Route-on-first-packet-per-flow semantics
- Flow reuse until idle timeout

### Relay Integration
- `SelectedRoute::Upstream` branch handles SOCKS5 upstream selection
- Pending lease dropped on unsupported chains
- Active lease held during upstream flow lifetime
- Cleanup on idle expiry, association close, and shutdown

### Metrics and Admin
- Upstream-specific Prometheus metrics with bounded labels
- `/-/udp` endpoint extended with `upstream_flows_active`
- Bridge from in-memory `UdpMetrics` to Prometheus registry

### Synthetic Test Server
- `Socks5UdpTestServer` with configurable modes:
  - No auth, username/password, auth failure, associate failure, echo
- Used for integration testing without external dependencies

## Files Modified

- `crates/eggress-udp/src/flow.rs` - new: flow model types
- `crates/eggress-udp/src/udp_capability.rs` - new: capability classification
- `crates/eggress-udp/src/upstream_socks5.rs` - new: SOCKS5 client handshake
- `crates/eggress-udp/src/relay.rs` - updated: upstream flow handling
- `crates/eggress-udp/src/metrics.rs` - updated: upstream metrics
- `crates/eggress-udp/src/testkit.rs` - updated: synthetic test server
- `crates/eggress-metrics/src/lib.rs` - updated: Prometheus bridge
- `crates/eggress-admin/src/routes.rs` - updated: admin endpoint

## Verification

All checks passed:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo check --workspace`

## Definition of Done Checklist

- [x] One-hop SOCKS5 upstream chains classified as UDP-capable
- [x] HTTP, SOCKS4, multi-hop explicitly unsupported with metrics
- [x] SOCKS5 UDP ASSOCIATE control connection established
- [x] Username/password auth works, failures handled without credential leakage
- [x] Unspecified relay address substituted correctly
- [x] Client packet traverses: client -> Eggress -> upstream -> target -> upstream -> Eggress -> client
- [x] Upstream target flows bounded by target-flow limits
- [x] Upstream control connections close on target-flow idle expiry
- [x] Active upstream leases held while flows active, released on close
- [x] Runtime shutdown closes upstream UDP flows and waits for tasks
- [x] Reload semantics documented and tested
- [x] `/metrics` exposes upstream UDP counters with bounded labels
- [x] `/-/udp` exposes safe upstream summary without client/target leakage
- [x] Unsupported upstream selections visible in logs/metrics
- [x] Docs state only one-hop SOCKS5 UDP upstream supported
- [x] All tests, lint, audit pass
- [x] No unsafe Rust, OpenSSL, or native dependencies introduced
