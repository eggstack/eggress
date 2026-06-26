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
| `protocols` | `["http", "socks4", "socks5"]` | yes | Accepted protocol list |
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

### `[listeners.tls]`

TLS termination on the listener (requires cert/key PEM files at startup).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `cert` | string | yes | Path to PEM certificate chain |
| `key` | string | yes | Path to PEM private key |
| `alpn` | `[string]` | none | ALPN protocols (reserved for future use) |

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

Supported AEAD methods: `aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305`. Legacy stream ciphers are not supported.

```toml
# Shadowsocks upstream (TCP)
uri = "shadowsocks://aes-256-gcm:password@192.168.1.1:8388"

# Shadowsocks with ChaCha20
uri = "shadowsocks://chacha20-ietf-poly1305:password@192.168.1.1:8388"
```

**Trojan URI:** `trojan://password@host:port`

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
