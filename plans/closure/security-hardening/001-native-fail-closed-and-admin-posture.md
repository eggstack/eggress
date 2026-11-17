# Security Hardening Milestone 001 — Closure

Status: closed

Source implementation plan:

- `plans/implementation/security-hardening/001-native-fail-closed-and-admin-posture.md`

Audit findings disposition:

| Finding | Description | Outcome |
|---------|-------------|---------|
| **#1** | Unauthenticated open proxy / warn-only validation | **Closed** — `[process.security] fail_on_security_warnings` + `eggress pproxy check --strict-security` |
| **#8** | Loopback admin without auth | **Closed** — opt-in `admin.require_auth` (warn / fail-closed when combined) |

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Fail-closed maps security warnings to load errors | `fail_closed_on_security_warnings` + config tests | pass |
| Default unset remains warn-only | `fail_closed_unset_still_warns_only` | pass |
| Compat translator does not enable fail-closed | pproxy-compat translate leaves `process.security` unset | pass |
| Admin require_auth warns on loopback without auth | `warn_loopback_admin_when_require_auth` | pass |
| CLI strict-security exit 3 | `cli_exit_codes` strict-security tests | pass |
| Manifest/matrix unchanged | No edits under `docs/parity/` | pass |

## Verification

```bash
cargo test -p eggress-config
cargo test -p eggress-runtime --test security_invariants
cargo test -p eggress-cli --test cli_exit_codes
```

## Residual risk

pproxy-compat open-listener default remains warn-only by design (ADR-0002).
