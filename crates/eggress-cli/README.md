# eggress-cli

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Command-line interface: the `eggress` binary and the `pproxy` compatibility binary.

## Installation

Prebuilt GitHub Release binaries are the preferred normal CLI install:

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash
```

This installs both `eggress` and `pproxy` from one version-aligned archive.
Canonical binary releases use the default crate features. See
[docs/INSTALLATION.md](https://github.com/eggstack/eggress/blob/main/docs/INSTALLATION.md)
for Windows, pinned versions, install directories, and checksums.

`cargo install eggress-cli --locked` remains the Rust/developer path (custom
features, unsupported targets, Cargo-managed provenance). Custom feature
builds require Cargo/source.

## When to use this crate

Use `eggress-cli` when you want to run eggress as a standalone proxy from the command line. This crate produces the `eggress` and `pproxy` binaries (both installed by either path).

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

`eggress version` prints the installed release version (`eggress X.Y.Z`);
`eggress update` self-updates a standalone installation from verified GitHub
Release assets (both binaries as one unit; never touches a Python
environment). The `pproxy` compatibility binary retains its flat surface
with `pproxy --version` and does not gain Eggress-native subcommands
(including `update`).

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Installation](https://github.com/eggstack/eggress/blob/main/docs/INSTALLATION.md)
- [Operations](https://github.com/eggstack/eggress/blob/main/docs/OPERATIONS.md)
- [Config reference](https://github.com/eggstack/eggress/blob/main/docs/CONFIG_REFERENCE.md)
- [pproxy migration](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
