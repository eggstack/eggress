# CLI Binary Artifact Matrix (Phase 49)

This document defines the supported CLI binary targets for the eggress
release, their capabilities, and known limitations.

## Supported targets

| Target triple | OS | Arch | TLS backend | Status |
|---|---|---|---|---|
| `x86_64-unknown-linux-gnu` | Linux | x86_64 | rustls (ring) | **Supported** |
| `aarch64-unknown-linux-gnu` | Linux | aarch64 | rustls (ring) | **Supported** |
| `x86_64-apple-darwin` | macOS | x86_64 | rustls (ring) | **Supported** |
| `aarch64-apple-darwin` | macOS | arm64 | rustls (ring) | **Supported** |
| `x86_64-pc-windows-msvc` | Windows | x86_64 | rustls (ring) | **Supported** |

## Deferred targets

| Target triple | Reason |
|---|---|
| `x86_64-unknown-linux-musl` | Static linking with rustls requires ring C assembly; musl cross-compilation deferred until CI supports it |
| `aarch64-unknown-linux-musl` | Same as above |
| `aarch64-pc-windows-msvc` | No CI runner available; Windows ARM64 is not in the release scope |

## Per-target capability matrix

| Capability | Linux x86_64 | Linux aarch64 | macOS x86_64 | macOS arm64 | Windows x86_64 |
|---|---|---|---|---|---|
| TCP proxy (HTTP, SOCKS4/4a, SOCKS5) | ✅ | ✅ | ✅ | ✅ | ✅ |
| UDP direct forwarding | ✅ | ✅ | ✅ | ✅ | ✅ |
| UDP SOCKS5 ASSOCIATE relay | ✅ | ✅ | ✅ | ✅ | ✅ |
| UDP standalone relay | ✅ | ✅ | ✅ | ✅ | ✅ |
| Shadowsocks AEAD TCP (SIP003) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Shadowsocks AEAD UDP | ✅ | ✅ | ✅ | ✅ | ✅ |
| Trojan upstream (TLS) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Reverse/backward proxy | ✅ | ✅ | ✅ | ✅ | ✅ |
| Unix domain socket listener | ✅ | ✅ | ✅ | ✅ | ❌ |
| Transparent TCP proxy (SO_ORIGINAL_DST) | ✅ | ✅ | ❌ | ❌ | ❌ |
| System proxy inspection | ✅ | ✅ | ✅ | ✅ | ✅ |
| System proxy apply (dry-run) | ❌ | ❌ | ✅ | ✅ | ❌ |
| Hot-reload (SIGHUP) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Prometheus metrics | ✅ | ✅ | ✅ | ✅ | ✅ |
| pproxy CLI translation | ✅ | ✅ | ✅ | ✅ | ✅ |

## TLS / root certificates

All binaries use **rustls** with the **ring** crypto provider. Root
certificates are sourced from the `webpki-roots` crate (Mozilla's
certificate bundle). No OpenSSL dependency exists.

- **Linux**: webpki-roots bundled in binary; no system cert store access
- **macOS**: webpki-roots bundled; system keychain not used
- **Windows**: webpki-roots bundled; Windows cert store not used

## Runtime dependencies

| Target | Dependencies |
|---|---|
| Linux (glibc) | glibc >= 2.17 (CentOS 7 era), libgcc_s |
| macOS | macOS 11+ (Big Sur) |
| Windows | Windows 10+ (MSVC runtime) |

No dynamic libraries are required beyond the C runtime. The binary is
self-contained for TLS.

## Binary naming

Release artifacts follow the pattern:

```
eggress-{version}-{target}.tar.gz
```

Contents of each archive:

```
eggress          # (or eggress.exe on Windows)
LICENSE-MIT
LICENSE-APACHE
```

## Verification

```bash
# Check binary runs
./eggress --version

# Check pproxy compatibility
./eggress pproxy check -- -l socks5://:1080

# Verify checksums
sha256sum -c SHA256SUMS
```
