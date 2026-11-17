# Security Hardening Milestone 003 — Closure

Status: closed

Source implementation plan:

- `plans/implementation/security-hardening/003-embed-ssh-strict-host-key-path.md`

Audit findings disposition:

| Finding | Description | Outcome |
|---------|-------------|---------|
| **#4** | `from_pproxy_uri` always `InsecureCompatibility` | **Closed** (additive) — `from_pproxy_uri_strict` / `from_pproxy_uri_with_host_key_policy`; default unchanged |
| **#12** | `EGRESS_SSH_INSECURE_HOST_KEYS` misuse | **Closed** — runtime messaging clarifies env applies only to compatibility mode |

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Additive strict API | `OutboundConnector::from_pproxy_uri_strict` + policy parameter | pass |
| Default `from_pproxy_uri` unchanged | Policy selection unit test + existing embed SSH tests | pass |
| Separate caches | Strict uses KnownHosts / KnownHostsFile cache constructors | pass |
| Python mirror | `PyOutboundConnector.from_pproxy_uri_strict` | pass |
| Docs | `docs/EMBED_API.md`, skill SSH section | pass |

## Residual risk

Default `from_pproxy_uri` remains MITM-capable by design for pproxy parity.
