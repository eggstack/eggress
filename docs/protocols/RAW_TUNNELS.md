# Raw Fixed-Target Tunnels

## Overview

Raw tunnel transport provides simple TCP port forwarding with no protocol
negotiation. The listener accepts TCP connections and forwards them directly
to a fixed target address specified in configuration. There is no handshake,
no address negotiation, and no encryption (unless TLS is applied at the
transport layer).

Source: `crates/eggress-core/src/tunnel/raw.rs`

## Behavior

```
Client                    Eggress                    Target
  │                         │                         │
  │  TCP connect            │                         │
  │────────────────────────>│                         │
  │                         │  TCP connect            │
  │                         │────────────────────────>│
  │                         │                         │
  │  Bidirectional relay    │  Bidirectional relay    │
  │<───────────────────────>│<───────────────────────>│
  │                         │                         │
  │  TCP close              │  TCP close              │
  │────────────────────────>│────────────────────────>│
```

1. Client connects to the listener bind address.
2. Eggress connects to the fixed target address (from config).
3. On successful connection, eggress relays bytes bidirectionally.
4. On target connection failure, eggress closes the client connection.
5. On either side closing, the other side is notified and closed.

No protocol detection, no application-layer handshake, and no address
encoding. The raw TCP stream is forwarded as-is.

## Configuration

### TOML Configuration

```toml
[[listeners]]
name = "raw-in"
bind = "0.0.0.0:9090"

[listeners.transport]
type = "raw"
target = "192.168.1.100:22"

[[upstreams]]
id = "direct"
uri = "direct://"
```

The `target` field specifies the fixed destination address. All connections
to this listener are forwarded to this target.

### Listener Config Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Must be `"raw"` |
| `target` | string | yes | Target address (`host:port`) |

## Route Engine Integration

Raw tunnels integrate with the route engine as a transport wrapper. When a
raw tunnel listener receives a connection:

1. The route engine evaluates rules based on the listener identity and
   connection metadata.
2. If a rule specifies an upstream group, the connection is forwarded through
   the selected upstream.
3. If no rule matches, the default action applies (typically direct).

Raw tunnels do **not** participate in protocol-based routing because there is
no application protocol to inspect. Routing decisions are based solely on
listener identity, source address, and configured rules.

### Route Example

```toml
[[rules]]
id = "raw-ssh"
upstream_group = "ssh-upstream"

[rules.match]
all = [
  { listener = "raw-in" },
]
```

## pproxy Compatibility

### Supported Schemes

| pproxy scheme | Eggress behavior | Notes |
|---------------|------------------|-------|
| `raw://` | Raw tunnel | Fixed target from config |
| `tunnel://` | Raw tunnel | Alias for `raw://` |

### Configuration Translation

pproxy-style invocation:

```bash
pproxy -l raw://:9090 -r direct:// --target 192.168.1.100:22
pproxy -l tunnel://:9090 -r direct:// --target 192.168.1.100:22
```

Eggress TOML equivalent:

```toml
[[listeners]]
name = "raw-in"
bind = "0.0.0.0:9090"

[listeners.transport]
type = "raw"
target = "192.168.1.100:22"

[[upstreams]]
id = "direct"
uri = "direct://"

[[rules]]
id = "default"
upstream_group = "default"
```

### pproxy Parity Tier

| Feature | Tier | Notes |
|---------|------|-------|
| `raw://` listener | Supported | Fixed target; no protocol negotiation |
| `tunnel://` listener | Supported | Alias for `raw://` |
| Bidirectional relay | Supported | Standard TCP byte forwarding |
| TLS on raw tunnel | Supported | Via `listeners.tls` config |

## Use Cases

### Simple Port Forwarding

Forward local port 9090 to a remote SSH server:

```toml
[[listeners]]
name = "ssh-forward"
bind = "127.0.0.1:9090"

[listeners.transport]
type = "raw"
target = "192.168.1.100:22"
```

### Database Access

Forward local port 5433 to a remote PostgreSQL server:

```toml
[[listeners]]
name = "pg-forward"
bind = "127.0.0.1:5433"

[listeners.transport]
type = "raw"
target = "db.internal:5432"
```

### Load Balancer Backend

Use raw tunnels as a load-balanced backend with upstream selection:

```toml
[[listeners]]
name = "backend"
bind = "0.0.0.0:8080"

[listeners.transport]
type = "raw"
target = "backend-pool.internal:8080"

[[upstream_groups]]
id = "backend-pool"
scheduler = "round-robin"
members = ["backend-1", "backend-2", "backend-3"]
```

## Test Coverage

- Listener accept and connect to fixed target
- Bidirectional relay (data echo)
- Target connection failure (connection refused)
- Target timeout (slow connect)
- Client disconnect (upstream notified)
- Target disconnect (client notified)
- TLS on raw tunnel
- pproxy URI parsing (`raw://`, `tunnel://`)
- Route engine integration with raw listener

Test count: planned across `eggress-core` and runtime integration tests.

## Limitations

- No protocol negotiation — target must accept raw TCP
- No authentication — anyone who can connect to the listener gets forwarded
- No encryption unless TLS is applied at the transport layer
- Fixed target only — no dynamic target selection per connection
- No health checking of the target
- No connection pooling or multiplexing
