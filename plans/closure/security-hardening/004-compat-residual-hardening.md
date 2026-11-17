# Security Hardening Milestone 004 — Closure

Status: closed

Source implementation plan:

- `plans/implementation/security-hardening/004-compat-residual-hardening.md`

Audit findings disposition:

| Finding | Description | Outcome |
|---------|-------------|---------|
| **#2** | Standalone `-ul` UDP without TCP auth | **Closed** (diagnostics) — non-loopback `standalone_pproxy_udp` security warning |
| **#3** | `--auth` IP reuse → `AUTH_NONE` | **Closed** (docs/warn) — NAT/shared-IP messaging + TTL>7d advisory; method selection unchanged |
| **#11** | `legacy-crypto` / insecure-* residual | **Closed** (docs) — threat/hardening guide residual risk callouts; defaults unchanged |

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Standalone UDP warning | `warn_standalone_udp_non_loopback` | pass |
| Auth reuse method selection unchanged | No edits to `accept/handlers.rs` AUTH_NONE path | pass |
| TTL advisory only | `auth-timeout-ttl` warning when `--auth` > 7 days | pass |
| Manifest tiers unchanged | No `docs/parity/` claim status flips | pass |
| Feature defaults unchanged | No Cargo feature default growth for insecure-* | pass |

## Residual risk

Parity-locked: standalone UDP remains usable without TCP auth; IP-scoped reuse may still select `AUTH_NONE` after cache hit; insecure features remain available only when explicitly enabled.
