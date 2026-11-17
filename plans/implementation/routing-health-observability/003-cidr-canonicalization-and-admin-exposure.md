# Routing, Health, Metrics, and Admin M003 — CIDR Canonicalization and Admin Exposure

Status: implemented

Repository baseline: `feat/auto-20261116-652bb64d` at `ba64e16` (workspace 1.0.12)

Source roadmaps:

- `plans/subsystems/routing-health-observability-roadmap.md` (new milestone M003)
- `plans/subsystems/security-hardening-roadmap.md` (S4)

Long-term requirements:

- `plans/000-long-term-specification.md` §4 (invariants 4–5, 10)
- `plans/001-terminology-and-domain-model.md` §2–§3 (Route, admin surface)

Applicable ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

Primary class: invariant

## 1. Objective

Make CIDR destination/source matching family-correct for IPv4-mapped IPv6 and
make the “literal IPs only, no DNS” contract explicit, so an operator cannot
believe a CIDR rule is a complete private-network ACL. Add Host/Origin
validation to the admin HTTP surface to block DNS-rebinding reads, and warn when
non-loopback admin is plaintext HTTP.

## 2. Why this milestone is ready

- `ipnet::IpNet::contains(&IpAddr)` intentionally returns `false` across
  address families (`ipnet` `Contains<&IpAddr>` implementation); an IPv4 CIDR
  therefore does not match `::ffff:10.0.0.1`. The matcher passes the raw
  `IpAddr` (`crates/eggress-routing/src/matcher.rs:190-208`).
- `MatchExpr::DestinationCidr` already returns `false` for every domain
  (`matcher.rs:190-196`); this is existing behavior, not a new decision.
  The security documentation currently presents CIDR rules as private-network
  blocking (`docs/security/SECURE_CONFIGURATION.md:127-155`).
- `eggress-admin` currently has no `Host`/`Origin` check in the request path
  (`crates/eggress-admin/src/server.rs`, `routes.rs`); the admin surface is
  read-only except `POST /-/route-explain`, but DNS rebinding can read
  `/-/status`, `/-/routes`, `/-/upstreams`, `/-/reverse`, and `/metrics`.
- `AdminConfig` has no TLS field (`crates/eggress-config/src/model.rs:346-355`),
  and the admin CLI client rejects `https://` URLs
  (`crates/eggress-admin/src/client.rs:297-301`). Non-loopback admin is
  therefore plaintext HTTP.

## 3. Current implementation evidence

- `DestinationCidr`: only `TargetHost::Ip` matches; domains are `false`
  (`matcher.rs:190-196`).
- `SourceCidr`: uses `request.source` and `cidr.contains(&addr.ip())`
  (`matcher.rs:202-208`); no canonicalization.
- `RouteRequest` carries `TargetAddr`/`Source` from the listener; SOCKS5,
  HTTP CONNECT, and H2 can all carry IPv4-mapped IPv6 literals.
- `validate_config_security` handles listener/admin/reverse warnings but does
  not warn about CIDR/domain limitations or admin plaintext HTTP
  (`crates/eggress-config/src/validate/security.rs:16-53`).
- `AdminServer::new` warns when bound non-loopback, but only about exposure;
  it does not validate Host/Origin (`crates/eggress-admin/src/server.rs:155-170`).
- `AdminState` has `auth`, snapshot, metrics, readiness; no allowed-host field.

## 4. Invariants that must not regress

- First-match-wins routing and `route explain` output semantics are unchanged.
- CIDR matching remains synchronous: no DNS resolution or blocking work inside
  route evaluation.
- `SourceCidr` remains an exact match on the actual peer address; no proxy
  header trust is introduced.
- Admin auth (bearer/basic), auth-failure limiter, connection cap, body cap, and
  read-only behavior remain unchanged.
- Admin endpoints continue to serve loopback clients (`curl 127.0.0.1:9090`,
  `localhost`, `[::1]`) without new configuration.
- No pproxy compatibility claim changes.

## 5. Scope

### In scope

- Canonical IPv4/IPv6 handling in `DestinationCidr` and `SourceCidr`.
- A config warning + docs for CIDR’s literal-only/domain limitation.
- `[admin] allowed_hosts` (or equivalent) and Host/Origin validation.
- Non-loopback admin plaintext warning.
- Admin tests for allowed/blocked Host, Origin, and JSON content type.
- Docs updates for CIDR and admin transport.

