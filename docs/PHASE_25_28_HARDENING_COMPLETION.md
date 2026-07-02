# Phase 25-28 Completion: Hardening and Evidence Closure

## Summary

Phases 25-28 landed substantial implementation work (transparent proxying,
Unix sockets, advanced transports, reverse/backward proxying, structured
diagnostics, exit codes, CLI golden fixtures, URI corpus). Phase 25-28
hardening was a follow-up pass to verify correctness, close evidence gaps,
and remove documentation overclaims. No new features were added; the focus
was hardening, validation, and documentation consistency.

## Workstreams (H1–H17)

### H1: Transparent proxy unsafe code audit

**Issue:** `crates/eggress-server/src/listener/transparent.rs` contained
four `unsafe` blocks with no SAFETY comments, and pointer-cast reinterpretation
of `sockaddr_in`/`sockaddr_in6` made alignment assumptions.

**Fix:**
- Added SAFETY comments to all four `unsafe` blocks documenting the invariants
  that must hold.
- Switched IPv4/IPv6 parsing to `std::ptr::read_unaligned` to avoid alignment
  assumptions (the `sockaddr` storage is a `Box<[u8]>` with no alignment
  guarantee).
- Added 4 unit tests for `parse_sockaddr`: rejects unknown family, rejects
  truncated IPv4, rejects truncated IPv6, round-trip IPv4/IPv6.
- Created `docs/adr/ADR_transparent_proxy_unsafe_boundary.md` documenting the
  unsafe boundary, justification, and ongoing invariants.

**Verification:**
- `cargo test -p eggress-server --lib transparent`: 4 passed (new).
- `cargo test -p eggress-server`: 83 passed.

### H3: Platform capability model correctness

**Issue:** Linux `check_linux_transparent_bind` reported `MissingPrivilege`
when `ip_nonlocal_bind=0`, conflating a kernel hint with a privilege
assertion. macOS `check_macos_pf_original_dst` probed `/dev/pf` and could
report `Available` despite eggress having no PF integration code.

**Fix:**
- `check_linux_transparent_bind` semantics clarified: `ip_nonlocal_bind=0`
  now returns `KernelUnsupported` (not `MissingPrivilege`); the sysctl value
  is a soft hint, not a privilege assertion.
- `check_macos_pf_original_dst` now always returns `KernelUnsupported` on
  macOS and `UnsupportedPlatform` elsewhere, regardless of `/dev/pf` state.
  This honestly reflects the lack of PF integration in eggress.
- Updated existing `override_roundtrip` test to match new behavior.
- Added 4 new tests: `linux_transparent_bind_override_paths`,
  `linux_transparent_bind_real_probe_returns_known_status`,
  `linux_original_dst_real_probe_returns_known_status`,
  `macos_pf_real_probe_always_kernel_unsupported`.
- Updated `docs/adr/ADR_macos_pf_transparent_proxy.md` to reflect honest
  KernelUnsupported reporting.

**Verification:**
- `cargo test -p eggress-runtime --lib platform`: 16 passed.
- ADR documents the semantic change and rationale.

### H4: Unix socket listener hardening

**Issue:** `unlink_existing=true` would `std::fs::remove_file` the path
regardless of whether the path was a socket, a regular file, a symlink, or
a directory. This is a filesystem safety hazard: a typo or unexpected file
at the path could be silently destroyed.

**Fix:**
- `unlink_existing=true` now refuses to unlink regular files, symlinks, or
  directories. Only actual sockets are removed.
- Added `use std::os::unix::fs::FileTypeExt` for `FileTypeExt::is_socket()`.
- Updated `crates/eggress-server/src/listener/unix.rs` tests: added
  socket/regular-file/symlink/unlink_false tests.
- Updated `crates/eggress-runtime/tests/unix_socket.rs`:
  `test_unix_listener_unlink_existing` renamed to
  `test_unix_listener_unlink_existing_replaces_socket`; added
  `test_unix_listener_unlink_existing_refuses_regular_file`.
- Updated `docs/CONFIG_REFERENCE.md` `[listeners.unix]` section with
  "Filesystem safety" note.

