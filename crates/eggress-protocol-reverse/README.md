# eggress-protocol-reverse

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Reverse / backward proxy control-channel protocol (pproxy-compatible). Allows an upstream proxy to initiate connections back through the local listener.

## When to use this crate

Use `eggress-protocol-reverse` directly when building custom reverse-proxy control logic. Most users enable this via the `reverse` feature on `eggress-runtime` or `eggress-embed`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [pproxy migration](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
