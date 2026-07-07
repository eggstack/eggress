# Secure Configuration

This document provides a hardened configuration checklist for Eggress deployments. It covers all surfaces that require operator action to secure.

## Hardened Configuration Checklist

### 1. Authentication Setup

Eggress supports per-listener authentication via `[listeners.auth]`. Without authentication, any host that can reach the listener can use the proxy.

**Recommended:** Use `password_env` to inject credentials from the environment rather than embedding plaintext in config files.

```toml
[[listeners]]
name = "proxy"
bind = "0.0.0.0:1080"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "proxyuser"
password_env = "EGGRESS_PROXY_PASSWORD"
```

**Protocol-specific auth notes:**

| Protocol | Built-in Auth | Notes |
|----------|--------------|-------|
| HTTP CONNECT | Optional (`[listeners.auth]`) | No auth by default; warns on non-loopback |
| SOCKS4/4a | Optional (`[listeners.auth]`) | No auth by default; warns on non-loopback |
| SOCKS5 | Optional (`[listeners.auth]`) | Username/password via SOCKS5 handshake |
| Shadowsocks | Built-in (AEAD) | Cryptographic auth via shared password; no warning needed |
| Trojan | Built-in (TLS SNI) | Password-only auth; upstream-only |

### 2. TLS Configuration

Use TLS for any listener exposed to untrusted networks. Shadowsocks and Trojan provide built-in encryption; TLS adds transport security to HTTP and SOCKS listeners.

```toml
[listeners.tls]
cert = "/etc/eggress/certs/proxy.pem"
key = "/etc/eggress/certs/proxy-key.pem"
```

For upstream connections, use the `+tls` suffix to encrypt traffic to the next hop:

```toml
[[upstreams]]
id = "secure-upstream"
uri = "socks5+tls://upstream.example:1080"
```

**TLS insecure mode** (`with_insecure()`) accepts any certificate. It is only available via the programmatic API and documented as "for testing only." Never use in production.

### 3. Admin Endpoint Security

The admin server exposes `/-/status`, `/-/config`, `/-/routes`, `/-/upstreams`, and `/-/metrics`. It has **no authentication** — access control relies entirely on network binding.

**Default:** `127.0.0.1:9090` (loopback only).

```toml
[admin]
bind = "127.0.0.1:9090"
enabled = true
metrics = true
```

If remote admin access is required:

1. Keep admin on loopback and use SSH port forwarding or a VPN
2. If you must bind to a non-loopback address, use OS-level firewall rules to restrict access
3. Consider disabling metrics (`metrics = false`) if not needed — metrics are exposed on the same admin endpoint

**Warning:** Config validation emits a structured warning if admin is bound to a non-loopback address. Use `load_and_validate_with_warnings()` in the embed API to review warnings before startup.

### 4. UDP Security Settings

UDP associations are owned by TCP control connections and support SOCKS5 UDP ASSOCIATE. Key settings:

```toml
[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
idle_timeout = "60s"
max_associations = 512
max_targets_per_association = 32
client_pin = true
```

| Setting | Default | Recommendation |
|---------|---------|----------------|
| `client_pin` | `true` | Keep `true` — prevents UDP reflection attacks |
| `max_associations` | 1024 | Lower for constrained environments |
| `idle_timeout` | `60s` | Reduce for tighter resource management |
| `validate_target()` | Always on | Rejects multicast, broadcast, unspecified — no config needed |

**Standalone UDP mode** (`mode = "standalone_pproxy_udp"`) accepts SOCKS5-framed datagrams without a TCP control connection. This mode has no authentication beyond client address pinning. Only use on trusted networks.

### 5. Reverse Proxy Security

The reverse control channel is **plaintext TCP by default**. The wire format is pproxy-compatible (1-byte handshake + raw `user:pass` auth).

```toml
[[reverse_servers]]
id = "rs-main"
control_bind = "0.0.0.0:9443"
auth_username = "tunnel"
auth_password_env = "RS_AUTH_PASSWORD"
max_streams = 256
```

