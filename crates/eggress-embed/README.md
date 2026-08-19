# eggress-embed

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Stable Rust embed API for embedding eggress in another Rust application. This is the public surface most downstream Rust users should depend on.

## When to use this crate

Use `eggress-embed` when you want to start, stop, reload, and inspect an eggress proxy from within your own Rust application. This crate provides the `EggressService` and `EggressConfig` types that manage the full proxy lifecycle.

## Feature flags

- `full` (default) — Enables `common`, `extended`, `operations`, `reverse`, `pproxy-compat`, and `pproxy-legacy`.
- `common` — Base protocol set (HTTP, SOCKS4/4a, SOCKS5, raw).
- `extended` — Adds Shadowsocks, Trojan, WebSocket, and Shadowsocks UDP relay.
- `operations` — Enable admin server and system-proxy integration.
- `reverse` — Enable reverse/backward proxy protocol.
- `pproxy-compat` — Enable pproxy URI and CLI flag translation.
- `pproxy-legacy` — Enable pproxy-compatible Shadowsocks compression.
- `legacy-crypto` — Enable legacy Shadowsocks stream ciphers.
- `quic` — Enable QUIC/H3 transport and protocol.
- `ssh` — Enable SSH upstream transport.

## Quick example

```rust
use eggress_embed::{EggressService, EggressConfig};

let config = EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "proxy"
    bind = "127.0.0.1:0"
    protocols = ["socks5"]
"#)?;

let handle = EggressService::new(config).start().await?;
println!("Listening on {:?}", handle.bound_addresses());
handle.shutdown().await?;
```

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Embed API](https://github.com/eggstack/eggress/blob/main/docs/EMBED_API.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/docs/ARCHITECTURE.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
