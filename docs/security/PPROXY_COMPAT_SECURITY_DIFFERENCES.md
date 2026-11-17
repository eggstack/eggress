# pproxy Compatibility Security Differences

This document covers the security trade-offs when running Eggress in pproxy compatibility mode versus a hardened eggress-native configuration. The pproxy compat layer (`eggress-pproxy-compat`) translates pproxy-style CLI arguments and URIs to eggress TOML config. Some pproxy conventions create security posture that differs from hardened eggress-native deployments.

## Open Proxy Risks in pproxy Compat Mode

### Authentication Comes From the Listener URI, Not `--auth`

pproxy listener credentials are carried by the listener URI itself. pproxy's documented form is the URI fragment (`-l http://0.0.0.0:8080#user:pass`); Eggress also accepts userinfo (`-l http://user:pass@0.0.0.0:8080`). On HTTP/SOCKS-style listeners the compat translator copies those credentials into `[listeners.auth]` whenever the URI carries both a username and a password, whether or not `--auth` appears on the command line. (Shadowsocks and Trojan use the same URI fields for their protocol credentials instead.) A listener URI without credentials stays open, which is pproxy's default.

`--auth N` is **not** an authentication switch. It sets the source-IP re-authentication **reuse interval** for credentials that came from the listener URI. The pproxy default is `2592000` seconds (30 days), including when the flag is omitted.

**Contrast with eggress-native:** A hardened eggress config would include `[listeners.auth]` on any non-loopback listener and config validation would emit a warning.

```bash
# pproxy compat (no credentials — open proxy)
eggress pproxy run -- -l socks5://0.0.0.0:1080

# pproxy compat (credentials in the URI fragment, pproxy's documented form)
eggress pproxy run -- -l 'socks5://0.0.0.0:1080#user:pass'

# eggress-native (explicit auth, no reuse cache)
eggress --config hardened.toml  # with [listeners.auth] configured
```

### Source-IP Authentication Reuse (30-day default, process-global)

When compatibility listeners carry credentials, pproxy remembers a successful authentication per client IP and skips re-authentication for `--auth` seconds. Eggress reproduces that behavior exactly, including its scope:

- one reuse cache per compatibility process, bounded and monotonic-expiry;
- keyed **only by source IP** — not by listener, port, or username;
- shared across **all** credentialed compatibility listeners in that process.

Consequences:

- A client that authenticates once to any credentialed compatibility listener is trusted, for the reuse interval, by every other credentialed compatibility listener in the same process.
- `--auth N` changes only the interval; `--auth 0` disables practical reuse (entries expire immediately, so clients re-authenticate per connection).
- Compatibility startup emits one warning naming the 30-day default, the source-IP-only key, and the cross-listener cache. The warning never contains a listener URI or credential.
- The cache holds at most 4096 entries and evicts expired entries lazily, so client volume cannot grow it without bound.

**Recommendation:** bind compatibility listeners to loopback or a trusted network, and prefer native `[listeners.auth]` for stricter deployments: native authentication verifies every connection and has no IP-keyed reuse cache.

This reuse is the pinned `pproxy==2.7.9` behavior (`AuthTable._auth` keyed by IP with `authtime = 86400 * 30`). It is documented and warned about rather than changed, because changing it would break drop-in compatibility.

### Non-Loopback Binds Without Warnings at Translate Time

The `pproxy translate` command generates TOML from pproxy arguments. It copies listener credentials into `[listeners.auth]` when the listener URI carries them, but it never invents credentials that were not supplied, and `translate` itself does not run open-listener validation. Config validation emits open-listener warnings when the translated configuration is actually started.

**Recommendation:** Always run `eggress pproxy check` after `translate` to review security warnings before starting the service.

### Standalone UDP Mode

pproxy's `-ul` flag generates `mode = "standalone_pproxy_udp"`. This mode accepts SOCKS5-framed UDP datagrams directly without a TCP control connection. There is no authentication beyond client address pinning (`client_pin = true`).

**Risk:** On a non-loopback bind, any host that can reach the UDP socket can relay traffic.

**Contrast with eggress-native:** Standard UDP mode (`socks5_udp_associate`) requires a TCP control connection with SOCKS5 handshake before UDP data flows.

### No TLS on Listeners by Default

pproxy's `-l` flag does not enable TLS unless `--ssl` is also passed. The compat translator applies `--ssl` to all compatible listeners (matching pproxy's behavior), but without `--ssl`, listeners are plaintext.

**Contrast with eggress-native:** A hardened config would include `[listeners.tls]` on any internet-facing listener.

### No Upstream Encryption by Default

pproxy's `-r` flag does not add TLS to upstream connections unless `+tls` is explicitly in the URI. The compat translator passes through the URI as-is.

**Contrast with eggress-native:** Hardened configs use `socks5+tls://` or Shadowsocks for untrusted upstreams.

### Authless Reverse Proxy

pproxy's backward mode (`-R`) can be invoked without authentication. The compat translator generates `[[reverse_servers]]` with no `auth_username` or `auth_password` if none was in the original args.

**Risk:** Any host that can reach the control port can proxy through the server.

**Contrast with eggress-native:** Config validation emits a warning for non-loopback reverse control binds without auth.

