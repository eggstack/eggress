# Configuration Reference

TOML configuration for eggress runtime mode (`--config path/to/config.toml`).

## pproxy-Compatible Arguments

If you are migrating from pproxy, you can translate pproxy-style CLI arguments
to TOML configuration using:

```bash
eggress pproxy translate -l socks5://:1080 -r http://proxy:8080
```

This outputs equivalent TOML that can be saved and used with `--config`. See
`docs/PPROXY_MIGRATION.md` for full migration guidance.

---

## Schema Version

```toml
version = 1  # Required. Only version 1 is supported.
```

## Top-Level Sections

```toml
version = 1
process = { ... }
timeouts = { ... }
listeners = [ ... ]
upstreams = [ ... ]
upstream_groups = [ ... ]
reverse_servers = [ ... ]
reverse_clients = [ ... ]
rules = [ ... ]
rules_file = "path/to/rules"
routing = { ... }
admin = { ... }
```

All sections are optional. An empty file is a valid config.

---

## `[process]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `log_format` | `"text"` / `"json"` / `"compact"` | `"text"` | Log output format |
| `log_level` | string | `"info"` | Tracing filter level |
| `shutdown_grace` | duration string | `"30s"` | Grace period for draining connections on shutdown |

---

## `[timeouts]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `handshake` | duration string | `"30s"` | Max time for inbound protocol detection + auth |
| `connect` | duration string | `"30s"` | Max time to establish outbound route |

Duration strings accept: `"5s"`, `"500ms"`, `"5m"`, `"1h"`.

---

## `[[listeners]]`

Each listener defines a TCP bind address and accepted protocols.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Unique listener identifier |
| `bind` | `"host:port"` | yes | Socket address to bind |
| `protocols` | `["http", "socks4", "socks5", "shadowsocks", "trojan"]` | yes | Accepted protocol list |
| `connection_limit` | u32 | 1024 | Max concurrent connections (semaphore) |
| `auth` | table | none | Inbound authentication policy |
| `udp_enabled` | bool | false | Legacy UDP flag (compatibility sugar) |
| `udp` | table | none | Nested UDP configuration |
| `tls` | table | none | Listener TLS configuration |

### `[listeners.auth]`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | `"password"` | yes | Only `"password"` auth type supported |
| `username` | string | no | Static username for Basic auth |
| `password` | string | no | Static password (plaintext) |
| `password_env` | string | no | Environment variable containing password |

### `[listeners.udp]`

UDP ASSOCIATE support for SOCKS5 listeners. Requires `protocols` to include `"socks5"`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable UDP association handling |
| `bind` | `"host:port"` | `"127.0.0.1:0"` | UDP relay socket bind address |
| `advertise` | `"ip"` | auto | IP advertised to clients in UDP ASSOCIATE reply |
| `idle_timeout` | duration string | `"60s"` | Association idle timeout |
| `target_idle_timeout` | duration string | `"30s"` | Per-target flow idle timeout |
| `max_associations` | usize | 1024 | Global max concurrent UDP associations |
| `max_targets_per_association` | usize | 64 | Max target flows per association |
| `max_datagram_size` | usize | 65535 | Max SOCKS5 UDP datagram size (257–65535) |
| `client_pin` | bool | `true` | Pin association to first client address |

The legacy `udp_enabled = true` without a `[listeners.udp]` section synthesizes default UDP config. If both are present and conflict, validation fails.

### Standalone UDP Mode

For pproxy-compatible standalone UDP relay (no TCP control connection required), set `mode = "standalone_pproxy_udp"` in the `[listeners.udp]` section:

```toml
[[listeners]]
name = "proxy"
bind = "0.0.0.0:1080"
protocols = ["socks5"]

[listeners.udp]
mode = "standalone_pproxy_udp"
bind = "0.0.0.0:1081"
idle_timeout = "60s"
max_associations = 1024
```

