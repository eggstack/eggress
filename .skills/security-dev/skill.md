# Security and Robustness

## When to use
Use when working on security features, writing security tests, hardening attack surfaces, or reviewing security invariants.

## DNS rebinding protection

`DirectConnector` in `crates/eggress-core/src/connector.rs` rejects DNS resolutions that resolve to private/reserved IP ranges:

- Loopback (127.0.0.0/8, ::1)
- Link-local (169.254.0.0/16, fe80::/10)
- Private ranges (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
- Unique-local IPv6 (fc00::/7)

This prevents DNS rebinding attacks where a malicious DNS response points to an internal service. The check applies to domain resolution only — explicit IP targets in URIs bypass DNS and are not affected.

New error variant: `ConnectError::ReservedTarget`.

## Auth failure metrics

`eggress_auth_failures_total` counter in `crates/eggress-metrics/src/lib.rs` tracks all inbound authentication failures. `SessionMetrics` trait in `crates/eggress-server/src/lib.rs` includes `record_auth_failure()`.

Incremented at:
- SOCKS5 username/password auth failure
- HTTP Proxy-Authorization failure
- Reverse proxy auth failure

This enables monitoring for brute-force attempts and auth misconfigurations.

## Standalone UDP security

`validate_standalone_target()` in `crates/eggress-udp/src/security.rs` validates UDP relay targets against private/reserved IP ranges (same ranges as DNS rebinding protection). Called from standalone UDP relay paths instead of the weaker `validate_target()`.

Uses `allow_private_egress = true` (pproxy compat default) to allow private targets when explicitly configured.

## Security testing patterns

### Credential leak tests
`crates/eggress-embed/tests/error_redaction.rs` — verifies no credentials appear in error messages, Display impls, or logging.

### Security invariant tests
`crates/eggress-runtime/tests/security_invariants.rs` — runtime security constraints and invariants.

### Fuzz targets (8 total)
All in `fuzz/fuzz_targets/`:
- `uri_parse` — URI parser
- `socks5_handshake` — SOCKS5 method negotiation + CONNECT/UDP_ASSOCIATE
- `socks5_udp_datagram` — SOCKS5 UDP datagram codec
- `http_connect_response` — HTTP CONNECT status/headers
- `trojan_request` — Trojan password + request
- `route_match` — Route matcher evaluation
- `shadowsocks_frame` — Shadowsocks frame parser
- `toml_config` — TOML config parser

### Soak tests (all `#[ignore]`)
In `crates/eggress-runtime/tests/load.rs`:
- `load_test_slowloris_handshake` — slow connection attacks
- `load_test_auth_failure_burst` — auth brute-force simulation
- `load_test_udp_association_churn` — UDP association churn stress

### Security invariants test
`crates/eggress-runtime/tests/security_invariants.rs` — runtime security constraints.

## Running security tests

```bash
# Security invariant tests
cargo test -p eggress-runtime --test security_invariants

# Load/soak tests (ignored by default)
cargo test -p eggress-runtime --test load -- --ignored

# Fuzz smoke tests (no cargo-fuzz needed)
cargo test -p eggress-protocol-socks --test fuzz_smoke
cargo test -p eggress-uri --test fuzz_smoke

# Fuzz targets (requires cargo-fuzz)
cargo fuzz run shadowsocks_frame -- -runs=1000
cargo fuzz run toml_config -- -runs=1000
cargo fuzz run uri_parse -- -runs=1000
cargo fuzz run socks5_handshake -- -runs=1000
cargo fuzz run socks5_udp_datagram -- -runs=1000
cargo fuzz run http_connect_response -- -runs=1000
cargo fuzz run trojan_request -- -runs=1000
cargo fuzz run route_match -- -runs=1000

# Fuzz target compilation check (no cargo-fuzz needed)
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo test --manifest-path fuzz/Cargo.toml --no-run

# Error redaction tests
cargo test -p eggress-embed --test error_redaction

# Full workspace security surface
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

## Security documentation

- `SECURITY.md` — vulnerability disclosure process
- `docs/security/SECURE_CONFIGURATION.md` — secure deployment guide
- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md` — security differences vs pproxy
- `docs/security/THREAT_MODEL.md` — full threat model
- `docs/security/REDACTION_POLICY.md` — credential redaction policy
- `docs/SECURITY_REVIEW.md` — security review and residual risks
- `docs/SECURITY_REVIEW.md` — Phase 50 security gate details

## Track B/C cipher regressions

The Track B/C verification pass surfaced and fixed two cipher defects. Both are now covered by regression tests in `python/tests/test_protocol_cipher.py`:

- `AEADCipher.setup_iv` previously set `self._iv` but did not update `self._current_nonce`, so `encrypt()` after `setup_iv()` would use a stale random nonce. Fix: added an `AEADCipher.setup_iv` override that delegates to `setup_nonce`.
- `BaseCipher.__copy__` re-ran `__init__`, which re-initialized the AEAD nonce. Fix: added `AEADCipher.__copy__` that does a proper shallow `__dict__` copy via `__new__`.

For new AEAD cipher work, also add a known-answer vector test against NIST SP 800-38D or RFC 8439 to `TestAEADKnownAnswerVectors`.
