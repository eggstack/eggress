# eggress-transport-tls

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Shared TLS client/server transport built on rustls. Used by Trojan, HTTPS upstreams, and H2 CONNECT.

## When to use this crate

Use `eggress-transport-tls` directly when building custom TLS-enabled protocol or transport handlers. Most users depend on this crate transitively through `eggress-server` or protocol crates like `eggress-protocol-trojan`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Security](https://github.com/eggstack/eggress/blob/main/docs/security/)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
