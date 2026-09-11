# Installation

Canonical install and update guide. The root README stays concise and links here.

Eggress has three distribution channels sharing one version/tag invariant
(`vX.Y.Z` == workspace version == Python package versions). Each serves a
different user persona; they are not competing "preferred" paths for the same
user.

| Channel | Artifact | For |
|---|---|---|
| PyPI | `eggress` wheel/sdist | Python users / migrating from `pproxy` |
| GitHub Releases | prebuilt `eggress` + `pproxy` binaries | Standalone command-line proxy users |
| crates.io | Rust crate source (manual publish) | Rust developers / custom builds |

## Python / pproxy migration (primary Python distribution)

```bash
pip install eggress

# For AEAD cipher support:
pip install "eggress[cipher-api]"
```

- This is the primary Python distribution and the first-class path for
  replacing or migrating from `pproxy`.
- Supported Python versions: 3.9, 3.10, 3.11, 3.12, 3.13. Prebuilt wheels are
  published for Linux x86_64/aarch64, macOS x86_64/arm64, and Windows x86_64
  (see `.github/workflows/publish-python.yml` for the exact matrix).
- The `eggress` wheel provides only the `eggress` package. For a bounded,
  Eggress-backed top-level `pproxy` import, additionally install the opt-in
  compatibility distribution from a repository checkout:

  ```bash
  pip install ./python-pproxy-compat
  ```

- Never install upstream `pproxy` and `eggress-pproxy-compat` together; they
  provide the same import namespace. Uninstall upstream `pproxy` first.

## Standalone CLI — preferred binary install

Unix (Linux, macOS):

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash
```

This installs both binaries from one version-aligned release archive:

```text
eggress
pproxy
```

Default locations:

```text
root       -> /usr/local/bin
non-root   -> $HOME/.local/bin
```

Pinned version:

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash -s -- --version 1.0.4
```

Custom directory:

```bash
bash install.sh --dir /custom/bin
bash install.sh --version 1.0.4 --dir "$HOME/.local/bin"
```

Auditable alternative (review before running; piping remote code to a shell
is convenient but never risk-free):

```bash
curl -fsSLO https://github.com/eggstack/eggress/releases/latest/download/install.sh
less install.sh
bash install.sh
```

The installer never invokes `sudo` and never edits shell startup files. If the
destination is not writable it fails with an explicit remediation (rerun with
appropriate privilege or pick a user-writable `--dir`). If
`$HOME/.local/bin` is not in `PATH` it prints an advisory; add it yourself:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### What the installer verifies

1. Downloads the target archive and matching `.sha256` sidecar.
2. Verifies SHA-256 of the complete archive before extraction. The sidecar is
   downloaded from the same GitHub Release: it detects corruption or
   mismatched assets but is not an independent signature or
   publisher-authentication guarantee.
3. Extracts and verifies both staged executables (`eggress version` and
   `pproxy --version` must report the same release version; pinned installs
   must equal the requested version).
4. Installs both binaries together. A failed verification leaves the current
   installation untouched.

Release binaries use the default `eggress-cli` feature set (the same as
`cargo install eggress-cli`). Optional features (`ssh`, `quic`, legacy
crypto, `pproxy-legacy`) are not in the prebuilt binaries; use a Cargo/source
build below when needed.

## Windows standalone install

Initial prebuilt support is Windows x86_64 only.

Auditable use (preferred):

```powershell
Invoke-WebRequest -Uri https://github.com/eggstack/eggress/releases/latest/download/install.ps1 -OutFile install.ps1
Get-Content install.ps1   # review before running
powershell -ExecutionPolicy Bypass -File install.ps1
```

