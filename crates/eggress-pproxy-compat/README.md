# eggress-pproxy-compat

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

pproxy URI and CLI flag translation, plus the bounded `pproxy` Python namespace adapter. The `pproxy` binary compatibility surface lives in `eggress-cli`.

## When to use this crate

Use `eggress-pproxy-compat` directly when translating pproxy CLI arguments or URIs into eggress configuration. Most users depend on this crate transitively through `eggress-cli` or `eggress-embed` with the `pproxy-compat` feature.

## Feature flags

- `ssh` — Enable SSH URI translation.
- `quic` — Enable QUIC URI translation.
- `legacy-crypto` — Enable legacy cipher URI translation.
- `daemon` — Enable pproxy daemon mode translation.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [pproxy migration](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md)
- [Proxy compatibility](https://github.com/eggstack/eggress/blob/main/docs/PROXY_COMPAT.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
