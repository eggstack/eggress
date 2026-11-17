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
- Config validation rejects non-loopback admin binds without authentication
- Shadowsocks/TLS listeners provide built-in authentication
- Password-authenticated HTTP/SOCKS listeners on a non-loopback bind emit a non-fatal warning when no `[listeners.tls]` is configured, because the credential would otherwise cross the wire in clear
- `unsafe_code = "deny"` prevents memory safety issues that could be exploited

### A2: Malicious Upstream Proxy

Compromised or malicious upstream server that logs or modifies proxied traffic.

**Mitigations:**
- TLS transport for upstream connections (`socks5+tls://`, `http+tls://`)
- Upstream TLS verifies the peer against the bundled Mozilla root set (`webpki-roots`), not the OS certificate store; pinning a private CA replaces that set rather than extending it
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
- `unsafe_code = "deny"` in all workspace crates

### A9: Log/Metrics Secret Leaker

Attacker reading logs, metrics, or admin output for credentials.

**Mitigations:**
- `PproxyUri::redacted_display()` replaces credentials with `****:****@`
- Config validation and compilation never log credentials
- Admin snapshots expose only metadata, not credentials
- Metrics labels never contain raw URIs or passwords

### A10: DNS-Rebinding Web Page (admin surface)

A page on an attacker-controlled origin resolves its own hostname to
`127.0.0.1` and issues same-origin requests to the local admin port. Without a
`Host` check the browser's request is indistinguishable from an operator's, so
`/-/status`, `/-/config`, `/-/routes`, `/-/upstreams`, `/-/reverse`, and
`/metrics` become readable and `POST /-/route-explain` becomes callable.

**Mitigations:**
- The admin server validates the `Host` header against `[admin] allowed_hosts`
  plus bind-derived defaults (`localhost`, `127.0.0.1`, `::1` for a loopback
  bind) before authentication and before any request body is read; a missing,
  unparseable, or unlisted host returns a generic `400 Bad Request` with no
  echo of the value
- `X-Forwarded-Host` is ignored: only the actual `Host` header counts
- `POST /-/route-explain` requires `Content-Type: application/json` and rejects
  an `Origin` whose host is not allowlisted (including `Origin: null` unless
  the literal entry `null` is configured)
- The admin surface stays read-only apart from the dry-run `route-explain`
  endpoint, so a successful rebinding read leaks topology, not state changes
- Non-loopback admin binds require authentication and additionally emit a
  plaintext-HTTP warning; admin has no native TLS, so remote access belongs on
  a VPN/SSH tunnel or behind a TLS-terminating reverse proxy whose host name is
  listed in `allowed_hosts`

## Entry Points

| Entry Point | Protocol | Auth | Default Bind |
|-------------|----------|------|--------------|
| TCP listener | HTTP, SOCKS4/5, Shadowsocks | Optional (per listener) | `127.0.0.1:8080` |
| UDP listener | SOCKS5 UDP associate | Via TCP control | `127.0.0.1:0` |
| Transparent listener | iptables REDIRECT / PF | None (OS-level) | N/A (kernel) |
| Unix socket | SOCKS4/5, HTTP | Optional (OS perms) | `/var/run/eggress.sock` |
| Admin server | HTTP | None (loopback only); `Host`/`Origin` allowlisted | `127.0.0.1:9090` |
| Reverse control | Custom binary | Optional | Configurable |
| Python embed | In-process | Application | N/A |

## Mitigations Summary

1. `unsafe_code = "deny"` in all workspace crates
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
17. SOCKS4 listeners with `[listeners.auth]` require the full pproxy `user:pass`
    string in the SOCKS4 user-ID field; a username-only or wrong-password
    attempt fails before any tunnel is created and never seeds the shared
    IP-reuse cache
18. `allow_private_egress = false` is applied after DNS on every locally
    resolving UDP path (standalone SOCKS5 UDP, SOCKS5 UDP ASSOCIATE, standalone
    Shadowsocks UDP): all answers of the single lookup are filtered through the
    shared special-purpose/private classifier and a private-only resolution is
    dropped without creating a flow
19. The shared special-purpose classifier covers CGNAT `100.64.0.0/10`, NAT64
    `64:ff9b::/96`, 6to4 `2002::/16`, Teredo `2001::/32`, IPv6 benchmarking
    `2001:2::/48`, and ORCHID `2001:10::/28` / `2001:20::/28`, including
    IPv4-mapped forms
20. Admin `Host`/`Origin` validation against `[admin] allowed_hosts` (plus
    bind-derived loopback defaults) blocks DNS-rebinding reads; `route-explain`
    additionally requires a JSON content type
21. IPv4-mapped IPv6 literals are canonicalized before `destination_cidr` /
    `source_cidr` matching, so the mapped form cannot bypass an IPv4 CIDR rule
