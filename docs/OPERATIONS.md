# Operations

## Running the Service

### CLI Mode (URI-based)

```bash
# Single protocol
eggress -l http://:8080
eggress -l socks5://:1080
eggress -l socks4://:1080

# Mixed-protocol listener
eggress -l http+socks5://:8080

# With authentication
eggress -l http+socks5://user:pass@:8080

# With upstream
eggress -l socks5://:1080 -r socks5://proxy.example:1080

# Multi-hop chain
eggress -l socks5://:1080 -r socks5://hop1:1080__http://hop2:8080
```

### Runtime Mode (TOML config)

```bash
eggress --config /etc/eggress/config.toml
```

See [CONFIG_REFERENCE.md](CONFIG_REFERENCE.md) for full TOML schema.

## Reload Behavior

**Signal:** `SIGHUP` (Unix only; on non-Unix, only Ctrl-C / SIGTERM are handled)

**Mechanism:** Atomic swap via `ArcSwap<Router>` — lock-free reads, zero-downtime swap.

### What IS reloaded (hot-swap, no downtime)

| Component | Details |
|-----------|---------|
| Upstream chains | Health config, Arc reuse for unchanged upstreams |
| Upstream groups | Schedulers, fallback policies |
| Routing rules | All rules, default action |
| Listener metadata | Name, bind, protocols, auth (not socket binding) |
| Admin config | PAC and static content configuration |

### What is NOT reloaded (requires full restart)

| Component | Reason |
|-----------|--------|
| Listener socket bindings | Bound before readiness; cannot re-bind |
| Process settings | Log format, log level, shutdown grace |
| Timeout configuration | Used at startup for connection setup |
| Admin bind address | Bound at startup |
| UDP bind address | Socket bound at startup |

### UDP-specific reload semantics

- UDP limits apply to **new** associations only; existing keep their limits
- UDP bind changes require restart
- Route changes apply immediately to future UDP packets

### Reload outcomes

- **Applied**: Config loaded, classified, snapshot built, router swapped. Logged with generation and upstream count.
- **Rejected**: Listener topology changed (count, name, bind, UDP bind). `eggress_reload_total` incremented, `eggress_reload_failures_total` incremented.
- **Failed**: Config parse error or snapshot build error. Both counters incremented.

## Shutdown Behavior

**Signals:** `SIGTERM`, `SIGINT` (Ctrl-C), or cancel token

### Shutdown sequence

| Step | Action | Details |
|------|--------|---------|
| 1 | Set readiness false | Admin `/-/ready` returns 503 |
| 2 | Stop listeners | No new TCP connections accepted |
| 3 | Stop health probes | No new upstream health checks |
| 4 | Close UDP associations | All active UDP associations closed |
| 5 | Drain UDP relay tasks | Wait up to `shutdown_grace` for relay tasks |
| 6 | Wait for accept loops | Ensure no new connections enter tracker |
| 7 | Drain active connections | Wait up to `shutdown_grace` for connections to complete |
| 8 | Force-cancel remaining | If grace expires, cancel all active connections |
| 9 | Stop admin server | Admin endpoints stop (readiness has been 503 since step 1) |

The admin server remains available through steps 1–8, allowing operators to observe drain progress via `/-/ready`, `/-/status`, and `/metrics`.

### Grace period

Configured via `[process].shutdown_grace` (default `"30s"`). Controls how long the service waits for active connections to drain before force-cancelling.

## Admin Endpoints

Admin server is enabled via `[admin].enabled = true` and binds to `[admin].bind` (default `127.0.0.1:9090`).