This mode accepts SOCKS5-framed UDP datagrams directly on the UDP socket without requiring a SOCKS5 TCP control connection.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mode` | string | `"socks5_udp_associate"` | UDP mode: `"socks5_udp_associate"` or `"standalone_pproxy_udp"` |
| `bind` | `"host:port"` | `"127.0.0.1:0"` | UDP relay socket bind address |
| `idle_timeout` | duration string | `"60s"` | Client flow idle timeout |
| `target_idle_timeout` | duration string | `"30s"` | Per-target flow idle timeout |
| `max_associations` | usize | 1024 | Max concurrent standalone flows |
| `max_targets_per_association` | usize | 64 | Max target flows per client |
| `max_datagram_size` | usize | 65535 | Max datagram size (257-65535) |
| `client_pin` | bool | `true` | Pin flow to first client address |

### `[listeners.transparent]`

Transparent TCP proxy support (Linux only, requires `SO_ORIGINAL_DST`).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `enabled` | bool | yes | Enable transparent proxy mode |
| `protocol` | string | `"redir"` | Transparent proxy protocol: `"redir"` (iptables REDIRECT) or `"pf"` (macOS PF — not implemented) |

When enabled, incoming connections have their original destination extracted via
`SO_ORIGINAL_DST` (Linux) before protocol detection. This requires iptables or
nftables REDIRECT rules to redirect traffic to the eggress listener port.

**Platform requirements:**
- Linux only (kernel 2.4+)
- Requires `CAP_NET_ADMIN` capability or root
- iptables/nftables REDIRECT rules must be configured

```toml
[[listeners]]
name = "transparent-in"
bind = "0.0.0.0:8080"
protocols = ["http", "socks5"]

[listeners.transparent]
enabled = true
protocol = "redir"
```

### `[listeners.unix]`

Unix domain socket listener (Unix only).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | Filesystem path for the Unix domain socket |
| `unlink_existing` | bool | `true` | Remove existing socket file before binding |
| `mode` | integer | `0o660` | Unix file permissions for the socket (octal) |

When configured, the listener binds to a Unix domain socket instead of a TCP
address. The `bind` field is ignored when `[listeners.unix]` is present.

**Filesystem safety:** `unlink_existing = true` only removes an existing
socket file at `path`. Regular files, symlinks, directories, and special
devices are preserved; binding fails with a clear error if the path is not a
socket. With `unlink_existing = false`, binding fails when any entry exists
at `path`.

**Platform requirements:**
- Unix only (Linux, macOS, BSDs)
- Not available on Windows

```toml
[[listeners]]
name = "unix-in"
protocols = ["http", "socks5"]

[listeners.unix]
path = "/run/eggress/proxy.sock"
unlink_existing = true
mode = 0o660
```

### `[listeners.tls]`

TLS termination on the listener (requires cert/key PEM files at startup).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `cert` | string | yes | Path to PEM certificate chain |
| `key` | string | yes | Path to PEM private key |
| `alpn` | `[string]` | none | ALPN protocols for TLS handshake |

#### ALPN Protocol Negotiation

The `alpn` field configures Application-Layer Protocol Negotiation during the TLS handshake:

```toml
[listeners.tls]
cert = "/path/to/cert.pem"
key = "/path/to/key.pem"
alpn = ["h2", "http/1.1"]
```

Supported ALPN values:
- `h2` — HTTP/2 (for H2 CONNECT proxy)
- `http/1.1` — HTTP/1.1 (default, for standard HTTP CONNECT)

ALPN is optional. If omitted, no protocol negotiation occurs during TLS handshake.

---

## `[[upstreams]]`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique upstream identifier |
| `uri` | string | yes | Upstream proxy URI |
| `health` | table | defaults | Health check configuration |

### Upstream URI Syntax

```
protocol://[user:pass@]host:port[/rule][+tls]
```

**Protocols:** `http`, `socks4`, `socks5`, `shadowsocks`, `trojan`

**TLS suffix:** `+tls` enables TLS on the connection (e.g., `socks5+tls://proxy:1080`)

**Chaining:** Multiple hops separated by `__` (e.g., `socks5://hop1:1080__http://hop2:8080`)

**Protocol+auth in URI:** `+` separates protocols within a hop (e.g., `socks5+tls://...`)

**Shadowsocks URI:** `shadowsocks://method:password@host:port`

Supported AEAD methods: `aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305`. Legacy stream ciphers are not supported and produce clear error messages (e.g., `LegacyMethodUnsupported`). ShadowsocksR (SSR) is not supported; SSR URIs (`ssr://`) are rejected with a clear `SsrUnsupported` error.

