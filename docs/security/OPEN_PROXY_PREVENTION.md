# Open Proxy Prevention

## Overview

An open proxy allows any network client to route traffic through the server
without authentication. Eggress implements multiple layers of defense to
prevent accidental or intentional open proxy exposure.

## Listener Bind Address Policy

### Warning System

Config validation emits warnings for non-loopback binds without authentication:

- **TCP listeners**: Warning if `bind` is not loopback and no auth/TLS/Shadowsocks
- **Admin server**: Warning if `bind` is not loopback
- **Reverse control**: Warning if `control_bind` is not loopback and no auth

It also warns when a non-loopback listener has password authentication, no
`[listeners.tls]`, and an HTTP/SOCKS protocol in its protocol set, because the
credential would otherwise cross the network in clear. The warning is advisory
only: the configuration still loads and runs.

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
- Config validation warns when password auth is configured without TLS

### SOCKS4/4a

- Optional username/password via `[listeners.auth]`
- The credential is carried in the SOCKS4 user-ID field as the full
  `user:pass` string, matching pproxy 2.7.9 (`userid in users`, where `users`
  holds `user:pass` strings); SOCKS4 has no separate password field, so a
  username-only user ID is rejected
- A rejected attempt fails before any tunnel is created and never seeds the
  auth-reuse cache; only a full-credential success is cached, and the cached
  identity is the authenticated username
- Config validation warns on non-loopback
- Config validation warns when password auth is configured without TLS

### SOCKS5

- Optional username/password auth (RFC 1929, constant-time compare)
- Config validation warns on non-loopback if no auth
- Config validation warns when password auth is configured without TLS

### Shadowsocks

- Built-in password authentication via AEAD
- No warning needed — cryptographically authenticated; the auth-without-TLS
  warning does not apply

## Administrative Controls

### Admin Server

- Default bind: `127.0.0.1:9090`
- No authentication — relies on network-level access control
- Warning emitted if bound to non-loopback

### Metrics

- Exposed on admin server
- Protected by admin server bind address
- No credential-bearing labels

## Testing

Config validation tests verify:
- Non-loopback listener without auth produces warning
- Loopback listener does not produce warning
- Authed listener does not produce warning
- Non-loopback HTTP/SOCKS4/SOCKS5 password auth without TLS produces the
  auth-without-TLS warning (`listeners[<i>].tls` path) and the message never
  echoes the username or password
- Authed listener with `[listeners.tls]` does not produce the auth-without-TLS
  warning
- Authed Shadowsocks and Trojan listeners do not produce the auth-without-TLS
  warning
- Loopback HTTP password auth without TLS does not produce the
  auth-without-TLS warning
- Non-loopback admin produces warning
- Loopback admin does not produce warning
- Non-loopback reverse control without auth produces warning
- Authed reverse control does not produce warning

SOCKS4 accept tests (`crates/eggress-server/src/accept/tests.rs`) verify the
full `user:pass` credential is accepted while username-only, wrong-password,
and empty user IDs are rejected without seeding the reuse cache.

See `crates/eggress-config/src/validate/tests.rs` and
`crates/eggress-server/src/accept/tests.rs`.
