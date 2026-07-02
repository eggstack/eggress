# Phase 25-28 Hardening Plan: Runtime Integration, Safety Review, and Evidence Closure

## Purpose

Phases 25-28 landed substantial implementation work: transparent proxying, Unix sockets, advanced transports, reverse/backward proxying, structured diagnostics, exit codes, CLI golden fixtures, URI corpus, and documentation updates. The repo is now more capable, but the implementation pace created a higher correctness and evidence burden.

This hardening pass treats the Phase 25-28 work as a strong foundation, not yet as release-grade closure. The goal is to consolidate, audit, downgrade overclaims where needed, and add missing runtime/differential evidence before continuing feature expansion.

The core outcome should be a repository where every Phase 25-28 claim falls into one of these states:

1. Runtime-integrated and evidence-backed.
2. Protocol-crate-level support only, explicitly labeled as such.
3. Configuration/translation support only, explicitly labeled as such.
4. Intentionally deferred or unsupported, with stable diagnostics.

## Current observed shape

The implementation burst since the Phase 28 plan changed roughly 100 files and added multiple new surfaces:

- `crates/eggress-runtime/src/platform.rs` for platform capability detection.
- `crates/eggress-server/src/listener/transparent.rs` for Linux original-destination recovery.
- `crates/eggress-server/src/listener/unix.rs` for Unix socket listener support.
- `crates/eggress-protocol-http/src/h2_connect.rs` for HTTP/2 CONNECT handling.
- `crates/eggress-protocol-websocket/` for WebSocket tunnel handling.
- `crates/eggress-protocol-raw/` for raw fixed-target tunnels.
- `crates/eggress-protocol-reverse/` for pproxy-compatible reverse/backward raw relay.
- `crates/eggress-runtime/src/reverse.rs` and admin reverse registry plumbing.
- `crates/eggress-pproxy-compat/src/diagnostics.rs` and `exit_codes.rs` for CLI/process hardening.
- CLI subprocess tests, URI golden corpus, pproxy CLI fixtures, and Python reverse URI helper tests.

The shape is promising, but the next pass should focus on safety and truthfulness rather than more features.

## Non-goals

Do not add new major protocol features in this hardening pass.

Do not implement SSH, MASQUE, H3/QUIC, system proxy settings, SSR, legacy Shadowsocks, or Trojan server mode.

Do not promote any Phase 25-28 feature to `compatible` unless real pproxy differential evidence exists and the feature is runtime-integrated through normal Eggress configuration.

Do not hide incomplete supervisor/runtime wiring behind documentation claims.

## Work items

### H1. Phase 25 transparent proxy unsafe and platform audit

The transparent listener uses `libc::getsockopt`, `sockaddr_storage`, and unsafe sockaddr casts. This needs a focused safety review.

Tasks:

- Audit every `unsafe` block in `crates/eggress-server/src/listener/transparent.rs`.
- Add `// SAFETY:` comments explaining every invariant before each unsafe block.
- Verify `getsockopt` length handling for IPv4 and IPv6.
- Verify byte-order conversion for IPv4 address, IPv6 address, and port.
- Verify `SO_ORIGINAL_DST` constant usage for IPv6. If Linux requires a different optname or level, fix or restrict IPv6 support.
- Confirm `libc::sockaddr_storage` alignment and cast assumptions.
- Add tests for `parse_sockaddr` using synthetic `sockaddr_in` and `sockaddr_in6` values.
- If workspace or crate policy forbids unsafe, add a narrow documented exception rather than silently weakening the policy.
- Add `docs/adr/ADR_transparent_proxy_unsafe_boundary.md` if unsafe remains.

Acceptance:

- All unsafe blocks are documented.
- IPv4 and IPv6 sockaddr parsing has direct tests.
- Manifest/docs do not claim IPv6 transparent support if IPv6 original-destination recovery is not proven.

### H2. Transparent proxy runtime behavior and evidence classification

The current transparent implementation should remain `Supported` unless real pproxy differential/gated platform evidence exists.

Tasks:

- Verify supervisor integration for transparent listeners actually routes the recovered original destination through normal routing policy.
- Confirm private-target, loop-prevention, and route rejection behavior applies to recovered destinations.
- Add integration tests using injected/mocked original destinations rather than root-only iptables.
- Add a gated Linux interop test scaffold under `EGRESS_REQUIRE_TRANSPARENT_INTEROP=1`.
- Document exact iptables/nftables setup and cleanup commands.
- Ensure metrics distinguish:
  - accept success;
  - original destination lookup failure;
  - route rejection;
  - relay failure.
- Confirm admin/status exposes transparent listener mode and capability state.

Acceptance:

- Ungated transparent tests verify routing and rejection behavior.
- Gated test scaffold exists and skips clearly unless enabled.
- Parity docs keep transparent proxy as `Supported`, not `Compatible`, until pproxy-gated evidence exists.

### H3. Platform capability model correctness

`platform.rs` currently uses procfs and platform probes. These checks must not overstate capability.

Tasks:

- Review `LinuxTransparentBind` detection. `ip_nonlocal_bind=1` is not the same as having `CAP_NET_ADMIN`/`CAP_NET_RAW` or successfully setting `IP_TRANSPARENT`; avoid naming it available if it only indicates sysctl state.
- Split capability status if necessary:
  - `KernelFeaturePresent`;
  - `PrivilegeAvailable`;
  - `RuntimeProbeSucceeded`.
- Add a real socket-option probe for transparent bind if feasible.
- Add tests using overrides for every capability/status path.
- Ensure Windows and macOS return deterministic statuses without filesystem side effects.
- Review macOS PF capability detection: `/dev/pf` existence does not imply original destination recovery is implemented.

Acceptance:

- Capability checks do not overclaim.
- Docs explain the difference between detected kernel support and privilege/runtime success.

### H4. Unix socket listener hardening

Unix socket support needs filesystem safety review.

Tasks:

- Audit `crates/eggress-server/src/listener/unix.rs` for unlink behavior.
- Ensure Eggress never unlinks arbitrary files by default.
- Add symlink and existing-regular-file tests.
- Add tests for:
  - normal bind/connect;
  - cleanup on shutdown;
  - `unlink_existing = false` failure;
  - `unlink_existing = true` only for socket files;
  - permission/mode behavior if implemented;
  - Windows unsupported diagnostics.
- Confirm Unix socket listener can run HTTP CONNECT and SOCKS5 if docs claim that.
- Confirm route identity and source metadata do not assume TCP peer addresses.

Acceptance:

- Filesystem safety tests exist.
- Docs specify exact cleanup behavior.
- Manifest distinguishes Unix socket transport support from protocol compatibility over Unix sockets.

### H5. Phase 26 H2 implementation audit

The current H2 implementation appears protocol-crate-level and direct-to-target. It must not be described as full runtime pproxy parity unless it is integrated through routing/config/listeners.

Tasks:

- Correct docs that refer to `crates/eggress-protocol-h2/src/`; current code lives in `crates/eggress-protocol-http/src/h2_connect.rs` unless the crate is later split.
- Audit whether H2 CONNECT is exposed through config and supervisor runtime.
- If not runtime-integrated, downgrade docs/manifest to `Experimental` or `Supported (protocol crate only)`.
- Ensure H2 target connection goes through routing/policy when runtime-integrated; direct `TcpStream::connect(target.to_string())` should not bypass policy in a production listener.
- Add authentication handling or explicitly document no auth support.
- Add tests for:
  - missing `:authority`;
  - invalid authority;
  - non-CONNECT method reset;
  - target connection failure status/reset behavior;
  - flow-control behavior;
  - cancellation and stream reset cleanup.
- Add metrics for H2 stream open/close/reset only if runtime-integrated.

Acceptance:

- H2 docs, manifest, and parity matrix accurately state whether support is protocol-crate-only or runtime-usable.
- H2 does not bypass route policy when exposed as a listener/upstream.

### H6. WebSocket tunnel integration audit

WebSocket support must be classified by actual integration depth.

Tasks:

- Inspect `crates/eggress-protocol-websocket` for frame size limits, binary/text frame semantics, ping/pong, close behavior, and fragmentation handling.
- Verify config model can express `ws://` and `wss://` listeners/upstreams if docs claim support.
- Verify supervisor runtime spawns WS/WSS listeners/upstreams if docs claim runtime support.
- Add tests for:
  - binary relay;
  - text frame behavior;
  - fragmented message behavior;
  - oversized frame rejection;
  - close frame propagation;
  - ping/pong;
  - WSS TLS behavior if claimed.
- Ensure WS/WSS URI credentials are redacted in diagnostics.

Acceptance:

- WS support is either runtime-integrated and tested or explicitly labeled protocol-crate-level.
- Manifest and compatibility evidence distinguish WS and WSS.

### H7. Raw tunnel integration audit

Raw fixed-target tunnels are easy to accidentally turn into an unsafe open proxy.

Tasks:

- Verify raw listener config requires a fixed target or explicit route policy.
- Reject raw listener config without a target.
- Ensure raw relay still flows through policy if configured.
- Add tests for fixed target, missing target, route rejection, half-close, large payload, and shutdown.
- Confirm raw tunnel URI translation cannot accidentally create arbitrary open-proxy behavior.
- Add metrics for raw sessions only if runtime-integrated.

Acceptance:

- Raw tunnel behavior is explicit and safe by default.
- Docs explain that raw listeners have no application-layer negotiation.

### H8. QUIC/H3 deferral consistency

H3/QUIC appears correctly deferred by ADR. Ensure no code/docs imply otherwise.

Tasks:

- Search docs and manifest for `h3`, `http3`, `quic` claims.
- Ensure all entries are `unsupported`, `experimental`, or `intentional_non_parity` according to the ADR.
- Ensure URI parser either rejects with structured diagnostics or classifies as unsupported.
- Add tests for H3/QUIC URI rejection and diagnostic code.
- Ensure README does not imply QUIC/H3 support.

Acceptance:

- H3/QUIC is consistently deferred across docs, manifest, URI parser, and README.

### H9. Reverse supervisor wiring closure

The reverse completion doc explicitly says `[[reverse_servers]]` and `[[reverse_clients]]` config objects are translated but not wired to live supervisor tasks. This is the biggest remaining Phase 27 runtime gap.

Tasks:

- Extend `ServiceSupervisor` to spawn a `ReverseServer` task for each `[[reverse_servers]]` entry.
- Extend `ServiceSupervisor` to spawn one or more `ReverseClient` tasks for each `[[reverse_clients]]` entry.
- Honor `parallel_connections` by spawning N client tasks where configured.
- Register servers in `ReverseRegistry` with `ReverseServerEntry`.
- Inject `RouteEngineTargetResolver` into clients when runtime routing should gate target access.
- Ensure task cancellation and shutdown drain correctly.
- Ensure bind failures fail startup or surface according to config policy.
- Add runtime integration tests that use TOML config, start the supervisor, and exercise an actual reverse relay.

Acceptance:

- Reverse is runtime-usable from normal Eggress config, not only protocol-crate standalone.
- Admin `/-/reverse` reflects live supervisor-spawned servers.
- Shutdown cleans up reverse tasks.

### H10. Reverse pproxy differential evidence

Current gated pproxy interop verifies handshake wiring but not relayed payload byte equality. Do not promote reverse to compatible until payload-level differential tests exist.

Tasks:

- Add gated tests under `EGRESS_REQUIRE_REVERSE_INTEROP=1` for:
  - Eggress reverse server + pproxy reverse client relays known payload;
  - pproxy reverse server + Eggress reverse client relays known payload;
  - auth success/failure parity;
  - target connection failure behavior if deterministic;
  - connection close/half-close behavior where practical.
- Capture logs/artifacts on failure.
- Update `docs/COMPATIBILITY_EVIDENCE.md` to keep reverse as `Supported` until these pass.
- Add manifest entries for payload-level interop tests separately from handshake-only tests.

Acceptance:

- Reverse pproxy interop status clearly says handshake-only or payload-verified.
- Compatible status requires payload byte equality.

### H11. Reverse security hardening

Reverse mode is security-sensitive because it exposes remote listeners and plaintext control channels.

Tasks:

- Ensure non-loopback bind addresses are denied by default unless explicitly allowlisted.
- Ensure allow-bind policy is documented and tested.
- Ensure plaintext auth is always redacted in logs, metrics, diagnostics, and admin output.
- Enforce max control connections, max streams, and max pending external clients.
- Add tests for denied bind, stream limit drops, auth redaction, and admin output redaction.
- Document recommended deployment through WireGuard/stunnel/haproxy.

Acceptance:

- Reverse mode defaults are conservative.
- Security review documents all plaintext risks and mitigations.

### H12. CLI/diagnostics taxonomy consistency

Phase 28 added structured diagnostics and exit codes. Ensure all CLI paths use them.

Tasks:

- Audit `crates/eggress-cli/src/main.rs` for ad hoc `eprintln!` + `exit(1)` paths.
- Replace with typed diagnostics and `exit_codes` constants where possible.
- Ensure `pproxy check --json` serializes stable diagnostic codes.
- Ensure unsupported protocol/flag/platform/security-sensitive feature errors carry feature IDs.
- Add snapshot/golden tests for representative diagnostics.
- Verify credential redaction in JSON and text modes.
- Ensure Python helper errors mirror diagnostic codes where practical.

Acceptance:

- No pproxy compatibility path exits with ad hoc generic behavior unless intentionally documented.
- All user-facing unsupported pproxy features have stable diagnostic codes.

### H13. URI corpus and fixture integrity

The 85-case URI corpus and CLI fixtures are useful; now make them harder to drift.

Tasks:

- Add a test that loads every `tests/compat/fixtures/pproxy_cli_cases/*.toml` fixture and validates schema.
- Add a test that loads `pproxy_uri_corpus.toml` and validates every case has:
  - id;
  - raw input;
  - expected tier;
  - expected diagnostic code or expected config mapping;
  - redacted display if credentials appear.