```toml
# Shadowsocks upstream (TCP)
uri = "shadowsocks://aes-256-gcm:password@192.168.1.1:8388"

# Shadowsocks with ChaCha20
uri = "shadowsocks://chacha20-ietf-poly1305:password@192.168.1.1:8388"
```

**Shadowsocks Inbound Listener**: Shadowsocks can be configured as an inbound listener. It must be the only protocol on the listener (no mixed-mode auto-detection). Requires a `[listeners.shadowsocks]` section with method and password.

```toml
[[listeners]]
name = "ss-inbound"
bind = "0.0.0.0:8388"
protocols = ["shadowsocks"]

[listeners.shadowsocks]
method = "aes-256-gcm"
password = "my-secret-password"
```

Shadowsocks UDP uses standard AEAD format (`salt + encrypted(address + payload)`).
It works as an upstream for UDP associations routed through a SOCKS5 listener:

```toml
# Shadowsocks UDP upstream (single-hop via SOCKS5 UDP ASSOCIATE)
[[upstreams]]
id = "ss-udp"
uri = "shadowsocks://aes-256-gcm:password@192.168.1.1:8388"

[[upstream_groups]]
id = "udp-egress"
members = ["ss-udp"]
```

**Trojan URI:** `trojan://password@host:port`

> **Note:** `h2`, `ws`/`wss`, `raw`/`tunnel` protocols are implemented as protocol
> crates only and are not integrated as inbound or upstream protocols through the
> runtime supervisor. They are rejected by `compile_protocol()` and `parse_listener_uri`.

### `[upstreams.health]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mode` | `"tcp_connect"` | `"tcp_connect"` | Only TCP connect probe supported |
| `interval` | duration string | `"30s"` | Probe interval |
| `timeout` | duration string | `"5s"` | Probe timeout |
| `failures_to_unhealthy` | u32 | 3 | Consecutive failures to mark unhealthy |
| `successes_to_healthy` | u32 | 2 | Consecutive successes to mark healthy |
| `initial_state` | string | `"unknown"` | `healthy`, `suspect`, `unhealthy`, `disabled` |

---

## `[[upstream_groups]]`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique group identifier |
| `members` | `[string]` | yes | Upstream IDs in this group |
| `scheduler` | string | `"first-available"` | `first-available`, `round-robin`, `random`, `least-connections` |
| `fallback` | string | `"reject"` | `reject`, `direct`, `use-unhealthy` |

---

## `[[reverse_servers]]`

A reverse server listens for incoming control connections from remote reverse
clients. When a control client connects and authenticates, the acceptor can
dispatch inbound connections back through the control channel to be handled by
the client. This enables NAT/firewall traversal: the client behind a NAT
exposes local services to the acceptor in the datacenter.

The wire format matches pproxy's raw-relay protocol:
- 1-byte handshake (`0x01` = accept, `0x00` = reject)
- Raw `user:pass` auth bytes sent by the client
- One session per control channel (no multiplexing)
- TCP only (no UDP, no built-in TLS)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique reverse server identifier |
| `control_bind` | string | yes | Address to bind the control listener (`host:port`) |
| `auth_username` | string | no | Authentication username for connecting clients |
| `auth_password` | string | no | Authentication password (plaintext) |
| `auth_password_env` | string | no | Environment variable containing the authentication password |
| `max_streams` | integer | no | Max concurrent streams per client (default: 1024) |
| `heartbeat_interval` | duration string | no | Heartbeat interval to detect dead connections (default: `"300s"`) |

```toml
[[reverse_servers]]
id = "rs-public"
control_bind = "0.0.0.0:9443"
auth_username = "tunnel"
auth_password_env = "RS_AUTH_PASSWORD"
max_streams = 512
heartbeat_interval = "60s"
```

### Concurrency and pproxy Compatibility

Each `[[reverse_servers]]` table accepts one control connection per client
identity. One control channel carries one session -- there is no multiplexing.
To accept multiple concurrent sessions, either:

- Define multiple `[[reverse_servers]]` tables (each bound to its own control
  port), or
