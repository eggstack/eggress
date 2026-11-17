# Security Hardening Milestone 001 — Native fail-closed and admin posture

Status: closed

Repository baseline: `ba64e166474ede7caeba736a885322323d965911` (workspace 1.0.12 / `origin/main`)

Source roadmap:

- `plans/subsystems/security-hardening-roadmap.md` (Milestone 001)

Long-term requirements:

- `plans/000-long-term-specification.md` §2–§4
- `plans/001-terminology-and-domain-model.md` (Listener, admin)

Applicable ADRs:

- `plans/adrs/ADR-0002-native-compat-security-hardening.md`

Primary class: capability / infrastructure

Audit findings addressed: **#1** (unauthenticated open proxy / warn-only validation), **#8** (loopback admin without auth).

## 1. Objective

Give native operators a supported way to **fail closed** on `validate_config_security` warnings, and tighten admin posture guidance/opt-in so multi-user hosts are not silently readable on loopback—without changing pproxy-compat warn-only defaults.

## 2. Why this milestone is ready

- ADR-0002 accepted (path-split / opt-in harden).
- `validate_config_security` and `ConfigWarning` already exist (`eggress-config`).
- Startup already logs security warnings without failing (`eggress-runtime` supervisor).
- Structural validate already rejects non-loopback admin without auth.
- Operator docs already describe open-proxy and admin risks.

## 3. Current implementation evidence

- `crates/eggress-config/src/validate/security.rs` — non-fatal warnings for non-loopback listeners/reverse/admin without auth (Shadowsocks/SSR/Trojan/TLS exemptions as today).
- `crates/eggress-config/src/lib.rs` — `load_and_validate_with_warnings` / `validate_and_compile_toml_with_warnings`.
- `crates/eggress-runtime` — logs warnings at startup; does not fail the process for security warnings alone.
- Admin defaults enabled on loopback; `[admin.auth]` optional on loopback (`docs/security/SECURE_CONFIGURATION.md`, `OPEN_PROXY_PREVENTION.md`).
- `eggress pproxy check` surfaces diagnostics but has no universal `--strict-security` fail mode for native TOML.

Gap: no first-class `fail_on_security_warnings` (or equivalent) and no opt-in “admin auth required even on loopback.”

## 4. Invariants that must not regress

- pproxy-compat startup remains warn-only for open listeners unless an explicit strict flag is set by the operator.
- Non-loopback admin without auth remains a hard structural error.
- Credential redaction and auth-failure metrics behavior unchanged.
- Listener topology still not hot-reloaded; security policy applies at compile/startup validation.
- No silent change to `cli.listen` manifest tier.

## 5. Scope

### In scope

- Config knob (preferred shape: `[process.security] fail_on_security_warnings = true`, or equivalent documented name) honored by native load/run paths.
- Runtime/CLI behavior: when enabled, security warnings become startup/load errors with stable, non-secret messages.
- Optional CLI: `eggress pproxy check --strict-security` and/or native config-check path that exits non-zero on security warnings.
- Opt-in admin harden: e.g. `admin.require_auth = true` or security warning/error when admin enabled without auth even on loopback (default remains today’s allow-unauth-loopback unless native-stricter default is explicitly chosen and documented).
- Docs updates under `docs/security/*` and `docs/CONFIG_REFERENCE.md`.

### Explicitly out of scope

- Changing pproxy default of no `--auth` on `-l`.
- Standalone UDP protocol auth (M004).
- Private egress / SSH / updater findings (M002–M005).
- Manifest retiering of `cli.listen`.

## 6. Required production changes

### Server/runtime/config

- Extend config model + validate/compile to carry fail-closed security policy.
- When fail-closed is enabled, map `validate_config_security` outputs to `ConfigError` (or refuse supervisor start) instead of log-only warnings.
- Optional loopback-admin-require-auth policy as warning or error under the same vocabulary.

### Compat translation and manifest

- Translator MUST NOT enable fail-closed by default.
- No manifest claim change.

### Delivery (CLI/embed/Python/release)

- Wire CLI check/run paths to honor the flag; preserve existing exit-code contracts where already defined (`cli_exit_codes` tests).

### Documentation and static guards

