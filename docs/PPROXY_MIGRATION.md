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
| `https://` | Yes (TLS) | Yes (HTTP+TLS) |
| `socks4://` | Yes | Yes |
| `socks4a://` | Yes | Yes |
| `socks5://` | Yes | Yes |
| `trojan://` | No (upstream-only) | Yes |
| `shadowsocks://` | Yes (AEAD methods only) | Yes (AEAD methods only) |
| `direct://` | No | Yes (direct connection) |
| `h2://` | Yes | Yes (H2 CONNECT tunnel) |
| `ws://` | Yes | Yes (WebSocket tunnel) |
| `wss://` | Yes | Yes (WebSocket tunnel over TLS) |
| `raw://` | Yes | Yes (raw fixed-target tunnel) |
| `tunnel://` | Yes | Yes (alias for raw) |

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

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| HTTP CONNECT | Compatible | Byte-exact payload match with differential tests |
| HTTP forward proxy | Compatible | Persistent session model with HTTP/1.1 keep-alive (Phase 19) |
| SOCKS4/4a | Compatible | Differential tests with pproxy 2.7.9 added (Phase 19) |
| SOCKS5 CONNECT | Compatible | Expanded differential evidence: auth, IPv6, domain, refused targets (Phase 19) |
| SOCKS5 UDP ASSOCIATE | Supported | Framing differs; relay success matches |
| Standalone UDP (`-ul`/`-ur`) | Compatible | pproxy-compatible standalone UDP relay mode (Phase 20) |
| Shadowsocks upstream | Supported | Standard AEAD framing; interoperable with standard Shadowsocks |
| Trojan upstream | Partial | Client-only; no Trojan server |
| HTTP/2 CONNECT | Supported | Synthetic tests; H2 CONNECT server and client implemented (Phase 26) |
| WebSocket tunnel | Supported | Synthetic tests; WS/WSS tunnel server and client implemented (Phase 26) |
| Raw fixed-target tunnel | Supported | Synthetic tests; raw TCP tunnel with no protocol negotiation (Phase 26) |
| TLS ALPN | Supported | Configurable ALPN values for H2 and HTTP/1.1 (Phase 26) |
| Hot reload | Partial | Routing/upstreams only; listener topology requires restart |

## Unsupported Features

The following pproxy features are explicitly unsupported:

- **Trojan listeners** -- Trojan is upstream-only
- **`--daemon` mode** -- Use systemd or a process manager instead
- **`--ssl` TLS listeners** -- Configure TLS in eggress TOML directly
- **`-b` block regex rules** -- Use eggress TOML routing rules
- **`--rulefile`** -- Use eggress TOML routing rules
- **`--reuse`** -- Connection pooling not implemented
- **`--log`** -- Use `RUST_LOG=debug` environment variable
- **`--sys`** -- System proxy configuration not supported
- **Multi-hop UDP** -- Not supported
- **SSH protocol** -- Not supported (SSH transport is out-of-scope for a proxy)
- **H3/QUIC transport** -- Deferred; pproxy H3 behavior is experimental and unstable. See ADR at `docs/adr/ADR_quic_h3_pproxy_parity.md`.
- **Unix domain sockets** -- Not supported
- **Transparent/system proxy mode** -- Not supported
- **Shadowsocks stream ciphers** -- Not supported (insecure; use AEAD methods). Detected during URI parsing; produces `LegacyMethodUnsupported` error. See `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`.
- **ShadowsocksR** -- Not supported (non-standard extension). `ssr://` URIs are recognized and rejected with structured `UnsupportedFeature` diagnostics (categories: `ssr-listener`, `ssr-upstream`). See `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`.

Unsupported features produce structured diagnostics when encountered in pproxy compat mode.

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
