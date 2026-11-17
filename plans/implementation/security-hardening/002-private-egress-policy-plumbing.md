# Security Hardening Milestone 002 — Private egress policy plumbing

Status: closed

Repository baseline: `ba64e166474ede7caeba736a885322323d965911` (workspace 1.0.12 / `origin/main`)

Source roadmap:

- `plans/subsystems/security-hardening-roadmap.md` (Milestone 002)

Long-term requirements:

- `plans/000-long-term-specification.md` §2–§4
- `plans/001-terminology-and-domain-model.md`

Applicable ADRs:

- `plans/adrs/ADR-0002-native-compat-security-hardening.md`

Primary class: capability / infrastructure

Audit findings addressed: **#5** (literal private IPs allowed), **#6** (`allow_private_egress` default true), **#7** (UDP domain admission without reserved-IP check).

## 1. Objective

Expose a coherent private-egress policy so operators can block literal private/reserved TCP targets and tighten UDP private/domain admission—while pproxy-compat keeps today’s permissive defaults.

## 2. Why this milestone is ready

- `ConnectOptions.enforce_literal_ip_check` already exists on `DirectConnector` (`eggress-core`); raw tunnel / H2 paths already force stricter checks.
- UDP `allow_private_egress` already exists on listener compile model and `validate_standalone_target`.
- Docs already recommend routing CIDR denies as a workaround (`PPROXY_COMPAT_SECURITY_DIFFERENCES.md`).
- Soft dependency on M001: reuse `[process.security]` / listener security vocabulary if present; otherwise introduce keys here.

## 3. Current implementation evidence

- `crates/eggress-core/src/connector.rs` — DNS rebinding default on; literal check default off.
- Server execute path uses connector options that leave literal check off for ordinary proxy sessions.
- `crates/eggress-config/src/compile/model.rs` — `allow_private_egress: true` default.
- `crates/eggress-udp/src/security.rs` — domain targets pass shape validation; private check deferred post-DNS (documented TOCTOU residual).
- Architecture: `architecture/core.md`, `architecture/udp.md`.

Gap: no first-class TOML/runtime plumbing for literal-IP enforcement on normal sessions; no native/compat default split for UDP private egress; UDP domain admission does not optionally resolve+filter like TCP.

## 4. Invariants that must not regress

- DNS-resolved private answers remain blocked when DNS rebinding enforcement is on.
- Compat translator / pproxy UDP paths keep permissive private egress unless operator overrides.
- No change to 64 KiB relay buffer or UDP pooling.
- Redaction and fail-closed unsupported composition unchanged.

## 5. Scope

### In scope

- Config keys to enable `enforce_literal_ip_check` for server/outbound direct connects (name aligned with M001 security vocabulary when available).
- Path-split defaults: native may default stricter for new installs **only if** documented and compat translate explicitly sets permissive `true`; safest approach for this milestone is **opt-in true enforcement** with docs recommending it, unless owners choose native-default-false for `allow_private_egress` with translator override.
- UDP: when private-egress denial is active, apply reserved-IP policy to resolved domain targets on the UDP direct/relay admission or flow-create path (opt-in or matching the same policy flag).
- Docs: CONFIG_REFERENCE, security differences, udp/core architecture.

### Explicitly out of scope

- Eliminating all DNS TOCTOU without connect-time revalidation (document residual).
- Changing routing engine semantics beyond using existing reject/CIDR as complementary controls.
- Open-proxy auth (M001), SSH (M003), updater (M005).

## 6. Required production changes

### Core/relay

- Keep `ConnectOptions` as the enforcement primitive; do not fork a second IP classification list—reuse `is_dns_rebinding_risk` (or shared helper) for literals and resolved addresses.

### Server/runtime/config

- Plumb policy from compiled config into session/direct connect options.
- Compile/validate defaults per ADR-0002 (compat permissive).

### Protocols/transports/chain

- Outbound direct hops / server execute paths honor the flag.
- UDP standalone + associate paths honor `allow_private_egress` and domain policy consistently.

### Compat translation and manifest

- pproxy translate leaves permissive UDP private egress.
- No claim change unless behavior of default pproxy path changes (it must not).

### Documentation and static guards

- Update `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md` to point at the new knobs instead of “routing only.”
- `.skills/security-dev/skill.md` UDP section.

## 7. Ordered work packages

### Work package A — TCP literal-IP plumbing

Intent: Config → `enforce_literal_ip_check` on ordinary proxy direct connects.

Acceptance evidence: unit/integration tests blocking `192.168.0.1` literals when enabled; allowed when disabled.

### Work package B — UDP default split / operator override

Intent: Documented default policy; compat keeps `true`; native opt-in or split default as decided in implementation notes without breaking translate.

Acceptance evidence: compile/translate tests; CONFIG_REFERENCE accuracy.

### Work package C — UDP domain reserved-IP alignment

Intent: When policy denies private egress, domain targets that resolve to reserved/private fail closed at flow create / connect.

Acceptance evidence: UDP tests with fake/resolver harness or existing testkit patterns; architecture/udp.md updated including residual TOCTOU note.

## 8. Failure, cancellation, restart, and contention semantics

- Policy is fixed for a compiled snapshot; reload swaps routing/upstream/health and must carry the same security policy from the new candidate or fail the candidate.
- DNS failures during UDP domain admission remain observational vs fail-closed per existing UDP error taxonomy—do not invent silent allow.
- Cancellation of connects must not leave half-installed associations (existing UDP drain rules).

## 9. Compatibility and migration

- Unset keys → today’s behavior.
- If native default for `allow_private_egress` flips to `false`, the pproxy translator MUST emit `true` explicitly; document migration in release notes.
- Prefer opt-in for literal-IP enforcement in the first landing to avoid breaking LAN proxy users of native TOML.

## 10. Required tests

### Focused unit tests

- `eggress-core` literal check on/off.
- `eggress-udp` security validate for IP and domain policy branches.

### Integration tests

- Server/outbound path with policy enabled rejects private literals.
- Standalone UDP with `allow_private_egress=false` rejects private literals.

### Security and negative tests

- Compat default still allows private literal UDP/TCP as today.
- Error paths redact URIs.

### Migration and compatibility tests

- Translator output includes explicit permissive UDP flag if native default changes.

## 11. Required verification commands

```bash
cargo test -p eggress-core
cargo test -p eggress-udp
cargo test -p eggress-config
cargo test -p eggress-server
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Use workspace suite before merge for substantial connect-path changes.

## 12. Documentation updates

- `docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md`
- `docs/security/SECURE_CONFIGURATION.md`
- `docs/CONFIG_REFERENCE.md`
- `architecture/core.md`, `architecture/udp.md`
- `.skills/security-dev/skill.md`

## 13. Acceptance criteria

- Operators can enable denial of literal private/reserved TCP targets via config.
- UDP private egress can be denied without forking ad-hoc validation.
- With enforcement on, UDP domains resolving to private/reserved fail closed.
- pproxy-compat permissive defaults unchanged unless translator explicitly preserves them under a native default split.
- Manifest unchanged.

## 14. Stop conditions

- Implementation would block private literals on compat path by default.
- Duplicate divergent IP classification lists appear.
- UDP pooling or relay buffer changes proposed as “needed” for this milestone.

## 15. Closure evidence required

- Commits/PR; findings #5–#7 matrix; test commands; note residual DNS TOCTOU; confirm manifest unchanged.

## 16. Handoff notes

- Prefer shared helper with TCP DNS-rebinding classification.
- Coordinate config key names with M001 if both land nearby.
- Routing `destination_cidr` remains a complementary control, not a substitute once knobs exist.
