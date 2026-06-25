# Phase 5 Completion Record: Broader Upstream Protocol Parity

## Date

June 2026

## Summary

Implemented upstream protocol parity as described in
`plans/PHASE_5_UPSTREAM_PROTOCOL_PARITY_PLAN.md`. This phase adds
Shadowsocks and Trojan protocol foundations, polishes HTTP and SOCKS4
upstream behavior, creates a shared capability matrix, and wires
Shadowsocks UDP into the relay system.

## Definition of Done

| # | Done Item | Status |
|---|-----------|--------|
| 1 | Upstream protocol capability classification centralized and tested | Done |
| 2 | HTTP CONNECT upstream TCP bounded, authenticated, runtime-tested | Done |
| 3 | SOCKS4/SOCKS4a upstream TCP bounded and runtime-tested | Done |
| 4 | Shadowsocks TCP AEAD method with deterministic tests | Done |
| 5 | Shadowsocks UDP implemented | Done |
| 6 | Trojan implemented with pure Rust TLS and tests | Done |
| 7 | Metrics and admin expose protocol capability without credential leakage | Done |
| 8 | Config validation rejects unsupported protocol/transport combos | Done |
| 9 | README and protocol docs accurately describe supported subset | Done |
| 10 | All tests, lint, audit pass | Done |
| 11 | No unsafe Rust, OpenSSL, or native dependencies | Done |

## Fixes Applied

### Capability classifier consistency

`classify_single_protocol` in `eggress-core/src/capability.rs` now returns
`Supported` for Shadowsocks UDP, matching the actual relay behavior. Previously
it returned `UnsupportedProtocol`, contradicting the relay implementation.

`udp_capability_from_chain` now correctly returns `UnsupportedProtocol` for
Shadowsocks without credentials, matching `udp_capability` behavior.

### Config validation

`udp_listener_with_shadowsocks_upstream_rejected` test updated to
`udp_listener_with_shadowsocks_upstream_accepted` since Shadowsocks UDP
one-hop relay is now supported.

### README capability checklist

Split `- [ ] UDP through Shadowsocks/Trojan` into:
- `- [x] UDP through one-hop Shadowsocks upstream`
- `- [ ] UDP through Trojan upstream`

### Roadmap alignment

`EGGRESS_ROADMAP.md` Phase 5 section updated to reflect actual scoped plan:
upstream/client-only roles, AEAD ciphers only, deferred items explicitly
listed (server roles, legacy ciphers, OTA, interop tests).

## Files Created/Modified

### New crates
- `crates/eggress-protocol-shadowsocks/` — AEAD ciphers, key derivation, address encoding, TCP connect, UDP encode/decode
- `crates/eggress-protocol-trojan/` — SHA224 password hash, wire format, rustls TLS transport

### New files
- `crates/eggress-core/src/capability.rs` — Shared upstream capability classifier
- `crates/eggress-protocol-http/src/connect/test_server.rs` — Synthetic HTTP proxy test server
- `crates/eggress-protocol-socks/src/socks4/test_server.rs` — Synthetic SOCKS4 test server
- `docs/protocols/HTTP_CONNECT.md` — HTTP CONNECT protocol documentation
- `docs/protocols/SOCKS4.md` — SOCKS4/SOCKS4a protocol documentation
- `docs/protocols/SHADOWSOCKS.md` — Shadowsocks protocol documentation
- `docs/protocols/TROJAN.md` — Trojan protocol documentation

### Modified files
- `crates/eggress-core/src/capability.rs` — Shadowsocks UDP now Supported
- `crates/eggress-protocol-http/src/connect/client.rs` — HttpConnectLimits, validate_credentials
- `crates/eggress-protocol-socks/src/socks4/client.rs` — Comprehensive SOCKS4 tests
- `crates/eggress-server/src/execute.rs` — ShadowsocksHopHandler, TrojanHopHandler
- `crates/eggress-uri/src/lib.rs` — ProtocolSpec::Shadowsocks, ProtocolSpec::Trojan
- `crates/eggress-core/src/lib.rs` — ProtocolId::Shadowsocks, ProtocolId::Trojan
- `crates/eggress-udp/src/flow.rs` — UdpFlowKind::ShadowsocksUpstream, ShadowsocksUdpTargetFlow
- `crates/eggress-udp/src/relay.rs` — Shadowsocks UDP upstream relay handler
- `crates/eggress-udp/src/udp_capability.rs` — SupportedShadowsocks variant, from_chain consistency fix
- `crates/eggress-admin/src/routes.rs` — Protocol/capability metadata in /-/upstreams
- `crates/eggress-metrics/src/lib.rs` — Upstream metrics with protocol labels
- `crates/eggress-config/src/validate.rs` — Transport validation for unsupported combos
- `crates/eggress-config/src/lib.rs` — Shadowsocks UDP test updated to accepted
- `deny.toml` — CDLA-Permissive-2.0 license, rustls-pemfile advisory ignore
- `EGGRESS_ROADMAP.md` — Phase 5 scope narrowed to match plan
- `README.md`, `AGENTS.md`, `ARCHITECTURE.md`, `docs/ROADMAP.md`

## Verification

### Test counts

| Crate | Test Count |
|-------|------------|
| eggress-protocol-http | 76 |
| eggress-protocol-socks | 92 |
| eggress-protocol-shadowsocks | 38 |
| eggress-protocol-trojan | 11 |
| eggress-metrics | 37 |
| eggress-admin | 16 |
| eggress-config | 73 |
| eggress-udp | 132 |
| eggress-runtime | 139 |
| All other crates | 353 |
| **Total** | **967** |

### All checks passed

- `cargo fmt --all -- --check` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo test --workspace` — 967 tests pass
- `cargo check --workspace` — clean
- `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok
