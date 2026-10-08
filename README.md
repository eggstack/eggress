# eggress

[![crates.io](https://img.shields.io/crates/v/eggress-core.svg)](https://crates.io/crates/eggress-core)
[![downloads](https://img.shields.io/crates/d/eggress-core.svg)](https://crates.io/crates/eggress-core)
[![docs](https://img.shields.io/docsrs/eggress-cli.svg)](https://docs.rs/eggress-cli)
[![license](https://img.shields.io/crates/l/eggress-cli.svg)](https://github.com/eggstack/eggress/blob/main/LICENSE)
[![PyPI](https://img.shields.io/pypi/v/eggress.svg)](https://pypi.org/project/eggress/)
[![PyPI Downloads](https://static.pepy.tech/personalized-badge/eggress?period=total&units=INTERNATIONAL_SYSTEM&left_color=BLACK&right_color=GREEN&left_text=downloads)](https://pepy.tech/projects/eggress)

A Rust-native, embeddable, multi-protocol proxy framework and CLI with practical compatibility with Python `pproxy`.

## Installation

```bash
pip install eggress                              # Python package
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash   # standalone eggress + pproxy binaries
cargo install eggress-cli --locked               # Rust/developer build (MSRV 1.89)
```

Details (platforms, Python versions, checksums, troubleshooting): [docs/INSTALLATION.md](https://github.com/eggstack/eggress/blob/main/docs/INSTALLATION.md).

## Quickstart

Start a SOCKS5 listener and send traffic through it:

```bash
eggress -l socks5://:1080
curl -x socks5://127.0.0.1:1080 http://example.com/
```

Mixed-protocol listeners, authenticated listeners, and upstream chains:

```bash
eggress -l http+socks5://:8080
eggress -l http+socks5://user:pass@:8080
eggress -l socks5://:1080 -r http://proxy.example:8080
eggress -l socks5://:1080 -r socks5://hop1:1080__http://hop2:8080
```

Verify with `eggress version`. Standalone installs self-update with `eggress update`.

The `pproxy` compatibility binary ships alongside `eggress`:

```bash
pproxy -l http://:8080 -r socks5://proxy:1080
eggress pproxy translate -- -l http://:8080 -r socks5://proxy:1080   # print TOML, start nothing
eggress pproxy check -- -l socks5://:1080 -r http://proxy:8080       # compatibility report
```

Full CLI reference, TOML config, reload, admin, and system-proxy: [docs/OPERATIONS.md](https://github.com/eggstack/eggress/blob/main/docs/OPERATIONS.md).

## Python

```python
from eggress import EggressService

toml = """
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
"""

with EggressService.from_toml(toml).start() as handle:
    print("Listening on", handle.bound_addresses)
```

pproxy-shaped helpers (`start_pproxy`, `eggress.pproxy.PPProxyService`) and the opt-in top-level `pproxy` distribution (`pip install ./python-pproxy-compat`, never alongside upstream `pproxy`):

```python
from eggress import start_pproxy

with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)

from eggress.pproxy import Server

server = Server(listen=["socks5://:1080"], remote=["http://proxy:8080"])
server.start()
server.close()
```

Full reference: [docs/PYTHON_BINDINGS.md](https://github.com/eggstack/eggress/blob/main/docs/PYTHON_BINDINGS.md).

## Rust

```toml
[dependencies]
eggress-embed = "1"
```

```rust
use eggress_embed::{EggressService, EggressConfig};

let config = EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "socks"
    bind = "127.0.0.1:0"
    protocols = ["socks5"]
"#)?;

let handle = EggressService::new(config).start_blocking()?;
println!("SOCKS5 listening on {}", handle.bound_addresses().listener("socks").unwrap());
handle.shutdown_blocking()?;
```

For listener-free outbound chains use `eggress-outbound` directly (`eggress_embed::outbound::*` re-exports the same API); for raw byte relay use `eggress-relay`. Full reference: [docs/EMBED_API.md](https://github.com/eggstack/eggress/blob/main/docs/EMBED_API.md). The published surface is classified in [docs/RUST_API.md](https://github.com/eggstack/eggress/blob/main/docs/RUST_API.md).

## pproxy compatibility

Behavioral compatibility targets pinned `pproxy==2.7.9`. Per-feature truth lives in the [compatibility matrix](https://github.com/eggstack/eggress/blob/main/docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md) and [capability manifest](https://github.com/eggstack/eggress/blob/main/docs/parity/pproxy_capability_manifest.toml) (`matched` / `supported_difference` / `platform_limited` / `intentional_non_parity`). Notable boundaries: SSH and QUIC/HTTP/3 are opt-in features, SSR and legacy ciphers are opt-in compat paths, `--daemon` is Linux-only, SOCKS BIND is refused, HTTPS uses CONNECT tunneling (no MITM). Migrating: [docs/PPROXY_MIGRATION.md](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md).

## Project structure

```text
eggress/
├── crates/               # Workspace crates (core, cli, server, runtime, protocols, transport, etc.)
├── architecture/         # Per-component architecture deep dives + overview index
├── compat/               # Upstream oracle definition and fixtures
├── fuzz/                 # Fuzz harness smoke targets
├── benches/              # Criterion benchmarks
├── tests/                # Cross-implementation tests (tests/compat)
├── scripts/              # Helper and validation scripts
├── python/               # Canonical Python package source (python/eggress)
├── python-pproxy-compat/ # Opt-in distribution owning the top-level `pproxy` namespace
├── docs/                 # Documentation, parity manifests, and release artifacts
```

## Documentation

| Topic | Link |
|-------|------|
| Installation | [docs/INSTALLATION.md](https://github.com/eggstack/eggress/blob/main/docs/INSTALLATION.md) |
| Operations (CLI, config, reload, admin) | [docs/OPERATIONS.md](https://github.com/eggstack/eggress/blob/main/docs/OPERATIONS.md) |
| Embed API | [docs/EMBED_API.md](https://github.com/eggstack/eggress/blob/main/docs/EMBED_API.md) |
| Python bindings | [docs/PYTHON_BINDINGS.md](https://github.com/eggstack/eggress/blob/main/docs/PYTHON_BINDINGS.md) |
| pproxy migration | [docs/PPROXY_MIGRATION.md](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md) |
| Compatibility matrix / manifest | [docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md](https://github.com/eggstack/eggress/blob/main/docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md) |
| Config reference / URI grammar | [docs/CONFIG_REFERENCE.md](https://github.com/eggstack/eggress/blob/main/docs/CONFIG_REFERENCE.md) / [docs/URI_GRAMMAR.md](https://github.com/eggstack/eggress/blob/main/docs/URI_GRAMMAR.md) |
| Architecture deep dives | [architecture/overview.md](https://github.com/eggstack/eggress/blob/main/architecture/overview.md) |
| Testing / metrics / security | [docs/TESTING.md](https://github.com/eggstack/eggress/blob/main/docs/TESTING.md) / [docs/METRICS.md](https://github.com/eggstack/eggress/blob/main/docs/METRICS.md) / [docs/SECURITY_REVIEW.md](https://github.com/eggstack/eggress/blob/main/docs/SECURITY_REVIEW.md) |
| Release process / roadmap | [docs/release/RELEASE_PROCESS.md](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md) / [docs/ROADMAP.md](https://github.com/eggstack/eggress/blob/main/docs/ROADMAP.md) |
