# Protocols Edge M003 — SOCKS4 Credential Enforcement and UDP Private-Egress Policy

Status: implemented

Repository baseline: `feat/auto-20261116-652bb64d` at `ba64e16` (workspace 1.0.12)

Source roadmaps:

- `plans/subsystems/protocols-edge-roadmap.md` (new milestone M003)
- `plans/subsystems/security-hardening-roadmap.md` (S3)

Long-term requirements:

- `plans/000-long-term-specification.md` §4 (invariants 2, 3, 10), §5 (compat contract)
- `plans/001-terminology-and-domain-model.md` §2–§3 (listener roles, route identity)

Applicable ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

Primary class: invariant

## 1. Objective

Close two inbound-enforcement bypasses and the related address-classification
gaps:

1. SOCKS4 listeners that configure `[listeners.auth]` must enforce the full
   pproxy credential, not only the username.
2. `allow_private_egress = false` must be enforced after DNS for every locally
   resolving UDP path, not only for literal IP targets.
3. Private/reserved-IP classification must include CGNAT and IPv4-embedding IPv6
   forms.
4. Non-loopback password-authenticated listeners without TLS must warn, because
   HTTP/SOCKS credentials would otherwise travel in clear.

This is a corrective pass; it adds no protocol or capability.

## 2. Why this milestone is ready

- The pinned pproxy 2.7.9 oracle defines SOCKS4 auth as
  `userid in users`, where `users` contains full `user:pass` credential strings
  (`pproxy/proto.py`, SOCKS4 `accept`). Eggress currently compares the SOCKS4
  user ID only to the username (`crates/eggress-server/src/accept/handlers.rs:151-160`),
  so the password is unenforced.
- The UDP validation comment explicitly claims a post-DNS check that does not
  exist on the UDP direct path (`crates/eggress-udp/src/security.rs:101-104`
  versus `crates/eggress-udp/src/direct.rs:64-70`).
- `eggress-udp` already depends on `eggress-core`, which owns the shared
  `is_reserved_or_private_ip` classifier
  (`crates/eggress-core/src/connector.rs:20-75`); no new crate dependency is
  required.
- `eggress-config` already emits non-fatal security warnings
  (`crates/eggress-config/src/validate/security.rs:16-53`); the TLS/auth warning
  fits that mechanism.
- No compatibility tier change is required if SOCKS4 is brought into line with
  the existing `matched` behavior. If evidence shows a claim change, the
  manifest/oracle gate applies.

## 3. Current implementation evidence

### SOCKS4 auth

`accept_socks4`:

- calls `cached_identity(auth, peer_ip)`;
- on cache miss destructures `auth_credentials(auth)` as `(username, _, _)` and
  compares `request.user_id` only to `username` (`handlers.rs:151-160`);
- records the identity in the shared auth-reuse cache after success
  (`handlers.rs:168-173`).

`read_socks4_request` bounds the user ID and reads it as UTF-8
(`crates/eggress-protocol-socks/src/socks4/server.rs`). SOCKS4 has no separate
password field; the pproxy-compatible carrier is the user ID.

### UDP private egress

`validate_standalone_target` rejects literal private IPs when
`allow_private_egress = false`, but the domain branch is `_ => {}` and the
comment defers the check to a post-DNS function (`security.rs:83-105`).
`UdpTargetFlow::new` calls `direct::resolve_target`, which does
`lookup_host(...).next()` and connects with no filtering
(`direct.rs:12-28,64-70`). Call sites include standalone SOCKS5 UDP
(`standalone.rs:114,622`), SOCKS5 UDP ASSOCIATE (`relay.rs:100,544`), and
standalone Shadowsocks UDP (`standalone_shadowsocks.rs:138,637`).

### Address classification

`is_reserved_or_private_ip` covers loopback, RFC1918, link-local, ULA,
multicast, documentation, benchmarking, 240/4, and some special-use IPv6, but
does not cover:

