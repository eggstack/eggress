# Phase 5 Corrective Closure Completion Record

## Date

June 2026

## Purpose

Phase 5 added upstream protocol parity work: HTTP CONNECT polish, SOCKS4/SOCKS4a,
Shadowsocks, Trojan, a shared capability classifier, and a TLS transport layer.
The implementation volume was real, but support claims were ahead of verified behavior.

This corrective pass enforced: protocol support is not advertised unless the
implementation is interoperable, bounded, configured cleanly, runtime-tested,
and documented accurately.

## Completion Criteria (all 12 met)

| # | Criterion | Status |
|---|-----------|--------|
| 1 | No protocol marked supported without full runtime coverage | PASS |
| 2 | Shadowsocks marked experimental/unsupported everywhere | PASS |
| 3 | Shadowsocks cannot accidentally send plaintext under "supported" label | PASS |
| 4 | Trojan uses sane password/server-name model | PASS |
| 5 | Trojan domain length encoding bounded and tested | PASS |
| 6 | TLS dependency policy explicit and enforced | PASS (corrected) |
| 7 | Phase numbering no longer conflicts with UDP Phase 4 | PASS |
| 8 | HTTP CONNECT and SOCKS4/SOCKS4a runtime tests exist | PASS |
| 9 | README, roadmap, protocol docs, admin output all agree | PASS |
| 10 | CI/status checks visible | PASS |
| 11 | All workspace tests, lint, audit pass | PASS |
| 12 | No unsafe Rust or unapproved native dependency | PASS (corrected) |

## Commit Sequence

### Commit 1: Phase 5 corrective closure (8035766)
- Shadowsocks capability downgraded to `UnsupportedProtocol`
- Trojan credential model refactored (`hop.credentials.password` + `hop.server_name`)
- Trojan domain length validation (1-255)
- `rustls` configured with `default-features = false`
- Phase numbering corrected (TLS doc renamed)
- Runtime protocol test matrix added
- Documentation truth pass across all files

### Commit 2: TLS dependency leak fix (pending)
- `tokio-rustls` configured with `default-features = false, features = ["logging", "tls12"]`
- Eliminates `aws-lc-sys` and `cmake` from production dependency graph
- `DEPENDENCY_POLICY.md` updated with corrected verification commands
- Completion record created

## Final Support Matrix

| Protocol | TCP CONNECT | UDP relay | Status | Runtime test |
|---|---|---|---|---|
| HTTP CONNECT | Supported | N/A | Fully tested | `upstream_protocols.rs` |
| SOCKS4/SOCKS4a | Supported | N/A | Fully tested | `upstream_protocols.rs` |
| SOCKS5 | Supported | Supported (one-hop) | Fully tested | `udp_upstream.rs`, `upstream_protocols.rs` |
| Shadowsocks | Experimental | Experimental | Header-only TCP, non-interop UDP | Ignored |
| Trojan | Supported | N/A | Fully tested (rustls) | `upstream_protocols.rs` |

## Dependency Policy

### Production dependencies (no native build tools)

- `rustls` with `ring` provider (`default-features = false`)
- `tokio-rustls` with `logging`, `tls12` only (`default-features = false`)
- No `aws-lc-rs`, `aws-lc-sys`, `cmake`, `cc`, `openssl-sys`

### Dev-only native deps (test infrastructure only)

- `rcgen` → `aws-lc-rs` (used for test certificate generation)
- Not compiled into production builds

### Verification commands

```bash
cargo tree -i aws-lc-sys -e normal  # should error (not found)
cargo tree -i cmake -e normal        # should error (not found)
cargo tree -i openssl-sys -e normal  # should error (not found)
cargo deny check                     # advisories ok, bans ok, licenses ok, sources ok
```

## Files Modified/Created in Corrective Pass

### Corrective modifications
- `Cargo.toml` — `tokio-rustls` default features disabled
- `crates/eggress-core/src/capability.rs` — Shadowsocks downgraded
- `crates/eggress-core/src/chain.rs` — HopHandler accepts `&ProxyHopSpec`
- `crates/eggress-server/src/execute.rs` — Handlers updated for new trait
- `crates/eggress-protocol-trojan/src/tcp.rs` — Domain length validation
- `crates/eggress-udp/src/udp_capability.rs` — Shadowsocks UDP unsupported
- `crates/eggress-config/src/lib.rs` — Shadowsocks UDP config test rejected
- `EGGRESS_ROADMAP.md` — Phase numbering corrected
- `docs/DEPENDENCY_POLICY.md` — Updated with tokio-rustls config and verification

### New files
- `crates/eggress-runtime/tests/upstream_protocols.rs` — Runtime protocol tests
- `docs/DEPENDENCY_POLICY.md` — Crypto/TLS dependency policy
- `docs/PHASE_5_CORRECTIVE_CLOSURE_COMPLETION.md` — This document

### Renamed
- `docs/PHASE_4_TLS_TRANSPORT_COMPLETION.md` → `docs/TRANSPORT_TLS_COMPLETION.md`

## Final Verification

```bash
cargo fmt --all -- --check          # clean
cargo clippy --workspace --all-targets -- -D warnings  # clean
cargo test --workspace              # all pass (2 ignored: Shadowsocks experimental)
cargo deny check                    # advisories ok, bans ok, licenses ok, sources ok
cargo tree -i aws-lc-sys -e normal  # not found
cargo tree -i cmake -e normal       # not found
```
