# pproxy URI Grammar (pproxy 2.7.9 Compatibility)

Eggress parses pproxy-style URIs via the `eggress-pproxy-compat` crate. This
document describes the full grammar as understood by the compatibility layer.

## Formal Grammar

```
uri          = scheme "://" [ userinfo "@" ] endpoint [ "?" query ]
             | unix_uri
             | plain_endpoint

scheme       = protocol [ "+tls" | "+ssl" ] [ "+in" ]

protocol     = "http" | "https"
             | "socks4" | "socks4a" | "socks5"
             | "ss" | "shadowsocks"
             | "trojan"
             | "ssr"                          # recognized → rejected
             | "ssh"                          # recognized → intentional non-parity
             | "direct"
             | "unix"
             | "redir"
             | "h2" | "ws" | "wss"
             | "raw" | "tunnel"
             | "bind" | "listen"
             | "backward" | "rebind"

userinfo     = user ":" password
             | password                        # Trojan: password-only

endpoint     = host ":" port
             | "[" ipv6 "]" ":" port
             | ":" port                        # empty host = bind all interfaces
             | host ":" port                   # host may be empty

host         = hostname | ipv4 | empty
ipv6         = hex_seq (":" hex_seq)*
query        = "rule" "=" regex

unix_uri     = "unix://" [ "/" ] path

plain_endpoint = host ":" port                 # for -ul/-ur shorthand
               | ":" port
               | port_number                   # bare port (UDP only)
```

## Modifier Chaining

Modifiers are appended to the scheme with `+` separators and can be combined:

| Modifier | Meaning | Example |
|----------|---------|---------|
| `+tls` | Wrap connection in TLS | `socks5+tls://proxy:1080` |
| `+ssl` | Alias for `+tls` | `socks5+ssl://proxy:1080` |
| `+in` | Backward/inbound modifier (repeatable) | `socks5+in://acceptor:1080` |

Multiple `+in` tokens indicate parallel backward connections:
`s socks5+in+in://acceptor:1080` → 2 parallel connections.

## Chain Separator (Core URI Parser)

Multi-hop chains use the `__` (double underscore) separator. Each segment is a
complete URI:

```
socks5://hop1:1080__http://hop2:8080
```

**Note:** The `__` separator is supported by the core `eggress-uri` parser but
is **not** supported by the pproxy compat layer. The compat layer treats `__`
in remote URIs as an unsupported jump chain and emits a diagnostic.

## Local Bind Modifier (Core URI Parser)

The core parser supports an `@` suffix for local bind address:

```
socks5://proxy:1080@127.0.0.1
```

This is not part of the pproxy compat grammar.

## Special URI Forms

### Standalone UDP (`-ul`/`-ur`)

The `-ul` flag accepts shorthand forms that are not full URIs:

| Form | Interpretation | Example |
|------|----------------|---------|
| `:port` | Bind all interfaces, default protocol | `-ul :1081` |
| `host:port` | Specific bind address | `-ul 127.0.0.1:1081` |
| `port` | Bare port number | `-ul 1081` |
| `scheme://host:port` | Full URI | `-ul socks5://:1081` |

These are translated to `mode = "standalone_pproxy_udp"` in the generated TOML.

### Unix Domain Sockets

```
unix:///absolute/path/to/socket
unix://relative/path/to/socket    # becomes /relative/path/to/socket
unix://                           # error: requires a path
```

Unix socket paths are redacted in display: `unix:///var/run/****`.

### Transparent Proxy (Linux only)

```
redir://:12345                    # bind all interfaces
redir://127.0.0.1:12345          # specific bind address
redir://user:pass@127.0.0.1:12345  # with auth
```

### Reverse/Backward Proxy

```
bind://user:pass@0.0.0.0:8080    # reverse server listener
listen://127.0.0.1:9090          # reverse server listener
backward://0.0.0.0:8080          # reverse server listener
rebind://0.0.0.0:8080            # reverse server listener
socks5+in://acceptor:1080        # backward client (+in modifier)
```

## Supported Schemes (pproxy compat)

| Scheme | As Listener | As Upstream | Status |
|--------|-------------|-------------|--------|
| `http` | Yes | Yes | Compatible |
| `https` | Yes (TLS) | Yes (HTTP+TLS) | Supported |
| `socks4` | Yes | Yes | Compatible |
| `socks4a` | Yes | Yes | Compatible |
| `socks5` | Yes | Yes | Compatible |
| `ss` / `shadowsocks` | Yes (AEAD only) | Yes (AEAD only) | Supported |
| `trojan` | No (upstream only) | Yes | Partial |
| `direct` | No | Yes | Supported |
| `h2` | Yes | Yes | Supported |
| `ws` / `wss` | Yes | Yes | Supported |
| `raw` / `tunnel` | Yes | Yes | Supported |
| `unix` | Yes | No | Supported |
| `redir` | Yes (Linux only) | No | Supported |
| `bind` / `listen` / `backward` / `rebind` | Yes | No | Supported |
| `ssr` | Rejected | Rejected | Intentional non-parity |
| `ssh` | Rejected | Rejected | Intentional non-parity |

## Unsupported Schemes (Rejected with Diagnostics)

| Scheme | Diagnostic | Reason |
|--------|-----------|--------|
| `ssr` | `unsupported_security_sensitive_legacy_feature` | Legacy non-standard extension |
| `ssh` | `unsupported_protocol` | SSH transport out-of-scope for proxy |
| `ftp` (or any other) | `unsupported_protocol` | Not implemented |

## Query Parameters

| Parameter | Meaning | Example |
|-----------|---------|---------|
| `rule=regex` | Route matching regex | `socks5://host:1080?rule=.*\\.com` |

## Credential Formats

| Scheme | Credential Format | Example |
|--------|------------------|---------|
| `http`, `socks4`, `socks5`, `ss` | `user:pass@host:port` | `http://admin:secret@proxy:8080` |
| `trojan` | `password@host:port` (password-only) | `trojan://mypassword@server:443` |
| `bind`, `listen`, `backward` | `user:pass@host:port` | `bind://user:pass@0.0.0.0:8080` |
| `redir` | `user:pass@host:port` | `redir://user:pass@127.0.0.1:12345` |

Credentials are always redacted in display and diagnostic output.

## Redaction Rules

- `user:pass@` → `****:****@` in display
- `unix://path/to/secret.sock` → `unix://path/to/****`
- All diagnostic messages, warnings, and error strings never contain raw credentials
