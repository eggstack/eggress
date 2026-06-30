# Phase 23 Completion: Parity Evidence Tightening and Cleanup

## Summary

Phase 23 tightened the repo's compatibility claims, evidence tracking, and documentation consistency. This was an evidence-hardening pass, not a feature sprint.

## Changes Made

### Manifest Invariant Enforcement (23.1)

- Added `crates/eggress-testkit/src/manifest.rs` with full manifest validation
- `validate_manifest()` checks all status/evidence invariants
- `validate_manifest_file()` parses TOML and validates against pinned version
- `manifest_test_names_exist()` verifies all concrete test names exist in the codebase
- 24 unit tests + 2 integration tests covering all validation rules

### Manifest Status Normalization (23.2)

- Fixed invalid `external_dependency = null` TOML literals (54 occurrences)
- Downgraded URI entries (`http_scheme`, `socks5_scheme`, `socks4_scheme`, `chain_separator`, `auth_in_uri`) from `compatible` to `supported` (synthetic evidence only)
- Downgraded `tls_suffix` from `compatible` to `supported`
- Changed Shadowsocks entries (`shadowsocks_tcp_upstream`, `shadowsocks_upstream`, `ss_scheme`, `shadowsocks_aead_ciphers`, `shadowsocks_password`) from `compatible` evidence to `implemented_interop` (standard interop, not pproxy differential)
- Downgraded scheduler entries (`round_robin_scheduler`, `first_available_scheduler`, `health_aware_skip`) from `compatible` to `supported` (synthetic evidence only)
- Updated `last_updated` to current date

### Manifest-to-Doc Consistency (23.3)

- Created `docs/COMPATIBILITY_EVIDENCE.md` as canonical evidence table
- Linked from README and PARITY_MATRIX.md

### CI Workflow Updates (23.4, 23.5)

- Added manifest validation step to `pproxy-compat.yml`
- Removed `|| true` from parity report generation in pproxy-compat workflow
- Created `.github/workflows/shadowsocks-interop.yml` as separate Shadowsocks interop workflow
- Added artifact upload on failure for both workflows

### Differential Testing Docs (23.6)

- Expanded `docs/DIFFERENTIAL_TESTING.md` with all Phase 19 HTTP/SOCKS test cases
- Added standalone UDP differential test subsection
- Added manifest validation command to "Running Without External Tools" section

### README Claim Discipline (23.7)

- Refined Shadowsocks claims to distinguish standard interop from pproxy parity
- Added protocol scope to differential tests claim
- Ensured standalone UDP claims are precise

### Parity Matrix Corrections (23.8)

- Downgraded SOCKS5 UDP ASSOCIATE from `Compatible` to `Supported` (framing differs)
- Downgraded schedulers from `Compatible` to `Supported`
- Downgraded URI entries from `Compatible` to `Supported`
- Added note that `Compatible` requires manifest `evidence_level = "compatible"` backed by pproxy differential tests
- Updated Shadowsocks notes to clarify standard interop vs pproxy parity

### Manifest Test Name Validation (23.9)

- Added `manifest_test_names_exist()` test that scans Rust/Python source files
- Group aliases (e.g., `integration_tests`, `unit_tests`) are whitelisted
- Concrete test names must exist in the codebase

### Standalone UDP Integration Tests (23.11 follow-up)

Added 10 standalone UDP integration tests to `crates/eggress-runtime/tests/udp.rs` that exercise the standalone pproxy-compatible UDP relay directly (without SOCKS5 TCP control channel):

- `standalone_direct_echo` — direct UDP echo through standalone relay
- `standalone_malformed_short_datagram` — silently drops packets too short for SOCKS5 header
- `standalone_nonzero_frag_dropped` — silently drops FRAG=1 packets
- `standalone_two_clients_same_listener` — two clients on same relay both get responses
- `standalone_two_targets_from_one_client` — one client routes to two different targets
- `standalone_domain_target` — domain name resolution through standalone relay
- `standalone_oversized_datagram_handled` — oversized packets handled without panic
- `standalone_route_reject_drops_packet` — rejected routes drop packets and record metrics
- `standalone_per_client_target_limit` — per-client target flow limit enforced
- `standalone_flow_reuse_allows_same_target` — flow reuse for same target is allowed

These complement the existing standalone unit tests in `eggress-udp/src/standalone.rs` (14 tests) and the differential tests in `differential_pproxy.rs` (7 gated tests).

## Verification

All verification commands pass:

```bash
cargo test -p eggress-testkit  # 43 passed
cargo test --workspace          # all workspace tests pass
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Known Remaining Gaps

- UDP multi-hop chains (not implemented)
- Trojan server/listener (not implemented)
- SSH transport (not implemented)
- Transparent proxy/redir (not implemented)
- HTTP/2, HTTP/3, QUIC, WebSocket (not implemented)
- True pproxy-shaped Python API drop-in replacement
- Legacy Shadowsocks/SSR non-parity (intentional)