- Accept that only one session is active at a time per control channel
  (pproxy-compatible default behavior).

The on-wire protocol is intentionally identical to pproxy's raw-relay: a
1-byte handshake followed by raw `user:pass` auth bytes, then bidirectional
TCP relay. This ensures interoperability with pproxy clients and servers.

### Security Hardening

The reverse control channel is **plaintext TCP by default**. Operators MUST:

- Use TLS via an external wrapper (stunnel, haproxy, or a WireGuard tunnel)
  when control traffic traverses untrusted networks.
- Restrict `control_bind` to a loopback or VPC-internal address when TLS is
  not in use. There is no built-in bind allowlist in the current
  implementation; restrict at the OS / firewall level until the
  `allow_bind` policy lands in a follow-up phase.
- Configure strong `auth_password` (use `auth_password_env` for environment
  injection rather than embedding plaintext in config).
- Apply firewall rules to limit which hosts can reach the control port.
- Monitor `eggress_reverse_control_connections_rejected_total` for
  unauthorized connection attempts.

### Unsupported Features

- **UDP reverse mode**: Not supported. Reverse sessions are TCP-only.
- **Jump chains through reverse**: Chains cannot transit a reverse hop.
- **TLS on the control channel**: Not built-in. Use stunnel or equivalent.

---

## `[[reverse_clients]]`

A reverse client connects to a reverse server and exposes local streams through
the control channel. It maintains the connection with automatic reconnection
and heartbeat keep-alive. On disconnect the client backs off exponentially
from `reconnect_initial` up to `reconnect_max`, resetting on successful
reconnect.

The wire format matches pproxy's raw-relay protocol (see `[[reverse_servers]]`
above for protocol details).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique reverse client identifier |
| `server_addr` | string | yes | Address of the reverse server to connect to (`host:port`) |
| `auth_username` | string | no | Authentication username |
| `auth_password` | string | no | Authentication password (plaintext) |
| `auth_password_env` | string | no | Environment variable containing the authentication password |
| `reconnect_initial` | duration string | no | Initial reconnect backoff (default: `"1s"`) |
| `reconnect_max` | duration string | no | Max reconnect backoff (default: `"30s"`) |
| `heartbeat_interval` | duration string | no | Heartbeat interval to keep the control channel alive (default: `"60s"`) |

```toml
[[reverse_clients]]
id = "rc-edge"
server_addr = "rs-public:9443"
auth_username = "tunnel"
auth_password_env = "RC_AUTH_PASSWORD"
reconnect_initial = "2s"
reconnect_max = "30s"
heartbeat_interval = "15s"
```

### Example: Reverse Server + Client Pair

```toml
# Server side — accepts control connections from remote clients
[[reverse_servers]]
id = "rs-datacenter"
control_bind = "0.0.0.0:9443"
auth_username = "tunnel"
auth_password_env = "RS_PASSWORD"
max_streams = 256
heartbeat_interval = "30s"

# Client side — connects to the server and tunnels local traffic
[[reverse_clients]]
id = "rc-office"
server_addr = "datacenter.example.com:9443"
auth_username = "tunnel"
auth_password_env = "RC_PASSWORD"
reconnect_initial = "1s"
reconnect_max = "60s"
heartbeat_interval = "30s"
```

---

## `[[rules]]`

First-match-wins routing rules. Each rule matches a condition and selects an action.

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique rule identifier |
| `direct` | bool | Route directly (no upstream) |
| `upstream_group` | string | Route to named upstream group |
| `reject` | string | Reject with reason string |
| `host_exact` | string | Match exact hostname |
| `host_suffix` | string | Match hostname suffix |
| `host_regex` | string | Match hostname by regex |
| `destination_port` | u16 | Match destination port |
| `match` | table | Recursive match expression (see below) |

### `[rules.match]` — Recursive Match Expressions

Recursive match expressions are bounded to a maximum depth of 10 and 100 total
matcher nodes. Configurations exceeding either limit are rejected at compile
time.

Composite matchers:

```toml
[rules.match]
all = [
  { host_suffix = "example.com" },
  { destination_port = 443 },
]

[rules.match]
any_of = [
  { protocol = "http" },
  { protocol = "socks5" },
]

[rules.match]
not = { source_cidr = "10.0.0.0/8" }
```

