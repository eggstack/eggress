# eggress-uri -- Proxy Chain URI Grammar

Leaf crate (no eggress dependencies). Parses proxy URIs into a typed AST that
config compilation, routing, and the pproxy compatibility layer all consume.
Single `src/lib.rs` (~1220 lines including tests and proptests).

## Module map

Single file. Public items:

| Item | Role |
|---|---|
| `ProxyChainSpec` | Ordered list of hops (the `-r` upstream chain) |
| `ProxyHopSpec` | One hop: protocols + endpoint + credentials + options |
| `ProtocolSpec` | Per-protocol selection within a hop (14 variants) |
| `EndpointSpec` | Host/port pair (host kept as raw string) |
| `CredentialSpec` | Username/password with redacted Debug and Display |
| `RedactedUri` | Display wrapper that masks credentials in output |
| `UriParseError` | Structured parse errors with span information |
| `parse_proxy_chain(uri) -> Result<ProxyChainSpec, UriParseError>` | Main entry point |

## Public API surface

### ProtocolSpec variants

Http, HttpOnly, Socks4, Socks5, Shadowsocks, ShadowsocksR, Trojan, Http2,
Http3, Quic, WebSocket, Raw, Ssh, Unix.

Scheme aliases accepted during parsing:

| Alias | Resolves to |
|---|---|
| `socks4a` | Socks4 |
| `ss` | Shadowsocks |
| `ws`, `wss` | WebSocket |
| `raw`, `tunnel` | Raw |

### ProxyHopSpec fields

| Field | Type | Default | Notes |
|---|---|---|---|
| `protocols` | `Vec<ProtocolSpec>` | -- | At least one required |
| `endpoint` | `EndpointSpec` | -- | host + port |
| `credentials` | `Option<CredentialSpec>` | None | Percent-decoded |
| `rule` | `Option<String>` | None | From `?rule=<value>` query param |
| `local_bind` | `Option<String>` | None | Trailing `@<ip>` modifier |
| `tls` | `bool` | false | From `+tls` in scheme |
| `server_name` | `Option<String>` | None | SNI override |
| `insecure` | `bool` | false | From `?insecure` query param |
| `plugins` | `Vec<String>` | empty | From path segment (e.g. `/plugin1,plugin2`) |
| `auth_prefix` | `Option<String>` | None | From `#fragment` |

### Grammar

```
chain       = hop ( "__" hop )+
hop         = scheme "://" [ creds "@" ] endpoint [ "?" query ] [ "/" plugins ] [ "@" local_bind ]
scheme      = proto ( "+" proto )*        -- "tls" sets hop.tls flag
proto       = "http" | "socks4" | "socks5" | "shadowsocks" | "ss" | "ssr"
            | "trojan" | "h2" | "h3" | "quic" | "ws" | "wss"
            | "raw" | "tunnel" | "ssh" | "unix" | "httponly"
creds       = user ":" pass               -- Trojan allows pass-only (no colon)
endpoint    = host ":" port               -- bracketed IPv6: [::1]:8080
query       = "rule=" <value> | "insecure" | "insecure=true"
plugins     = name ( "," name )*
local_bind  = <ip-addr>                   -- e.g. @127.0.0.1
```

Key parsing rules:
- `__` separates hops; `___` (triple) is rejected as `DuplicateHopSeparator`
- `+` stacks protocols within one hop; `tls` in the scheme sets `hop.tls = true`
- Credentials are percent-decoded (`%40` -> `@`, `%3A` -> `:`, UTF-8 sequences)
- The userinfo separator is the **last** unbracketed `@` after `://` -- a password containing `@` is preserved correctly
- SSH defaults to port 22 when no port is given
- Port 0 is rejected (except for Unix protocol)
- Empty host with port (e.g. `http://:8080`) is allowed for bind-to-all-interfaces
- Bracket depth is tracked; unmatched `[` or `]` is rejected before hop splitting

### RedactedUri::Display

Renders the chain with credentials masked:
- With creds: `protocol://****:****@host:port`
- Without creds: `protocol://host:port`
- Hops joined with `__`
- IPv6 hosts are bracketed: `[::1]:port`
- `hop.tls` appends `+tls` to the protocol list in output

### CredentialSpec

- `Debug` impl: username visible, password replaced with `"****"`
- `Display` is not implemented (use `RedactedUri` for safe output)
- `Clone`, `Serialize`, `Deserialize`, `PartialEq`, `Eq`

### UriParseError

| Variant | When |
|---|---|
| `InvalidFormat { message, span }` | Malformed URI structure |
| `UnsupportedProtocol(String)` | Unknown scheme token |
| `MissingHost` | Empty host after `://` |
| `InvalidPort(String)` | Non-numeric or out-of-range port |
| `EmptyHost` | Empty host string |
| `DuplicateHopSeparator` | `___` or adjacent separators |

