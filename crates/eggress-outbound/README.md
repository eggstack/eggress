# eggress-outbound

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Listener-free outbound chain execution: the direct Rust dependency for
opening proxy-chained TCP connections (and optional UDP associations)
without starting a listener service. Ordinary HTTP/SOCKS TCP chains are
available in the base profile with no features enabled.

## When to use this crate

Need listener-free outbound proxy-chain dialing? -> `eggress-outbound`.
Need a full in-process proxy lifecycle (listeners, supervision, reload,
metrics)? -> `eggress-embed`. Need only generic byte relay? ->
`eggress-relay`.

Use `eggress-outbound` when you need to execute an Eggress proxy chain
in-process. `eggress-server` consumes the hop registry, executor factory,
and classifier for its listener-bound sessions; `eggress-embed` re-exports
this API (`eggress_embed::outbound::*`) as its full-service facade.

## Feature flags

- `toml` — `OutboundConnector::from_toml()` / `validate_outbound_config()`
  via the canonical config boundary.
- `pproxy-compat` — `OutboundConnector::from_pproxy_uri()` for canonical
  `__` multi-hop chains (fail-closed, redacted errors).
- `udp` — `associate_udp()` / `UdpAssociation` (direct + single-hop SOCKS5;
  composed/Shadowsocks UDP fail with `UnsupportedFeature`).
- `extended` — Shadowsocks, Trojan, WebSocket hops.
- `pproxy-legacy` — ShadowsocksR (`extended` + SSR framing).
- `legacy-crypto` — legacy Shadowsocks stream ciphers.
- `ssh` — SSH upstream transport (does not activate `pproxy-compat`).
- `quic` — QUIC/H3 transport and protocol.
- `insecure-tls` — per-hop `?insecure` verifier (test-gated; never default).

`ssh` never implies `pproxy-compat` and `pproxy-compat` never implies
`ssh`; pproxy-style SSH requires both. No insecure/test-only transport
feature belongs in normal defaults.

## Quick example

```rust
use eggress_outbound::OutboundConnector;

let chain = eggress_uri::parse_proxy_chain("socks5://127.0.0.1:1080").unwrap();
let connector = OutboundConnector::from_chain(chain).unwrap();
assert_eq!(connector.hop_count(), 1);
```

```rust
let connector = OutboundConnector::direct();
assert_eq!(connector.upstream_count(), 0);
```

Ordinary `connect_tcp()` / `connect_tcp_timeout()` remain the simple
compatibility API (`OutboundError::Runtime`). Detailed
`connect_tcp_detailed()` / `connect_tcp_timeout_detailed()` share the same
single execution and return `OutboundConnectError` with stable
`kind()` / `stage()` / `hop_index()` / `protocol()` facts.

Kind, stage, hop, and protocol are diagnostic facts, not retry
recommendations; callers own retry/backoff policy and no direct fallback
occurs on proxy failure. Display/Debug are bounded and credential-safe.

`OutboundInfo` carries observational socket metadata: `Some(addr)` means that
address belongs to the physical transport carrying the returned stream;
`None` means Eggress cannot prove that relationship. Direct TCP and ordinary
TCP-preserving first hops report actual addresses without another DNS lookup.
Hop-zero pooled SSH/H2 may return `None` because cache reuse can discard the
candidate socket; nested SSH/H2 is unpooled and retains actual hop-zero
metadata. Unix and other non-TCP first hops may also return `None`. Missing
metadata never changes successful connection behavior.

Physical SSH/H2 reuse is allowed at hop 0. Nested SSH/H2 hops use the supplied
chain stream and are unpooled. Hop-zero SSH/H2 reuse is policy-scoped:
explicit `local_bind` disables reuse, explicit insecure H2 is unpooled, and
Eggress H2 registries are bounded and scoped to the identity of the TLS
client-config object. The public `H2PoolKey` omits TLS trust policy and is not
the complete identity by itself. `OutboundInfo::Some(addr)` still identifies
the physical transport carrying the stream.

ALPN adaptation of a caller-supplied `Arc<rustls::ClientConfig>` (e.g. when a
hop's `h2` protocol adds the H2 ALPN list to a configured override) clones
the existing `ClientConfig` and only mutates `alpn_protocols`. The
`eggress_transport_tls::client_config_with_alpn` helper is the single
authority for that adaptation: trust roots, custom CA stores, mTLS client
identity, custom verifier, and every other `rustls::ClientConfig` field are
preserved by `ClientConfig::clone()`. The outbound TLS wrapper never rebuilds
a fresh system-roots `ClientConfig` for an existing override, so custom TLS
policies survive H2 ALPN adaptation. A caller-supplied `tls_override`
combined with a per-hop `insecure=true` request is rejected explicitly (no
silent substitution of Eggress's default insecure verifier); callers that
need that combination must supply an explicit insecure override or remove
`insecure=true`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/architecture/outbound.md)

## License

MIT OR Apache-2.0
