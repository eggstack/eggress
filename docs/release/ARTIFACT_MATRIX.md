# Release Artifact Matrix (Phase 49)

This document lists every artifact produced by a full eggress release and
where to find it.

## Artifact categories

### 1. Source distribution

| Artifact | Format | Source |
|---|---|---|
| `eggress-{version}-src.tar.gz` | tar.gz | `git archive` or GitHub auto-generated |

### 2. CLI binary archives

Each archive contains the `eggress` binary, license files, and is
gzip-compressed.

| Artifact | Target triple | Runner |
|---|---|---|
| `eggress-{version}-x86_64-unknown-linux-gnu.tar.gz` | `x86_64-unknown-linux-gnu` | `ubuntu-latest` |
| `eggress-{version}-aarch64-unknown-linux-gnu.tar.gz` | `aarch64-unknown-linux-gnu` | `ubuntu-latest` (cross) |
| `eggress-{version}-x86_64-apple-darwin.tar.gz` | `x86_64-apple-darwin` | `macos-13` |
| `eggress-{version}-aarch64-apple-darwin.tar.gz` | `aarch64-apple-darwin` | `macos-latest` |
| `eggress-{version}-x86_64-pc-windows-msvc.zip` | `x86_64-pc-windows-msvc` | `windows-latest` |

### 3. Python wheels

Built by maturin for Python 3.9--3.14.

| Artifact pattern | Platform |
|---|---|
| `eggress-{version}-cp3*-manylinux_2_17_x86_64.manylinux2014_x86_64.whl` | Linux x86_64 |
| `eggress-{version}-cp3*-manylinux_2_17_aarch64.manylinux2014_aarch64.whl` | Linux aarch64 |
| `eggress-{version}-cp3*-macosx_*_x86_64.whl` | macOS x86_64 |
| `eggress-{version}-cp3*-macosx_*_arm64.whl` | macOS arm64 |
| `eggress-{version}-cp3*-win_amd64.whl` | Windows x86_64 |

### 4. Python source distribution

| Artifact | Format |
|---|---|
| `eggress-{version}.tar.gz` | sdist (requires Rust toolchain to install) |

### 5. Checksums and verification

| Artifact | Contents |
|---|---|
| `SHA256SUMS` | SHA-256 hashes for every binary, wheel, and sdist |
| `SHA256SUMS.sig` | Detached signature (if signing is enabled) |

### 6. SBOM

| Artifact | Format |
|---|---|
| `sbom.json` | CycloneDX or SPDX SBOM for all binary/wheel artifacts |

### 7. Container image

| Artifact | Registry |
|---|---|
| `ghcr.io/{owner}/eggress:{version}` | GitHub Container Registry |
| `ghcr.io/{owner}/eggress:latest` | GitHub Container Registry (latest stable) |

## Artifact download locations

- **GitHub Release**: All binaries, wheels, sdist, checksums, SBOM, release notes
- **PyPI**: Python wheels and sdist (`pip install eggress=={version}`)
- **GitHub Container Registry**: Container image (`docker pull ghcr.io/{owner}/eggress:{version}`)

## Checksum verification

```bash
# Download SHA256SUMS from the GitHub Release
# Verify all artifacts
sha256sum -c SHA256SUMS

# Or verify a single artifact
sha256sum eggress-0.1.0-x86_64-unknown-linux-gnu.tar.gz
# Compare with the corresponding line in SHA256SUMS
```

## Integrity chain

```
git tag v0.1.0
  → GitHub Release created
    → binaries built and checksummed
    → wheels built and checksummed
    → SBOM generated
    → SHA256SUMS file uploaded
    → container image built and pushed
```
