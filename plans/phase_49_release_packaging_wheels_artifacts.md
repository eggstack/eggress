# Phase 49: release packaging, wheels, and artifacts

## Goal

Make eggress installable and verifiable as a release artifact across the intended surfaces: Rust crates, standalone CLI binaries, PyPI wheels, and optionally containers. The current parity work is only useful to external users if installation does not require a bespoke Rust toolchain and if release artifacts carry enough metadata for verification and support.

## Release surfaces

### Required

- Standalone CLI binaries for Linux, macOS, and Windows where supported.
- PyPI wheels for the Python package/bindings.
- Source distributions where appropriate.
- Checksums for release artifacts.
- Release notes tied to manifest/report state.
- Reproducible or at least repeatable release procedure.

### Strongly recommended

- SBOM for binaries/wheels.
- Signed artifacts.
- Container image.
- crates.io publishing decision and package metadata review.
- GitHub release workflow with artifact upload.

## Workstream A: Rust binary artifacts

Define the CLI binary matrix:

- Linux x86_64 glibc;
- Linux x86_64 musl if feasible;
- Linux aarch64 glibc/musl if feasible;
- macOS x86_64;
- macOS arm64;
- Windows x86_64 MSVC.

For each target, document:

- supported status;
- TLS/root certificate behavior;
- system-proxy support status;
- transparent proxy support status;
- required runtime dependencies;
- known limitations.

Add release workflow jobs only for supported targets.

## Workstream B: PyPI wheels

Use the existing PyO3/maturin packaging path and harden it for release.

Target wheel matrix:

- manylinux x86_64;
- manylinux aarch64 if practical;
- macOS arm64;
- macOS x86_64 or universal2 if practical;
- Windows x86_64.

Requirements:

- `import eggress` smoke test on every built wheel;
- `egress.start_pproxy` / `PPProxyService.from_args` smoke test if networking is allowed;
- `py.typed` included;
- `.pyi` stubs included;
- metadata classifiers correct;
- package version synchronized with Rust crate version;
- no accidental private credentials or test artifacts in wheel.

## Workstream C: crates.io / Rust library packaging

Audit each crate intended for publication:

- package name;
- version;
- description;
- license;
- repository;
- readme;
- include/exclude patterns;
- feature flags;
- dependency versions;
- MSRV policy if any.

Decide whether all internal crates should publish or only a public facade crate plus CLI.

## Workstream D: container image

If shipping a container:

- minimal image base;
- non-root default user;
- healthcheck endpoint if admin enabled;
- config volume path;
- port documentation;
- multi-arch support if practical;
- SBOM and vulnerability scan.

## Workstream E: SBOM, signing, checksums

Add release steps for:

- SHA256 checksums;
- artifact signing through `cosign`, `gitsign`, or equivalent if accepted;
- SBOM generation, e.g. Syft or cargo-auditable;
- dependency license report;
- vulnerability audit output.

## Workstream F: release docs

Update or create:

- `docs/release/RELEASE_PROCESS.md`
- `docs/release/ARTIFACT_MATRIX.md`
- `docs/release/PYPI_RELEASE.md`
- `docs/release/BINARY_INSTALL.md`
- `docs/release/CONTAINER.md` if applicable
- README install section

Docs must clearly distinguish:

- pproxy-compatible CLI usage;
- native eggress config usage;
- Python embedding usage;
- unsupported/intentional non-parity capabilities from the generated report.

## Workstream G: release workflow

Add or update GitHub Actions:

- build/test matrix;
- wheel build with maturin;
- binary build;
- artifact checksum generation;
- SBOM generation;
- release draft upload;
- PyPI publish gated by tag/manual approval;
- crates.io publish gated by tag/manual approval.

Do not publish automatically from arbitrary pushes.

## Acceptance criteria

- A user can install the Python package from a wheel and run a pproxy-compatible smoke test.
- A user can download the CLI binary and run `eggress --version` and `eggress pproxy check`.
- Release artifact matrix is documented.
- Generated parity report is included or linked from release notes.
- Checksums exist for every binary/wheel artifact.
- Signing/SBOM decision is documented even if not implemented.
- CI produces artifacts in a reproducible release workflow.

## Verification commands

```bash
cargo fmt --all -- --check
cargo test --workspace
python -m pytest python/tests -v
python -m build python/  # if applicable
maturin build --release
python -m pip install target/wheels/*.whl --force-reinstall
python -c "import eggress; print(egress.version() if hasattr(egress, 'version') else eggress.__version__)"
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

Adjust commands to the actual packaging layout.

## Non-goals

- Do not publish crates/PyPI packages without explicit release approval.
- Do not claim support for targets not built and smoke-tested.
- Do not require users to compile Rust just to use the Python package.
