# eggress-uri

`crates/eggress-uri/`

URI parser for proxy chain specifications. Parses pproxy-compatible URI syntax into typed AST nodes.

## Key Types

| Type | Description |
|---|---|
| `ProxyChainSpec` | A complete proxy chain: one or more `ProxyHopSpec` entries joined by `__` |
| `ProxyHopSpec` | A single hop: protocol(s), endpoint, credentials, TLS flag, rule, local bind |
| `ProtocolSpec` | Protocol identifier within a hop (e.g., `socks5`, `http`, `ss`) |
| `EndpointSpec` | `host:port` endpoint specification |
| `CredentialSpec` | `user:password` credentials (may be absent) |
| `RedactedUri` | Wrapper that redacts credentials in `Display` output |

## URI Grammar

```
protocol+protocol://user:password@host:port?rule#local
    hop1    __    hop2://...
```

- `+` separates multiple protocols within a single hop (e.g., `socks5+tls`)
- `__` separates proxy hops in a chain
- `?` introduces route rule expressions
- `#` introduces local bind address

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
