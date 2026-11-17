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
| SOCKS4/4a | Optional (`[listeners.auth]`) | Credential is the full `user:pass` string in the SOCKS4 user-ID field (pproxy 2.7.9 semantics); username-only user IDs are rejected; warns on non-loopback |
| SOCKS5 | Optional (`[listeners.auth]`) | Username/password via SOCKS5 handshake |
| Shadowsocks | Built-in (AEAD) | Cryptographic auth via shared password; no warning needed |
| Trojan | Built-in (TLS SNI) | Password-only auth; upstream-only |

**Auth without TLS.** When a listener binds to a non-loopback address, has
`[listeners.auth]`, and offers HTTP, SOCKS4, or SOCKS5 without
`[listeners.tls]`, config validation emits a warning: the credential crosses
the network in clear. Add `[listeners.tls]`, bind to loopback, or terminate TLS
in a trusted tunnel. Shadowsocks and Trojan listeners are exempt (their own
layer encrypts and authenticates), and the configuration is never rejected for
this.

SOCKS4 has no password field. A SOCKS4 client must send the complete
`username:password` string as the user ID, because the listener compares the raw
user ID against the configured pair before creating any tunnel.

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

**Upstream trust roots.** Outbound TLS verification uses the Mozilla root set bundled through the `webpki-roots` crate. That is *not* the operating-system certificate store: the roots are compiled into the binary and change only when Eggress itself is updated, so a private CA installed in the OS store is not trusted automatically. Custom roots are a programmatic surface (`TlsClientConfigBuilder::with_custom_ca_pem`, surfaced to chain execution as `tls_override`); supplying them *replaces* the bundled set rather than extending it, and there is no TOML key for outbound trust roots in this release.

**TLS insecure mode** (`?insecure` / `?insecure=true` on a hop URI, or `with_insecure()` in code) accepts any certificate, and any handshake signature, without validation. It is documented as "for testing only" and must never be used in production. It is reachable from configuration in shipped CLI builds, not only from the programmatic API: `eggress-cli` and `eggress-runtime` compile the `insecure-tls` feature unconditionally. Configuration compilation emits one warning per upstream that selects it, naming the upstream and the affected hop indexes and protocols (never endpoints, credentials, or the source URI), before any chain executes. Insecure QUIC/H3 additionally requires the separate `insecure-quic` feature and is rejected at config compile in default builds.

### 3. Admin Endpoint Security

The admin server exposes `/-/status`, `/-/config`, `/-/routes`, `/-/upstreams`, `/-/reverse`, and `/-/metrics`, plus a dry-run `POST /-/route-explain`. Configure authentication for non-loopback access; configured credentials protect every endpoint.

**Default:** `127.0.0.1:9090` (loopback only).

```toml
[admin]
bind = "127.0.0.1:9090"
enabled = true
metrics = true

[admin.auth]
bearer_token_env = "EGGRESS_ADMIN_TOKEN"
```

**Plaintext transport.** The admin listener speaks plaintext HTTP/1.1: there is no native admin TLS, and the `eggress` admin client rejects `https://` admin URLs. A non-loopback bind therefore puts Basic/Bearer credentials, status, metrics, and routing topology on the network in clear, and emits a config warning (in addition to the existing "no auth" warning). Remote admin access belongs to one of:

1. loopback plus SSH port forwarding or a VPN
2. OS-level firewall rules restricting who can reach the port
3. a TLS-terminating reverse proxy in front of a loopback bind

**Host and Origin validation (`allowed_hosts`).** The admin server validates the `Host` header of every request to block DNS-rebinding reads: a page on an attacker origin can resolve its own name to `127.0.0.1` and fetch the loopback admin port, so the header — not the socket address — decides whether a request is acceptable.

```toml
[admin]
bind = "127.0.0.1:9090"
# Required when admin is reached through a reverse proxy or a DNS name.
allowed_hosts = ["admin.example", "127.0.0.1"]
```

