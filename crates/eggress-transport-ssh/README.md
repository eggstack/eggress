# eggress-transport-ssh

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Optional SSH upstream transport using the russh crate. Enables `ssh://` upstream URIs in proxy chains.

## When to use this crate

Use `eggress-transport-ssh` directly when building custom SSH transport logic. Most users enable this via the `ssh` feature on `eggress-server`, `eggress-runtime`, or `eggress-embed`.

## Feature flags

- `ssh` — No additional sub-features; this crate is itself opt-in at the workspace level.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [Architecture](https://github.com/eggstack/eggress/blob/main/docs/ARCHITECTURE.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
