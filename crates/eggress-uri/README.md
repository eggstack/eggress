# eggress-uri

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

URI parsing for eggress proxy URIs (`socks5://user:pass@host:port`, `http://host:port`, `raw://host:port/path`, etc.).

## When to use this crate

Use `eggress-uri` directly when you need to parse or validate proxy URI strings outside of the full eggress runtime. Most users interact with this crate indirectly through `eggress-config` or `eggress-embed`.

## Quick example

```rust
use eggress_uri::ProxyUri;

let uri: ProxyUri = "socks5://user:pass@proxy.example:1080".parse()?;
assert_eq!(uri.scheme(), "socks5");
assert_eq!(uri.host(), Some("proxy.example"));
```

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [URI grammar](https://github.com/eggstack/eggress/blob/main/docs/URI_GRAMMAR.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
