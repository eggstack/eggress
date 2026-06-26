# Phase 5 Completion Record: Upstream Protocol Parity

## Date

June 2026

## Summary

Implemented upstream protocol parity as described in
`plans/PHASE_5_UPSTREAM_PROTOCOL_PARITY_PLAN.md`. This phase adds
Shadowsocks and Trojan protocol foundations, polishes HTTP and SOCKS4
upstream behavior, creates a shared capability matrix, and corrects
overstated support claims.

## Corrective Closure

After initial implementation, a corrective pass identified that support
claims were ahead of verified behavior. The following corrections were
applied:

- **Shadowsocks TCP**: Marked experimental — sends encrypted address
  header but does NOT encrypt subsequent bidirectional data. Full stream
  encryption requires a bidirectional AEAD stream adapter.
- **Shadowsocks UDP**: Marked experimental — packet format
  (`nonce + ciphertext`) is non-interoperable with standard Shadowsocks
  servers.
- **Capability classifier**: Shadowsocks TCP and UDP downgraded from
  `Supported` to `UnsupportedProtocol` with experimental notes.
- **Trojan credential model**: Refactored to use `hop.credentials.password`
  for password and `hop.server_name` for TLS SNI (previously overloaded
  username/password fields).
- **Trojan domain length**: Added bounds validation (1-255 chars) for
  domain encoding.
- **TLS dependency**: Fixed `aws-lc-rs` default feature leak by configuring
  `rustls` with `default-features = false`.
- **Phase numbering**: Renamed `PHASE_4_TLS_TRANSPORT_COMPLETION.md` to
  `TRANSPORT_TLS_COMPLETION.md` to avoid conflict with UDP Phase 4.

## Definition of Done

| # | Done Item | Status |
|---|-----------|--------|
| 1 | Upstream protocol capability classification centralized and tested | Done |
| 2 | HTTP CONNECT upstream TCP bounded, authenticated, runtime-tested | Done |
| 3 | SOCKS4/SOCKS4a upstream TCP bounded and runtime-tested | Done |
| 4 | Shadowsocks TCP AEAD methods with deterministic tests | Done (experimental) |
| 5 | Shadowsocks UDP packet encode/decode with tests | Done (experimental, non-interoperable) |
| 6 | Trojan implemented with pure Rust TLS and tests | Done |
| 7 | Metrics and admin expose protocol capability without credential leakage | Done |
| 8 | Config validation rejects unsupported protocol/transport combos | Done |
| 9 | README and protocol docs accurately describe supported subset | Done (corrected) |
| 10 | All tests, lint, audit pass | Done |
| 11 | No unsafe Rust, OpenSSL, or native dependencies | Done |
| 12 | Trojan uses sane credential model (password field, server_name field) | Done (corrective) |
| 13 | Trojan domain length encoding bounded and tested | Done (corrective) |
| 14 | TLS dependency policy documented and enforced | Done (corrective) |

## Support Matrix (Corrected)

| Protocol | TCP CONNECT | UDP relay | Status |
|---|---|---|---|
| HTTP CONNECT | Supported | N/A | Fully tested |
| SOCKS4/SOCKS4a | Supported | N/A | Fully tested |
| SOCKS5 | Supported | Supported (one-hop) | Fully tested |
| Shadowsocks | Experimental | Experimental | Header-only TCP, non-interop UDP |
| Trojan | Supported | N/A | Fully tested (rustls) |

## Files Created/Modified

### New crates
- `crates/eggress-protocol-shadowsocks/` — AEAD ciphers, key derivation, address encoding, TCP connect, UDP encode/decode
- `crates/eggress-protocol-trojan/` — SHA224 password hash, wire format, rustls TLS transport

### New files
- `crates/eggress-core/src/capability.rs` — Shared upstream capability classifier
- `crates/eggress-runtime/tests/upstream_protocols.rs` — Runtime protocol test matrix
- `docs/DEPENDENCY_POLICY.md` — TLS/crypto dependency policy
- `docs/protocols/HTTP_CONNECT.md` — HTTP CONNECT protocol documentation
- `docs/protocols/SOCKS4.md` — SOCKS4/SOCKS4a protocol documentation
- `docs/protocols/SHADOWSOCKS.md` — Shadowsocks protocol documentation (experimental)
- `docs/protocols/TROJAN.md` — Trojan protocol documentation

### Corrective modifications
- `crates/eggress-core/src/capability.rs` — Shadowsocks downgraded to UnsupportedProtocol
- `crates/eggress-core/src/chain.rs` — HopHandler trait accepts `&ProxyHopSpec` instead of `Option<&CredentialSpec>`
- `crates/eggress-server/src/execute.rs` — All handlers updated for new trait; Trojan uses server_name field
- `crates/eggress-protocol-trojan/src/tcp.rs` — Domain length validation added
- `crates/eggress-config/src/lib.rs` — Shadowsocks UDP test updated to rejected
- `Cargo.toml` — rustls configured with `default-features = false` to exclude aws-lc-rs
- `docs/TRANSPORT_TLS_COMPLETION.md` — Renamed from PHASE_4_TLS to avoid numbering conflict
- `EGGRESS_ROADMAP.md` — Phase numbering corrected

## Verification

### All checks passed

- `cargo fmt --all -- --check` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo test --workspace` — all tests pass
- `cargo check --workspace` — clean
