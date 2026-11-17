# Protocols Edge Milestone 003 — Closure Status

Status: closed

Source implementation plan:

- `plans/implementation/protocols-edge/003-socks4-auth-and-udp-private-egress.md`

Source subsystem roadmaps:

- `plans/subsystems/protocols-edge-roadmap.md` §7 Milestone 3, §12 milestone status
- `plans/subsystems/security-hardening-roadmap.md` §7 Milestone S3

Repository baseline reviewed: `ba64e16` (branch `feat/auto-20261116-652bb64d`,
workspace `1.0.12`) plus this pass's uncommitted working-tree changes

Implementation commits or pull requests:

- None. This pass is uncommitted working-tree work in the shared corrective
  wave; no commit, tag, or publication was created by this milestone.

## 1. Executive finding

The two inbound-enforcement bypasses named by the milestone are closed, and the
address-classification and auth-transport gaps they depended on are covered by
tests.

- SOCKS4 listeners with `[listeners.auth]` now enforce the full pproxy
  `user:pass` credential in the SOCKS4 user-ID field, compare it with
  `subtle::ConstantTimeEq`, report a generic authentication failure without
  echoing either component, derive the session identity from the authenticated
  username, and seed the shared IP auth-reuse cache only on that
  full-credential success. Username-only, wrong-password, and empty user IDs
  fail before any tunnel or cache entry exists.
- `allow_private_egress = false` is now enforced after DNS on all three locally
  resolving UDP paths (standalone SOCKS5 UDP, SOCKS5 UDP ASSOCIATE, standalone
  Shadowsocks UDP). The existing single `lookup_host` call is collected in full
  and every answer is filtered through the shared classifier; the first
  permitted address is connected and an all-private resolution returns
  `UdpError::PrivateEgressBlocked` without creating a flow.
- The shared classifier in `eggress-core::connector` now treats CGNAT
  `100.64.0.0/10`, NAT64 `64:ff9b::/96`, 6to4 `2002::/16`, Teredo `2001::/32`,
  IPv6 benchmarking `2001:2::/48`, and ORCHID `2001:10::/28` / `2001:20::/28`
  as reserved/private, including IPv4-mapped forms. `eggress-udp` reuses it and
  its duplicate private-IP helpers are gone; the UDP-local shape checks (port
  zero, multicast, broadcast, unspecified, loopback) remain.
- Config validation emits a non-fatal warning when a listener is non-loopback,
  configures `[listeners.auth]`, has no `[listeners.tls]`, and offers HTTP,
  SOCKS4, or SOCKS5. Shadowsocks, Trojan, and TLS listeners do not warn, and
  the configuration is never rejected.

