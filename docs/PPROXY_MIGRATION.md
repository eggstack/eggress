# Migrating from pproxy to Eggress

Eggress provides a pproxy compatibility layer that translates common pproxy invocations and URI shapes into native Eggress configuration.

## Quick Start

### Translate pproxy arguments to Eggress TOML

```bash
eggress pproxy translate -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

### Check compatibility of pproxy arguments

```bash
eggress pproxy check -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

### Run directly from pproxy-style arguments

```bash
eggress pproxy run -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

## Supported URI Forms

| Scheme | As Local Listener | As Upstream |
|--------|------------------|-------------|
| `http://` | Yes | Yes |
| `socks4://` | Yes | Yes |
| `socks5://` | Yes | Yes |
| `trojan://` | No (upstream-only) | Yes |
| `shadowsocks://` | No | Experimental |

### URI Format

```
scheme://[user:pass@]host:port[+tls][?rule=regex]
```

### Examples

```bash
# Local SOCKS5 proxy on port 1080
-l socks5://127.0.0.1:1080

# Local HTTP proxy with authentication
-l http://admin:secret@0.0.0.0:8080

# Upstream through HTTP proxy
-r http://proxy.example:8080

# Upstream through SOCKS5 with TLS
-r socks5+tls://secure-proxy:1080

# Trojan upstream
-r trojan://password@server:443

# Chain: SOCKS5 through HTTP then SOCKS5
-r http://proxy1:8080 -r socks5://proxy2:1080
```

## Common pproxy Commands -> Eggress Equivalents

### pproxy

```bash
python3 -m pproxy -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

### Eggress (pproxy-compatible)

```bash
eggress pproxy run -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

### Eggress (native TOML)

```toml
version = 1

[[listeners]]
name = "local"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[[upstreams]]
id = "upstream"
uri = "http://proxy:8080"

[[upstream_groups]]
id = "chain"
scheduler = "first-available"
members = ["upstream"]
fallback = "reject"

[[rules]]
id = "default"
any = true
upstream_group = "chain"
```

## Unsupported Features

The following pproxy features are explicitly unsupported:

- **Shadowsocks listeners** -- Shadowsocks is experimental as upstream only
- **`--daemon` mode** -- Use systemd or a process manager instead
- **`-ul` / `-ur` UDP flags** -- Eggress uses SOCKS5 UDP ASSOCIATE instead
- **`--rulefile`** -- Use Eggress TOML routing rules
- **Multi-hop UDP** -- Not supported
- **SSH protocol** -- Not supported
- **Transparent/system proxy mode** -- Not supported

## Parity Tiers

When you run `eggress pproxy check`, it reports a parity tier:

- **Compatible** -- Full behavioral match with pproxy
- **Supported** -- Works correctly with minor warnings
- **Partial** -- Some features unsupported; service may not behave as expected

## Credential Handling

- Credentials in generated TOML are stored in plaintext (config file only)
- Credentials are **never** printed in warnings or error messages
- The `--annotate` flag adds comments but still redacts credentials in warnings

## Troubleshooting

### "unsupported protocol" error

Check that your URI scheme is one of: `http`, `socks4`, `socks5`, `trojan`.

### "no local listener specified"

You must provide at least one `-l` argument.

### Generated TOML doesn't validate

Run `eggress pproxy translate` and pipe to `eggress --config /dev/stdin` to test.
