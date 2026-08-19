# eggress

A Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical and behavioral parity with Python `pproxy`.

> **Status:** The Rust-native CLI and runtime are production-ready. Eggress provides broad, behavior-oriented compatibility with `pproxy==2.7.9` across HTTP/SOCKS, encrypted-proxy, routing, CLI, UDP, reverse, optional SSH/QUIC, and Python workflows. See the [compatibility matrix](https://github.com/eggstack/eggress/blob/main/docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md) and [capability manifest](https://github.com/eggstack/eggress/blob/main/docs/parity/pproxy_capability_manifest.toml).

## Design goals

- Nearly identical common CLI usage to `pproxy`
- Mixed-protocol listeners
- Arbitrary compatible multi-hop proxy chains
- TCP and UDP
- Secure defaults with explicit legacy compatibility
- Embeddable Rust library
- Pure Rust dependencies wherever practical
- Differential interoperability tests against Python `pproxy`
- Linux, macOS, and Windows support where the underlying capability exists

## Installation

### CLI

```bash
cargo install --path crates/eggress-cli
```

This installs both the `eggress` and `pproxy` binaries. The workspace declares Rust **MSRV 1.85**.

### Lean local build

```bash
# Lean HTTP/SOCKS local proxy
cargo build -p eggress-cli --release --no-default-features --features common

# Optional smallest optimization profile
cargo build -p eggress-cli --profile release-small --no-default-features --features common

# Optional pproxy legacy crypto and Linux daemon compatibility
cargo build -p eggress-cli --features legacy-crypto,pproxy-daemon
```

### Rust library

```toml
[dependencies]
eggress-embed = { path = "crates/eggress-embed" }
```

### Python package

```bash
pip install eggress

# For AEAD cipher support:
pip install "eggress[cipher-api]"
```

Programs using the bounded public `pproxy` API can keep `import pproxy` after installing Eggress. Uninstall the upstream `pproxy` distribution first because both wheels provide the same import namespace.

Supported Python versions: 3.9, 3.10, 3.11, 3.12, 3.13. Prebuilt wheels available for Linux x86_64/aarch64, macOS x86_64/arm64, and Windows x86_64.

## CLI usage

```text
eggress -l http://:8080
eggress -l socks4://:1080
eggress -l socks5://:1080
eggress -l http+socks4+socks5://:8080
eggress -l http+socks5://user:pass@:8080
eggress -r http://proxy.example:8080
eggress -r socks5://proxy.example:1080
eggress -r socks5://hop1:1080__http://hop2:8080
```

SSH upstreams (opt-in, requires `ssh` feature):

```bash
cargo run -p eggress-cli --features ssh -- -r ssh://user:password@ssh.example:22
cargo run -p eggress-cli --features ssh -- -r ssh://user::/path/to/id_ed25519@ssh.example
```

The `pproxy` compatibility binary is also available:

```bash
pproxy -l http://:8080 -r socks5://proxy:1080
pproxy translate -- -l http://:8080 -r socks5://proxy:1080
pproxy check -- -l socks5://:1080 -r http://proxy:8080
```

See the [operations guide](https://github.com/eggstack/eggress/blob/main/docs/OPERATIONS.md) for full CLI reference, TOML configuration, reload behavior, admin endpoints, and system-proxy integration.

## Rust library

Use `eggress-embed` to embed the proxy in another Rust application.

### Blocking usage

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
let addrs = handle.bound_addresses();
println!("SOCKS5 listening on {}", addrs.listener("socks").unwrap());
handle.shutdown_blocking()?;
```

### Async usage

```rust
use eggress_embed::{EggressService, EggressConfig};

let config = EggressConfig::from_toml_str(r#"
    version = 1

    [[listeners]]
    name = "http"
    bind = "127.0.0.1:0"
    protocols = ["http"]
"#)?;

let handle = EggressService::new(config).start().await?;
println!("generation: {}", handle.status().generation);
handle.shutdown().await?;
```

### Hot-reload

```rust
match handle.reload_toml_str(new_config) {
    Ok(eggress_embed::ReloadOutcome::Applied { generation, upstreams }) => {
        println!("reloaded: generation={generation}, upstreams={upstreams}");
    }
    Err(e) => eprintln!("reload failed: {e}"),
}
```

See the [Embed API reference](https://github.com/eggstack/eggress/blob/main/docs/EMBED_API.md) for full API docs, lifecycle details, feature groups, and limitations.

## Python library

### Context manager (recommended)

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
# service is shut down automatically
```

### Starting from pproxy arguments

```python
from eggress import start_pproxy

with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)
```

### pproxy compatibility API

```python
from eggress.pproxy import PPProxyService, Server

with PPProxyService.from_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
    print(handle.bound_addresses)

server = Server(listen="socks5://:1080", remote="http://proxy:8080")
server.start()
server.close()
```

See the [Python bindings reference](https://github.com/eggstack/eggress/blob/main/docs/PYTHON_BINDINGS.md) for full API docs, async support, Connection object, protocol/cipher objects, error model, and type stubs.

## pproxy compatibility

eggress maintains a behavior-oriented compatibility contract against `pproxy==2.7.9`. The bundled `eggress.pproxy` module provides URI-mode translation, CLI flag translation, compatibility routing, structured diagnostics, differential tests, and a bounded top-level `pproxy` package backed by Eggress adapters.

### Key boundaries

- **SSH listeners** — upstream-only; requires opt-in `ssh` feature
- **QUIC/HTTP/3** — optional behind `quic` feature
- **Legacy ciphers** — `cast5-cfb`, `idea-cfb`, `rc2-cfb`, `seed-cfb` are excluded; other legacy ciphers require `legacy-crypto`
- **SOCKS4/SOCKS5 BIND** — refused (pproxy 2.7.9 also requires CONNECT)
- **TLS interception** — HTTPS uses CONNECT tunneling, not MITM
- **macOS PF transparent proxy** — intentional non-parity

See the [pproxy migration guide](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md), [compatibility matrix](https://github.com/eggstack/eggress/blob/main/docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md), and [capability manifest](https://github.com/eggstack/eggress/blob/main/docs/parity/pproxy_capability_manifest.toml).

## Capabilities

See the [full capability checklist](https://github.com/eggstack/eggress/blob/main/docs/CAPABILITIES.md) for protocol, routing, UDP, TLS, Shadowsocks, Trojan, WebSocket, HTTP/2, reverse proxy, administration, and security status.

## Project structure

```text
eggress/
├── crates/          # Workspace crates (core, cli, server, runtime, protocols, transport, etc.)
├── compat/          # Upstream oracle definition and fixtures
├── fuzz/            # Fuzz harness smoke targets
├── benches/         # Criterion benchmarks
├── tests/           # Cross-implementation tests
├── scripts/         # Helper and validation scripts
├── docs/            # Documentation, parity manifests, and release artifacts
└── plans/           # Historical planning documents
```

## Documentation

| Topic | Link |
|-------|------|
| Architecture | [docs/ARCHITECTURE.md](https://github.com/eggstack/eggress/blob/main/docs/ARCHITECTURE.md) |
| Embed API | [docs/EMBED_API.md](https://github.com/eggstack/eggress/blob/main/docs/EMBED_API.md) |
| Python bindings | [docs/PYTHON_BINDINGS.md](https://github.com/eggstack/eggress/blob/main/docs/PYTHON_BINDINGS.md) |
| pproxy migration | [docs/PPROXY_MIGRATION.md](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_MIGRATION.md) |
| pproxy parity spec | [docs/PPROXY_PARITY_SPEC.md](https://github.com/eggstack/eggress/blob/main/docs/PPROXY_PARITY_SPEC.md) |
| Config reference | [docs/CONFIG_REFERENCE.md](https://github.com/eggstack/eggress/blob/main/docs/CONFIG_REFERENCE.md) |
| URI grammar | [docs/URI_GRAMMAR.md](https://github.com/eggstack/eggress/blob/main/docs/URI_GRAMMAR.md) |
| Testing | [docs/TESTING.md](https://github.com/eggstack/eggress/blob/main/docs/TESTING.md) |
| Metrics | [docs/METRICS.md](https://github.com/eggstack/eggress/blob/main/docs/METRICS.md) |
| Operations | [docs/OPERATIONS.md](https://github.com/eggstack/eggress/blob/main/docs/OPERATIONS.md) |
| Failure semantics | [docs/FAILURE_SEMANTICS.md](https://github.com/eggstack/eggress/blob/main/docs/FAILURE_SEMANTICS.md) |
| Security review | [docs/SECURITY_REVIEW.md](https://github.com/eggstack/eggress/blob/main/docs/SECURITY_REVIEW.md) |
| Secure configuration | [docs/security/SECURE_CONFIGURATION.md](https://github.com/eggstack/eggress/blob/main/docs/security/SECURE_CONFIGURATION.md) |
| Threat model | [docs/security/THREAT_MODEL.md](https://github.com/eggstack/eggress/blob/main/docs/security/THREAT_MODEL.md) |
| Dependency policy | [docs/DEPENDENCY_POLICY.md](https://github.com/eggstack/eggress/blob/main/docs/DEPENDENCY_POLICY.md) |
| Capabilities | [docs/CAPABILITIES.md](https://github.com/eggstack/eggress/blob/main/docs/CAPABILITIES.md) |
| Release process | [docs/release/RELEASE_PROCESS.md](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md) |
| Roadmap | [docs/ROADMAP.md](https://github.com/eggstack/eggress/blob/main/docs/ROADMAP.md) |