- CGNAT `100.64.0.0/10`;
- NAT64 `64:ff9b::/96`;
- 6to4 `2002::/16`;
- Teredo `2001::/32`;
- smaller IETF special-purpose blocks such as `2001:2::/48`.

The UDP-local `is_private_ipv4/6` helpers
(`crates/eggress-udp/src/security.rs:55-75`) cover only RFC1918/loopback/
link-local/ULA/mapped forms, so literal CGNAT also passes
`allow_private_egress = false`.

### Auth transport warning

`validate_config_security` warns only when a non-loopback listener has no auth,
Shadowsocks, SSR, or Trojan (`security.rs:16-35`). It does not warn when HTTP/
SOCKS password auth is configured without `[listeners.tls]`.

## 4. Invariants that must not regress

- SOCKS5, HTTP CONNECT/forward, H2, WebSocket, and reverse auth remain
  constant-time and unchanged.
- SOCKS4 auth failures remain generic (`AuthenticationFailed`), with no
  credential echo.
- The auth-reuse cache remains bounded, monotonic-expiry, and policy-scoped as
  it is today; a SOCKS4 username-only identity must never seed it.
- UDP association/flow limits, client pinning, datagram bounds, and idle
  timeouts are unchanged.
- `allow_private_egress = true` remains the pproxy-compatible default; local
  UDP echo tests that use loopback continue to work when explicitly permitted.
- Domain resolution uses the existing `lookup_host`; do not add a second lookup
  or a blocking resolver.
- No pproxy compatibility tier change without manifest/evidence.

## 5. Scope

### In scope

- `eggress-server::accept` SOCKS4 credential comparison and cache seeding.
- `eggress-udp` post-DNS destination filtering for standalone SOCKS5 UDP,
  SOCKS5 UDP ASSOCIATE, and standalone Shadowsocks UDP.
- Shared private/reserved-IP classification for CGNAT/NAT64/6to4/Teredo and
  other selected special-purpose ranges.
- `eggress-config` warning for non-loopback password auth without TLS.
- Focused unit/integration tests, docs, and metrics where available.

### Explicitly out of scope

- CIDR route matching and admin Host validation (Routing M003).
- pproxy `--auth` reuse documentation/guardrails (Parity M003).
- Admin TLS or native admin transport changes.
- TCP `allow_private_egress` unification; the documented literal-private TCP
  default is unchanged.
- SSH/TLS dependency upgrades.

## 6. Required production changes

### A. Enforce the full SOCKS4 credential

In `accept_socks4`:

1. Build the expected user-ID credential as `username + ":" + password`.
2. Compare `request.user_id.as_bytes()` against that byte string with
   `subtle::ConstantTimeEq`.
3. On mismatch, return `AuthenticationFailed` without revealing which component
   failed.
4. On success, record an identity derived from the authenticated username (for
   example `ClientIdentity::Username(username)`), not the raw username-only
   user ID. Never call `record_authenticated` from a path that accepted a
   username-only match.
5. If `auth_credentials` is `None`, keep the current unauthenticated path.

Consider adding SOCKS4 auth-failure accounting through the existing session
metrics path if the accept signature can carry it without broad churn; if not,
record it as a follow-up rather than widening this milestone.

Add tests for:

- `user:pass` accepted;
- `user` alone rejected;
- `user:wrong` rejected;
- empty user ID rejected when auth is configured;
- cache is not seeded by a rejected SOCKS4 attempt;
- a successful SOCKS4 full-credential auth does not authorize a different
  listener/credential set through the shared cache (coordinate with Parity M003
  for the cross-listener case).

### B. Apply policy after UDP DNS resolution

Introduce one destination-policy parameter on the direct UDP flow path:

- Add `allow_private_egress: bool` (or a small `TargetPolicy` value) to
  `UdpTargetFlow::new` and `direct::resolve_target`.
