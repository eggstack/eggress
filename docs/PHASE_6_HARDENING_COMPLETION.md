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

### Build-time-only dependencies

These dependencies are present in the workspace but **never enter production
binary artifacts**:

- `criterion` (HTML benchmark reports): workspace dep of the root
  `eggress-bench` package, which declares only `[[bench]]` targets and no
  `[[bin]]` / `[lib]`. Criterion is compiled only when building benchmarks
  (`cargo bench` or `cargo build --benches`); `cargo build --bins
  --release` does not pull it in. The deliverable `eggress-cli` binary is
  unaffected.
- `libfuzzer-sys` (fuzz harness): workspace dep of the standalone `fuzz/`
  workspace, which declares its own `[workspace]` block and is not a member
  of the main `eggress-bench` / `crates/*` workspace. `cargo build
  --workspace` and `cargo test --workspace` never compile it; only
  `cargo build --manifest-path fuzz/Cargo.toml` does.

## Observability / Lifecycle Test Depth

### Observability Tests (9)

All observability tests are **semantic**: they parse the Prometheus output
and assert on counter/gauge values, not just string presence. Helpers
`metric_value`, `metric_value_with_labels`, and `label_keys` are defined in
the test file. They correctly handle the `prometheus-client` 0.22 quirk of
unconditionally appending `_total` to counter names (e.g. the registered
`eggress_connections_total` is encoded as `eggress_connections_total_total`).
Specifics:

- `metrics_renders_after_direct_tcp_session`: parses
  `eggress_connections_total`, `eggress_connection_failures_total`,
  `eggress_bytes_upstream_total`, `eggress_connections_active` and asserts
  the values reflect the session (≥ 1 connection, ≥ 5 bytes upstream,
  failures == 0, active == 0 after close).
- `metrics_renders_after_udp_direct_association`: parses
  `eggress_udp_associations_total`, `eggress_udp_packets_up_total`,
  `eggress_udp_packets_down_total` and asserts ≥ 1 each.
- `metrics_renders_after_upstream_relay`: parses
  `eggress_route_decisions_total{outcome="selected"}` and
  `eggress_connections_total` and asserts ≥ 1; verifies
  `eggress_upstream_open_total` is registered (HELP+TYPE present).
- `metrics_renders_with_upstream_group_for_udp`: parses
  `eggress_udp_associations_total` and asserts ≥ 1; verifies
  `eggress_route_decisions_total` and `eggress_upstream_open_total` are
  registered.
- `route_decision_counters_increment`: parses
  `eggress_route_decisions_total{outcome="selected"}` and
  `eggress_route_decisions_total{action="direct"}` and asserts ≥ 1 each.
- `udp_active_gauges_return_to_zero_after_close`: parses
  `eggress_udp_associations_active` and asserts == 0 after TCP close.
- `metrics_no_secrets_in_labels`: enumerates **all** label keys across the
  body and asserts none of `client`, `client_addr`, `client_ip`, `source`,
  `target`, `target_host`, `dst`, `username`, `password`, `token`, `secret`,
  `payload`, `credential` appear as keys (catches high-cardinality label
  regressions). Also asserts known payload/client/target strings do not
  appear in the body.
- `admin_upstreams_redact_credentials`: parses the `/-/upstreams` JSON and
  asserts configured credentials (`supersecret`, `hunter2`) do not appear.
- `admin_route_explain_no_secrets`: POSTs a route-explain request and
  asserts configured upstream passwords do not appear in the response.

### Lifecycle Invariant Tests (11)

All lifecycle tests are **deterministic**: they poll for the post-close
invariant within a deadline rather than sleeping a fixed duration. A
`wait_for(deadline, predicate, msg)` helper drives the polling at 20ms
steps. Specifics:

- `tcp_active_lease_increments_and_decrements`: opens a SOCKS5 session
  through an upstream, polls `active` and `in_flight` until both return to
  0 (deadline 5s).
- `failed_upstream_connect_does_not_increment_active`: opens a SOCKS5
  session against a refusing upstream, polls `active` and `in_flight` until
  both are 0.
- `udp_association_close_removes_registry_entry`: opens a UDP ASSOCIATE,
  closes TCP control, polls `state.udp_registry.active_count()` until 0.
- `shutdown_drains_active_tcp_sessions_within_grace`: triggers shutdown,
  asserts `active_connections` reaches 0 and elapsed < grace budget.
- `shutdown_cancels_udp_and_leaves_counts_zero`: same for UDP shutdown.
- `reload_failure_preserves_generation`, `reload_atomically_swaps_snapshot_*`,
  and 4 unsupported-protocol-combo rejection tests: deterministic, no
  sleeps.

The 11 lifecycle tests complete in ~0.3 s with the poll-wait pattern (vs.
the previous `sleep(200ms..500ms)` baseline that was an upper-bound only).

### Known Production Wiring Gaps Surfaced

The strengthened observability tests surfaced one real pre-Phase-6 wiring gap
that is now tracked for a follow-up phase:

- `eggress_upstream_open_total` is **registered** in the metrics registry
  with HELP/TYPE comments but **not yet called** from the TCP chain executor
  (`record_upstream_open` is only exercised by unit tests in
  `eggress-metrics/src/lib.rs`). The HTTP CONNECT, SOCKS5, SOCKS4, Trojan,
  and Shadowsocks chain handlers do not increment it after a successful
  upstream connect.
- `eggress_upstream_open_failures_total` has the same gap
  (`record_upstream_failure` is only exercised by unit tests).

The observability tests assert HELP/TYPE registration for these metrics;
a future phase should wire the call sites and promote the assertions to
value-based checks.

## CI Test Coverage

The hosted CI workflow (`.github/workflows/ci.yml`) invokes:

- `cargo check --workspace --all-targets` (3 OS matrix)
- `cargo test --workspace` (3 OS matrix) — runs all per-crate test suites
  including the new observability, lifecycle, security, fuzz smoke, and
  property tests
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo deny check`
- `cargo audit`
- `cargo test --test interoperability_pproxy` (env-gated)
- `cargo test --test interoperability_curl` (env-gated)

The `differential_pproxy.rs` test file is **not** invoked in CI. It is a
standalone test file with `#[ignore]` annotations and a runtime env-var
panic (`EGRESS_REQUIRE_EXTERNAL_INTEROP=1`) so it is exercised only via the
opt-in command:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli \
    --test differential_pproxy -- --ignored
```

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
