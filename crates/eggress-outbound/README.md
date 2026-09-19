# eggress-outbound

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Listener-free outbound chain execution: the direct Rust dependency for
opening proxy-chained TCP connections (and optional UDP associations)
without starting a listener service.

## When to use this crate

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

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/architecture/outbound.md)

## License

MIT OR Apache-2.0