- Resolve the domain, collect all returned addresses, and filter each through
  `eggress_core::connector::is_reserved_or_private_ip` when the policy is
  restrictive.
- Connect to the first allowed address; if none remain, return a structured
  `UdpError` and record the existing dropped metric. Do not truncate to the
  first address before filtering.
- Pass `config.allow_private_egress` from `StandaloneUdpConfig` and
  `RelayConfig`; the standalone Shadowsocks path passes its own flag.
- Keep the literal-target shape checks (port zero, multicast, broadcast,
  unspecified) in `validate_standalone_target`.

Add tests:

- `allow_private_egress = false` plus a domain resolving to `127.0.0.1` is
  dropped and no datagram reaches a local echo server;
- the same target is allowed when `allow_private_egress = true`;
- a domain resolving to a public address still works (use a test resolver or a
  hosts-file-free approach; do not depend on external DNS);
- mixed public/private resolution filters private addresses rather than
  connecting to the first answer.

### C. Extend the shared address classifier

Add the missing ranges to `eggress-core::connector::is_reserved_or_private_ip`:

- IPv4 CGNAT `100.64.0.0/10`;
- IPv6 NAT64 `64:ff9b::/96`;
- IPv6 6to4 `2002::/16`;
- IPv6 Teredo `2001::/32`;
- selected IETF special-purpose ranges (`2001:2::/48` benchmarking,
  `2001:10::/28`/`2001:20::/28` ORCHID where appropriate).

Reuse the core classifier from `eggress-udp` instead of maintaining a second
set of `is_private_ipv4/6` rules; keep the UDP-local shape checks separate.
Add table-driven tests for every new range and for IPv4-mapped forms.

### D. Warn on auth without TLS

In `validate_config_security`, add a warning when:

- the listener bind is non-loopback;
- `listener.auth` is configured;
- `listener.tls` is `None`;
- the protocol set contains HTTP, SOCKS4, or SOCKS5 (Shadowsocks and Trojan
  provide their own encryption/auth; do not warn for those).

The warning must say credentials may be transmitted in clear and recommend
`[listeners.tls]`, loopback, or a trusted tunnel. Do not reject the config in
this milestone; it is a compatibility-preserving warning.

## 7. Ordered work packages

### Work package A — SOCKS4 full-credential enforcement

Intent: remove the password bypass and cache seeding.

Acceptance evidence: focused accept tests; pproxy oracle/differential evidence
for SOCKS4 auth if the manifest claim is touched; no credential in errors.

### Work package B — UDP post-DNS policy

Intent: make `allow_private_egress = false` true for domain targets.

Acceptance evidence: unit + integration tests for private domain drop and
public/private mixed resolution; no double lookup; existing UDP tests green.

### Work package C — Address classification

Intent: close CGNAT/NAT64/6to4/Teredo gaps and deduplicate UDP classification.

Acceptance evidence: table-driven tests; `is_reserved_or_private_ip` used by
UDP; no behavior change when policy allows private egress.

### Work package D — Auth-transport warning and docs

Intent: make plaintext credential exposure visible.

Acceptance evidence: config warning test; docs updated; no false warning for
Shadowsocks/Trojan/TLS listeners.

## 8. Failure, cancellation, restart, and contention semantics

- A failed SOCKS4 auth returns before any tunnel/flow is created; no cache
  entry is written.
- UDP resolution failure or all-private resolution returns a dropped datagram;
  it does not create a flow or retry another address after a connect error.
- Cancellation while resolving/validating a UDP target does not leak a flow;
  existing flow registry cleanup remains responsible for established flows.
- The warning path is read-only and runs at config validation; no runtime
  locking changes.

## 9. Compatibility and migration

- `allow_private_egress` remains default `true` for pproxy compatibility;
  opting into `false` becomes actually enforced for domains.
- SOCKS4 changes from username-only to full `user:pass` user-ID matching. This
  restores the pproxy 2.7.9 behavior; verify the manifest claim and run the
  oracle/differential suite if the claim text changes.
