# eggress-protocol-shadowsocks

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Shadowsocks AEAD and legacy stream/UDP cipher support (`aes-256-gcm`, `chacha20-ietf-poly1305`, etc.).

## When to use this crate

Use `eggress-protocol-shadowsocks` directly when building custom Shadowsocks protocol handlers. Most users depend on this crate transitively through `eggress-server` with the `extended` feature.

## Feature flags

- `pproxy-legacy` — Enable pproxy-compatible legacy compression (flate2).
- `legacy-crypto` — Enable legacy stream ciphers: `aes`, `blowfish`, `camellia`, `chacha20`, `des`, `hmac`, `salsa20`.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Protocols overview](https://github.com/eggstack/eggress/blob/main/docs/protocols/)
- [pproxy migration](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
