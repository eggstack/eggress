# Phase B2: Trojan Protocol Completion Record

## Summary

Completed Trojan protocol parity with pproxy across 8 workstreams: reference characterization, client correctness, server correctness (fuzz target), fallback routing, runtime/config/CLI integration, Python exposure, interoperability suite, and UDP decision. All three Trojan capabilities upgraded from `compatible_with_warning` to `drop_in` tier.

## Status: Complete

## Workstream Results

### WS1: Reference Characterization

pproxy Trojan implementation characterized:
- **Wire format**: Standard Trojan spec — SHA224 hex (56 chars) + CRLF + CMD + ATYP + addr + port + CRLF
- **Client ATYP**: Always 0x03 (domain), even for IP addresses — `pproxy` hardcodes this
- **Server fallback**: Protocol chaining (`trojan+tunnel{localhost:80}+ssl://:443#password`) — rolls back 56 bytes on auth failure
- **UDP**: Not supported — `Trojan` class doesn't override `udp_accept`/`udp_connect`
- **Auth check**: Plain `==` comparison (vs eggress's constant-time via `subtle::ConstantTimeEq`)

### WS2: Client Correctness

- **ATYP 0x03 fix**: `encode_trojan_request()` now always uses ATYP 0x03 (domain) for all targets — matches pproxy behavior. IPs converted to string representation.
- **Structured diagnostics**: Added `TrojanDiagnosticCode` enum with 6 variants: `IoError`, `TlsError`, `AuthenticationFailed`, `ConnectionRefused`, `ProtocolViolation`, `InvalidTarget`. Each variant maps via `diagnostic_code()` method.
- **Tests**: Updated 3 unit tests, added 1 new test (`encode_trojan_request_always_uses_atyp_0x_03`), 7 diagnostic code tests.

### WS3: Server Correctness (Fuzz Target)

- Added `fuzz/fuzz_targets/trojan_accept.rs` — feeds arbitrary bytes to `trojan_accept()` via tokio current_thread runtime
- Tests with fixed password, empty password, and long password (from fuzz input)
- Invariant: must never panic for any input
- Registered in `fuzz/Cargo.toml` as `[[bin]]` target; 9 total fuzz targets compile

### WS4: Fallback Routing

Implemented pproxy-compatible fallback behavior:
- **`trojan_check_password()`** in `hash.rs` — checks 56-byte hash vs expected password, constant-time comparison
- **`fallback: Option<String>`** field on `ListenerTrojanConfig` (config model) — format `host:port`
- **Two-phase accept handler** (`accept.rs`): reads 56-byte hash first, checks password; on match → normal Trojan accept; on no match + fallback → relay to fallback target via `PrefixedStream`; on no match + no fallback → `AuthFailed`
- **Config validation**: fallback address validated as `TargetAddr`
- **Tests**: 3 integration tests (`trojan_auth_rejected`, `trojan_fallback_on_auth_failure`, `trojan_correct_password`), 2 config validation tests

### WS5: Runtime/Config/CLI Integration

- Standalone `crates/eggress-runtime/tests/trojan.rs` integration test file with 3 tests covering the full supervisor stack
- Config template uses no upstreams/rules (defaults to direct routing)
- All tests pass individually and together

### WS6: Python Exposure

No changes needed — Trojan already in `supported_features()` and `check_pproxy_uri()`. Parity manifest classifies all Trojan capabilities as `python = "not_applicable"`. 4 existing URI corpus test cases cover Trojan.

### WS7: Interoperability Suite

Added Trojan differential tests to `crates/eggress-cli/tests/pproxy_differential.rs`:
- **`differential_trojan_upstream`**: Starts pproxy (`trojan+ssl://127.0.0.1:{port}#password --ssl cert,key -r direct`) and eggress (TOML with trojan listener), sends payload through both, compares via `compare_tcp_echo`
- **`differential_trojan_auth_failure`**: Same setup with wrong password, both should fail — compares via `assert_coarse_failure_equivalence`
- **`send_through_trojan` helper**: Connects TCP, creates TLS config, calls `trojan_connect()`, writes payload, reads response

### WS8: UDP Decision

pproxy does NOT support Trojan UDP. Manifest correctly has `no_udp` constraint applying to `["http", "socks4", "socks4a", "trojan"]`. Composition matrix: `trojan × upstream × udp` = `unsupported`. No changes needed.

## Manifest Changes

### Capability Tier Upgrades

| Capability | Before | After | Evidence |
|---|---|---|---|
| `uri.scheme_trojan` | `compatible_with_warning` | `drop_in` | unit → differential |
| `protocol.trojan_client` | `compatible_with_warning` | `drop_in` | unit → differential |
| `protocol.trojan_server` | `compatible_with_warning` | `drop_in` | unit → differential |

### Composition Matrix Updates

| Cell | Before | After |
|---|---|---|
| `trojan × listener × tcp` | `compatible_with_warning` (integration) | `drop_in` (differential) |
| `trojan × upstream × tcp` | `compatible_with_warning` (integration) | `drop_in` (differential) |
| `trojan × upstream × udp` | `unsupported` (none) | unchanged |

### Aggregate Counts

| Tier | Before | After |
|---|---|---|
| `drop_in` | 84 | 87 |
| `compatible_with_warning` | 19 | 16 |
| `native_equivalent` | 14 | 14 |
| `intentional_non_parity` | 17 | 17 |
| `unsupported` | 5 | 5 |

## Acceptance Criteria

| Criterion | Status | Evidence |
|---|---|---|
| ATYP 0x03 matches pproxy | ✓ | `encode_trojan_request_always_uses_atyp_0x_03` test |
| Fallback routing on auth failure | ✓ | `trojan_fallback_on_auth_failure` integration test |
| Structured diagnostic codes | ✓ | `TrojanDiagnosticCode` with 6 variants, 7 tests |
| Differential tests pass | ✓ | `differential_trojan_upstream`, `differential_trojan_auth_failure` |
| Fuzz target compiles | ✓ | `cargo check --manifest-path fuzz/Cargo.toml --bins` |
| Manifest validates | ✓ | 139 capabilities, all pass strict validation |
| Composition matrix validates | ✓ | 31 cells, 4 chains, 5 constraints |
| Existing tests unaffected | ✓ | 868 pass, 0 fail, 30 ignore |

## Verification Commands

```bash
cargo test -p eggress-protocol-trojan
cargo test -p eggress-config -- trojan
cargo test -p eggress-pproxy-compat -- trojan
cargo test -p eggress-runtime --test trojan
cargo test -p eggress-testkit canonical_manifest
cargo test -p eggress-testkit parity_manifest_consistency
cargo test -p eggress-testkit composition
cargo check --manifest-path fuzz/Cargo.toml --bins
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-matrix docs/parity/composition_matrix.toml docs/parity/pproxy_capability_manifest.toml
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- differential_trojan
```