The pproxy SOCKS4 `matched` claim was restored, not changed: no manifest or
compatibility-tier edit was required, and no oracle/differential gate was
triggered by this pass. Closure is conditional on one item only — the
coordinator's workspace-wide `fmt`/`clippy`/`test` gate, which runs after every
corrective wave has landed.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result | Notes |
|---|---|---|---|
| SOCKS4 `[listeners.auth]` enforces the full `user:pass` credential in the user-ID field | `crates/eggress-server/src/accept/handlers.rs` (`accept_socks4`); `test_socks4_auth_full_credential_accepted` | pass | `ct_eq` over `username:password`; expected bytes zeroized after compare |
| Username-only SOCKS4 user ID rejected | `test_socks4_auth_username_only_rejected` | pass | Generic `AuthenticationFailed` |
| Wrong-password SOCKS4 user ID rejected | `test_socks4_auth_wrong_password_rejected` | pass | — |
| Empty SOCKS4 user ID rejected when auth is configured | `test_socks4_auth_empty_user_id_rejected` | pass | — |
| Failed SOCKS4 attempt never seeds the auth-reuse cache | `test_socks4_auth_rejected_attempt_does_not_authorize_retry` (three failure shapes, `cache.is_empty()` after each) | pass | Cache seeded only in the full-credential branch |
| Successful SOCKS4 auth records the authenticated username identity | `test_socks4_auth_full_credential_accepted` (`ClientIdentity::Username("user")`, `cache.lookup(peer)`) | pass | Raw user ID is no longer the cached identity |
| SOCKS4 auth failure does not echo credentials | `test_socks4_auth_failure_does_not_leak_credentials` | pass | Rendered error is exactly `authentication failed` |
| Reuse cache stays peer-scoped | `test_socks4_auth_reuse_cache_is_peer_scoped` | pass | Same peer reuses; another peer is rejected |
| `allow_private_egress = false` blocks a domain resolving to a private address (flow level) | `crates/eggress-udp/src/direct.rs` `private_domain_is_dropped_when_policy_disallows` | pass | `PrivateEgressBlocked`; counting echo server observes zero datagrams |
| Same target allowed when `allow_private_egress = true` | `private_domain_is_allowed_when_policy_permits` | pass | Echo round-trip |
| Mixed public/private resolution filters rather than truncating | `restrictive_policy_selects_first_permitted_answer` | pass | `[public, private]` and `[cgnat, public]` select the public answer |
| Public destination still resolves under a restrictive policy | `public_domain_still_resolves_when_policy_disallows_private` | pass | Numeric public destination, no external DNS |
| Empty answer set fails with `UnresolvedTarget` | `restrictive_policy_selects_first_permitted_answer` | pass | — |
| Standalone SOCKS5 UDP path drops private-resolving domain and records rejection | `crates/eggress-udp/tests/standalone_udp.rs` `standalone_udp_private_domain_dropped_when_policy_disallows` | pass | `standalone_rejected_datagrams == 1`, no datagram at target |
| Standalone SOCKS5 UDP path still works with the default policy | `standalone_udp_private_domain_allowed_when_policy_permits` | pass | — |
| SOCKS5 UDP ASSOCIATE path drops private-resolving domain and records `dropped_packets` | `crates/eggress-udp/src/relay.rs` `relay_private_domain_dropped_when_policy_disallows` | pass | Previously the error escaped without a drop count |
| SOCKS5 UDP ASSOCIATE permissive path unchanged | `relay_private_domain_allowed_when_policy_permits` | pass | — |
| Standalone Shadowsocks UDP drops private-resolving domain and records rejection | `crates/eggress-udp/src/standalone_shadowsocks.rs` `build_direct_flow` passes `config.allow_private_egress`; `shadowsocks_standalone_private_domain_dropped_when_policy_disallows` | pass | `standalone_rejected_datagrams == 1`, no datagram at target |
| No second DNS lookup | `direct.rs` `resolve_target` collects the one `lookup_host` result; no other call added | pass | Code inspection + diff |
| Literal shape checks retained | `crates/eggress-udp/src/security.rs` `validate_target_shape` (unchanged) | pass | `security.rs` unit tests unchanged and green |
| Shared classifier covers CGNAT | `eggress-core/src/connector.rs` `is_v4_cgnat`; `reserved_special_purpose_ranges_table`; UDP `reject_cgnat_ipv4_egress` | pass | 100.63.255.255 / 100.128.0.0 remain public |
| Shared classifier covers NAT64 / 6to4 / Teredo | `is_v6_nat64` / `is_v6_6to4` / `is_v6_teredo`; table test; UDP `reject_nat64_and_teredo_egress` | pass | — |
| Shared classifier covers `2001:2::/48` and ORCHID | `is_v6_benchmarking` / `is_v6_orchid`; table test | pass | `2001:1::/32`, `2001:3::/32`, `2001:30::/32` stay public |
| IPv4-mapped forms classified | `is_reserved_or_private_ip` recursion; table entries `::ffff:100.64.0.1`, `::ffff:100.127.255.254`, `::ffff:8.8.8.8` | pass | — |
| UDP duplication removed | `security.rs` delegates to `is_reserved_or_private_ip`; local `is_private_ipv4/6` bodies replaced | pass | — |
| Auth-without-TLS warning for non-loopback HTTP/SOCKS with auth | `crates/eggress-config/src/validate/security.rs`; `warn_non_loopback_http_auth_without_tls`, `warn_non_loopback_socks4_and_socks5_auth_without_tls` | pass | Path `listeners[<i>].tls` |
| No warning for TLS / Shadowsocks / Trojan / loopback | `no_warn_authed_http_with_tls`, `no_warn_authed_shadowsocks_without_tls`, `no_warn_authed_trojan_protocol_without_tls`, `no_warn_loopback_http_auth_without_tls` | pass | — |
| Warning text never echoes credentials | `warn_non_loopback_http_auth_without_tls` asserts the username and password are absent | pass | Message names listener and bind only |
| Config never rejected for this condition | Warning path only; `cargo test -p eggress-config` (135 + 1 tests) | pass | — |
| pproxy SOCKS4 claim unchanged (`matched`) | `docs/parity/pproxy_capability_manifest.toml` `protocol.socks4.connect_ipv4` / `protocol.socks4.user_id` untouched; no manifest or matrix edit in this pass | pass | Behavior restored to the pinned oracle's `userid in users` semantics |
| `allow_private_egress = true` default preserved | `CompiledListenerUdpConfig::default()` unchanged; `StandaloneUdpConfig`/`RelayConfig` test fixtures still `true`; existing UDP suites green | pass | — |