- Existing configs with SOCKS4 + auth must send the full credential in the
  SOCKS4 user ID. Document this in the migration/security docs.
- No protocol wire change.

## 10. Required tests

### Focused unit tests

- SOCKS4: full credential accepted; username-only rejected; wrong password
  rejected; empty rejected; cache not seeded on failure.
- UDP: private domain dropped with policy false; allowed with true; mixed
  addresses filtered; public domain still resolved.
- Classifier: CGNAT, NAT64, 6to4, Teredo, mapped IPv6, documentation ranges.
- Config: auth-without-TLS warning for HTTP/SOCKS; no warning for TLS,
  Shadowsocks, or Trojan.

### Integration tests

- `eggress-udp` standalone/relay private-domain drop.
- `eggress-runtime` UDP suite with an echo server.
- SOCKS4 accept tests in `eggress-server`.

### Security and negative tests

- No raw username/password in errors/logs.
- No datagram sent to the private target on rejection.
- No cache reuse after a failed SOCKS4 attempt.

### Migration and compatibility tests

- Existing `allow_private_egress = true` tests continue to pass.
- Oracle/differential SOCKS4 auth scenarios, if the claim changed.

## 11. Required verification commands

```bash
cargo test -p eggress-server accept
cargo test -p eggress-protocol-socks
cargo test -p eggress-udp
cargo test -p eggress-runtime --test udp
cargo test -p eggress-runtime --test security_invariants
cargo test -p eggress-config
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

If the SOCKS4 manifest claim changes:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli \
  --test differential_pproxy -- --ignored --test-threads=1
```

## 12. Documentation updates

- `docs/security/THREAT_MODEL.md` — UDP post-DNS policy and SOCKS4 credential
  semantics.
- `docs/security/SECURE_CONFIGURATION.md` — private egress and auth-with-TLS
  guidance; correct the “CIDR is a complete private-IP ACL” implication.
- `docs/security/OPEN_PROXY_PREVENTION.md` — non-loopback auth-without-TLS
  warning.
- `architecture/protocols-socks.md` / `architecture/udp.md` — target policy.
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` only if the SOCKS4
  claim changes.

## 13. Acceptance criteria

- A SOCKS4 listener with auth accepts `user:pass` and rejects username-only and
  wrong-password attempts.
- A failed SOCKS4 attempt cannot seed the auth-reuse cache.
- `allow_private_egress = false` drops domain targets resolving to private/
  reserved addresses on all three UDP paths.
- CGNAT/NAT64/6to4/Teredo are classified as non-public.
- Non-loopback HTTP/SOCKS auth without TLS produces a warning.
- Existing UDP/SOCKS behavior remains green when private egress is allowed.

## 14. Stop conditions

Stop and report if:

- the pproxy oracle shows a different SOCKS4 credential carrier than
  `user:pass`;
- UDP resolution needs a new resolver abstraction or timeout policy beyond the
  existing flow;
- the classifier change would reject a legitimate pproxy-compatible target that
  the oracle accepts;
- the auth warning requires a compatibility tier change;
- the SOCKS4 change requires a public API change outside the accept path.

## 15. Closure evidence required

- Focused test output for SOCKS4/UDP/classifier/config.
- pproxy differential output if the claim changed; otherwise a statement that
  the existing `matched` claim was restored.
- `cargo audit`/`cargo deny` status (dependency advisories are owned by other
  milestones; record remaining IDs).
- Documentation diff.
- Explicit no-release statement.

## 16. Handoff notes

- pproxy SOCKS4 uses the raw user ID as the credential carrier; do not add a
  nonstandard password field.
- Do not resolve domains twice; validate the addresses returned by the existing
  lookup.
- `allow_private_egress = true` is intentional compatibility; do not change the
  default.
- Coordinate the cross-listener auth-reuse documentation with Parity M003.
