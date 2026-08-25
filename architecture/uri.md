# eggress-uri — Proxy Chain URI Grammar

Leaf crate (no eggress dependencies). Parses proxy URIs into a typed AST that
config compilation, routing, and the pproxy compatibility layer all consume.

## Module map

Single `src/lib.rs`. Public AST:

- `ProxyChainSpec` — ordered list of hops (the `-r` upstream chain)
- `ProxyHopSpec` — one hop: protocols + endpoint + credentials + options
- `ProtocolSpec` — per-protocol selection within a hop (`+` combines, e.g. `socks5+tls://`)
- `EndpointSpec` — host/port pair (host kept as written)
- `CredentialSpec` — username/password, never rendered in plain text

Grammar notes:

- `__` separates chained hops: `socks5://a:1080__http://b:8080`
- `+` stacks transport on protocol within one hop: `trojan+tls://`
- Scheme-to-default-port mapping lives alongside parsing
- `RedactedUri` / redacted `Display` implementations keep secrets out of logs

## Behaviors worth reviewing

- Strict rejection of malformed chains vs. lenient host handling — check both
  directions when touching the parser (fuzz target `uri_parse` covers both this
  grammar and the pproxy URI parser).
- Parsing is shared by native config paths and by
  `eggress-pproxy-compat`'s separate pproxy-flavored parser; do not conflate
  the two grammars when making changes.

## Interactions

- Depended on by `eggress-routing`, `eggress-config`, `eggress-core` consumers,
  `eggress-embed` (`OutboundConnector::from_pproxy_uri` path), compat layer.

## Review entry points

- Verify: `cargo test -p eggress-uri` plus `cargo fuzz` target `uri_parse`
  (see [testing-and-tooling.md](testing-and-tooling.md)).