- `OPEN_PROXY_PREVENTION.md`, `SECURE_CONFIGURATION.md`, `HARDENING_GUIDE.md`, `CONFIG_REFERENCE.md`, `architecture/config.md` / `runtime.md` as needed.
- `.skills/security-dev/skill.md` note for the new knob.

## 7. Ordered work packages

### Work package A — Config + validate plumbing

Intent: Represent fail-closed (and optional admin-require-auth) in compiled config.

Required changes: schema/model/validate/compile; defaults preserve today’s behavior.

Acceptance evidence: unit tests in `eggress-config` validate/tests covering warn vs fail.

### Work package B — Runtime/CLI enforcement

Intent: Native run/check paths honor fail-closed; compat default path unchanged.

Required changes: supervisor startup and CLI check commands; exit codes documented.

Acceptance evidence: runtime security_invariants / CLI exit-code tests; negative test that pproxy translate+run without flag still warns only.

### Work package C — Docs and operator messaging

Intent: Operators know how to enable harden and what still remains warn-only under compat.

Acceptance evidence: docs updated; no claim that pproxy defaults are fail-closed.

## 8. Failure, cancellation, restart, and contention semantics

- Fail-closed failures occur at config load / process start, before accepting listener traffic.
- Reload must not introduce a generation that bypasses fail-closed if the flag remains set (failed candidates never swap — existing invariant).
- Concurrent check commands are read-only and do not mutate runtime state.

## 9. Compatibility and migration

- Existing configs without the new keys behave exactly as today.
- Enabling fail-closed is an explicit operator action (or a clearly documented native-only default if chosen—prefer explicit opt-in for this milestone to minimize surprise).
- No parity tier changes.

## 10. Required tests

### Focused unit tests

- Security warnings alone vs fail-closed error for non-loopback unauthenticated listener.
- Loopback unauthenticated listener still allowed under default policy.
- Optional admin-require-auth cases.

### Integration tests

- Runtime refuses start when fail-closed + warning condition present.
- Runtime starts with warnings when fail-closed unset.

### Security and negative tests

- Error/warning text contains no credentials.
- Compat path without flag does not fail closed.

### Migration and compatibility tests

- Translated pproxy configs without auth still load under default policy.

## 11. Required verification commands

```bash
cargo test -p eggress-config
cargo test -p eggress-runtime --test security_invariants
cargo test -p eggress-cli --test cli_exit_codes
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Broaden to `cargo test --workspace --locked` before merge if Rust surface is substantial.

## 12. Documentation updates

- `docs/security/OPEN_PROXY_PREVENTION.md`
- `docs/security/SECURE_CONFIGURATION.md`
- `docs/security/HARDENING_GUIDE.md`
- `docs/CONFIG_REFERENCE.md`
- `architecture/config.md` / `architecture/runtime.md` / `architecture/admin.md` as ownership requires
- `.skills/security-dev/skill.md`

## 13. Acceptance criteria

- Operators can configure native fail-closed on security warnings and observe non-zero startup/check failure when warnings exist.
- Default pproxy-compat and unset-native behavior remains warn-only for open listeners.
- Admin multi-user risk is either opt-in fail/warn or clearly documented with a supported `[admin.auth]` path (knob preferred).
- No parity manifest claim changes.

## 14. Stop conditions

- Proposal would make unauthenticated `0.0.0.0` pproxy listens fail by default.
- Structural vs security validate ownership becomes unclear without an ADR amendment.
- Exit-code contract conflict with published CLI behavior cannot be resolved without a delivery ADR.

## 15. Closure evidence required

- Commits/PR link.
- Requirement-to-evidence matrix for findings #1 and #8.
- Command output summary for §11 tests.
- Confirmation that manifest/matrix were unchanged.
- Residual risk: compat open-proxy default still exists by design (ADR-0002).

## 16. Handoff notes

- Prefer opt-in fail-closed over native default flip in this milestone unless product owners explicitly request native-stricter defaults in the PR.
- Keep warning text actionable (bind, missing auth, how to set the flag).
- Do not conflate Shadowsocks/Trojan “crypto auth” exemptions with password auth — preserve existing exemption logic unless tests prove otherwise.
