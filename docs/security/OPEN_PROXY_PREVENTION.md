# Open Proxy Prevention

## Overview

An open proxy allows any network client to route traffic through the server
without authentication. Eggress implements multiple layers of defense to
prevent accidental or intentional open proxy exposure.

## Listener Bind Address Policy

### Warning System

Config validation emits warnings for dangerous combinations:

- **TCP listeners**: Warning if `bind` is not loopback and no auth/Shadowsocks/SSR/Trojan
- **Standalone UDP**: Warning if `standalone_pproxy_udp` binds non-loopback (no TCP auth channel)
- **Admin server**: Warning if non-loopback without auth; with `admin.require_auth = true`, also warn on loopback without auth
- **Reverse control**: Warning if `control_bind` is not loopback and no auth

By default these are log-only. Native operators can fail closed:

```toml
[process.security]
fail_on_security_warnings = true
```

`eggress pproxy check --strict-security` exits non-zero when translated config
emits the same security warnings. pproxy-compat run defaults remain warn-only.

### Loopback Detection

A bind address is considered loopback if it resolves to `127.x.x.x` or `::1`.
Non-loopback addresses include `0.0.0.0`, `::`, and specific non-local IPs.

### Auth Detection

A listener is considered authenticated if it has any of:
- `[listeners.auth]` with `type = "password"`
- `[listeners.tls]` with cert/key
- `[listeners.shadowsocks]` with method/password

## Protocol-Level Controls

### HTTP CONNECT

- No built-in authentication
- Client IP logged at connection time
- Config validation warns on non-loopback

### SOCKS4/4a

- No built-in authentication
- Config validation warns on non-loopback

### SOCKS5

- Optional username/password auth
- Config validation warns on non-loopback if no auth

### Shadowsocks

- Built-in password authentication via AEAD
- No warning needed — cryptographically authenticated

## Administrative Controls

### Admin Server

- Default bind: `127.0.0.1:9090`
- Loopback may be unauthenticated by default (local-machine trust)
- Non-loopback without `[admin.auth]` is a structural validation error
- Set `admin.require_auth = true` on multi-user hosts; combine with
  `[process.security] fail_on_security_warnings = true` to refuse startup

### Metrics

- Exposed on admin server
- Protected by admin server bind address
- No credential-bearing labels

## Testing

Config validation tests verify:
- Non-loopback listener without auth produces warning
- Loopback listener does not produce warning
- Authed listener does not produce warning
- Non-loopback admin produces warning
- Loopback admin does not produce warning
- Non-loopback reverse control without auth produces warning
- Authed reverse control does not produce warning

See `crates/eggress-config/src/validate.rs` tests.
