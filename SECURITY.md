# Security

## Supported Versions

| Version | Supported |
|---------|-----------|
| Current (main) | Yes |
| < Current | No |

Eggress follows a rolling release model. Only the latest release receives security fixes.

## Reporting a Vulnerability

Report security issues via [GitHub Issues](https://github.com/anomalyco/eggress/issues) using a private disclosure if possible (GitHub supports confidential issues for repo collaborators).

**Expected response timeline:**

- Acknowledgment: within 72 hours
- Triage and severity assessment: within 7 days
- Fix or mitigation: depends on severity, but critical issues are prioritized for the next release

Do not disclose vulnerabilities publicly until a fix is available.

## Security Features

- **`unsafe_code = "forbid"`** in all workspace crates — no memory safety issues possible through Rust code
- **No OpenSSL** — uses `rustls` with `ring` crypto provider, eliminating C FFI attack surface
- **Credential redaction** — `RedactedUri` replaces credentials with `****:****@` in all logs, metrics, admin output, and error messages
- **UDP amplification prevention** — `validate_target()` rejects multicast, broadcast, and unspecified addresses
- **UDP client pinning** — prevents address spoofing for UDP association ownership
- **HTTP header injection prevention** — control character validation in proxy credentials
- **Config validation** — structural errors, duplicate names, invalid references, and dangerous binds are rejected at load time
- **Admin loopback default** — admin server binds to `127.0.0.1` unless explicitly configured otherwise
- **Non-loopback bind warnings** — config validation emits warnings for unauthenticated network-facing listeners
- **Atomic config reload** — lock-free reads via `ArcSwap`; hot-reloadable fields swapped atomically
- **Bounded parsing** — HTTP headers, SOCKS fields, and credential inputs have explicit size limits

## Documentation

Detailed security documentation lives in [`docs/security/`](docs/security/):

- [Threat Model](docs/security/THREAT_MODEL.md) — trust boundaries, attackers, entry points
- [Hardening Guide](docs/security/HARDENING_GUIDE.md) — default posture, hardening checklist, dangerous configurations
- [Open Proxy Prevention](docs/security/OPEN_PROXY_PREVENTION.md) — bind address policy, auth detection
- [Reverse Proxy Security](docs/security/REVERSE_SECURITY.md) — reverse/backward proxy trust model and mitigations
- [Credential Redaction Policy](docs/security/REDACTION_POLICY.md) — canonical redaction format and regression testing
- [Secure Configuration](docs/security/SECURE_CONFIGURATION.md) — hardened configuration checklist
- [pproxy Compat Security Differences](docs/security/PPROXY_COMPAT_SECURITY_DIFFERENCES.md) — security trade-offs in pproxy compatibility mode

## Scope

Eggress is a single-operator, controlled-network proxy toolkit. The threat model assumes a trusted operator managing configuration on a trusted host. Untrusted inputs are network clients, upstream proxy responses, and parsed config file contents. See [docs/security/THREAT_MODEL.md](docs/security/THREAT_MODEL.md) for the full threat model.