## DNS Rebinding Protection

Eggress blocks DNS resolutions that resolve to private/reserved IP ranges (loopback, link-local, RFC 1918, etc.). This prevents DNS rebinding attacks where a malicious domain resolves to a private IP to access internal services.

Literal private IP targets (e.g., `192.168.1.1`) are allowed for pproxy compatibility with local/LAN proxy usage. This is the deliberate default — pproxy users commonly proxy to local network addresses.

A strict mode could block both DNS-resolved and literal private IPs, but the default favors compatibility. For hardened deployments, use routing rules (`destination_cidr` matchers) to restrict accessible destinations.

## What pproxy Compat Mode Exposes That Hardened Mode Doesn't

| Surface | pproxy compat default | Hardened eggress-native |
|---------|----------------------|------------------------|
| Listener authentication | From listener URI credentials; none when the URI has none | `[listeners.auth]` required on non-loopback |
| Auth reuse | Process-global, source-IP keyed, 30-day default (`--auth`) | Per-connection verification, no reuse cache |
| Admin server | Enabled by default with `--pac` | Disabled unless explicitly enabled |
| Standalone UDP | Available via `-ul` | Disabled (only `socks5_udp_associate`) |
| TLS on listeners | Only with `--ssl` | `[listeners.tls]` on internet-facing |
| Upstream encryption | Plaintext unless `+tls` in URI | `+tls` or Shadowsocks |
| Reverse proxy auth | Optional | `auth_password_env` required |
| Routing default | `direct` (allow-all) | `reject` (deny-by-default) |
| Metrics | On with admin | Off unless needed |

## How to Transition from pproxy Compat to Hardened Eggress-Native

### Step 1: Generate Baseline Config

```bash
eggress pproxy translate -- -l socks5://0.0.0.0:1080 -r socks5://upstream:1080
```

Save the output to a file and review it.

### Step 2: Add Authentication

Add `[listeners.auth]` to each listener:

```toml
[listeners.auth]
type = "password"
username = "proxyuser"
password_env = "EGGRESS_PASSWORD"
```

### Step 3: Enable TLS

Add TLS to internet-facing listeners:

```toml
[listeners.tls]
cert = "/etc/eggress/certs/proxy.pem"
key = "/etc/eggress/certs/proxy-key.pem"
```

### Step 4: Secure Upstream Connections

Change upstream URIs to use TLS:

```toml
[[upstreams]]
id = "upstream"
uri = "socks5+tls://upstream.example:1080"
```

### Step 5: Restrict Admin

Either keep admin on loopback or disable it:

```toml
[admin]
bind = "127.0.0.1:9090"
enabled = false  # or true with loopback bind
metrics = false
```

### Step 6: Secure Reverse Proxy

Add auth and restrict bind:

```toml
[[reverse_servers]]
id = "rs"
control_bind = "127.0.0.1:9443"
auth_username = "tunnel"
auth_password_env = "RS_AUTH_PASSWORD"
```

### Step 7: Set Deny-by-Default Routing

```toml
[routing]
default = "reject"
```

Add explicit allow rules for permitted destinations.

### Step 8: Run Validation

```bash
eggress pproxy check -- -l socks5://0.0.0.0:1080 -r socks5://upstream:1080
```

Review diagnostics for any security warnings before starting.

## Specific Settings That Differ

| Setting | pproxy compat generated | Hardened recommended |
|---------|------------------------|---------------------|
| `bind` on listeners | From `-l` arg (often `0.0.0.0`) | `127.0.0.1` or `0.0.0.0` with auth |
| `listeners.auth` | From listener URI credentials (`#user:pass` or `user:pass@`) | Always on non-loopback |
| `auth` reuse interval | `--auth` seconds, default `2592000` (30 days), source-IP keyed | Native auth: per connection |
| `listeners.tls` | Only with `--ssl` | Always on internet-facing |
| `upstreams[].uri` | From `-r` arg | `+tls` suffix |
| `admin.bind` | `127.0.0.1:9090` (default) | Same, or disabled |
| `admin.enabled` | `true` with `--pac` | `false` unless needed |
| `routing.default` | `direct` | `reject` |
| `listeners.udp.mode` | `standalone_pproxy_udp` with `-ul` | `socks5_udp_associate` |
| `listeners.udp.client_pin` | `true` (default) | `true` |
| `reverse_servers.auth_password` | From `-R` arg | `auth_password_env` |
| `reverse_servers.control_bind` | From `-R` arg | Loopback |
| `process.log_level` | `info` (default) | `info` (avoid `debug`) |

## Reference

- [SECURE_CONFIGURATION.md](SECURE_CONFIGURATION.md) — full hardened configuration checklist
- [OPEN_PROXY_PREVENTION.md](OPEN_PROXY_PREVENTION.md) — bind address policy and auth detection
- [REVERSE_SECURITY.md](REVERSE_SECURITY.md) — reverse proxy security details
- [HARDENING_GUIDE.md](HARDENING_GUIDE.md) — default posture and hardening checklist
- [docs/CONFIG_REFERENCE.md](../CONFIG_REFERENCE.md) — full configuration schema