Error messages include hop context (e.g. `"hop 1: missing scheme"`).

## How it works (control flow)

1. `parse_proxy_chain(uri)` calls `split_hops(uri)` which splits on `__` with bracket-depth tracking
2. Each hop string is passed to `parse_hop()` which:
   - Detects trailing local-bind modifier (`find_last_at_outside_scheme`)
   - Extracts scheme, calls `parse_protocols()` to split on `+` and map to `ProtocolSpec`
   - Extracts `#auth_prefix` fragment
   - Extracts credentials (`find_at_outside_brackets` for the `@` separator)
   - Parses plugin path segment
   - Splits endpoint from query string
   - Calls `parse_endpoint()` for host:port
   - Extracts `?rule=` and `?insecure` query params
   - Validates port != 0 (except Unix)
3. `parse_credentials()` percent-decodes username and password; Trojan allows password-only (no colon)
4. Results are wrapped in `ProxyChainSpec { hops }`

## Error & failure model

- All parse errors are `UriParseError` -- no panics on any input (verified by proptest `test_parse_never_panics`)
- Hop-level errors are wrapped with `add_hop_context()` to include the hop index
- `DuplicateHopSeparator` is a dedicated variant (not a generic format error)

## Configuration/features

- No feature flags
- Dependencies: `serde`, `serde_json` (tests), `thiserror`, `proptest` (tests)
- No `unsafe` code

## Security notes

- `CredentialSpec::Debug` redacts passwords (verified by `test_credential_debug_is_redacted`)
- `RedactedUri::Display` replaces creds with `****:****@` (verified by multiple roundtrip and redaction tests)
- Percent-decoding is lossy via `String::from_utf8_lossy` -- invalid UTF-8 sequences are replaced rather than rejected

## Concurrency & lifecycle

- Entirely synchronous parsing; no async or concurrency concerns
- `parse_proxy_chain()` is safe to call from any thread

## Test coverage map

| Category | Count | Key tests |
|---|---|---|
| Basic parsing | 10 | Empty URI, simple http/socks4/socks5, named host, missing scheme, empty host, invalid port, port zero |
| Multi-protocol | 3 | `http+socks4+socks5`, tls suffix, tls+http |
| Credentials | 8 | User:pass, Trojan password-only, password-only rejected for non-Trojan, percent-decoded @ and : in password/username, UTF-8 creds |
| Multi-hop | 4 | Two hops, triple-hop separator rejected |
| IPv6 | 4 | Bracketed, full, unterminated bracket, mismatched brackets |
| Query/rule | 3 | Rule extraction, no rule, insecure flag |
| Redaction | 4 | Credentialed/uncensored display, redacted Debug, roundtrip |
| Roundtrip | 6 | Simple, multi-hop, multi-protocol, IPv6, with rule |
| Regression | 3 | Password containing @, redacted display, IPv6 with @ in password |
| Protocol variants | 4 | Shadowsocks, ss alias, Shadowsocks roundtrip, quic+http, h3, socks4a |
| TLS | 3 | socks5+tls, http+tls, roundtrip |
| SSH | 1 | Defaults to port 22 |
| Proptest | 4 | Never panics on arbitrary input, valid chain roundtrips, hop separator split, protocol separator |
| **Total** | **53 + 4 proptests** | |

## Reviewer gotchas

- The `CredentialSpec` derives `Serialize`/`Deserialize` but `Debug` is manually overridden to redact -- do not rely on derived `Debug` for credential safety.
- `parse_proxy_chain` allows empty host with port (e.g. `http://:8080`). This is intentional for bind-to-all-interfaces use cases.
- The `+` separator is for protocol stacking within a scheme; `__` is for hop chaining. Do not confuse with URI path separators.
- `find_at_outside_brackets` finds the **last** unbracketed `@` after `://`. This is critical for passwords containing `@`.
- Port 0 is rejected for all protocols except Unix (where port is always 0).
- `split_hops` rejects `___` (triple underscore) as `DuplicateHopSeparator` but does not check for longer runs -- `____` would be caught as two consecutive separators.
- The `plugins` path segment is parsed from the URI path after the endpoint (e.g. `socks5://host:1080/plugin1,plugin2`). Leading commas are trimmed.

## See also

- [core.md](core.md) -- `ChainExecutor` consumes `ProxyChainSpec`
- [config.md](config.md) -- upstream URIs are parsed via `parse_proxy_chain`
- [routing.md](routing.md) -- upstream chain specs used in capability classification
- [pproxy-compat.md](pproxy-compat.md) -- pproxy URI translation layer
