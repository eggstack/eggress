# Routing, Health, Metrics, and Admin Milestone 003 — Closure Status

Status: closed

Source implementation plan:

- `plans/implementation/routing-health-observability/003-cidr-canonicalization-and-admin-exposure.md`

Source subsystem roadmap:

- `plans/subsystems/routing-health-observability-roadmap.md` (Milestone 3)
- `plans/subsystems/security-hardening-roadmap.md` (S4; audit findings M4, L12, M8 admin half)

Repository baseline reviewed: `ba64e16` (workspace 1.0.12) with the
2026-11-16 corrective waves present as uncommitted working-tree changes

Implementation commits or pull requests:

- none — this milestone is delivered as an uncommitted working-tree change per
  the coordinator's instruction (no commit, tag, or release was created)

## 1. Executive finding

The milestone is complete at the implementation level. Both routing matchers
now canonicalize IPv4-mapped IPv6 literals before a CIDR comparison, so
`::ffff:10.0.0.1` matches `10.0.0.0/8` for `destination_cidr` and
`source_cidr` (including inside a negation) without altering the target or
source reported by `route explain`. The literal-IP-only CIDR contract is now
explicit in the config validator (a conservative advisory warning) and in the
operator documentation, and the admin HTTP surface validates `Host` (plus
`Origin` and `Content-Type` for the one POST endpoint) against
`[admin] allowed_hosts` before authentication and before any body is read, with
a plaintext-transport warning for non-loopback binds.

No DNS resolution was added to routing, no admin state-changing endpoint was
introduced, admin auth/limiter/connection/body limits are unchanged, and no
compatibility claim was touched.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result | Notes |
|---|---|---|---|
| `::ffff:10.0.0.1` matches `10.0.0.0/8` for `destination_cidr` | `eggress-routing` `ipv4_mapped_destination_matches_ipv4_cidr`; live `route explain` demo | pass | Also `mapped_destination_cannot_bypass_negated_cidr` |
| `::ffff:10.0.0.1` matches an IPv4 CIDR for `source_cidr` | `ipv4_mapped_source_matches_ipv4_source_cidr`; runtime `route_explain_source_field_changes_decision` (mapped source) | pass | Also `mapped_source_cannot_bypass_negated_source_cidr` |
| Native IPv6 still matches native IPv6; unrelated v4/v6 does not match | `ipv6_cidr_match`, `native_ipv6_source_cidr_still_matches`, `ipv4_cidr_does_not_match_unrelated_native_ipv6`, `native_ipv6_cidr_does_not_match_ipv4`, `ipv4_mapped_destination_outside_ipv4_cidr_no_match` | pass | Family-correct in both directions |
| Route semantics and `route explain` reporting unchanged | `mapped_destination_canonicalization_does_not_rewrite_explanation`; live demo reports `[::ffff:10.1.2.3]:443` | pass | Canonicalization is local to the comparison |
| Config warning for private/reserved `destination_cidr` + `reject` + no host condition | `warn_private_destination_cidr_reject_rule_without_host_condition`, `warn_private_destination_cidr_inside_negation`, `warn_private_ipv4_mapped_destination_cidr` | pass | Non-fatal; message names the literal-only contract and upstream remote resolution |
| No warning when a host condition exists, for public ranges, or for non-reject actions | `no_warn_private_cidr_when_host_condition_present`, `no_warn_private_cidr_when_shortcut_host_condition_present`, `no_warn_public_destination_cidr_reject_rule`, `no_warn_private_cidr_for_non_reject_action` | pass | Conservative by design |
| Warning never echoes credentials or upstream URIs | `cidr_warning_does_not_echo_credentials_or_upstream_uri`, `warn_plaintext_non_loopback_admin_even_with_auth` | pass | Asserts on the secret and endpoint strings |
| `[admin] allowed_hosts` TOML + compiled field, additive to bind defaults | `compile_admin_allowed_hosts_default_to_loopback_names`, `compile_admin_allowed_hosts_are_additive_and_normalized`, `compile::admin_hosts::tests::*` | pass | Case-insensitive, port-insensitive, bracketed IPv6 |
| Invalid/wildcard entry rejected at validation | `invalid_admin_allowed_host_entry_is_rejected` | pass | `admin.allowed_hosts[0]` diagnostic |
| Host validation before auth and before body read | `host_rejection_precedes_authentication` | pass | Bad Host → 400 even with a bearer token configured |
| Missing/unparseable/unknown Host → generic 400, no echo, no redirect | `dns_rebinding_host_is_rejected`, `missing_or_unparseable_host_is_rejected`, runtime `admin_host_origin_and_content_type_policy_from_config` | pass | Body is exactly `bad request` |
| `X-Forwarded-Host` ignored | `forwarded_host_header_does_not_widen_the_allowlist` | pass | Only the actual `Host` header counts |
| Default `http://127.0.0.1:9090` works with no config | `default_loopback_hosts_are_accepted`, `empty_allowed_hosts_falls_back_to_the_bound_address`, live curl `200` | pass | `localhost` and `[::1]:port` too |
| `POST /-/route-explain` requires `application/json` | `route_explain_requires_json_content_type`, `route_explain_accepts_json_content_type` | pass | `415` otherwise; parameters and case tolerated |
| `Origin` host must be allowlisted; `Origin: null` rejected unless listed | `route_explain_rejects_cross_origin_post`, `route_explain_accepts_same_origin_post`, `route_explain_rejects_null_origin_unless_allowed` | pass | `403` with a generic body, no echo |
| Non-loopback plaintext admin warning | `warn_plaintext_non_loopback_admin_even_with_auth`; live supervisor log capture | pass | Existing no-auth warning retained |
| Reload policy for `allowed_hosts` | `reload_rejects_admin_allowed_hosts_change`, `eggress-runtime --test reload` | pass | `classify_reload_config` now compares the allowlist, so a change requires a restart (same policy as `admin.bind`) |
| Existing admin auth/limiter/status tests remain green | `cargo test -p eggress-admin`, `cargo test -p eggress-runtime --test admin` | pass | 50 + 23 tests |
| No pproxy compatibility claim change | no manifest or tier change in scope | pass | Manifest untouched |
| Docs updated | `docs/security/SECURE_CONFIGURATION.md`, `docs/CONFIG_REFERENCE.md`, `docs/security/THREAT_MODEL.md`, `architecture/routing.md`, `architecture/admin.md` | pass | Admin/CIDR sections only in the shared security doc |

## 3. Production implementation evidence

Routing (`crates/eggress-routing/src/matcher.rs`):

- `canonical_ip(IpAddr) -> IpAddr` uses `Ipv6Addr::to_ipv4_mapped()` and is
  applied in `MatchExpr::DestinationCidr` and `MatchExpr::SourceCidr`
  immediately before `IpNet::contains`. `TargetHost::Domain` still returns
  `false`; nothing is resolved. The `RouteRequest` target/source is untouched,
  so `Router::explain` keeps reporting the client-supplied literal.
- No async work, lock, or allocation was added beyond the copy of an `IpAddr`.

Config (`crates/eggress-config/`):

- `validate/security.rs`: `warn_literal_only_cidr_reject_rules` walks each
  `reject` rule's match expression (bounded by `MAX_MATCH_EXPR_DEPTH`),
  records whether a `host_exact`/`host_suffix`/`host_regex` condition scopes
  the rule (including the rule-level shortcut fields) and the first
  `destination_cidr` whose network or last address is private/reserved per
  `eggress_core::connector::is_reserved_or_private_ip`. One advisory warning per
  such rule, at the leaf path, naming the CIDR and the literal-only contract.
  The non-loopback admin block now emits a plaintext-HTTP warning in addition to
  the retained no-auth warning.
- `model.rs`: `AdminConfig.allowed_hosts: Option<Vec<String>>` with
  `#[serde(default)]`.
- `compile/admin_hosts.rs` (new): `normalize_host_entry`,
  `derive_allowed_hosts`, `effective_allowed_hosts`.
