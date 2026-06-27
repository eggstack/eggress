# Corrective Parity Audit — Completion Record

**Date**: June 2026

## Summary

Phase 7–12 landed substantial pproxy-parity work (URI translation, capability classifier, scheduler defaults, Shadowsocks/Trojan upstream, multihop chains, differential tests). However, several claims were overstated or incorrect. This audit corrected them: downgrading non-standard Shadowsocks TCP framing, fixing a scheduler default bug, closing a Trojan multihop test gap, and documenting all findings.

## Audit Outcomes

### Shadowsocks TCP Framing

- **Finding**: Non-standard AEAD TCP framing with 3 critical deviations from standard Shadowsocks:
  1. Cleartext ciphertext-length prefix (standard uses encrypted length chunk)
  2. Single AEAD operation per chunk (standard uses 2 separate operations)
  3. Nonce increment by 1 (standard increments by 2)
- **Impact**: Not wire-compatible with shadowsocks-rust, shadowsocks-libev, or other standard implementations
- **Decision**: Downgraded to Experimental. Not corrected in this pass — significant rework required
- **Audit doc**: `docs/protocols/SHADOWSOCKS_TCP_AUDIT.md`

### Shadowsocks UDP

- **Finding**: Standard-compliant. Packet layout matches `salt + AEAD(nonce=0, address+payload)`
- **Status**: Supported. No changes needed to code.
- **Documentation fix**: Updated stale references in SHADOWSOCKS_PARITY.md

### pproxy Repeated Remote Semantics

- **Finding**: Multiple `-r` remotes correctly become upstream groups (not chains), matching pproxy behavior
- **Bug fixed**: Default scheduler was `first-available` for all cases; changed to `round-robin` when `len(remotes) > 1`
- **Tests**: Added `test_scheduler_default_round_robin_for_multiple_remotes`

### Trojan Multihop Test

- **Finding**: Test gap closed. Added SOCKS5 → Trojan → TCP echo test
- **Approach**: Test-only insecure TLS via `TlsClientConfigBuilder::with_insecure()` injected through `ServiceSupervisor::with_tls_client_config()`
- **No production insecure default introduced**

### Capability Classifier

- **Change**: Shadowsocks TCP downgraded from `Supported` to `UnsupportedProtocol("Shadowsocks-tcp-nonstandard-framing")`
- **Shadowsocks UDP**: Remains `Supported`
- **Security invariants test**: Updated to expect new classification

### Differential Test Environment

- **Created**: `docs/DIFFERENTIAL_TESTING.md` with Python version requirements, pproxy install instructions, env vars, and known failures
- **Status**: Gated tests remain unrunnable on Python 3.14. Documented as unverified.

## Files Created

- `docs/protocols/SHADOWSOCKS_TCP_AUDIT.md`
- `crates/eggress-cli/tests/interoperability_shadowsocks.rs`
- `docs/DIFFERENTIAL_TESTING.md`

## Files Modified

| File | Change |
|------|--------|
| `crates/eggress-core/src/capability.rs` | Shadowsocks TCP downgrade |
| `crates/eggress-pproxy-compat/src/translate.rs` | Scheduler default fix |
| `crates/eggress-server/src/lib.rs` | TLS config field addition |
| `crates/eggress-server/src/execute.rs` | TLS override threading |
| `crates/eggress-runtime/src/supervisor.rs` | TLS config builder |
| `crates/eggress-runtime/Cargo.toml` | Dev-dependencies |
| `crates/eggress-runtime/tests/multihop_tcp.rs` | Trojan chain test |
| `crates/eggress-runtime/tests/security_invariants.rs` | Updated assertion |
| `crates/eggress-cli/Cargo.toml` | Dev-dependency |
| `crates/eggress-cli/src/main.rs` | TLS config field |
| `crates/eggress-cli/tests/differential_pproxy.rs` | TLS config field |
| `crates/eggress-cli/tests/interoperability_pproxy.rs` | TLS config field |
| `crates/eggress-cli/tests/interoperability_curl.rs` | TLS config field |
| `crates/eggress-cli/tests/pproxy_cli.rs` | Updated scheduler assertion |
| `README.md` | Downgraded Shadowsocks TCP claims |
| `AGENTS.md` | Added test commands, audit facts |
| `docs/PARITY_MATRIX.md` | Downgraded TCP tier |
| `docs/protocols/SHADOWSOCKS.md` | TCP status downgrade |
| `docs/protocols/SHADOWSOCKS_PARITY.md` | Fixed stale UDP refs, updated status |
| `docs/PHASE_7_PPROXY_PARITY_SPEC_COMPLETION.md` | Corrective audit notice |
| `docs/PHASE_8_PPROXY_COMPAT_CLI_URI_COMPLETION.md` | Corrective audit notice |
| `docs/PHASE_9_SHADOWSOCKS_TCP_PARITY_COMPLETION.md` | Corrective audit notice |
| `docs/PHASE_11_REMAINING_PROTOCOL_PARITY_COMPLETION.md` | Corrective audit notice |
| `docs/PHASE_12_SCHEDULER_CHAIN_FAILURE_PARITY_COMPLETION.md` | Removed Trojan blocker |
| `docs/PPROXY_PARITY_SPEC.md` | Updated scheduler notes |
| `.skills/testing/skill.md` | Added interop test info |
| `.skills/rust-proxy-dev/skill.md` | Added capability classifier note |

## Remaining Blockers for Phase 13

- Shadowsocks TCP framing is non-standard; correction would require significant rework
- pproxy differential tests still gated (Python 3.14 incompatibility)
- No inbound Shadowsocks listener
- No persistent HTTP forwarding

## Local Verification Commands and Results

```bash
cargo fmt --all -- --check                                          # PASS
cargo check --workspace                                             # PASS
cargo clippy --workspace --all-targets -- -D warnings               # PASS
cargo test --workspace                                              # PASS (all tests green)
cargo deny check                                                    # PASS (advisories ok, bans ok, licenses ok, sources ok)
cargo audit                                                         # PASS (1 allowed warning: rustls-pemfile unmaintained)
```