The effective allowlist is the configured entries **plus** defaults derived from `bind`:

- loopback bind, or a wildcard bind (`0.0.0.0` / `::`): the bind host, `localhost`, `127.0.0.1`, and `::1`, so `curl http://127.0.0.1:9090/-/status` works without configuration
- any other non-loopback bind: the bind host and `localhost`

Matching is case-insensitive and port-insensitive (`Host: Admin.Example:9090` matches `admin.example`), and bracketed IPv6 forms are normalized. A missing, unparseable, or unlisted `Host` returns a generic `400 Bad Request` before authentication and before any body is read; the rejected value is never echoed. `X-Forwarded-Host` is ignored — only the actual `Host` header counts. Wildcard entries (`*`) are rejected at config validation.

`POST /-/route-explain` additionally requires `Content-Type: application/json` (media type only; parameters such as `; charset=utf-8` are ignored) and rejects an `Origin` whose host is not allowlisted. A browser-style `Origin: null` is rejected unless the literal entry `null` is added to `allowed_hosts`. Both checks run before the body is read.

**Migration note:** if you reach admin through a reverse proxy or a DNS name, add that host to `allowed_hosts` before upgrading; otherwise the request fails closed with `400`.

Non-loopback admin binds are rejected unless authentication is configured.

**CIDR rules are not a name-based ACL.** See [Private IP Blocking and CIDR Rules](#private-ip-blocking-and-cidr-rules): `destination_cidr` matches literal IP targets only.

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
3. **Prefer built-in native TLS** (`[reverse_servers.tls]` / `[reverse_clients.tls]`, server-authenticated with optional mTLS) when control traffic traverses untrusted networks; external wrappers remain an option for `pproxy_compat` wire.
4. **Monitor** `eggress_reverse_control_connections_rejected_total` for unauthorized attempts.
5. **Apply firewall rules** to limit which hosts can reach the control port.

See [REVERSE_SECURITY.md](REVERSE_SECURITY.md) for full details.

### 6. Network Policy

#### Private IP Blocking and CIDR Rules

Eggress does not have built-in IP allowlist/denylist on listeners. Use routing rules to control which destinations are accessible:

**CIDR rules are literal-IP-only.** `destination_cidr` and `source_cidr` compare against the literal address a client supplied. Route evaluation never resolves a name, so:

- a rule matching `10.0.0.0/8` does **not** block `internal.example` even when that name resolves to `10.1.2.3`;
- a request routed through an upstream group is resolved by the upstream, so no local CIDR check can see the address the upstream connects to;
- an IPv4-mapped IPv6 literal (`::ffff:10.0.0.1`) is canonicalized before matching, so it matches the same `10.0.0.0/8` rule as `10.0.0.1` and cannot be used to bypass one.

Eggress emits a non-fatal config warning when a `reject` rule uses a private/reserved `destination_cidr` and no host condition, because that rule is not a complete private-network ACL. For name-based protection use the egress guard below or a trusted upstream; CIDR rules complement those controls, they do not replace them.

**DNS rebinding protection:** Eggress rejects DNS resolutions that resolve to private/reserved IP ranges (loopback, link-local, RFC 1918, CGNAT `100.64.0.0/10`, IPv6 unique-local, NAT64 `64:ff9b::/96`, 6to4 `2002::/16`, Teredo `2001::/32`, documentation/benchmarking/ORCHID special-purpose blocks, and IPv4-mapped forms of the IPv4 ranges). This prevents DNS rebinding attacks. Literal private IP targets (e.g., `192.168.1.1`) are allowed for pproxy compatibility with local/LAN proxy usage.

**UDP private egress:** `allow_private_egress` (default `true` for pproxy compatibility) governs the locally resolving UDP paths — standalone SOCKS5 UDP, SOCKS5 UDP ASSOCIATE, and standalone Shadowsocks UDP. When it is `false`, literal private/reserved datagram targets are rejected before a flow is created, and a domain target is checked after the single DNS lookup: every resolved address is filtered through the same private/reserved classifier and a resolution whose answers are all non-public is dropped (no datagram is sent, no flow is created). Setting it to `false` therefore closes domain-based private-network access, not just literal addresses:

```toml
[[listeners]]
name = "udp-in"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[listeners.udp]
enabled = true
allow_private_egress = false
```

```toml
# Block access to internal networks
[[rules]]
id = "block-rfc1918"
reject = "blocked"

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

### 8. Self-Update Trust Boundary

`eggress update` installs GitHub Release binaries only after checksum and staged
candidate verification. Its trust boundary is deliberately narrow:

- **No source override.** Release and default builds ignore
  `EGRESS_UPDATE_BASE_URL` and reject every non-HTTPS update URL. The
  `file://` fixture hook exists only in test builds or builds with the
  non-default `update-fixtures` cargo feature; never ship a binary built with
  that feature.
- **Trusted tools only.** `curl` must exist at `/usr/bin/curl` or `/bin/curl`
  (`%SystemRoot%\System32\curl.exe` on Windows). `PATH` is not scanned, and a
  symlinked or group/other-writable tool is refused. SHA-256 is computed
  in-process, so `sha256sum`/`shasum` are not required and cannot influence
  verification.
- **Private staging.** Each transaction creates a fresh owner-only (`0700`)
  temporary directory with unpredictable naming, opens its download outputs
  with `create_new`, and re-hashes the archive immediately before extraction.
  A pre-existing path or a post-verification replacement fails the update
  closed; the installed pair is left untouched.
- **No privilege escalation.** The updater never invokes `sudo`/UAC and never
  edits shell or `PATH` state. An unwritable install directory fails before the
  download with a remediation message.
- **Advisories are gated.** CI and the release preflight run
  `cargo deny --workspace check`. The only ignores are the tracked
  `zip 6.0.0` advisories inherited from `eggup-archive`; they are listed with
  their removal condition in `deny.toml`.

For a fully operator-controlled deployment, prefer the OS package flow (install
to a root-owned directory, update through the package manager) and let
self-update remain a convenience for user-owned installations.

## Default vs. Hardened Mode

| Setting | Default | Hardened |
|---------|---------|----------|
| Admin bind | `127.0.0.1:9090` | `127.0.0.1:9090` (or VPN/firewall if remote) |
| Admin auth | None | Network-level (loopback/VPN) |
| Admin transport | Plaintext HTTP on loopback | Loopback + SSH/VPN, or TLS-terminating proxy |
| Admin `allowed_hosts` | Bind-derived loopback names | Explicit proxy/DNS host name |
| Listener auth | None | `[listeners.auth]` with password |
| Listener bind | `127.0.0.1:8080` | `0.0.0.0:port` with auth, or loopback |
| TLS on listener | None | `[listeners.tls]` with cert/key |
| Upstream encryption | Plaintext | `+tls` suffix or Shadowsocks |
| UDP client pinning | `true` | `true` |
| UDP standalone mode | Disabled | Disabled unless on trusted network |
| Reverse control auth | None | `auth_password_env` required |
| Reverse control bind | Configurable | Loopback or VPC-internal |
| Reverse control TLS | None | Built-in `[reverse_servers.tls]` / `[reverse_clients.tls]` (or external wrapper for `pproxy_compat`) |
| Config credentials | Plaintext in file | `password_env` + file permissions `0600` |
| Routing default | `direct` | `reject` (deny-by-default) |
| Metrics | Off | On, behind admin bind |
| Log level | `info` | `info` (avoid `debug` in production for credential safety) |

## Checklist Summary

- [ ] Authentication configured on all non-loopback listeners
- [ ] Admin server on loopback (or behind VPN/firewall)
- [ ] `[admin] allowed_hosts` lists the host name when admin is proxied or addressed by name
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
