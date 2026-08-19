# eggress-protocol-websocket

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

WebSocket tunnel protocol (client + server) used to wrap proxy traffic over WebSocket connections.

## When to use this crate

Use `eggress-protocol-websocket` directly when building custom WebSocket tunnel logic. Most users depend on this crate transitively through `eggress-server` with the `extended` feature.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
