# eggress-admin

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Local admin HTTP server providing PAC files, Prometheus metrics, listener status, and route explanations.

## When to use this crate

Use `eggress-admin` directly when building a custom admin or monitoring endpoint. Most users enable this via the `operations` feature on `eggress-runtime` or `eggress-embed`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Operations](https://github.com/eggstack/eggress/blob/main/docs/OPERATIONS.md)
- [Metrics](https://github.com/eggstack/eggress/blob/main/docs/METRICS.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