Pinned version or custom directory:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 -Version 1.0.4
powershell -ExecutionPolicy Bypass -File install.ps1 -InstallDir "$env:USERPROFILE\.local\bin"
```

Convenience one-liner (runs remote code; review the script above first):

```powershell
irm https://github.com/eggstack/eggress/releases/latest/download/install.ps1 | iex
```

The default is a user-writable location (`$env:USERPROFILE\.local\bin`) so no
Administrator rights are needed. For a system-wide location, rerun from an
elevated shell with `-InstallDir`. The installer verifies the archive
SHA-256 (via `Get-FileHash`) and both staged executable versions before
installing, places `eggress.exe` and `pproxy.exe` together, and never mutates
PATH registry/profile state automatically (it prints PATH advice when needed).

## Cargo installation (Rust/developer path)

```bash
cargo install eggress-cli --locked
```

Appropriate for Rust users, unsupported prebuilt targets, Cargo-managed
installation provenance, and custom builds/features. This installs both the
`eggress` and `pproxy` binaries. The workspace declares Rust MSRV 1.85.

From a repository checkout:

```bash
cargo install --path crates/eggress-cli
```

## Custom feature / source builds

Canonical release binaries use default `eggress-cli` features. Build from
source when the deployment needs opt-in features or an unsupported target:

```bash
# Lean HTTP/SOCKS local proxy
cargo build -p eggress-cli --release --no-default-features --features common

# Optional smallest optimization profile
cargo build -p eggress-cli --profile release-small --no-default-features --features common

# Optional SSH upstream transport
cargo build -p eggress-cli --features ssh

# Optional QUIC/H3
cargo build -p eggress-cli --features quic

# Optional pproxy legacy crypto and Linux daemon compatibility
cargo build -p eggress-cli --features legacy-crypto,pproxy-daemon
```

See `crates/eggress-cli/README.md` for the full feature list.

## Updating

There is no background update check and no `eggress update` self-update
command in this release. Update by re-running the installer (latest or
pinned); it replaces `eggress` and `pproxy` together after re-verifying
checksums and staged versions:

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash
```

Python installs are separate and unaffected by the binary installer:

```bash
python -m pip install --upgrade eggress
```

Cargo-managed installs stay Cargo-managed if preferred:

```bash
cargo install eggress-cli --locked --force
```

## Version inspection

```bash
eggress version
pproxy --version
```

Both binaries ship in one version-aligned archive per target. `eggress
version` prints `eggress X.Y.Z` deterministically with no network access;
`eggress --version` remains functional. The standalone `pproxy` binary keeps
its pproxy-style `--version` surface (`eggress-pproxy-compat X.Y.Z`).

## Supported prebuilt matrix

| Platform | Rust target | Archive |
|---|---|---|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `eggress-x86_64-unknown-linux-gnu.tar.gz` |
| Linux AArch64 | `aarch64-unknown-linux-gnu` | `eggress-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | `eggress-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `eggress-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `eggress-x86_64-pc-windows-msvc.zip` |

Each archive contains both executables from the same build/version plus a
`.sha256` sidecar. `install.sh` and `install.ps1` are attached to every
release so `/releases/latest/download/install.sh` stays stable.

Linux GNU artifacts target an explicit glibc 2.17 floor (via cargo-zigbuild +
Zig, matching the manylinux2014 Python wheel floor) rather than whatever
glibc the build runner happens to provide.

## Troubleshooting

- `$HOME/.local/bin` not in `PATH`: the installer prints an advisory; add the
  directory to `PATH` yourself. Shell startup files are never edited.
- Unsupported platform/architecture: the installer fails with the
  Cargo/source-build alternative instead of compiling from source.
- Checksum/version verification failure: the installer exits before touching
  the current installation. Re-run; if it persists, the downloaded asset does
  not match the release (network issue or mismatched mirror).
- Destination not writable: the installer never escalates privilege. Rerun
  with a user-writable `--dir` / `-InstallDir`, or from a shell with write
  access to the destination.
- Need for optional features (`ssh`, `quic`, legacy crypto, SSR plugins) not
  present in the default binary: use a Cargo/source build with the documented
  opt-in features.
- `eggress update` not found: expected — standalone updates are installer
  re-runs in this release; Python users update with `pip`.