| Endpoint | Method | Response | Description |
|----------|--------|----------|-------------|
| `/-/health` | GET | `200 ok` | Liveness probe; always returns 200 when admin is up |
| `/-/ready` | GET | `200 ready` / `503 not ready` | Readiness probe; false during shutdown drain |
| `/-/status` | GET | JSON | Version, generation, uptime, active connections, listeners |
| `/-/config` | GET | JSON | Config summary: rule count, group count, default action, listener names |
| `/-/routes` | GET | JSON | All routing rules with IDs and actions, default action |
| `/-/upstreams` | GET | JSON | Upstream groups with members, health states, protocols, capabilities |
| `/-/udp` | GET | JSON | UDP association counts, target flow counts, per-listener UDP status |
| `/-/route-explain` | POST | JSON | Explain route decision for a given target/listener/protocol/identity |
| `/metrics` | GET | Prometheus text | All metrics in Prometheus exposition format |
| `/pac` | GET | PAC JS | Auto-config file (requires `[admin.pac]` config) |
| `/*` | GET | varies | Static content routes (requires `[[admin.static_content]]` config) |

### Route Explain (POST `/-/route-explain`)

Request body:

```json
{
  "target": "example.com:443",
  "listener": "http-in",
  "protocol": "socks5",
  "source": "127.0.0.1:54321",
  "identity": "admin"
}
```

Fields `target`, `listener`, `protocol` are required. `source` and `identity` are optional. Response includes the matched rule, action taken, and selection reasoning.

### Admin Credential Exposure

Admin endpoints expose only metadata (generation, uptime, rule IDs, listener names, health states, protocol names). Upstream URIs with credentials are **never** exposed. See [SECURITY_REVIEW.md](SECURITY_REVIEW.md) for details.

## Transparent Proxy Setup (Linux)

Transparent proxy intercepts TCP connections redirected by the kernel, extracting the original destination via `SO_ORIGINAL_DST`.

### Prerequisites

- Linux kernel 2.4+
- `CAP_NET_ADMIN` capability or root
- iptables or nftables

### iptables configuration

```bash
# Redirect HTTP/HTTPS traffic to eggress on port 8080
iptables -t nat -A PREROUTING -p tcp --dport 80 -j REDIRECT --to-ports 8080
iptables -t nat -A PREROUTING -p tcp --dport 443 -j REDIRECT --to-ports 8080

# Optionally redirect specific subnets
iptables -t nat -A PREROUTING -p tcp -s 10.0.0.0/8 --dport 80 -j REDIRECT --to-ports 8080
```

### nftables configuration

```bash
nft add table ip nat
nft add chain ip nat PREROUTING '{ type nat hook prerouting priority 0; }'
nft add rule ip nat PREROUTING tcp dport { 80, 443 } redirect to :8080
```

### eggress TOML config

```toml
[[listeners]]
name = "transparent-in"
bind = "0.0.0.0:8080"
protocols = ["http", "socks5"]

[listeners.transparent]
enabled = true
protocol = "redir"
```

### Running with capabilities (non-root)

```bash
# Grant only the needed capability
sudo setcap cap_net_admin+ep /usr/bin/eggress

# Or run directly as root
sudo eggress --config /etc/eggress/config.toml
```

### Troubleshooting

- **Connection refused**: Verify iptables/nftables rules are in place (`iptables -t nat -L -n`)
- **Original destination unavailable**: Ensure the listener is bound before traffic is redirected
- **Permission denied**: Check `CAP_NET_ADMIN` or run as root
- **macOS PF**: Not supported; use pfctl with a standard TCP listener instead

## Unix Socket Listener Setup

Unix domain sockets provide local-only proxy access without TCP port exposure.

### Configuration

```toml
[[listeners]]
name = "unix-in"
protocols = ["http", "socks5"]

[listeners.unix]
path = "/run/eggress/proxy.sock"
unlink_existing = true
mode = 0o660
```

### Socket file management

- The socket file is created on listener startup
- `unlink_existing = true` removes a stale socket file from a previous run
- On shutdown, the socket file is **not** automatically removed (operator-managed)
- Clean up stale sockets on service restart or use a systemd unit with `RuntimeDirectory=eggress`

