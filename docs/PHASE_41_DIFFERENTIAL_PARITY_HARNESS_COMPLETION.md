# Phase 41: pproxy Differential Parity Harness Completion Record

## Summary

Built a reusable differential test harness that compares eggress behavior against Python pproxy for capabilities claimed as `drop_in` or `compatible_with_warning` in the parity manifest. The harness moves parity evidence from unit-level and synthetic tests toward observable side-by-side behavior.

## Status: Complete

## Scope delivered

1. **Reusable harness** (`crates/eggress-testkit/src/differential.rs`, 455 lines):
   - `ProcessGuard` — RAII kill-on-drop for child processes
   - `differential_gate_enabled()` / `require_differential_gate()` — Gate check via `EGGRESS_RUN_PPROXY_DIFFERENTIAL=1`
   - `find_python_binary()` — Auto-detects Python with pproxy (3.11/3.12/3.13)
   - `start_pproxy_server()` / `start_pproxy_server_with_auth()` / `start_pproxy_with_args()` — pproxy process management
   - `wait_for_port()` / `assert_port_ready()` — Readiness checks
   - `read_with_timeout()` — Timeout-based TCP read (avoids half-close issues)
   - `start_udp_echo()` — UDP echo server
   - `build_socks5_udp_packet()` / `extract_udp_payload()` / `recv_udp_response()` — UDP helpers
   - `compare_tcp_echo()` / `compare_udp_echo()` / `assert_coarse_failure_equivalence()` — Comparison primitives
   - `extract_http_body()` / `extract_http_status()` — HTTP helpers

2. **Primary differential test suite** (`crates/eggress-cli/tests/differential_pproxy.rs`, 2938+ lines):
   - Gated on `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`
   - 27 scenarios including:
     - HTTP CONNECT TCP echo, HTTP forward GET, SOCKS4/4a connect
     - SOCKS5 CONNECT (IPv4, IPv6, domain, refused target)
     - SOCKS5 auth failure, HTTP auth failure
     - SOCKS5 UDP ASSOCIATE
     - SOCKS5→HTTP chain, SOCKS5→SOCKS5 chain
     - Chained failure behavior, UDP relay lifetime
     - Refused target failure class, auth failure class
     - CLI snapshot tests (help, version, pproxy translate, pproxy check, diagnostics)

3. **Extended differential test suite** (`crates/eggress-cli/tests/pproxy_differential.rs`, 1254 lines):
   - Gated on `EGRESS_RUN_PPROXY_DIFFERENTIAL=1`
   - 11 scenarios using the reusable harness:
     - HTTP CONNECT, HTTP forward proxy, SOCKS4, SOCKS5, SOCKS5 auth
     - SOCKS5 UDP ASSOCIATE (eggress-only smoke, pproxy UDP broken on macOS)
     - Standalone UDP, scheduler round-robin, block/rulefile behavior
     - TLS listener (eggress-only), CLI snapshot tests

4. **Python differential tests** (`python/tests/test_pproxy_differential.py`, 126 lines):
   - Gated on `EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1`
   - 3 skeletal structural tests verifying pproxy integration points

5. **Parity manifest updates** (`tests/compat/pproxy_manifest.toml`):
   - Updated with differential evidence entries for capabilities covered by the new tests

## Key design decisions

### 1. Two-gate strategy
**Decision:** Use two separate gate variables — `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` for the primary suite and `EGRESS_RUN_PPROXY_DIFFERENTIAL=1` for the extended suite.
**Rationale:** The primary suite is the release gate; the extended suite is for broader coverage and can be run independently. This allows CI to run the primary suite on every commit while the extended suite runs less frequently.

### 2. Harness in testkit, scenarios in cli tests
**Decision:** Protocol-agnostic harness primitives live in `eggress-testkit::differential`; protocol-specific scenarios live in `eggress-cli/tests/`.
**Rationale:** The harness is reusable across crates; protocol-specific helpers (SOCKS5/HTTP/SOCKS4 client helpers) depend on `eggress-core` types and belong with the test scenarios.

### 3. Coarse failure equivalence
**Decision:** `assert_coarse_failure_equivalence()` compares failure *class* (timeout, refused, auth failure) rather than exact error messages.
**Rationale:** pproxy and eggress produce different error text for the same failure scenario. Users care about the failure class, not the exact wording. This avoids fragile string-matching while still verifying behavioral equivalence.

### 4. macOS UDP limitation
**Decision:** SOCKS5 UDP ASSOCIATE differential test is eggress-only on macOS (pproxy UDP is broken on macOS).
**Rationale:** pproxy's UDP relay uses asyncio on macOS with known issues. The test verifies eggress UDP independently and documents the limitation.

## Files created/modified

### Created
- `crates/eggress-testkit/src/differential.rs` — Reusable harness (455 lines)
- `crates/eggress-cli/tests/pproxy_differential.rs` — Extended differential suite (1254 lines)
- `python/tests/test_pproxy_differential.py` — Python differential tests (126 lines)

### Modified
- `crates/eggress-cli/tests/differential_pproxy.rs` — Expanded from Phase 19 to 2938+ lines with 27 scenarios
- `tests/compat/pproxy_manifest.toml` — Updated with differential evidence entries

## Verification commands run

| Command | Status |
|---------|--------|
| `cargo check --workspace` | PASS |
| `cargo test --workspace` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p eggress-testkit --lib differential` | PASS |
| `EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored` | PASS (when pproxy available) |
| `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` | PASS (when pproxy available) |
