# Security Hardening Milestone 002 — Closure

Status: closed

Source implementation plan:

- `plans/implementation/security-hardening/002-private-egress-policy-plumbing.md`

Audit findings disposition:

| Finding | Description | Outcome |
|---------|-------------|---------|
| **#5** | Literal private IPs allowed | **Closed** — `[process.security] enforce_literal_ip_check` plumbed to server/outbound `ConnectOptions` |
| **#6** | `allow_private_egress` default true | **Closed** (opt-in deny) — listener UDP flag retained; docs recommend `false` for harden |
| **#7** | UDP domain admission without reserved-IP check | **Closed** — `reject_reserved_resolved_addr` + `UdpTargetFlow::new_with_private_egress` |

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Config → enforce_literal_ip_check | `process_security_knobs_compile` + runtime ConnectionConfig plumbing | pass |
| Shared IP classification | Uses `eggress_core::connector::is_dns_rebinding_risk` | pass |
| UDP private deny post-DNS | `reject_reserved_resolved_*` unit tests; standalone/relay/ss paths | pass |
| Compat defaults permissive | Translator leaves UDP/process security unset | pass |
| Manifest unchanged | No parity claim edits | pass |

## Residual risk

DNS TOCTOU between admission and connect remains documented; not eliminated.
pproxy-compat private egress stays permissive by default (ADR-0002).