### Explicitly out of scope

- DNS-aware routing matchers or resolving domains inside route evaluation.
- Native admin TLS implementation.
- Unified TCP `allow_private_egress` policy.
- Admin authentication changes or new admin endpoints.
- Changing the default bind or admin enabled state.

## 6. Required production changes

### A. Canonicalize CIDR inputs

Add a small canonicalization helper, for example:

```rust
fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    }
}
```

Use it in both `DestinationCidr` and `SourceCidr` before `cidr.contains(...)`.
Do not mutate the original `TargetAddr`; route explanation should still show the
client-supplied literal.

Add tests:

- `::ffff:10.0.0.1` matches `10.0.0.0/8`;
- `::ffff:10.0.0.1` in `SourceCidr` matches an IPv4 source CIDR;
- native IPv6 CIDRs still match native IPv6;
- IPv4 CIDRs do not match unrelated IPv6;
- mapped addresses do not bypass a `not { destination_cidr = ... }` rule.

### B. Make the CIDR contract explicit

1. Add a config warning when a rule’s action is `reject` (or equivalent) and it
   contains a `destination_cidr` in a private/reserved range without any
   host/host-suffix/host-regex/domain condition. The warning should say CIDR
   matches literal IP targets only; a domain resolving into that range is not
   matched by this rule, and upstream chains resolve names remotely.
2. Update `docs/security/SECURE_CONFIGURATION.md` and
   `docs/CONFIG_REFERENCE.md` to state the literal-only contract and to point
   operators to `allow_private_egress`/egress policy or trusted upstreams for
   names.
3. Do not add DNS resolution to the router. A DNS-aware policy, if ever needed,
   is a separate architecture decision.

Be conservative with the warning: it must not fire for rules that intentionally
match only literal IPs and also have a host condition that covers names, and it
must not echo credentials.

### C. Add admin Host/Origin validation

Add an optional `[admin] allowed_hosts = ["admin.example", "127.0.0.1"]` field
(compiled into `AdminConfig`). Defaults:

- loopback bind: allow the bind host, `localhost`, `127.0.0.1`, and `[::1]`/
  `::1`;
- non-loopback bind: allow the bind host and `localhost`; operators behind a
  reverse proxy or DNS name must add the host explicitly.

Implement a `host_allowed(req, allowed_hosts)` check before auth/handle:

- parse the `Host` header (strip the port; handle bracketed IPv6);
- reject a missing/unparseable/unknown Host with `400 Bad Request`;
- never redirect or echo the rejected value into HTML;
- keep the response body generic.

For `POST /-/route-explain`:

- require `Content-Type: application/json` (case-insensitive media type);
- if an `Origin` header is present, require its host to be in `allowed_hosts`;
- reject `Origin: null` for browser-style requests unless the operator
  explicitly allows it.

Do not apply Host validation to the CLI admin client in a way that breaks
`http://127.0.0.1:9090`; the default allowlist must cover it.

### D. Warn on plaintext non-loopback admin

In `validate_config_security`, for a non-loopback `admin.bind`, emit a warning
that the admin protocol is plaintext HTTP and that Basic/Bearer credentials and
topology are exposed; recommend loopback, a VPN/SSH tunnel, or a TLS-terminating
reverse proxy with `allowed_hosts`. Keep the existing “no auth” warning.

### E. Tests

- CIDR canonicalization tests above.
- Config warning tests for private CIDR + reject + no host condition; no warning
  for literal-only intentional rules or CIDR + host condition.
- Admin Host acceptance/rejection (including IPv6 and port forms).
- Admin Origin acceptance/rejection for POST.
- `Content-Type` rejection for `route-explain`.
- Non-loopback admin warning.
- Existing admin auth/limiter/status tests remain green.

## 7. Ordered work packages

### Work package A — CIDR canonicalization

Intent: remove the mapped-IPv6 bypass without changing route semantics.

Acceptance evidence: destination/source matcher tests; no async work added.

### Work package B — Literal-only warning and docs

Intent: prevent operators from treating CIDR rules as a complete private ACL.

Acceptance evidence: warning tests; docs updated; no credential echo.

### Work package C — Admin Host/Origin validation

Intent: block DNS-rebinding reads and simple cross-origin POSTs.

