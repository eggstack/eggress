# eggress-runtime

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Service supervisor, lifecycle management, composition, and graceful shutdown for the eggress proxy.

## When to use this crate

Use `eggress-runtime` directly when embedding eggress in a Rust application that needs full control over service lifecycle. Most users depend on this crate transitively through `eggress-embed`.

## Feature flags

- `full` (default) — Enables `common`, `extended`, `operations`, and `reverse`.
- `common` — Base protocol set (HTTP, SOCKS4/4a, SOCKS5, raw).
- `extended` — Adds Shadowsocks, Trojan, WebSocket, and Shadowsocks UDP relay.
- `pproxy-legacy` — Enables `extended` plus pproxy-compatible Shadowsocks compression.
- `legacy-crypto` — Enables `extended` plus legacy Shadowsocks stream ciphers.
- `operations` — Enable admin server and system-proxy integration.
- `reverse` — Enable reverse/backward proxy protocol (implies `operations`).
- `ssh` — Enable SSH upstream transport.
- `quic` — Enable QUIC/H3 transport and protocol.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/docs/ARCHITECTURE.md)
- [Operations](https://github.com/eggstack/eggress/blob/main/docs/OPERATIONS.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
