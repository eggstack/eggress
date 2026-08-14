# eggress-uri

`crates/eggress-uri/`

URI parser for native proxy chains. The pproxy compatibility crate has its own
typed AST because pproxy's grammar deliberately differs from native URI
configuration.

## Key Types

| Type | Description |
|---|---|
| `ProxyChainSpec` | A complete proxy chain: one or more `ProxyHopSpec` entries joined by `__` |
| `ProxyHopSpec` | A single hop: protocol(s), endpoint, credentials, TLS flag, rule, local bind |
| `ProtocolSpec` | Protocol identifier within a hop (e.g., `socks5`, `http`, `ss`) |
| `EndpointSpec` | `host:port` endpoint specification |
| `CredentialSpec` | `user:password` credentials (may be absent) |
| `RedactedUri` | Wrapper that redacts credentials in `Display` output |

## Native URI Grammar

```
protocol+protocol://user:password@host:port?rule#local
    hop1    __    hop2://...
```

- `+` separates multiple protocols within a single hop (e.g., `socks5+tls`)
- `__` separates proxy hops in a chain
- A hop separator must contain exactly two underscores; repeated separators
  such as `___` are rejected.
- URI endpoint ports must be in `1..=65535`. Port `0` is reserved for TOML
  listener binds where the operating system assigns a port.
- `?` introduces route rule expressions
- `#` introduces local bind address

## pproxy compatibility grammar

```text
scheme[+scheme...][+tls|+ssl|+in...]://[cipher-or-userinfo@]netloc[/@localbind][,plugin...][?rules][#auth]
```

The compatibility parser retains protocol tokens, modifiers, raw input,
fragment authentication, local binding, brace-delimited fixed targets, plugin
metadata (for diagnostic purposes), and raw rule suffixes. Plugin execution is
explicitly unsupported — the Python compatibility factory rejects
plugin-bearing URIs. Combined
`http+socks4+socks5://:8080` listeners translate to one listener with three
protocol detectors. Unsupported fields produce explicit diagnostics after
parsing.

## Key Functions

| Function | Description |
|---|---|
| `parse_proxy_chain(uri)` | Parse a full chain URI into `ProxyChainSpec` |

## Security

- `RedactedUri` replaces credentials with `****` in display output
- Credentials are never logged by any crate that holds a `ProxyChainSpec`

## Dependencies

None — this is a foundational crate.

See [overview.md](overview.md) for context.
