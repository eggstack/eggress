# CLI Binary Release and Bootstrap Installer

## Status

Implementation plan for handoff. This is Phase 2 of `CLI_CLEANUP_AND_BINARY_DELIVERY_ROADMAP.md`.

## Objective

Add a narrow release-only binary distribution path for `eggress-cli` so users who want the command-line proxy can install prebuilt `eggress` and `pproxy` executables without Rust or Python.

This does **not** replace PyPI. PyPI remains the primary Python/pproxy-migration distribution channel. It also does not automate crates.io publication, which remains an explicit maintainer action.

The preferred standalone Unix CLI install should become:

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash
```

Windows should have an equivalent documented PowerShell bootstrap path.

## Reference implementation to reuse selectively

Use `eggstack/gregg` as a design reference for:

- release-only artifact workflow separated from ordinary CI;
- target-to-asset naming contract;
- Linux portability builds with an explicit glibc floor;
- SHA-256 sidecars;
- bootstrap platform detection;
- pinned-version installer support;
- candidate executable version verification;
- no hidden privilege escalation.

Do not copy Gregg’s daemon/service installer logic, restart semantics, daemon health checks, or generic fleet/release machinery.

Where code is copied, adapt it to Eggress terminology and keep only logic required by this repository. Avoid retaining Gregg-specific abstractions merely because they already exist.

## Distribution contract

### Supported prebuilt targets

Initially publish the same five major platform/architecture combinations already represented by the Python wheel matrix and current project support claims:

| Platform | Rust target | Archive |
|---|---|---|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Linux AArch64 | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | `.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `.zip` |

Do not claim ARMv7, musl, Windows ARM64, FreeBSD, or other targets until artifacts are actually produced and tested.

### Feature set

Release binaries must use the **default `eggress-cli` feature set**, equivalent to normal `cargo install eggress-cli` behavior. Do not silently enable every optional compatibility/legacy feature.

In particular, optional features such as SSH, QUIC/H3, legacy crypto, or pproxy legacy functionality that are not part of the default feature set must remain opt-in source/Cargo build choices unless the project separately changes the default feature policy.

Documentation must state this clearly so the binary installer is not mistaken for a build containing every workspace feature.

### Archive contents

Prefer one archive per target containing both executables from the same build/version:

Unix:

```text
eggress-<target>.tar.gz
  eggress
  pproxy
```

Windows:

```text
eggress-<target>.zip
  eggress.exe
  pproxy.exe
```

Each archive must have:

```text
eggress-<target>.<ext>.sha256
```

Also attach:

```text
install.sh
install.ps1
```

to every successful GitHub Release so `/releases/latest/download/install.sh` and `/install.ps1` remain stable bootstrap URLs.

The release tag supplies the version namespace, so archive names should not duplicate `X.Y.Z` unless implementation evidence shows a concrete need.

### Why archive both binaries together

`eggress-cli` is one crate producing two process surfaces that are intended to remain version-aligned. Publishing them as one archive avoids a common failure mode where an installer or updater replaces `eggress` but leaves an older `pproxy` facade alongside it.

## Phase A — Release preflight and version invariants

Create a reusable release preflight script, e.g. `scripts/release-preflight.sh`, rather than embedding all validation in YAML.

For tag-triggered builds it must hard-fail unless:

1. the tag has the form `vX.Y.Z` (allow semver pre-release only if the existing release policy intentionally supports it);
2. the tag equals `v<workspace.package.version>`;
3. `crates/eggress-python/pyproject.toml` carries the same canonical version expected by the existing release policy;
4. `python-pproxy-compat/pyproject.toml` is aligned where the repository currently requires lockstep versions;
5. internal exact version pins expected by the current release process are aligned;
6. the checked-out commit is exactly the tagged commit;
7. the working tree is clean in local/manual invocation mode.

The binary release workflow must build from the tag commit, never from whatever happens to be the current default-branch head.

For manual workflow dispatch, require an existing tag input and validate it identically. Do not let manual dispatch synthesize an untagged production release.

### Acceptance criteria

- pushing a mismatched tag cannot produce binary release assets;
- manual dispatch against an invalid/mismatched tag fails before expensive matrix builds;
- the same preflight can be run locally by a maintainer.

## Phase B — Add a release-only GitHub Actions workflow

Add a workflow such as:

```text
.github/workflows/release-binaries.yml
```

Triggers:

```yaml
on:
  push:
    tags: ['v*']
  workflow_dispatch:
    inputs:
      tag: ...
```

Permissions should be minimal, normally `contents: write` only for the final release job and read-only/default elsewhere where GitHub permits job-specific permissions.

Ordinary CI must not build this matrix. Do not add release artifact jobs to `.github/workflows/ci.yml`.

