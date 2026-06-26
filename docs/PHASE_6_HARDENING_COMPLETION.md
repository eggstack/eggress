# Phase 6 Hardening Completion Record

## Summary

Phase 6 hardened the existing supported protocol subset with property tests, fuzz
harnesses, runtime lifecycle invariants, observability tests, security reviews,
benchmarks, load tests, and comprehensive documentation. No new protocols were
added. All 11 workstreams are complete. A follow-up commit closed the four
to-acceptance-criteria gaps (full fuzz coverage, HTTP CONNECT upstream latency
benchmark, `cargo audit` recorded, property-test count corrected).

## Commit List

1. Plan archive and CI status docs (WS1, WS2)
2. Property tests for codecs/parsers (WS3)
3. Runtime lifecycle invariants (WS5)
4. Observability privacy tests (WS10)
5. Fuzz smoke harnesses (WS4)
6. pproxy differential tests (WS6)
7. Security review and dependency guardrails (WS7, WS11)
8. Bench/load tests (WS8)
9. Docs and release readiness (WS9)
10. README, AGENTS.md, skills updates
11. Gap closure: full fuzz target coverage (socks5_handshake, http_connect_response, route_match, trojan_request), HTTP CONNECT upstream latency benchmark, `cargo audit` recorded, property-test count corrected to 58

## Final Parity Matrix

See [PARITY_MATRIX.md](PARITY_MATRIX.md) for full feature-by-feature comparison.

| Feature | Eggress | pproxy | Test Coverage |
|---|---|---|---|
| SOCKS5 CONNECT | Supported | Supported | differential_pproxy.rs |
| HTTP CONNECT | Supported | Supported | differential_pproxy.rs |
| SOCKS5 UDP ASSOCIATE | Supported | Supported | differential_pproxy.rs, udp.rs |
| SOCKS5 through HTTP upstream | Supported | Supported | differential_pproxy.rs |
| SOCKS5 through SOCKS5 upstream | Supported | Supported | differential_pproxy.rs |
| SOCKS4 inbound | Supported | Supported | upstream_protocols.rs |
| Trojan TCP upstream | Supported | N/A | upstream_protocols.rs |
| Shadowsocks TCP | Experimental | N/A | Not tested end-to-end |
| QUIC/MASQUE/HTTP3 | Not implemented | N/A | N/A |

## Property/Fuzz Coverage Summary

### Property Tests (58 total)
- `codec_properties.rs` (14): SOCKS5 UDP encode/decode round-trip, FRAG/RSV rejection, domain length
- `connect_properties.rs` (12): HTTP CONNECT credential validation, control char rejection
- `request_properties.rs` (8): Trojan password hash (56 hex chars, deterministic, hex-only)
- `properties.rs` (20): Routing rule matching (reject, direct fallback, CIDR, port, identity, composites)
- URI property tests (4 inline): parse/display/redact round-trip, never-panic

### Fuzz Harnesses
- `fuzz/fuzz_targets/socks5_udp_datagram.rs` — SOCKS5 UDP codec fuzzing
- `fuzz/fuzz_targets/socks5_handshake.rs` — SOCKS5 method negotiation + CONNECT/UDP_ASSOCIATE request parsing
- `fuzz/fuzz_targets/http_connect_response.rs` — HTTP CONNECT response status / authority / header parsing
- `fuzz/fuzz_targets/trojan_request.rs` — Trojan request encoder + password_hash
- `fuzz/fuzz_targets/route_match.rs` — Route matcher evaluation with constructed routers and requests
- `fuzz/fuzz_targets/uri_parse.rs` — URI parser fuzzing
- `crates/eggress-protocol-socks/tests/fuzz_smoke.rs` — smoke test (UDP codec + handshake parsers)
- `crates/eggress-uri/tests/fuzz_smoke.rs` — smoke test

### Smoke Tests
- Fuzz smoke tests pass with arbitrary byte/string inputs

## Differential pproxy Coverage Summary

7 gated differential tests in `crates/eggress-cli/tests/differential_pproxy.rs`:
1. SOCKS5 CONNECT TCP echo (byte-exact match)
2. HTTP CONNECT TCP echo (byte-exact match)
3. SOCKS5 UDP ASSOCIATE direct relay
4. SOCKS5 through HTTP upstream
5. SOCKS5 through SOCKS5 upstream
6. SOCKS5 auth failure behavior
7. HTTP auth failure behavior

All tests are `#[ignore]` and gated on `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`.

## Benchmark/Load Summary

### Benchmarks (Criterion)
- `benches/tcp_relay.rs` — TCP direct relay throughput (1KB, 64KB payloads)
- `benches/udp_relay.rs` — SOCKS5 UDP encode/decode throughput (IPv4/IPv6/Domain)
- `benches/route_match.rs` — Route matcher evaluation (9 rules, 7 request patterns)

### Load Tests (`#[ignore]`)
- `load_test_100_concurrent_tcp_sessions` — 100 concurrent SOCKS5 TCP connections
- `load_test_udp_associations_up_to_limit` — UDP associations up to configured limit

## Security Review Summary

See [SECURITY_REVIEW.md](SECURITY_REVIEW.md) for full review.

### Key Findings
- **No release blockers identified**
- Credential redaction working correctly (URI display, admin, logs)
- UDP broadcast/multicast/unspecified targets rejected
- HTTP CONNECT credentials with control chars rejected
- `unsafe_code = "forbid"` active in all workspace crates
- No OpenSSL/native-tls in dependency tree

### Security Tests (8)
- `security_invariants.rs`: URI redaction, admin credential hiding, control char rejection, dangerous UDP targets, unsupported protocol rejection

## Dependency Policy Verification

- `deny.toml` bans: openssl-sys, native-tls, aws-lc-sys, cmake
- Dependency tree checks: none of the banned crates present
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok
- `unsafe_code = "forbid"` active in workspace lints

## CI/Local Verification Status

- **Hosted CI**: Billing-blocked (GitHub Actions spending limit). No jobs execute.
- **Local verification**: All commands pass locally.
  - `cargo fmt --all -- --check` — clean
  - `cargo check --workspace --all-targets` — clean
  - `cargo test --workspace` — 1094 passed, 11 ignored (load/differential/shadowsocks)
  - `cargo clippy --workspace --all-targets -- -D warnings` — clean
  - `cargo deny check` — ok
  - `cargo audit` — exit 0; one unmaintained warning (rustls-pemfile 2.2.0, RUSTSEC-2025-0134); no vulnerabilities
  - `cargo bench --no-run` — compiles (4 benches: tcp_relay, udp_relay, route_match, http_connect_upstream)

## Remaining Release Blockers

None. Phase 6 definition of done is satisfied:

1. Planning archive/status is coherent
2. CI status documented with local verification fallback
3. Property tests cover main parser/codec surfaces
4. Fuzz smoke harnesses exist for SOCKS5 UDP and URI parsing
5. Runtime lifecycle invariants cover TCP leases, UDP associations, reload, shutdown
6. Observability tests prove metrics/admin do not leak secrets
7. Gated pproxy differential tests exist for core supported subset
8. Security review doc exists with no high-severity findings
9. TCP and UDP benchmarks/load paths exist and are documented
10. Dependency policy enforced by deny.toml
11. All docs current (PARITY_MATRIX, CONFIG_REFERENCE, METRICS, OPERATIONS, TESTING, RELEASE_READINESS)
12. All normal verification commands pass locally
13. No unsupported protocol promoted to supported
14. No unsafe Rust, OpenSSL/native-tls, or unapproved native build dependency introduced