Acceptance evidence: admin tests for allowed/blocked Host, Origin, and content
type; default loopback clients still work.

### Work package D — Admin plaintext warning

Intent: make non-loopback admin transport risk visible.

Acceptance evidence: warning test; docs updated.

## 8. Failure, cancellation, restart, and contention semantics

- A rejected Host/Origin fails before auth and before reading a body; no state
  change.
- Host validation is a pure header check; no DNS lookup, blocking work, or
  additional connection.
- Admin connection timeout/cap and auth-failure limiter remain in force.
- Route matching remains synchronous and snapshot-based; no new locks.
- Config warnings remain non-fatal and are emitted once at load/reload.

## 9. Compatibility and migration

- Adding `allowed_hosts` is additive; existing configs keep working because the
  default allowlist is derived from the bind and loopback names.
- Operators using a DNS name or reverse proxy for admin must add that host;
  document the migration.
- CIDR canonicalization changes only previously broken mapped-IPv6 matches;
  literal IPv4/IPv6 behavior is unchanged.
- The CIDR warning is advisory; configurations are not rejected.
- No pproxy claim change.

## 10. Required tests

### Focused unit tests

- `eggress-routing` destination/source CIDR mapped IPv6, native IPv6, and
  negation cases.
- `eggress-config` private-CIDR warning and admin `allowed_hosts` validation.
- `eggress-admin` Host parsing (IPv4, IPv6 brackets, port), allowed/blocked
  Host, allowed/blocked Origin, content-type rejection.

### Integration tests

- Admin status/routes/upstreams/metrics with default loopback Host.
- Admin route-explain POST with JSON content type.
- Admin with a configured allowed host and a mismatched Host.
- Reload: admin bind/auth unchanged; `allowed_hosts` changes follow the
  existing restart-required policy or are applied consistently.

### Security and negative tests

- DNS-rebinding-style `Host: attacker.example` is rejected.
- `Origin: https://attacker.example` POST is rejected.
- Rejected requests do not include credentials or internal topology in the
  response.
- CIDR warning does not echo a password or upstream URI.

## 11. Required verification commands

```bash
cargo test -p eggress-routing
cargo test -p eggress-config
cargo test -p eggress-admin
cargo test -p eggress-runtime --test security_invariants
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

No oracle/differential run is required unless a compatibility claim changes.

## 12. Documentation updates

- `docs/security/SECURE_CONFIGURATION.md` — CIDR literal-only limitation,
  upstream-chain trust, admin loopback/VPN/TLS terminator, `allowed_hosts`.
- `docs/CONFIG_REFERENCE.md` — `admin.allowed_hosts`, CIDR semantics.
- `architecture/routing.md` — canonical IP matching and no DNS in routing.
- `architecture/admin.md` — Host/Origin policy and plaintext warning.
- `docs/security/THREAT_MODEL.md` — admin DNS-rebinding surface.

## 13. Acceptance criteria

- `::ffff:10.0.0.1` matches `10.0.0.0/8` for destination and source CIDRs.
- CIDR rules remain literal-only; docs and a config warning say so.
- Admin rejects unexpected Host and cross-origin POSTs while default loopback
  clients continue to work.
- Non-loopback admin produces a plaintext/warning message.
- No admin auth/limiter behavior regresses.
- No pproxy claim change.

## 14. Stop conditions

Stop and report if:

- `IpAddr::to_canonical`/mapped helper is unavailable at MSRV 1.89;
- admin Host validation breaks legitimate loopback or reverse-proxy access in a
  way that cannot be solved with `allowed_hosts`;
- the CIDR warning requires resolving domains or changing route async semantics;
- the change would alter an existing compatibility claim without evidence;
- admin TLS becomes necessary to satisfy the finding (separate decision).

## 15. Closure evidence required

- Matcher/config/admin test output.
- Warning output for a sample private-CIDR rule and a non-loopback admin.
- Documentation diff.
- Statement that no DNS was added to routing and no admin state-changing
  endpoint was introduced.
- Explicit no-release statement.

## 16. Handoff notes

- Keep `route explain` output useful: canonicalize for matching, not for the
  reported target.
- Do not trust `X-Forwarded-Host`; only the actual `Host` header is in scope.
- `allowed_hosts` should compare host names case-insensitively and ignore the
  port.
- Coordinate the admin warning wording with Parity M003’s security-doc changes.
