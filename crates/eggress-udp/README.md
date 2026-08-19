# eggress-udp

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

UDP association tracking, datagram relay, and SOCKS5 UDP upstream support.

## When to use this crate

Use `eggress-udp` directly when building custom UDP relay or association logic. Most users depend on this crate transitively through `eggress-server`.

## Feature flags

- `shadowsocks` (default) — Enable Shadowsocks UDP relay support.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
