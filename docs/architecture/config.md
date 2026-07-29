# eggress-config

`crates/eggress-config/`

TOML configuration file loading, deserialization, semantic validation, and compilation into a runtime configuration.

## Key Types

| Type | Description |
|---|---|
| `ConfigFile` | Deserialized TOML structure (the "file model") |
| `RuntimeConfig` | Compiled configuration ready for the runtime |
| `ConfigError` | Validation error with location and message |
| `ConfigWarning` | Non-fatal validation warnings |

## TOML Schema

### Listeners

```toml
[[listeners]]
name = "http-in"
bind = "0.0.0.0:8080"
protocols = ["http"]
# Optional TLS
[listeners.tls]
cert = "/path/to/cert.pem"
key = "/path/to/key.pem"
# Optional UDP
[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
```

### Upstreams

```toml
[[upstreams]]
id = "socks-upstream"
uri = "socks5://user:pass@127.0.0.1:1080"
```

### Upstream Groups

```toml
[[upstream_groups]]
id = "default"
scheduler = "first-available"
members = ["socks-upstream"]
fallback = "reject"  # or "direct"
```

### Rules

```toml
[[rules]]
id = "dns-direct"
upstream_group = "direct"
[rules.match]
destination_port = 53
```

### Reverse Proxy

```toml
[[reverse_servers]]
id = "acceptor"
control_bind = "0.0.0.0:8443"
external_bind = "0.0.0.0:9000"

[[reverse_clients]]
id = "server-behind-nat"
server_addr = "acceptor-host:8443"
```

## Validation

- Duplicate listener/upstream/rule IDs rejected
- Unknown upstream group references rejected
- Invalid URI syntax rejected
- Regex compilation validated
- CIDR notation validated
- Duration strings parsed and validated
- Health configuration per upstream validated

## Secret Sources

Credentials can be specified as:
- Inline in URI (e.g., `socks5://user:pass@host`)
- Environment variable reference
- File path reference

## Compilation

`load_and_validate()` or `load_and_validate_with_warnings()` parse TOML and produce a `RuntimeConfig` consumed by `eggress-runtime`.

## Dependencies

- `eggress-uri` — URI parsing for listener/upstream URIs
- `eggress-routing` — rule compilation
- `eggress-udp` — UDP configuration model

See [overview.md](overview.md) for context.
