# eggress-metrics

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Prometheus-compatible metrics registry and bridge to the admin HTTP server.

## When to use this crate

Use `eggress-metrics` directly when exposing custom metrics from a protocol handler or transport. Most users depend on this crate transitively through `eggress-runtime` or `eggress-admin`.

## Feature flags

- `full` (default) — Enables `extended`.
- `common` — Base metrics set.
- `extended` — Adds Shadowsocks-specific metrics.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Metrics](https://github.com/eggstack/eggress/blob/main/docs/METRICS.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
