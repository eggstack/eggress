# eggress-uri -- Proxy Chain URI Grammar

Leaf crate (no eggress dependencies). Parses proxy URIs into a typed AST that
config compilation, routing, and the pproxy compatibility layer all consume.

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | `ProxyChainSpec`/`ProxyHopSpec` AST, `parse_proxy_chain`, redaction |
| `src/syntax.rs` | Shared lexical/endpoint primitives reused by the compat parser |

Public items:

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
| `syntax::{split_chain_hops, split_once_outside_brackets, find_userinfo_separator, parse_host_port, split_userinfo, format_host}` | Neutral lexical helpers (no grammar policy) |

## Public API surface

### ProtocolSpec variants

Http, HttpOnly, Socks4, Socks5, Shadowsocks, ShadowsocksR, Trojan, Http2,
Http3, Quic, WebSocket, Raw, Ssh, Unix.

`ProtocolSpec::canonical_name()` is the single native name per variant
(matches redacted display); `ProtocolSpec::parse_name()` is the single native
recognition point plus explicit aliases; `FromStr`/`Display` delegate to both.
`ProtocolSpec::all_variants()` enumerates all 14 for exhaustive disposition tests.

Scheme aliases accepted during parsing:

| Alias | Resolves to |
|---|---|
| `socks4a` | Socks4 |
| `ss` | Shadowsocks |
| `ws`, `wss` | WebSocket |
| `raw`, `tunnel` | Raw |

`tls` is a transport modifier, not a protocol. Compatibility-only names
(`https`, `direct`, `redir`, `echo`, `bind`, `listen`, `backward`, `rebind`,
`secure`, `in`, `websocket`) intentionally return `None` from `parse_name`.

### Shared lexical primitives (`syntax`)

```
shared lexical primitives != shared grammar
```

`syntax` owns bracket/brace-aware chain splitting, `@` detection, neutral
host/port splitting, userinfo splitting (no decoding), and host formatting.
It returns neutral structures (`HostPort`, raw userinfo parts) and
`SyntaxError`; each grammar maps errors and applies its own policy:

- empty/duplicate `__` handling stays with the owning grammar (native keeps
  `DuplicateHopSeparator` for `___`; compat keeps leading/trailing/doubled checks);
- percent-decoding stays native-only (compat keeps values verbatim);
- default ports stay compat-owned (8080 / ssh 22); native requires explicit ports;
- empty hosts are rejected for native proxy hops but allowed for compat listeners.

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
- Empty hosts are rejected for proxy hops (e.g. `http://:8080`); listener bind
  addresses are configured separately and may use unspecified addresses
- Bracket depth is tracked; unmatched `[` or `]` is rejected before hop splitting

### RedactedUri::Display

Renders the chain with credentials masked:
- With creds: `protocol://****:****@host:port`
- Without creds: `protocol://host:port`
- Hops joined with `__`
- IPv6 hosts are bracketed: `[::1]:port`
- `hop.tls` appends `+tls` to the protocol list in output

### redact_proxy_uri (canonical tolerant redactor)

`redact_proxy_uri()` is the single authority for scrubbing
credentials from arbitrary URI-like strings in logs, diagnostics, redacted
TOML, and oracle transcripts. It is scheme-agnostic (keyed on `://`, last
unbracketed `@` wins via `syntax::find_userinfo_separator`) and returns
`scheme://****@host`, or the input unchanged when no userinfo is present.
`eggress-embed` (`to_redacted_toml`) and `eggress-testkit` (oracle transcript
scrubbing) both delegate to it instead of maintaining scheme whitelists.
Compat structured displays (`PproxyUri::redacted_display`) use the shared
`syntax::format_host` for IPv6 bracketing rather than a second formatter.

### Syntax-to-runtime disposition

`ProtocolSpec` is syntax; `eggress_core::ProtocolId` is runtime dispatch.
`ProtocolId::from_protocol_spec()` in `eggress-core` is the central exhaustive
conversion: `HttpOnly` collapses to `Http`, `Unix` serves `Raw` semantics,
`Ssh` fails explicitly as upstream-only, and `Echo`/`Reverse` are runtime-only
with no syntax counterpart. The CLI listener path delegates to it; config
string compilation (`compile_protocol`) keeps its own string arms for
runtime-only names like `echo`/`websocket`.

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

1. `parse_proxy_chain(uri)` calls `split_hops()` — triple-`_` check plus shared
   `syntax::split_chain_hops` (bracket/brace-aware, unmatched fails closed)
2. Each hop string is passed to `parse_hop()` which:
   - Detects trailing local-bind modifier (`find_last_at_outside_scheme` over shared `@` scan)
   - Extracts scheme, calls `parse_protocols()` (`+` split, `tls` modifier, `ProtocolSpec::parse_name`)
   - Extracts `#auth_prefix` fragment
   - Extracts credentials (shared `@` scan, native percent-decode)
   - Parses plugin path segment
   - Splits endpoint from query string
   - Calls `parse_endpoint()` (shared `syntax::parse_host_port` + native port/host policy)
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

| Category | Location | Key tests |
|---|---|---|
| Basic parsing | `src/lib.rs` | Empty URI, simple http/socks4/socks5, named host, missing scheme, empty host, invalid port, port zero |
| Multi-protocol | `src/lib.rs` | `http+socks4+socks5`, tls suffix, tls+http |
| Credentials | `src/lib.rs` | User:pass, Trojan password-only, password-only rejected for non-Trojan, percent-decoded @ and : in password/username, UTF-8 creds |
| Multi-hop | `src/lib.rs` | Two hops, triple-hop separator rejected |
| IPv6 | `src/lib.rs` | Bracketed, full, unterminated bracket, mismatched brackets |
| Query/rule | `src/lib.rs` | Rule extraction, no rule, insecure flag |
| Redaction | `src/lib.rs` | Credentialed/uncensored display, redacted Debug, roundtrip |
| Roundtrip | `src/lib.rs` | Simple, multi-hop, multi-protocol, IPv6, with rule |
| Regression | `src/lib.rs` | Password containing @, redacted display, IPv6 with @ in password |
| Protocol variants | `src/lib.rs` | Shadowsocks, ss alias, Shadowsocks roundtrip, quic+http, h3, socks4a |
| TLS | `src/lib.rs` | socks5+tls, http+tls, roundtrip |
| SSH | `src/lib.rs` | Defaults to port 22 |
| Canonical recognition | `src/lib.rs` | `canonical_name` roundtrip for all 14 variants; explicit alias + compat-only exclusion table |
| Shared syntax | `src/syntax.rs` | `@` separator, chain split with braces, host/port corpus, userinfo split, host formatting |
| Cross-parser equivalence | `eggress-pproxy-compat/tests/uri_syntax_equivalence.rs` | Shared endpoint/userinfo/chain/TLS/alias/redaction corpus, malformed fail-closed, empty-host + percent-decode differences, compat-only constructs |
| Runtime disposition | `eggress-core` | Exhaustive `ProtocolSpec` → `ProtocolId` mapping (HttpOnly→Http, Unix→Raw, Ssh explicit error) |
| Proptest | `src/lib.rs` | Never panics on arbitrary input, valid chain roundtrips, hop separator split, protocol separator |

## Reviewer gotchas

- The `CredentialSpec` derives `Serialize`/`Deserialize` but `Debug` is manually overridden to redact -- do not rely on derived `Debug` for credential safety.
- `parse_proxy_chain` rejects empty hosts for proxy hops; listener bind
  addresses are configured separately.
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
