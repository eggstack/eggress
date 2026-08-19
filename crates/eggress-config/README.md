# eggress-config

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

TOML configuration schema, validation, and hot-reload primitives for eggress.

## When to use this crate

Use `eggress-config` directly when parsing or validating eggress TOML configuration files. Most users depend on this crate transitively through `eggress-embed` or `eggress-runtime`.

## Feature flags

- `quic` — Enable QUIC/H3 configuration support.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Config reference](https://github.com/eggstack/eggress/blob/main/docs/CONFIG_REFERENCE.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
