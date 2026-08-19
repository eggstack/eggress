# eggress-system-proxy

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Inspect and configure platform system-proxy settings: Linux environment variables, macOS preferences, and Windows registry.

## When to use this crate

Use `eggress-system-proxy` directly when you need to read or apply system-wide proxy configuration from Rust. Most users access this functionality through the `eggress` CLI's `operations` feature or the admin server.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [System proxy](https://github.com/eggstack/eggress/blob/main/docs/system_proxy/)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