## 3. Production implementation evidence

- `crates/eggress-server/src/accept/handlers.rs` — `accept_socks4` rewritten around
  a three-way decision: cached identity, full-credential verification, or the
  unauthenticated path. The credential is assembled as `username:password` into a
  zeroized buffer and compared with `subtle::ConstantTimeEq`; success records
  `ClientIdentity::Username(username)`. Cache-hit sessions no longer re-record the
  entry, matching the SOCKS5 accept path (reuse proves an earlier authentication;
  only a fresh full-credential success refreshes the cache).
- `crates/eggress-core/src/connector.rs` — `is_reserved_or_private_ip` extended
  with `is_v4_cgnat`, `is_v6_nat64`, `is_v6_6to4`, `is_v6_teredo`,
  `is_v6_benchmarking`, and `is_v6_orchid`; documented range list updated.
- `crates/eggress-udp/src/security.rs` — literal-target policy now delegates to
  the shared classifier and returns the dedicated
  `UdpError::PrivateEgressBlocked`; the duplicated private-IP helpers are gone and
  the stale "deferred post-DNS check" comment is replaced by the real contract.
- `crates/eggress-udp/src/direct.rs` — `UdpTargetFlow::new` and
  `resolve_target` take `allow_private_egress`; `select_resolved_addr` filters the
  full answer set and fails closed when nothing is permitted.
- `crates/eggress-udp/src/relay.rs` — direct-flow creation passes the policy and
  records `record_dropped()` on failure instead of propagating a bare error.
- `crates/eggress-udp/src/standalone.rs`,
  `crates/eggress-udp/src/standalone_shadowsocks.rs` — pass their configured
  policy; the existing `record_standalone_rejected()` call sites now also count
  post-DNS rejections.
- `crates/eggress-udp/src/error.rs` — added `UdpError::PrivateEgressBlocked`.
- `crates/eggress-config/src/validate/security.rs` — auth-without-TLS warning.

Public-surface note: `UdpTargetFlow::new` gained a required `allow_private_egress`
parameter (plan §6B directed one policy parameter on the direct flow path). The
crate is unpublished for this pass; the signature change is deliberate so a caller
cannot silently default into permissive egress.

## 4. Verification executed

### Commands run

```bash
cargo test -p eggress-server accept
cargo test -p eggress-protocol-socks
cargo test -p eggress-udp
cargo test -p eggress-runtime --test udp
cargo test -p eggress-runtime --test security_invariants
cargo test -p eggress-config
cargo test -p eggress-core --lib connector
cargo clippy -p eggress-server -p eggress-udp -p eggress-core -p eggress-config --all-targets -- -D warnings
cargo fmt -p eggress-server -p eggress-udp -p eggress-core -p eggress-config
```

### Results

| Command | Result |
|---|---|
| `cargo test -p eggress-server accept` | pass — 47 tests (1 filtered in `public_api`) |
| `cargo test -p eggress-protocol-socks` | pass — 112 + 14 + 2 tests |
| `cargo test -p eggress-udp` | pass — 221 lib + 12 + 17 + 28 tests |
| `cargo test -p eggress-runtime --test udp` | pass — 26 tests |
| `cargo test -p eggress-runtime --test security_invariants` | pass — 8 tests |
| `cargo test -p eggress-config` | pass — 135 + 1 tests |
| `cargo test -p eggress-core --lib connector` | pass — 31 tests (classifier table included) |
| `cargo clippy -p eggress-server -p eggress-udp -p eggress-core -p eggress-config --all-targets -- -D warnings` | pass — no warnings |
| `cargo fmt -p eggress-server -p eggress-udp -p eggress-core -p eggress-config` | pass — no drift after formatting |