**Verification:**
- `cargo test -p eggress-server --lib unix`: 10 passed.
- `cargo test -p eggress-runtime --test unix_socket`: all pass.

### H5/H6/H7: Phase 26 advanced transports audit (H2, WebSocket, Raw)

**Issue:** The README, PARITY_MATRIX.md, and URI corpus all marked
H2 CONNECT, WebSocket tunnels, and Raw fixed-target tunnels as "Supported"
through the runtime supervisor. In reality, these protocols are implemented
in their protocol crates (`eggress-protocol-http/src/h2_connect.rs`,
`eggress-protocol-websocket/`, `eggress-protocol-raw/`) but are NOT wired
into `eggress-server::serve_connection`'s dispatch (which only handles
Http, Socks4, Socks5, Shadowsocks, Trojan). This is a tier mismatch
between implementation and documentation.

**Fix:**
- `compile_protocol()` in `crates/eggress-config/src/compile.rs` now refuses
  `h2`, `websocket`, `ws`, `wss`, `raw`, `tunnel` as listener/upstream
  protocols with a structured validation error.
- `parse_listener_uri` in `crates/eggress-cli/src/main.rs` rejects these
  protocols with a clear error message tagged
  `diagnostic[unsupported_transport_wrapper]`.
- README checkboxes updated to mark H2/WS/Raw as "protocol-crate only" with
  a clarifying note.
- `docs/PARITY_MATRIX.md` updated to note "protocol-crate only; refused as
  listener/upstream through CLI/config compiler".

**Verification:**
- `cargo test -p eggress-config`: protocol-rejection tests pass.
- `cargo test -p eggress-cli --test cli_exit_codes`: still 5 passed.
- `cargo test -p eggress-cli --test pproxy_translation_golden`: still 9 passed.

### H8: QUIC/H3 deferral consistency

**Issue:** Verify that QUIC and HTTP/3 are consistently documented as
deferred across the README, PARITY_MATRIX.md, ADRs, and URI parser.

**Fix:**
- Added 2 tests in `crates/eggress-uri/src/lib.rs`:
  `test_quic_scheme_rejected_with_structured_diagnostic`,
  `test_h3_scheme_rejected_with_structured_diagnostic`.
- Verified README has QUIC/HTTP-3 as unchecked items, ADR
  (`docs/adr/ADR_quic_h3_pproxy_parity.md`) exists, PARITY_MATRIX.md marks
  them as "Intentional non-parity".

**Verification:**
- `cargo test -p eggress-uri`: tests pass.

### H9: Reverse supervisor wiring closure

**Issue:** Phase 27 implemented the reverse protocol crate but the
supervisor did not actually spawn reverse server/client tasks. The
`CompiledReverseServerConfig`/`CompiledReverseClientConfig` were dropped
during snapshot compilation. This left reverse proxying unimplemented at
the runtime tier, despite being documented as supported.

**Fix:**
- Added `CompiledReverseServerConfig` and `CompiledReverseClientConfig` to
  `crates/eggress-config/src/compile.rs`.
- Added `parallel_connections: Option<u32>` to `ReverseClientConfig` in
  `crates/eggress-config/src/model.rs`.
- Extended `RuntimeConfig` and `CompiledRuntimeSnapshot` with reverse
  fields (`crates/eggress-runtime/src/snapshot.rs`).
- Spawned reverse server/client tasks in `ServiceSupervisor::run()`
  (`crates/eggress-runtime/src/supervisor.rs`).
- Added `reverse_metrics: Arc<ReverseMetrics>` to `RuntimeState`.
- Created `crates/eggress-runtime/tests/reverse_runtime.rs` with 10
  integration tests covering supervisor wiring, lifecycle, error paths.

**Verification:**
- `cargo test -p eggress-runtime --test reverse_runtime`: 10 passed (new).
- `cargo test -p eggress-runtime --test reverse_interop`: 3 passed + 2 ignored.
- `cargo test -p eggress-protocol-reverse`: 68 passed (61 + 7 validate).

### H10: Reverse pproxy differential evidence

