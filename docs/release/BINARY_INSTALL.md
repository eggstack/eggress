# Installing Pre-Built Binaries (Phase 49)

This document explains how to install and verify pre-built `eggress` CLI
binaries from GitHub Releases.

## Quick install

### Linux (x86_64)

```bash
# Download and extract
curl -L https://github.com/{owner}/eggress/releases/download/v0.1.0/eggress-0.1.0-x86_64-unknown-linux-gnu.tar.gz | tar xz

# Move to PATH
sudo mv eggress /usr/local/bin/

# Verify
eggress --version
```

### Linux (aarch64)

```bash
curl -L https://github.com/{owner}/eggress/releases/download/v0.1.0/eggress-0.1.0-aarch64-unknown-linux-gnu.tar.gz | tar xz
sudo mv eggress /usr/local/bin/
eggress --version
```

### macOS (arm64, Apple Silicon)

```bash
curl -L https://github.com/{owner}/eggress/releases/download/v0.1.0/eggress-0.1.0-aarch64-apple-darwin.tar.gz | tar xz
sudo mv eggress /usr/local/bin/
eggress --version
```

### macOS (x86_64, Intel)

```bash
curl -L https://github.com/{owner}/eggress/releases/download/v0.1.0/eggress-0.1.0-x86_64-apple-darwin.tar.gz | tar xz
sudo mv eggress /usr/local/bin/
eggress --version
```

### Windows (x86_64)

Download `eggress-0.1.0-x86_64-pc-windows-msvc.zip` from the GitHub
Release, extract, and add to your `PATH`.

## Verifying checksums

Every release includes a `SHA256SUMS` file with hashes for all artifacts.

```bash
# Download the checksums file
curl -LO https://github.com/{owner}/eggress/releases/download/v0.1.0/SHA256SUMS

# Verify all artifacts
sha256sum -c SHA256SUMS

# Or verify a single file
grep eggress-0.1.0-x86_64-unknown-linux-gnu.tar.gz SHA256SUMS | sha256sum -c -
```

## First run

```bash
# Check version
eggress --version

# Run a simple HTTP proxy
eggress -l http://:8080

# Translate a pproxy command
eggress pproxy translate -- -l socks5://:1080 -r http://proxy:8080

# Check pproxy compatibility
eggress pproxy check -- -l socks5://:1080 -r http://proxy:8080
```

## TOML configuration

For advanced configurations, create a TOML config file:

```toml
# config.toml
[[listeners]]
protocol = "socks5"
bind = ":1080"

[[upstreams]]
name = "default"
uri = "direct://"

[routing.rules]
default = "default"
```

```bash
eggress --config config.toml
```

See [CONFIG_REFERENCE.md](../CONFIG_REFERENCE.md) for the full schema.

## Python package

If you need the Python bindings instead of the CLI binary:

```bash
pip install eggress
```

See [PYPI_RELEASE.md](PYPI_RELEASE.md) for details.

## Runtime requirements

| Platform | Minimum version |
|---|---|
| Linux (glibc) | glibc 2.17+ (CentOS 7 era) |
| macOS | macOS 11+ (Big Sur) |
| Windows | Windows 10+ |

No additional dependencies are required. The binary is self-contained with
rustls for TLS.

## Troubleshooting

### Permission denied

```bash
chmod +x eggress
```

### macOS: "app is damaged" or Gatekeeper block

```bash
xattr -d com.apple.quarantine eggress
```

### Linux: "no such file or directory" on aarch64

You may have downloaded the wrong architecture. Check with:

```bash
uname -m  # x86_64 or aarch64
```