- Ensure every corpus feature maps to a manifest feature ID.
- Add deterministic generated TOML comparison for every supported fixture.
- Add explicit fixture cases for new advanced transport and reverse unsupported combinations.

Acceptance:

- Fixture files are mechanically validated.
- No stale fixture can silently pass as unused documentation.

### H14. Evidence and documentation claim audit

The new docs are broad. Audit them against actual runtime integration.

Files to review:

- `README.md`;
- `docs/PARITY_MATRIX.md`;
- `docs/COMPATIBILITY_EVIDENCE.md`;
- `docs/PPROXY_PARITY_SPEC.md`;
- `docs/CONFIG_REFERENCE.md`;
- `docs/OPERATIONS.md`;
- `docs/METRICS.md`;
- `docs/SECURITY_REVIEW.md`;
- `docs/protocols/*.md`;
- `tests/compat/pproxy_manifest.toml`.

Rules:

- `Compatible` requires pproxy differential evidence.
- `Supported` requires runtime integration or explicit protocol-crate-level wording.
- `Experimental` is appropriate for protocol-crate-only implementations not exposed via supervisor.
- `Unsupported` means parser rejects or runtime cannot run it.
- `Intentional non-parity` requires rationale and stable diagnostics.

Acceptance:

- No doc claims runtime support for a protocol-crate-only feature.
- No doc points to non-existent crate paths or tests.
- Manifest validation still passes.

### H15. Metrics/admin consistency audit

Phase 25-27 added many metrics. Ensure metrics are actually emitted, not only documented.

Tasks:

- For transparent, Unix, H2, WebSocket, raw, and reverse metrics, trace each documented metric to an increment/gauge site.
- Add tests for Prometheus rendering where available.
- Add admin snapshot tests for reverse registry and platform capability state.
- Remove or mark planned metrics that are documented but not emitted.

Acceptance:

- Every documented metric has a code path and test or is clearly marked planned.

### H16. Validation matrix and completion doc

Run focused and broad validation.

Baseline commands:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-testkit manifest
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test cli_exit_codes
cargo test -p eggress-cli --test pproxy_translation_golden
cargo test -p eggress-runtime transparent
cargo test -p eggress-runtime unix_socket
cargo test -p eggress-protocol-reverse
cargo test -p eggress-runtime reverse_interop
cargo test -p eggress-protocol-websocket
cargo test -p eggress-protocol-http h2
cargo test -p eggress-protocol-raw
```

Broader validation:

```bash
cargo test --workspace
cargo deny check
cargo audit
```

Gated/manual validation where available:

```bash
sudo -E EGRESS_REQUIRE_TRANSPARENT_INTEROP=1 cargo test -p eggress-runtime transparent -- --ignored --test-threads=1
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 cargo test -p eggress-cli --test advanced_transport_interop -- --ignored --test-threads=1
```

Completion doc:

```text
docs/PHASE_25_28_HARDENING_COMPLETION.md
```

The completion doc must record:

- what was downgraded;
- what was promoted;
- what remains protocol-crate-only;
- what is runtime-integrated;
- unsafe audit results;
- gated test status;
- commands run and pass/fail status;
- remaining gaps.

## Acceptance criteria for the hardening pass

This pass is complete when:

- Transparent proxy unsafe code is audited and documented.
- Platform capability checks do not overclaim runtime privilege/support.
- Unix socket filesystem behavior is safe and tested.
- H2/WebSocket/raw docs and manifest match actual runtime integration depth.
- QUIC/H3 remains consistently deferred.
- Reverse servers/clients are either supervisor-wired or explicitly downgraded to protocol-crate/config-only support everywhere.
- Reverse has payload-level pproxy differential tests or remains `Supported`, not `Compatible`.
- CLI diagnostics/exit codes are consistently used across pproxy compatibility paths.
- URI/CLI fixtures are schema-validated and mapped to manifest feature IDs.
- Metrics/admin docs match emitted code paths.
- All Phase 25-28 docs, manifest, README, and compatibility evidence agree.

## Expected remaining gaps after hardening

Hardening should not erase all long-term roadmap gaps. Remaining expected gaps may include:

- SSH upstream transport.
- H3/QUIC if still deferred.
- MASQUE/CONNECT-UDP if not part of pproxy-compatible behavior.
- System proxy setting integration.
- True pproxy-shaped Python API drop-in replacement.
- Legacy Shadowsocks/SSR intentional non-parity.
- Trojan inbound/server mode if still deferred.

## Handoff notes

The key theme is claim discipline. The repo now contains many new pieces, but each piece has a different maturity level. Do not let protocol-crate existence become a compatibility claim. Runtime integration, pproxy differential evidence, and operator-safe defaults are the gates for stronger labels.
