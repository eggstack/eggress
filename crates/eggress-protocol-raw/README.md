# eggress-protocol-raw

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Raw fixed-target TCP passthrough protocol used by pproxy's `raw://` URIs.

## When to use this crate

Use `eggress-protocol-raw` directly when implementing a custom raw-tunnel handler. Most users depend on this crate transitively through `eggress-server`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