**Hardening requirements:**

1. **Always configure `auth_username` and `auth_password`** (or `auth_password_env`). Without auth, any host that can reach the control port can proxy through your server.
2. **Restrict `control_bind`** to loopback or a VPC-internal address unless remote clients are explicitly needed.
3. **Use TLS over the control channel** via stunnel, haproxy, or WireGuard when traversing untrusted networks. There is no built-in TLS on the control channel.
4. **Monitor** `eggress_reverse_control_connections_rejected_total` for unauthorized attempts.
5. **Apply firewall rules** to limit which hosts can reach the control port.

See [REVERSE_SECURITY.md](REVERSE_SECURITY.md) for full details.

### 6. Network Policy

#### Private IP Blocking and CIDR Rules

Eggress does not have built-in IP allowlist/denylist on listeners. Use routing rules to control which destinations are accessible:

```toml
# Block access to internal networks
[[rules]]
id = "block-rfc1918"
reject = "private_network"

[rules.match]
any_of = [
  { destination_cidr = "10.0.0.0/8" },
  { destination_cidr = "172.16.0.0/12" },
  { destination_cidr = "192.168.0.0/16" },
]

# Allow only specific destinations
[[rules]]
id = "allow-internet"
upstream_group = "egress"
direct = true

[rules.match]
not = { destination_cidr = "10.0.0.0/8" }
```

Source-based rules can restrict which clients are allowed:

```toml
[[rules]]
id = "allow-office-only"
direct = true

[rules.match]
source_cidr = "10.0.1.0/24"
```

#### Default Routing

Set `routing.default = "reject"` for deny-by-default posture:

```toml
[routing]
default = "reject"
```

### 7. Config File Permissions

Config files contain passwords in plaintext when `password` is used instead of `password_env`. Protect config files at the OS level:

```bash
chmod 600 /etc/eggress/config.toml
chown root:root /etc/eggress/config.toml
```

Never commit config files with credentials to version control.

## Default vs. Hardened Mode

| Setting | Default | Hardened |
|---------|---------|----------|
| Admin bind | `127.0.0.1:9090` | `127.0.0.1:9090` (or VPN/firewall if remote) |
| Admin auth | None | Network-level (loopback/VPN) |
| Listener auth | None | `[listeners.auth]` with password |
| Listener bind | `127.0.0.1:8080` | `0.0.0.0:port` with auth, or loopback |
| TLS on listener | None | `[listeners.tls]` with cert/key |
| Upstream encryption | Plaintext | `+tls` suffix or Shadowsocks |
| UDP client pinning | `true` | `true` |
| UDP standalone mode | Disabled | Disabled unless on trusted network |
| Reverse control auth | None | `auth_password_env` required |
| Reverse control bind | Configurable | Loopback or VPC-internal |
| Reverse control TLS | None | External wrapper (stunnel/WireGuard) |
| Config credentials | Plaintext in file | `password_env` + file permissions `0600` |
| Routing default | `direct` | `reject` (deny-by-default) |
| Metrics | Off | On, behind admin bind |
| Log level | `info` | `info` (avoid `debug` in production for credential safety) |

## Checklist Summary

- [ ] Authentication configured on all non-loopback listeners
- [ ] Admin server on loopback (or behind VPN/firewall)
- [ ] TLS configured on listeners exposed to untrusted networks
- [ ] Upstream connections use `+tls` or Shadowsocks
- [ ] Reverse control channel has auth + loopback bind (or TLS)
- [ ] UDP standalone mode disabled on untrusted networks
- [ ] Routing default set to `reject` for deny-by-default
- [ ] Private IP ranges blocked via CIDR rules
- [ ] Config file permissions set to `0600`
- [ ] Credentials injected via `password_env`, not plaintext
- [ ] Log level set to `info` (not `debug`)
- [ ] Metrics endpoint protected by admin bind