- `compile/model.rs`: compiled `AdminConfig.allowed_hosts: Vec<String>`.
- `compile/process.rs`: `compile_admin` derives the effective allowlist from the
  resolved bind (default `127.0.0.1:9090`) plus configured entries.
- `validate/core.rs`: `validate_admin` rejects an unparseable entry or a
  wildcard (`admin.allowed_hosts[i]`).

Admin (`crates/eggress-admin/`):

- `server.rs`: `AdminState.allowed_hosts`; `run()` replaces an empty list with
  bind-derived defaults for the address that actually bound; `host_allowed()`
  runs first in the per-request service (before auth, before any body read) and
  returns a generic `400 "bad request"`. `request_origin_allowed()` and
  `request_content_type_is_json()` live here; `normalize_host_entry` is reused
  from `eggress-config` so configured entries and header values normalize
  identically. The non-loopback bind warning now states plaintext HTTP and
  points at `allowed_hosts`.
- `routes.rs`: `/-/route-explain` rejects a non-JSON `Content-Type` (`415`) and
  a disallowed `Origin`/`Origin: null` (`403`) before `collect_limited`.
- `client.rs`: no behavior change; the client already sent
  `Content-Type: application/json` and `Host: <host>:<port>`.

Runtime (`crates/eggress-runtime/`): the compiled `allowed_hosts` is cloned
into `AdminState` at admin task spawn (`supervisor/services.rs`). Because the
allowlist is fixed when the admin task starts, `supervisor/reload.rs`'s
`classify_reload_config` now compares it alongside `enabled`/`bind`, so editing
`allowed_hosts` fails the reload with "admin endpoint bind/allowed_hosts
configuration changed; restart required" instead of silently keeping the old
policy.

## 4. Verification executed

### Commands run

```bash
cargo test -p eggress-routing
cargo test -p eggress-config
cargo test -p eggress-admin
cargo test -p eggress-runtime --test admin
cargo test -p eggress-runtime --test security_invariants
cargo clippy -p eggress-routing -p eggress-admin -p eggress-config -p eggress-runtime --all-targets -- -D warnings
cargo fmt -p eggress-routing -p eggress-admin -p eggress-config -p eggress-runtime -- --check
cargo build -p eggress-cli --bin eggress
cargo check -p eggress-pproxy-compat --all-targets
cargo test -p eggress-runtime --lib
cargo test -p eggress-runtime --test reload
```

### Results

- `eggress-routing`: 153 lib tests passed (183 total across lib, `properties`,
  `scheduler_parity`), 0 failed.
- `eggress-config`: 153 lib tests + 1 `tests/fuzz_smoke.rs` passed, 0 failed.
- `eggress-admin`: 50 lib tests passed, 0 failed.
- `eggress-runtime --test admin`: 23 passed, 0 failed.
- `eggress-runtime --test security_invariants`: 8 passed, 0 failed.
- clippy (four crates, all targets, `-D warnings`): clean, no warnings.
- `cargo fmt --check` (four crates): clean. `--all` was deliberately not used;
  the workspace-wide gate is the coordinator's step.
- `cargo build -p eggress-cli --bin eggress`: succeeded (used for the live
  evidence below).
- `cargo check -p eggress-pproxy-compat --all-targets`: clean after the
  one-field compile fix described in §10.
- `eggress-runtime --lib`: 70 passed, 0 failed (includes the new
  `reload_rejects_admin_allowed_hosts_change`).
- `eggress-runtime --test reload`: 18 passed, 0 failed.

Not run, by instruction from the coordinator: `cargo test --workspace`,
`cargo clippy --workspace`, `cargo fmt --all`, `--all-features`, benches,
oracle/differential/interop suites, `cargo deny`/`cargo audit`.