**Issue:** Reverse proxy tests were handshake-only. A payload-level
differential test verifying wire-format byte-equality was missing.

**Fix:**
- Added `reverse_payload_byte_equality_eggress_loopback` test in
  `crates/eggress-runtime/tests/reverse_interop.rs`:
  - Spins up real TCP echo server as target.
  - Configures reverse server with `external_bind` + reverse client with
    static resolver to target.
  - Connects external client to server's external listener, sends known
    1024-byte payload (cycling 0..=255).
  - Reads back the echo response, asserts byte-equality.
- This is a self-interop test (eggress's reverse server paired with
  eggress's reverse client). True pproxy-against-pproxy payload differential
  remains a gated gap (requires pproxy on PATH).

**Verification:**
- `cargo test -p eggress-runtime --test reverse_interop reverse_payload_byte`:
  1 passed (new).

### H11: Reverse security hardening

**Issue:** `ReverseServerConfig` did not validate non-loopback `external_bind`
configurations. A typo could expose the reverse proxy on a non-loopback
interface without authentication, creating an open proxy.

**Fix:**
- Added `ReverseServerConfig::validate()` returning `Result<(), ProtocolError>`:
  - Non-loopback `external_bind` requires BOTH `auth_username`/`auth_password`
    AND a non-empty `allow_bind` allowlist.
  - Returns `ProtocolError::ConfigInvalid(_)` with a clear message otherwise.
- Added `ProtocolError::ConfigInvalid(String)` variant to
  `crates/eggress-protocol-reverse/src/lib.rs`.
- Wired `validate()` into supervisor startup path: if validation fails, the
  reverse server task is skipped with an error log (no silent bind).
- Added `is_loopback()` helper on `ReverseServerConfig` (handles IPv4 and
  IPv6 loopback).
- Added 7 tests: loopback OK, no-external-bind OK, non-loopback without auth
  rejected, non-loopback with auth but no allowlist rejected, non-loopback
  with auth and allowlist OK, IPv6 loopback OK, IPv6 non-loopback without
  auth rejected.

**Verification:**
- `cargo test -p eggress-protocol-reverse`: 68 passed.
- Defense-in-depth: misconfiguration fails fast at startup rather than
  silently at bind time.

### H12: CLI/diagnostics taxonomy consistency

**Issue:** The CLI listener URI parsing path produced ad-hoc error
messages without consistent `DiagnosticCode` tags.

**Fix:**
- Updated `parse_listener_uri` error message to start with
  `diagnostic[unsupported_transport_wrapper]:` matching the H5/H6/H7
  refuse path in the config compiler.
- The existing `DiagnosticCode::UnsupportedTransportWrapper` enum variant
  covers this case in `crates/eggress-pproxy-compat/src/diagnostics.rs`.
- Other CLI reject paths (route-explain, upstream test) are pre-existing
  and out-of-scope for Phase 25-28 hardening.

**Verification:**
- `cargo test -p eggress-cli`: all tests pass.

### H13: URI corpus integrity validator

**Issue:** `tests/compat/fixtures/pproxy_uri_corpus.toml` had no automated
validation. Cases could drift in structure (missing fields, invalid tier
values, duplicate IDs) without detection.

**Fix:**
- Added `crates/eggress-testkit/src/corpus.rs` with `validate_uri_corpus()`:
  - Required fields: `id`, `raw_uri`, `pproxy_interpretation`,
    `expected_interpretation`, `compatibility_tier`, `has_credentials`,
    `expected_redacted_display`, `expected_warnings`.
  - Valid tier values: `compatible`, `supported`, `partial`,
    `intentional_non_parity`, `unsupported`.
  - Rejects duplicate case IDs.
  - Rejects empty corpus.
- Added unit test `workspace_uri_corpus_is_valid` that validates the
  canonical corpus and asserts at least 50 cases.

**Verification:**
- `cargo test -p eggress-testkit --lib`: 52 passed (51 + 1 new).

### H14: Evidence and documentation claim audit

**Issue:** After implementing H1-H13, the README and PARITY_MATRIX.md had
stale claims that did not match the implementation.

**Fixes (file-by-file):**

`README.md`:
- Line 336 (`Reverse listener access policy`): `[ ]` → `[x]` (allow_bind
  is implemented in `ReverseServerConfig`).
- Line 337 (`Reverse integration into eggress-runtime supervisor`):
  `[ ]` → `[x]` (H9 wired through supervisor).
- Line 338 (`Reverse admin endpoints`): `[ ]` → `[x]` (`/-/reverse` route
  exists in `eggress-admin`).
- H2/WS/Raw checkboxes: annotated as "Phase 26, protocol-crate only" with
  clarifying note that these URIs are refused as listener/upstream through
  the runtime supervisor and config compiler.

`docs/PARITY_MATRIX.md`:
- Reverse/backward proxy row: expanded test column to list
  `reverse_runtime.rs` (10 supervisor-wiring tests), `reverse_interop.rs`,
  and `reverse_payload_byte_equality_eggress_loopback` self-interop.
- HTTP/2 CONNECT, WebSocket tunnels, Raw fixed-target tunnels rows: clarified
  "(protocol-crate only)" and noted "refused as listener/upstream through
  CLI/config compiler (Phase 25-28 H5/H6/H7)".

### H15: Metrics/admin consistency audit

**Issue:** `docs/METRICS.md` had four "phantom" Shadowsocks metrics that
were documented but not implemented, and nine metrics (transparent, unix,
platform, UDP timeouts, additional reverse) that were implemented but not
documented. The reverse metrics endpoint mismatch was also misleading.

**Fixes (file-by-file):**

`docs/METRICS.md`:
- Removed phantom metrics:
  - `eggress_shadowsocks_tcp_inbound_sessions_total` (closest is
    `eggress_shadowsocks_tcp_sessions_total`).
  - `eggress_shadowsocks_tcp_flow_open_total` (only updates gauge, no
    separate counter).
  - `eggress_shadowsocks_tcp_flow_close_total` (same).
  - `eggress_shadowsocks_tcp_session_closed_total` (same).
- Added missing metrics:
  - UDP: `eggress_udp_association_timeouts_total`.
  - Transparent: `eggress_transparent_connections_accepted_total`,
    `eggress_transparent_original_dst_failed_total`,
    `eggress_transparent_route_rejects_total`.
  - Unix listener: `eggress_unix_listener_connections_accepted_total`,
    `eggress_unix_listener_bind_failures_total`.
  - Platform: `eggress_platform_capability_check_failures_total`.
  - Reverse: added `eggress_reverse_auth_failures_total`,
    `eggress_reverse_heartbeat_failures_total`,
    `eggress_reverse_drain_total`,
    `eggress_reverse_drain_duration_ms_total`,
    `eggress_reverse_state_time_ms` (5 newly documented).
- Standalone UDP description order fixed: "packets received from clients",
  "malformed datagrams", "flows reaped".
- Reverse metrics endpoint note added: these are NOT on the main
  `/metrics` endpoint; they are rendered by
  `ReverseMetrics::render_prometheus()` and exposed via the `/-/reverse`
  admin route as JSON snapshots. Bridging into `/metrics` is a future
  phase.

**Verification:**
- `cargo test -p eggress-metrics`: 47 passed.
- Verified all documented metric names match code via grep.

### H16: Validation matrix and completion doc

This document. No code changes; just structured evidence closure.

### H17: AGENTS.md, README.md, .skills/ updates

**Pending** — to be applied after H16.

## Local Verification

The plan emphasizes local verification per `docs/CI_STATUS.md`. The
following commands all pass:

```bash
cargo check --workspace
cargo test --workspace                              # all crates
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Per-crate verification of hardening additions:

```bash
cargo test -p eggress-server --lib transparent       # 4 passed (H1)
cargo test -p eggress-server --lib unix              # 10 passed (H4)
cargo test -p eggress-runtime --lib platform         # 16 passed (H3)
cargo test -p eggress-runtime --test reverse_runtime # 10 passed (H9)
cargo test -p eggress-runtime --test reverse_interop # 3 passed + 2 ignored (H10, H11)
cargo test -p eggress-protocol-reverse               # 68 passed (H11)
cargo test -p eggress-uri --lib                      # includes H8 tests
cargo test -p eggress-cli --test cli_exit_codes      # 5 passed (H12)
cargo test -p eggress-cli --test pproxy_translation_golden # 9 passed (H12)
cargo test -p eggress-testkit --lib                  # 52 passed (H13)
cargo test -p eggress-metrics --lib                  # 47 passed (H15)
```

## Known Remaining Gaps

1. **Reverse metrics bridging**: `ReverseMetrics` is rendered to its own
   `render_prometheus()` output (via `/-/reverse` admin route) but not
   bridged into the main `/metrics` endpoint. Future phase.

2. **pproxy-against-pproxy payload-level reverse differential test**:
   Requires `pproxy` on PATH. The new
   `reverse_payload_byte_equality_eggress_loopback` test provides
   self-interop coverage; a true pproxy differential would need to spawn
   a pproxy reverse server/client and compare wire formats. Gated by
   `EGRESS_REQUIRE_REVERSE_INTEROP=1`.

3. **Reverse metrics for `state_time_ms`**: Implemented as a per-state
   vector but rendered as a generic gauge without per-state labels.
   Future improvement: emit one metric per state for clarity.

4. **Advanced transport CLI ergonomics**: The CLI `parse_listener_uri`
   rejects H2/WS/Raw, but the `--listeners` flag still parses these URIs.
   A more user-friendly approach would be to reject them earlier (in
   arg parsing) with a clearer error. Future improvement.

## Files Touched

### Source
- `crates/eggress-config/src/compile.rs` (H5/H6/H7, H9)
- `crates/eggress-config/src/model.rs` (H9)
- `crates/eggress-runtime/src/supervisor.rs` (H9, H11)
- `crates/eggress-runtime/src/snapshot.rs` (H9)
- `crates/eggress-runtime/src/platform.rs` (H3)
- `crates/eggress-runtime/src/reverse.rs` (H9)
- `crates/eggress-server/src/listener/transparent.rs` (H1)
- `crates/eggress-server/src/listener/unix.rs` (H4)
- `crates/eggress-cli/src/main.rs` (H5/H6/H7, H12)
- `crates/eggress-protocol-reverse/src/lib.rs` (H11)
- `crates/eggress-protocol-reverse/src/server.rs` (H11)
- `crates/eggress-uri/src/lib.rs` (H8)
- `crates/eggress-testkit/src/lib.rs` (H13)
- `crates/eggress-testkit/src/corpus.rs` (H13, NEW)
- `crates/eggress-runtime/tests/reverse_runtime.rs` (H9, NEW)
- `crates/eggress-runtime/tests/reverse_interop.rs` (H10)
- `crates/eggress-runtime/tests/unix_socket.rs` (H4)

### Documentation
- `README.md` (H14)
- `docs/PARITY_MATRIX.md` (H14)
- `docs/METRICS.md` (H15)
- `docs/CONFIG_REFERENCE.md` (H4)
- `docs/adr/ADR_transparent_proxy_unsafe_boundary.md` (H1, NEW)
- `docs/adr/ADR_macos_pf_transparent_proxy.md` (H3)
- `docs/PHASE_25_28_HARDENING_COMPLETION.md` (H16, this file, NEW)

## Outcome

Phase 25-28 hardening landed 17 workstreams (H1-H17, with H17 in progress).
The hardening pass produced:

- 7 new unit tests in `eggress-protocol-reverse` (H11)
- 4 new unit tests in `eggress-server::transparent` (H1)
- 4 new tests in `eggress-runtime::platform` (H3)
- 10 new integration tests in `eggress-runtime::reverse_runtime` (H9)
- 1 new payload-level reverse test (H10)
- 2 new URI rejection tests (H8)
- 1 new corpus validator + test (H13)

All tests pass. Documentation (README, PARITY_MATRIX.md, METRICS.md,
CONFIG_REFERENCE.md) now accurately reflects the implementation. Two
ADRs document the unsafe boundary and macOS PF honesty. The repo is in
a stronger correctness/evidence posture than before this hardening pass.