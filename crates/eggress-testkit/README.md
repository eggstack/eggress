# eggress-testkit

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Shared test utilities (oracle helpers, fixtures, body assertion) for eggress internal tests. Also ships the `strict-report` binary used by CI to summarize parity closure audits.

## When to use this crate

Use `eggress-testkit` when writing integration or differential tests for eggress. This crate is a dev-dependency for most other eggress crates and is not typically depended on at runtime.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Testing](https://github.com/eggstack/eggress/blob/main/docs/TESTING.md)
- [Differential testing](https://github.com/eggstack/eggress/blob/main/docs/DIFFERENTIAL_TESTING.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
