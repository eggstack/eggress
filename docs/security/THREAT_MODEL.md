# Threat Model

## Purpose

This document describes the threat model for Eggress, a network proxy toolkit.
It covers all trust boundaries, assets, entry points, and mitigations across the
TCP/UDP listeners, chains, Shadowsocks, Trojan, transparent listeners, Unix
sockets, reverse/backward proxying, Python embedding, admin endpoints, metrics,
config reload, and system proxy surfaces.

## Trust Boundaries

| Boundary | Trusted | Untrusted |
|----------|---------|-----------|
| Network listeners | TCP/UDP clients connecting to configured ports | Any network client, including the public internet |
| Admin server | Local operator (loopback only by default) | Remote clients if bound to non-loopback |
| Reverse control channel | Reverse server operator | Reverse client operators (if no auth configured) |
| Config file | File system owner with write access | TOML content parsed from disk |
| Python embedding | Application embedding Eggress | Python callers using the embed API |
| Upstream connections | Trusted upstream proxies | Remote proxy servers that may log/modify traffic |
| Logs/metrics | Operator with file/process access | Anyone who can read stdout, metrics endpoints, or log files |

## Assets

1. **Confidentiality of proxied traffic** — user data should not leak to observers.
2. **Integrity of routing decisions** — rules should not be bypassable by clients.
3. **Authentication of control planes** — reverse proxy auth should prevent unauthorized proxying.
4. **Availability** — resource exhaustion should not prevent legitimate use.
5. **Credential safety** — passwords, URIs, and secrets should never appear in logs, metrics, admin output, or error messages.
6. **Configuration integrity** — dangerous configurations should be visible and rejected or warned about.

## Attackers

### A1: Unauthenticated External Client

Uses Eggress as an open proxy by connecting to non-loopback listener or admin.

**Mitigations:**
- Default admin bind is `127.0.0.1:9090`
- Config validation warns on non-loopback binds without authentication
- Shadowsocks/TLS listeners provide built-in authentication
- `unsafe_code = "forbid"` prevents memory safety issues that could be exploited

### A2: Malicious Upstream Proxy

Compromised or malicious upstream server that logs or modifies proxied traffic.

**Mitigations:**
- TLS transport for upstream connections (`socks5+tls://`, `http+tls://`)
- Operator chooses trusted upstreams
- Upstream traffic is inherently trust-dependent

### A3: Malicious Local User

User with config file write access who can inject dangerous configurations.

**Mitigations:**
- Config validation rejects structural errors (duplicate IDs, invalid references)
- Security warnings for non-loopback binds without auth
- TOML schema enforces known fields (`deny_unknown_fields`)

### A4: Malicious Python Caller

Python code using the embed API to extract credentials or abuse the proxy.

**Mitigations:**
- GIL is released on blocking Rust calls via `py.detach()`
- No credential exposure in Python exceptions
- `EggressHandle` cleans up on drop
- Thread ownership is explicit

### A5: Compromised Reverse Client/Server

Compromised end of a reverse proxy control channel.

**Mitigations:**
- `allow_bind` policy limits which addresses can be bound
- `max_streams_per_listener`, `max_listeners_per_client`, `max_pending_external` cap resources
- Auth required for non-loopback `external_bind`
- Non-loopback `control_bind` without auth emits warning

### A6: Network Observer of Reverse Control Channel

Passive observer of plaintext reverse proxy control traffic.

**Mitigations:**
- Auth credentials are in the control channel (plaintext by default)
- TLS is recommended for non-loopback reverse channels
- `auth_password_env` supports environment variable injection

### A7: Resource Exhaustion Attacker

Attacker inducing resource exhaustion via connection churn or payload size.

**Mitigations:**
- `connection_limit` per listener
- `max_associations` for UDP
- `max_control_connections`, `max_streams_per_listener`, `max_pending_external` for reverse
- Bounded sniff buffers, header parsing, credential fields
- Admin request size bounded by hyper

### A8: Parser Edge Case Exploiter

Attacker sending malformed protocol data to trigger bugs.

**Mitigations:**
- Bounded parsing (no unbounded reads)
- Property tests for all protocol codecs
- Fuzz harnesses for SOCKS, HTTP, Trojan, URI, Shadowsocks parsers
- `unsafe_code = "forbid"` in all workspace crates

### A9: Log/Metrics Secret Leaker

Attacker reading logs, metrics, or admin output for credentials.

**Mitigations:**
- `PproxyUri::redacted_display()` replaces credentials with `****:****@`
- Config validation and compilation never log credentials
- Admin snapshots expose only metadata, not credentials
- Metrics labels never contain raw URIs or passwords

## Entry Points

| Entry Point | Protocol | Auth | Default Bind |
|-------------|----------|------|--------------|
| TCP listener | HTTP, SOCKS4/5, Shadowsocks | Optional (per listener) | `127.0.0.1:8080` |
| UDP listener | SOCKS5 UDP associate | Via TCP control | `127.0.0.1:0` |
| Transparent listener | iptables REDIRECT / PF | None (OS-level) | N/A (kernel) |
| Unix socket | SOCKS4/5, HTTP | Optional (OS perms) | `/var/run/eggress.sock` |
| Admin server | HTTP | None (loopback only) | `127.0.0.1:9090` |
| Reverse control | Custom binary | Optional | Configurable |
| Python embed | In-process | Application | N/A |

## Mitigations Summary

1. `unsafe_code = "forbid"` in all workspace crates
2. Credential redaction in all display/output paths
3. Config validation for structural integrity and security warnings
4. UDP amplification prevention (`validate_target()` rejects multicast/broadcast/unspecified)
5. UDP client pinning prevents address spoofing
6. HTTP header injection prevention (control character validation)
7. Transparent proxy SAFETY comments and `read_unaligned`
8. Unix socket file-type safety checks (`unlink_existing`)
9. Legacy stream ciphers and SSR intentionally rejected
10. Platform capability checks with honest reporting
11. Bounded sniff buffers, header limits, credential fields
12. Connection semaphore and bounded task pools
13. Atomic config reload (lock-free reads)
14. Graceful shutdown ordering (drain before cancel)
15. No secrets in Python package data
16. Config warnings for non-loopback binds without authentication