### Permission considerations

- Default mode `0o660` allows owner and group read/write
- Set `mode = 0o666` for world-accessible sockets (not recommended)
- Socket ownership follows the process user/group
- Use filesystem ACLs or group membership to control access

### Client usage

```bash
# curl through Unix socket
curl --unix-socket /run/eggress/proxy.sock http://example.com

# SOCKS5 via socat
socat - UNIX-CONNECT:/run/eggress/proxy.sock
```

## Monitoring Transparent Proxy and Unix Socket Metrics

Transparent proxy and Unix socket listeners expose standard eggress metrics:

| Metric | Description |
|--------|-------------|
| `eggress_connections_total` | Total connections (label: `listener`, `protocol`) |
| `eggress_active_connections` | Current active connections |
| `eggress_bytes_sent` / `eggress_bytes_received` | Traffic counters |
| `eggress_upstream_health` | Upstream health state |

Query via the admin endpoint:

```bash
curl http://127.0.0.1:9090/metrics
curl http://127.0.0.1:9090/-/status
```

For transparent proxy connections, the `listener` label identifies which
transparent listener handled the connection. The original destination is
logged at debug level but not exposed in metrics labels.

## Scheduler Behavior

Eggress supports four scheduling algorithms for upstream group selection:

| Scheduler | Config value | Behavior |
|-----------|-------------|----------|
| Round-robin | `round-robin` (default) | Cycles through upstreams in order; skips ineligible |
| First-available | `first-available` | Returns first eligible upstream |
| Random | `random` | Randomly selects from eligible upstreams |
| Least-connections | `least-connections` | Selects upstream with fewest active+in-flight connections |

### Fallback modes

| Mode | Config value | Behavior |
|------|-------------|----------|
| Reject | `reject` (default) | Return error when no eligible upstream |
| Direct | `direct` | Fall back to direct connection |
| Use-unhealthy | `use-unhealthy` | Include unhealthy upstreams as last resort |

### Health-aware filtering

Upstreams in Unhealthy or Disabled states are excluded from selection. Only
Unknown, Healthy, Suspect, and Recovering states are eligible.

### Retry behavior

Eggress makes a single upstream selection per request. If the connection fails,
the error is returned to the client. No automatic retry across upstreams is
performed. This keeps behavior predictable and avoids amplifying load during
outages.

## Defaults and Recommendations

| Setting | Default | Recommendation |
|---------|---------|----------------|
| Admin bind | `127.0.0.1:9090` | Keep loopback-only unless you need remote admin |
| Admin auth | None | Use network-level access control (firewall, loopback) |
| Shutdown grace | `30s` | Increase for long-lived connections (e.g., WebSocket) |
| Connection limit | 1024 per listener | Adjust based on expected load |
| UDP max associations | 1024 | Adjust based on expected concurrent UDP usage |

## Logging

Configure via `[process]`:

```toml
[process]
log_format = "json"    # "text", "json", or "compact"
log_level = "info"     # "trace", "debug", "info", "warn", "error"
```

At `debug` level, connection metadata (peer address, protocol, target, route, outcome, duration) is logged per connection. Operators should be cautious about log retention in sensitive environments.

## Health Checks

Health probes run per-upstream using TCP connect mode (`tcp_connect`). Configuration per upstream:

```toml
[[upstreams]]
id = "proxy1"
uri = "socks5://proxy.example:1080"

[upstreams.health]
mode = "tcp_connect"
interval = "30s"        # Probe interval
timeout = "5s"          # Probe timeout
failures_to_unhealthy = 3   # Consecutive failures to mark unhealthy
successes_to_healthy = 2    # Consecutive successes to mark healthy
initial_state = "unknown"   # "healthy", "suspect", "unhealthy", "disabled"
```

Health states are exposed via `/-/upstreams` and as `eggress_upstream_health` gauge in Prometheus metrics.