A reasonable DAG is:

```text
preflight
  ├─ linux-x86_64
  ├─ linux-aarch64
  ├─ macos-x86_64
  ├─ macos-aarch64
  └─ windows-x86_64
         ↓
     assemble/release
```

Each target job should:

1. check out the exact tag;
2. install the pinned/stable Rust toolchain required by the repository policy;
3. build `eggress-cli` in release mode with default features and `--locked`;
4. stage both binaries;
5. smoke-test version/help on the native runner;
6. package the pair into the target archive;
7. generate SHA-256 after the final archive is created;
8. upload the archive and checksum as workflow artifacts for the release job.

Do not rerun the entire ordinary workspace CI suite in every matrix leg. The release workflow’s job is artifact correctness, not duplicate certification.

## Phase C — Linux portability floor

Do not publish Linux GNU executables tied accidentally to the newest GitHub runner glibc.

Reuse Gregg’s `cargo-zigbuild` + Zig pattern if Eggress’s dependency graph builds correctly under it. Start by validating an explicit floor such as glibc 2.17 for both GNU targets:

```text
x86_64-unknown-linux-gnu.2.17
aarch64-unknown-linux-gnu.2.17
```

Before committing to that floor, verify the full default CLI dependency set, including crypto/TLS and any native dependencies, can actually link and run. If a dependency imposes a higher real floor, document the lowest tested floor instead of forcing 2.17 cosmetically.

Store Zig installation/version logic in a small script if needed rather than duplicating it across Linux jobs.

### Acceptance criteria

- Linux artifact compatibility floor is explicit in release documentation;
- both Linux artifacts are smoke-tested natively or on matching runners after build;
- the workflow does not claim glibc 2.17 unless the resulting executable has been demonstrated to meet it.

## Phase D — Native smoke checks

At minimum, every native artifact job must verify:

```text
eggress version

eggress --help

pproxy --version

pproxy --help
```

Expected version must match the preflight version.

Add one lightweight proxy startup/bind smoke on platforms where it is deterministic and cheap, for example binding an ephemeral localhost listener and confirming clean startup/shutdown. Do not duplicate the full protocol interoperability test suite.

The archive must be created only after candidate binaries pass smoke tests.

### Acceptance criteria

- broken/mis-versioned binaries cannot reach the final release job;
- both binaries in every archive report the expected release version;
- release validation remains proportionate and fast relative to ordinary CI.

## Phase E — Bootstrap `install.sh`

Add a Bash installer, preferably under `packaging/install.sh`, and attach its exact release copy as `install.sh`.

It should be derived from the small, reusable parts of Gregg’s installer and implement only Eggress needs.

### Supported behavior

```bash
./packaging/install.sh
./packaging/install.sh --version X.Y.Z
./packaging/install.sh --dir /custom/bin
```

The curl path with no arguments must install the latest stable binary release for the detected target.

### Host detection

Map:

```text
Linux + x86_64/amd64   -> x86_64-unknown-linux-gnu
Linux + aarch64/arm64  -> aarch64-unknown-linux-gnu
Darwin + x86_64        -> x86_64-apple-darwin
Darwin + arm64/aarch64 -> aarch64-apple-darwin
```

Unsupported hosts should fail with a concise message and the documented Cargo/source-build alternative. Do not silently compile from source in the first binary-first installer implementation.

This differs intentionally from Gregg’s Cargo fallback: Eggress’s standalone binary path should remain deterministic and should not unexpectedly require a Rust toolchain after the user chose the binary installer.

### Destination policy

Default:

- root: `/usr/local/bin`;
- non-root: `$HOME/.local/bin`.

Allow `--dir <PATH>` for explicit override.

Never invoke `sudo` internally. If the selected directory is not writable, fail and show an explicit command the user can choose to rerun with appropriate privilege or a user-writable `--dir`.

If `$HOME/.local/bin` is absent from PATH, print an advisory; do not mutate shell startup files automatically.

### Download and verification flow

For latest:

- resolve GitHub’s latest stable release URL/redirect or use the stable `/releases/latest/download/<asset>` path;
- download the target archive and matching `.sha256`.

For `--version X.Y.Z`:

- use exact tag `vX.Y.Z`;
- never silently fall forward to latest.

Then:

1. create a secure temporary directory;
2. download archive and checksum;
3. calculate SHA-256 using `sha256sum` or `shasum -a 256`;
4. fail on mismatch before extraction/install;
5. extract archive;
6. make Unix binaries executable;
7. execute staged `eggress version` and staged `pproxy --version`;
8. verify both report the release version and agree;
9. install both to the destination;
10. print installed paths and version;
11. clean temporary state on exit.

Do not verify only one member of the archive.

