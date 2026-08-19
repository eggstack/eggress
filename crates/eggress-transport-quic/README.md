# eggress-transport-quic

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

QUIC transport wrapper using Quinn. Used by the H3 protocol for HTTP/3 CONNECT proxying.

## When to use this crate

Use `eggress-transport-quic` directly when building custom QUIC transport logic. Most users enable this via the `quic` feature on `eggress-server`, `eggress-runtime`, or `eggress-embed`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