Leaf matchers:

| Matcher | Type | Description |
|---------|------|-------------|
| `host_exact` | string | Exact hostname match |
| `host_suffix` | string | Suffix match (e.g., `.example.com`) |
| `host_regex` | string | Regex match |
| `destination_port` | u16 | Exact port |
| `destination_port_range` | `[u16; 2]` | Port range `[min, max]` |
| `destination_port_set` | `[u16]` | Set of ports |
| `destination_cidr` | string | CIDR match on target |
| `source_cidr` | string | CIDR match on client source |
| `source_port` | u16 | Client source port |
| `listener` | string | Listener name |
| `protocol` | string | `http`, `socks4`, `socks5` |
| `identity` | string | Client identity (username) |
| `transport` | string | `tcp` or `udp` |

---

## `[routing]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default` | string | `"direct"` | Default action when no rule matches |

Value can be `"direct"`, `"reject"`, or a group ID reference.

---

## `[admin]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bind` | string | `"127.0.0.1:9090"` | Admin HTTP bind address |
| `enabled` | bool | false | Enable admin server |
| `metrics` | bool | false | Enable `/metrics` endpoint |
| `pac` | table | none | PAC file configuration |
| `static_content` | table array | none | Static content routes |

### `[admin.pac]`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | URL path (must start with `/`) |
| `proxy` | string | yes | Proxy directive for PAC |
| `direct_fallback` | bool | false | Include `; DIRECT` fallback |
| `direct_hosts` | `[string]` | none | Hostnames to route directly |
| `direct_suffixes` | `[string]` | none | Suffixes to route directly |

### `[[admin.static_content]]`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | URL path (must start with `/`) |
| `content_type` | string | `"text/plain"` | MIME type |
| `body` | string | yes | Response body (non-empty) |

Reserved paths (`/-/*`, `/metrics`, `/pac`) cannot be overridden.

---

## Full Example

```toml
version = 1

[process]
log_format = "json"
log_level = "info"
shutdown_grace = "10s"

[timeouts]
handshake = "5s"
connect = "30s"

[[listeners]]
name = "mixed-in"
bind = "127.0.0.1:8080"
protocols = ["http", "socks5"]
connection_limit = 1000

[listeners.auth]
type = "password"
username = "admin"
password_env = "EGGRESS_PASSWORD"

[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
advertise = "127.0.0.1"
idle_timeout = "60s"
max_associations = 512
client_pin = true

[[upstreams]]
id = "socks-proxy"
uri = "socks5://proxy.example:1080"

[upstreams.health]
interval = "15s"
timeout = "3s"
failures_to_unhealthy = 5
successes_to_healthy = 3

[[upstreams]]
id = "tls-proxy"
uri = "socks5+tls://secure.example:1080"

[[upstream_groups]]
id = "egress"
scheduler = "round-robin"
members = ["socks-proxy", "tls-proxy"]
fallback = "direct"

[[reverse_servers]]
id = "rs-main"
control_bind = "0.0.0.0:9443"
auth_username = "tunnel"
auth_password_env = "RS_PASSWORD"
max_streams = 256
heartbeat_interval = "300s"

[[reverse_clients]]
id = "rc-branch"
server_addr = "hq.example.com:9443"
auth_username = "tunnel"
auth_password_env = "RC_PASSWORD"
reconnect_initial = "1s"
reconnect_max = "30s"
heartbeat_interval = "60s"

[[rules]]
id = "block-ads"
host_suffix = "ads.example.com"
reject = "blocked"

[[rules]]
id = "corp-via-proxy"
upstream_group = "egress"

[rules.match]
all = [
  { host_suffix = "corp.internal" },
  { destination_port = 443 },
]

[[rules]]
id = "allow-all"
direct = true

[routing]
default = "direct"

[admin]
bind = "127.0.0.1:9090"
enabled = true
metrics = true

[admin.pac]
path = "/proxy.pac"
proxy = "127.0.0.1:8080"
direct_fallback = true
direct_hosts = ["localhost"]
direct_suffixes = ["local"]

[[admin.static_content]]
path = "/status"
content_type = "text/html"
body = "<h1>OK</h1>"
```