### Bootstrap shell safety

Because the documented command pipes into Bash, include an early POSIX-safe Bash guard so accidental `| sh` usage emits a useful error before Bash-specific syntax fails.

Use `set -euo pipefail` after the guard.

Quote all paths. Handle spaces in `$HOME`/custom install directories.

### Integrity language

SHA-256 downloaded from the same GitHub Release detects corruption or mismatched assets; it is not an independent signature/provenance guarantee. Documentation and comments must not claim otherwise.

Signing/Sigstore/SBOM work is outside this plan.

## Phase F — Bootstrap `install.ps1`

Add `packaging/install.ps1` and attach it as `install.ps1`.

Supported initial target is Windows x86_64.

Provide parameters equivalent to the Bash installer where practical:

```powershell
- Version X.Y.Z
- InstallDir <path>
```

Default to a user-writable binary location consistent with project documentation rather than requiring Administrator rights. If a system-wide location is desired, the user must explicitly run an elevated shell or specify a destination.

The PowerShell installer must:

- download the `.zip` and `.sha256`;
- use built-in SHA-256 facilities (`Get-FileHash`);
- extract to a temporary directory;
- verify both candidate versions;
- place `eggress.exe` and `pproxy.exe` together;
- avoid automatic PATH registry/profile mutations unless explicitly approved later;
- provide PATH advice when needed.

Document a concise PowerShell one-liner, but do not present `irm | iex` as more secure than it is; link to the auditable installer script as well.

## Phase G — Create/attach the GitHub Release

The current Eggress release policy forbids automated GitHub Release creation. This plan intentionally supersedes that narrow restriction for **canonical CLI binary artifacts only**.

The final release job may use GitHub CLI/API to:

1. create the GitHub Release for the existing tag if it does not exist, or update the release created by the same workflow;
2. attach all five archives;
3. attach all five checksum files;
4. attach `install.sh` and `install.ps1`;
5. publish only after all required target jobs succeed.

Prefer built-in `gh`/GitHub API over introducing an unnecessary third-party release action. If a third-party action is selected, pin it to a commit SHA and justify it in the workflow comments.

Do not create or move tags from the workflow.

Do not publish crates.io packages from this workflow.

### Interaction with Python publishing

The existing `publish-python.yml` remains independent and continues to run on the same `v*` tag.

The release procedure must require maintainers to verify both outcomes:

- Python workflow succeeded and PyPI exposes the tagged version;
- binary workflow succeeded and GitHub Release exposes the tagged assets.

Do not make the binary workflow republish Python packages or vice versa.

## Phase H — Installer/release tests

Add focused tests/scripts rather than a large packaging framework.

Required coverage:

- target mapping tests;
- archive naming tests;
- checksum success;
- checksum mismatch failure;
- requested-version mismatch failure;
- candidate `eggress` version mismatch failure;
- candidate `pproxy` version mismatch failure;
- unsupported target error;
- non-writable destination behavior;
- custom `--dir` behavior;
- version-pinned URL selection;
- Bash syntax check (`bash -n`);
- PowerShell parser/smoke on a Windows runner;
- release job refuses missing target artifacts.

Installer tests should use local fixtures or a small mock HTTP/release endpoint where practical. Do not burn real production releases merely to exercise installer logic.

## Phase I — Release-policy integration dependency

The implementation must land together with the release-documentation changes in `CLI_DISTRIBUTION_DOCUMENTATION_AND_RELEASE_POLICY.md` so the repository does not simultaneously contain a binary release workflow and documentation declaring such a workflow prohibited.

## Final acceptance criteria

This plan is complete when:

1. a `vX.Y.Z` tag can produce five supported target archives containing both `eggress` and `pproxy` from the same version;
2. each archive has a verified SHA-256 sidecar;
3. Linux artifacts have an explicit tested glibc compatibility floor;
4. release artifacts use default `eggress-cli` features unless the default feature policy itself changes separately;
5. native artifact jobs smoke-test both binaries before packaging;
6. the final GitHub Release is created/updated only after all required artifact jobs succeed;
7. `install.sh` is attached to the release and works through the documented `/releases/latest/download/install.sh` curl command;
8. `install.ps1` is attached and provides the documented Windows bootstrap path;
9. installers verify archive checksum and both candidate executable versions before installation;
10. installers place `eggress` and `pproxy` together at the same version;
11. installers never invoke `sudo` or silently modify shell/registry PATH state;
12. unsupported platforms fail clearly and point to Cargo/source installation rather than silently compiling;
13. ordinary CI does not build the release matrix;
14. crates.io publication remains manual;
15. Python/PyPI publishing remains a separate unchanged first-class channel;
16. the repository release policy is updated to explicitly allow this narrow binary artifact automation.