The clippy and `cargo fmt --check` gates were re-run after the final Shadowsocks
standalone test was added (split per crate, identical clean result), so the
recorded evidence matches the final working-tree state.

Not run in this session (by instruction): workspace-wide `cargo test`, workspace
`clippy`, `cargo fmt --all`, `--all-features`, the external pproxy oracle /
differential / interop suites, benchmarks, fuzz, soak, and `cargo audit`/`cargo
deny`. The differential suites are not required because the SOCKS4 `matched`
claim did not change (see §7). Dependency advisories are owned by other
milestones.

**Coordinator broad gate (2026-11-16, after all six corrective waves):** pass —
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --locked`, the documented CLI/embed feature slices, the
embed OpenSSH regression (6/6), and `cargo deny --workspace check`
(advisories/bans/licenses/sources). `cargo audit --ignore RUSTSEC-2023-0071`
reports only the four tracked `zip 6.0.0` advisories. The condition recorded in
§11 is therefore satisfied.

## 5. Invariant review

- SOCKS5, HTTP CONNECT/forward, H2, WebSocket, and reverse auth paths were not
  modified; their constant-time comparisons still use `subtle::ConstantTimeEq`
  and `cargo test -p eggress-server accept` covers them.
- SOCKS4 failures remain `AcceptError::AuthenticationFailed` with a static
  message; `test_socks4_auth_failure_does_not_leak_credentials` asserts the
  rendered text contains neither the username, the password, nor the wrong
  password.
- The auth-reuse cache remains bounded, monotonic-expiry, and peer-keyed
  (`AuthReuseCache` untouched); the only change is which paths may write to it.
  Username-only matches cannot reach `record_authenticated`.
- UDP association/flow limits, client pinning, datagram bounds, and idle
  timeouts are untouched; the full `eggress-udp` suite (278 tests) is green.
- `allow_private_egress = true` remains the compiled default and the UDP test
  fixtures keep using it; loopback echo tests still pass.
- Domain resolution still uses one `tokio::net::lookup_host` call per flow; no
  blocking resolver, timeout layer, or second lookup was added.
- No compatibility tier, manifest row, or generated parity report changed.

## 6. Failure and recovery review

- A failed SOCKS4 authentication returns before the stream is joined into a
  tunnel and before any cache write; there is no partially constructed flow.
- UDP resolution failure or an all-private answer set returns
  `UdpError::PrivateEgressBlocked` / `UnresolvedTarget`/`Io` before
  `UdpSocket::connect`, so no flow is registered and the existing flow-reaping
  and registry-cleanup paths are unchanged. The per-path metrics
  (`dropped_packets` for the associate path, `standalone_rejected_datagrams` for
  the standalone paths) count the drop.
- Cancellation while resolving is unchanged: `select_resolved_addr` is a pure
  function after the lookup, and the flow registry still owns established-flow
  cleanup.
- The config warning path is read-only and runs during validation; no runtime
  locking, reload, or startup ordering changed.
- Malformed/unauthorized input handling is unchanged: SOCKS4 request parsing
  remains bounded by `MAX_USER_ID_LEN`/`MAX_DOMAIN_LEN`, and UDP datagram
  decoding remains size-gated.

## 7. Migration and compatibility review

- `allow_private_egress` still defaults to `true`; operators who opted into
  `false` now actually get domain enforcement. No config schema change; the key
  remains `[listeners.udp].allow_private_egress` (and the `defaults` equivalent).
- SOCKS4 clients on auth-enabled listeners must now send the complete
  `user:pass` string as the user ID. This restores the pinned pproxy 2.7.9
  behavior (`userid in users`, `users` holding `user:pass` strings); it is
  documented in `docs/security/SECURE_CONFIGURATION.md` and
  `docs/security/OPEN_PROXY_PREVENTION.md`.
- The pproxy SOCKS4 `matched` claim was **restored**, not changed. No manifest
  row, compatibility matrix, or generated parity report was edited, and the
  oracle/differential gate was not triggered. The `protocol.socks4.connect_ipv4`
  and `protocol.socks4.user_id` rows still describe the pre-pass `matched`
  behavior, which is now truthful for auth-enabled listeners as well.
- Cache-hit sessions no longer refresh the SOCKS4 reuse entry. That aligns
  SOCKS4 with the SOCKS5 accept path and does not widen authorization: any
  cached identity still originates from a full-credential success. pproxy
  `--auth` semantics remain owned by Parity M003.
- The classifier extension changes TCP DNS-rebinding outcomes for domains that
  resolve into CGNAT or the IPv4-embedding IPv6 ranges (they are now rejected
  like RFC 1918). This is the milestone's stated goal; literal targets are
  unaffected, and `allow_private_egress = true` continues to permit UDP
  loopback/LAN flows.
- No wire-format, protocol-negotiation, or translator change.

## 8. Security review

- Authorization: SOCKS4 now requires both credential components; the comparison
  is constant-time, the failure is generic, and the accept path returns before
  tunnel creation, so no partially authorized session exists.
- Secret handling: the assembled `username:password` buffer is zeroized after
  comparison; `InboundAuthentication`'s existing `Drop` zeroization of the
  configured password is untouched; no new `Debug`/`Display` surface exposes
  credentials.
- The config warning text contains only the listener name and bind address —
  never a username, password, env var value, or URI (asserted by test).
- Path validation: UDP literal shape checks stay in place; post-DNS filtering
  uses the same shared classifier as the TCP guard, so the two gates cannot
  drift.
- Denial-of-service bounds: unchanged (no new allocations proportional to input
  beyond the answer list already collected, which the previous `.next()` also
  materialized from the same iterator).
- Audit behavior: dropped/rejected datagrams now produce a metric on the
  associate direct path where previously the error was only traced.

## 9. Documentation and operations

- `docs/security/THREAT_MODEL.md` — A1 mitigations (auth-without-TLS warning),
  and mitigations 17–19 (SOCKS4 full-credential semantics, UDP post-DNS policy,
  classifier ranges).
- `docs/security/OPEN_PROXY_PREVENTION.md` — warning-system bullet, expanded
  SOCKS4/SOCKS5/HTTP/Shadowsocks protocol controls, and the updated testing list.
- `docs/security/SECURE_CONFIGURATION.md` — auth table note for SOCKS4, an
  "Auth without TLS" paragraph, and the private-egress/UDP subsection (CGNAT and
  friends, `allow_private_egress` example). CIDR and admin wording intentionally
  left to the routing milestone.
- `architecture/protocols-socks.md` — SOCKS4 user-ID credential-carrier contract
  and the cross-crate test coverage note.
- `architecture/udp.md` — module-map row for `direct.rs`, post-DNS policy
  contract, per-packet step, test coverage, and a reviewer gotcha.
- No operator commands, exit codes, metrics names, or configuration keys were
  added or renamed.

## 10. Unresolved findings

| Severity | Finding | Impact | Required action |
|---|---|---|---|
| low | `UdpTargetFlow::new` gained a required `allow_private_egress` parameter (public crate surface). | Downstream callers in this workspace are updated; an external embedder compiling against the next release must pass the flag. | Covered by the next release's changelog/versioning decision; no action inside this milestone. |
| low | A domain resolving to `localhost`-style dual-stack answers under `allow_private_egress = true` still connects to the first answer, which may be an address family the local UDP bind cannot reach. | Pre-existing behavior, unchanged by this pass; tests use numeric or all-private resolution to stay deterministic. | Not in scope; record only if cross-family UDP target selection is prioritized. |
| low | SOCKS4 has no auth-failure session metric (the accept signature carries no session metrics). | Failures are logged/handled generically but not counted. | Plan §6A allowed deferral; candidate for a later metrics milestone. |

No critical or high finding remains.

## 11. Roadmap disposition

Milestone closed: production implementation, the focused verification set, and
the coordinator's workspace-wide `fmt`/`clippy`/`test` gate all passed. No
corrective implementation plan is required for M003 scope.

## 12. Registry updates

- `plans/subsystems/protocols-edge-roadmap.md`: top status line, dependency-graph
  label, Milestone 3 status, and §12 milestone row moved from `ready` to
  `closed` with this closure record.
- `plans/implementation/protocols-edge/003-socks4-auth-and-udp-private-egress.md`:
  status `ready` → `implemented`.
- `plans/subsystems/security-hardening-roadmap.md` S3: no edit required by this
  milestone's ownership (its §12 row is coordinated by the security roadmap's own
  wave); the evidence above satisfies the S3 exit conditions.
- No change to `plans/registry.md` was made, because the registry row for this
  implementation belongs to the shared corrective-wave bookkeeping owned by the
  coordinator.
- No release, tag, or commit created.
