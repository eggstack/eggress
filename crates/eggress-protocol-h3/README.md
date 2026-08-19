# eggress-protocol-h3

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

HTTP/3 CONNECT proxy over QUIC. Used by the H3 listener when the `quic` feature is enabled.

## When to use this crate

Use `eggress-protocol-h3` directly when building custom HTTP/3 proxy logic. Most users enable this via the `quic` feature on `eggress-server` or `eggress-runtime`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
