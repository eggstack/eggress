# eggress-routing

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Rule-based routing: matchers (host, geoip, CIDR, regex), schedulers (round-robin, failover, sticky), and upstream health tracking.

## When to use this crate

Use `eggress-routing` directly when building custom route-selection logic. Most users depend on this crate transitively through `eggress-config` or `eggress-embed`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/docs/ARCHITECTURE.md)
- [Config reference](https://github.com/eggstack/eggress/blob/main/docs/CONFIG_REFERENCE.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
