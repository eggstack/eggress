# Hardening Guide

## Default Security Posture

Eggress defaults to a conservative security posture:

- Admin server binds to `127.0.0.1:9090` (loopback only)
- Listeners warn if bound to non-loopback without authentication
- Reverse proxy warns if control channel is bound to non-loopback without auth
- UDP client pinning enabled by default
- `unsafe_code = "forbid"` in all workspace crates
- Credentials never logged (redacted display implementations)

## Hardening Checklist

### 1. Admin Server

- **Default**: `127.0.0.1:9090` — no authentication
- **Risk**: If bound to `0.0.0.0`, admin endpoints are exposed to the network
- **Action**: Keep admin on loopback unless remote admin is explicitly needed
- **If remote admin required**: Use network-level access control (firewall, VPN)

### 2. Listener Authentication

- **HTTP listeners**: No built-in auth — rely on network-level access control
- **SOCKS listeners**: No built-in auth — rely on network-level access control
- **Shadowsocks listeners**: Password-based auth via AEAD encryption
- **TLS listeners**: Client certificate auth via TLS (if configured)
- **Action**: Add `[listeners.auth]` with `type = "password"` for HTTP/SOCKS if exposed

### 3. Reverse Proxy

- **Control channel**: Optional username/password auth
- **Risk**: Plaintext auth by default (no TLS)
- **Action**: Use TLS transport for reverse control channels over untrusted networks
- **Action**: Configure `auth_password_env` instead of inline passwords
- **Action**: Use loopback `control_bind` unless remote clients are needed

### 4. Upstream Security

- **SOCKS5/HTTP**: Traffic sent in plaintext to upstream
- **SOCKS5+tls/HTTP+tls**: TLS-encrypted to upstream
- **Shadowsocks**: AEAD-encrypted to upstream
- **Action**: Use `+tls` suffix or Shadowsocks for untrusted upstreams

### 5. Config File

- **Risk**: Config file contains passwords in plaintext
- **Action**: Use `auth_password_env` for environment variable injection
- **Action**: Set restrictive file permissions (0600) on config files
- **Action**: Avoid committing config files with credentials to VCS

### 6. Metrics

- **Default**: Metrics exposed at `/metrics` on admin server
- **Risk**: High-cardinality labels could leak information
- **Action**: Disable metrics (`metrics = false`) if not needed
- **Action**: Keep admin on loopback to protect metrics endpoint

### 7. UDP

- **Client pinning**: Enabled by default — prevents address spoofing
- **Amplification**: `validate_target()` rejects multicast/broadcast
- **Action**: Review `max_associations` and `idle_timeout` for your use case

### 8. Transparent Proxy

- **Requires**: `CAP_NET_ADMIN` (Linux) or root
- **Setup**: iptables/nftables rules for REDIRECT
- **Risk**: If configured on unsupported platform, falls back to normal listener
- **Action**: Check platform capability with `/-/status` endpoint

## Dangerous Configurations

| Configuration | Risk | Severity |
|---------------|------|----------|
| Admin on `0.0.0.0` without auth | Admin endpoints exposed to network | High |
| HTTP listener on `0.0.0.0` without auth | Open proxy | High |
| Reverse control on `0.0.0.0` without auth | Unauthorized proxying | High |
| Config file with plaintext passwords | Credential exposure | Medium |
| Metrics on non-loopback admin | Information leakage | Medium |
| Transparent proxy without capability check | Silent fallback to normal listener | Low |
