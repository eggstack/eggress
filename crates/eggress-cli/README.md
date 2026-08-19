# eggress-cli

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Command-line interface: the `eggress` binary and the `pproxy` compatibility binary. Install via `cargo install eggress-cli`.

## When to use this crate

Use `eggress-cli` when you want to run eggress as a standalone proxy from the command line. This crate produces the `eggress` and `pproxy` binaries.

## Feature flags

- `full` (default) — Enables `common`, `extended`, `operations`, `reverse`, and `pproxy-compat`.
- `common` — Base protocol set (HTTP, SOCKS4/4a, SOCKS5, raw).
- `extended` — Adds Shadowsocks, Trojan, WebSocket, and Shadowsocks UDP relay.
- `operations` — Enable admin server and system-proxy integration.
- `reverse` — Enable reverse/backward proxy protocol.
- `pproxy-compat` — Enable the `pproxy` compatibility binary.
- `pproxy-daemon` — Enable pproxy daemon mode (implies `pproxy-compat`).
- `pproxy-legacy` — Enable pproxy-compatible Shadowsocks compression.
- `legacy-crypto` — Enable legacy Shadowsocks stream ciphers.
- `quic` — Enable QUIC/H3 transport and protocol.
- `ssh` — Enable SSH upstream transport.

## Quick example

```bash
eggress -l socks5://:1080 -r http://proxy.example:8080
pproxy -l http://:8080 -r socks5://proxy:1080
```

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Operations](https://github.com/eggstack/eggress/blob/main/docs/OPERATIONS.md)
- [Config reference](https://github.com/eggstack/eggress/blob/main/docs/CONFIG_REFERENCE.md)
- [pproxy migration](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
