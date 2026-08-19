# eggress-server

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Connection acceptance, listener orchestration, and protocol dispatch for incoming proxy connections.

## When to use this crate

Use `eggress-server` directly when building a custom listener or protocol dispatch layer. Most users depend on this crate transitively through `eggress-runtime` or `eggress-embed`.

## Feature flags

- `full` (default) — Enables `extended`.
- `common` — Base protocol set (HTTP, SOCKS4/4a, SOCKS5, raw).
- `extended` — Adds Shadowsocks, Trojan, WebSocket, and Shadowsocks UDP relay.
- `pproxy-legacy` — Enables `extended` plus pproxy-compatible Shadowsocks compression.
- `legacy-crypto` — Enables `extended` plus legacy Shadowsocks stream ciphers.
- `ssh` — Enable SSH upstream transport.
- `quic` — Enable QUIC/H3 transport and protocol.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/docs/ARCHITECTURE.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