**Coordinator broad gate (2026-11-16, after all six corrective waves):** pass —
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --locked`, the documented CLI/embed feature slices, the
embed OpenSSH regression (6/6), and `cargo deny --workspace check`
(advisories/bans/licenses/sources). `cargo audit --ignore RUSTSEC-2023-0071`
reports only the four tracked `zip 6.0.0` advisories. The condition recorded in
§11 is therefore satisfied.

## 5. Invariant review

- **First-match-wins / explanation semantics**: unchanged. Only the value
  handed to `IpNet::contains` is canonicalized; rule ordering, action mapping,
  and `RouteExplanation` fields are untouched (proved by
  `mapped_destination_canonicalization_does_not_rewrite_explanation` and the
  live demo).
- **CIDR matching stays synchronous and literal-only**: no DNS, no blocking
  work, no new locks, no new allocations per rule beyond an `IpAddr` copy; a
  `Domain` target still never matches. The warning makes the contract explicit
  instead of silently implying a private-network ACL.
- **`SourceCidr` remains the exact peer address**: no proxy-header trust was
  added; `X-Forwarded-For` is still not read.
- **Admin auth/limiter/caps unchanged**: `authorized()`, the per-IP failure
  limiter, the 64-connection semaphore, 16 KiB body cap, 30 s connection
  timeout, endpoint set, default bind, and enabled default are untouched.
  Host validation was inserted ahead of auth, so it cannot weaken it;
  `host_rejection_precedes_authentication` asserts both orderings (400 then
  401).
- **Loopback clients keep working with no configuration**: bind-derived
  defaults cover `127.0.0.1`, `localhost`, and `::1` (and stay in place for a
  wildcard bind), asserted by `default_loopback_hosts_are_accepted`,
  `empty_allowed_hosts_falls_back_to_the_bound_address`, and live curl.
- **No compatibility claim change**: manifest, tier vocabulary, and generated
  parity reports untouched.

## 6. Failure and recovery review

- **Rejected Host/Origin/content-type**: fails before auth and before the body
  is consumed, so no state changes and no unbounded read. Responses are generic
  (`bad request`, `{"error":"origin not allowed"}`,
  `{"error":"expected content-type: application/json"}`) and never echo the
  rejected value, credentials, or topology.
- **Missing/unparseable Host**: fails closed with 400. A request without
  `Content-Length`/body is never inspected first.
- **Empty allowlist in a hand-built `AdminState`**: `run()` derives defaults
  from the bound address instead of denying everything; the production path
  always supplies the compiled list, so this is a fallback, not a policy.
- **Startup bind failure**: unchanged — still a startup error before readiness.
- **Reload**: the allowlist is bound to the listener, not to the snapshot, and
  `classify_reload_config` now rejects a change to it with the same
  restart-required verdict as `admin.bind`; routing/health/upstream swaps are
  unaffected. No partial state swap was added.
- **Cancellation/shutdown**: unchanged; the admin task still stops last through
  the ordered shutdown path. No new task or timer was introduced.
- **Malformed input**: identity length/emptiness, invalid `source`/`target`,
  and oversized body handling are unchanged and covered by the existing runtime
  admin tests.

## 7. Migration and compatibility review

- `[admin] allowed_hosts` is additive with a serde default. Existing configs
  compile; loopback admin keeps working with no new key because the defaults
  are derived from `bind`.
- Operators reaching admin through a reverse proxy or a DNS name must add that
  host; otherwise requests fail closed with `400`. Documented migration note in
  `docs/security/SECURE_CONFIGURATION.md` and the new key in
  `docs/CONFIG_REFERENCE.md`.
- Wildcard allowlist entries are rejected at validation rather than silently
  ignored, so a mistaken "allow everything" cannot look like a security control.
- CIDR canonicalization only changes previously broken mapped-IPv6 matches;
  literal IPv4 and native IPv6 behavior is unchanged. The warning is advisory —
  no configuration is rejected.
- One internal compile-only consequence: the TOML `AdminConfig` gained a field,
  so `eggress-pproxy-compat`'s in-process translator literal needed
  `allowed_hosts: None` (see §10). No translator output, URI grammar, or tier
  behavior changed.

## 8. Security review

- The DNS-rebinding read path is closed for the admin surface: `/-/status`,
  `/-/config`, `/-/routes`, `/-/upstreams`, `/-/reverse`, and `/metrics` all
  require an allowlisted `Host`; `/-/route-explain` additionally requires JSON
  content type and an allowlisted `Origin`.
- The check is a pure header comparison: no DNS lookup, no additional
  connection, and the allowlist is fixed for the listener lifetime.
- `X-Forwarded-Host` is deliberately ignored, so a header the client controls
  cannot widen the allowlist.
- A rejected request never reflects attacker input, so the endpoint cannot be
  used as an HTML/JSON injection vector, and the generic body leaks no topology.
- Plaintext admin transport is now warned twice (config load and runtime) with
  concrete alternatives; admin TLS remains explicitly out of scope.
- No credentials appear in any new warning or response: asserted by
  `cidr_warning_does_not_echo_credentials_or_upstream_uri`,
  `warn_plaintext_non_loopback_admin_even_with_auth`, and the response-body
  assertions on the admin tests.
- Redaction rules, auth constant-time comparison, limiter behavior, and the
  bounded body/connection caps are unchanged.

## 9. Documentation and operations

- `docs/security/SECURE_CONFIGURATION.md`: admin section rewritten (plaintext
  transport, `allowed_hosts`, Origin/Content-Type policy, migration note) and
  the CIDR section now states the literal-only contract, the mapped-IPv6
  canonicalization, and the advisory warning. Default-vs-hardened table and
  checklist gained admin transport/allowlist rows.
- `docs/CONFIG_REFERENCE.md`: `[admin] allowed_hosts` row plus a policy
  paragraph; CIDR leaf-matcher descriptions now say "literal target/source IP"
  with a paragraph on no-DNS, mapped-IPv6 canonicalization, and the warning.
- `docs/security/THREAT_MODEL.md`: new attacker A10 (DNS-rebinding web page),
  admin entry-point note, and mitigations 20–21.
- `architecture/routing.md`: new "CIDR matching rules" section (canonicalization,
  literal-only, explanation unchanged) plus security-note and test-map updates.
- `architecture/admin.md`: `AdminState.allowed_hosts`, the Host/Origin gate in
  the request pipeline, invariants, security notes, test map, and a reviewer
  gotcha about bind-derived defaults.
- Operator diagnostics: warnings carry a stable path (`admin.bind`,
  `rules[i]…destination_cidr`) so they can be filtered.

## 10. Unresolved findings

| Severity | Finding | Impact | Required action |
|---|---|---|---|
| low | `crates/eggress-pproxy-compat/src/translate/native.rs` needed a one-field compile fix (`allowed_hosts: None`) because the TOML `AdminConfig` gained a field | none — behavior identical; the file is outside this milestone's file list | Coordinator should include the one-line diff in the parity-compat wave review so it is not mistaken for a compat change |
| low | `crates/eggress-runtime/src/supervisor/reload.rs` gained the `allowed_hosts` comparison and `crates/eggress-runtime/src/supervisor.rs` gained one unit test (`reload_rejects_admin_allowed_hosts_change`) to satisfy plan §10's reload requirement | none — admin config changes already require a restart for `bind`; this closes a silent divergence for the new security field | Coordinator should include these two files in the runtime review scope; only `supervisor/services.rs` was listed in the assignment |
| low | Admin TLS is still not implemented: non-loopback admin remains plaintext and is only warned about | expected — explicitly out of scope for M003 | separate architecture decision if native admin TLS is ever required |
| low | `destination_cidr`/`source_cidr` remain literal-IP-only; the guard for names is the resolved-target egress policy | by contract, not a defect | document/consume the egress guard rather than adding DNS to routing |

## 11. Roadmap disposition

Milestone closed. The coordinator's workspace-wide `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace --locked` gate passed on 2026-11-16 after all six
corrective waves landed. No corrective implementation plan is required for M003
scope itself.

## 12. Registry updates

- `plans/subsystems/routing-health-observability-roadmap.md`: status line,
  Milestone 3 status, and the milestone table row now record
  `closed` with this closure record as the link.
- `plans/implementation/routing-health-observability/003-cidr-canonicalization-and-admin-exposure.md`:
  `Status: implemented`.
- `plans/registry.md`: no change made by this session — the milestone row is
  owned by the coordinator's registry reconciliation across the corrective
  waves.
