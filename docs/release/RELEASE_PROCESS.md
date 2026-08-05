# Manual Release Process

Egress releases are operator-driven. GitHub Actions does not publish crates, build release bundles, create GitHub Releases, push container images, publish Python packages, or react to version tags.

No release cadence is encoded in the repository. A maintainer releases when the code and version are ready.

## Release target

The primary release channel is crates.io using local `cargo publish` commands. Git tags and GitHub Releases are optional bookkeeping performed manually after crates.io publication; they are not release prerequisites or automation triggers.

**Current publication status:** Crates.io publication is blocked. The CLI (`eggress-cli`) is the only intended public product, but it depends on ~20 internal crates by workspace path. All internal crates are marked `publish = false`. Publishing the CLI requires either publishing the internal dependency closure or restructuring crate boundaries. This is a deliberate architectural decision, not an oversight. Publication will resume when a crate-boundary consolidation plan is completed.

Python/PyPI distribution is a separate manual operation and must not be coupled to the Rust release workflow.

## Prerequisites

- A clean local checkout of the intended commit.
- Rust stable and the repository MSRV available.
- crates.io credentials configured with `cargo login` or `CARGO_REGISTRY_TOKEN`.
- The intended version committed in all affected manifests and user-visible version constants.
- Release notes or changelog text prepared when the change warrants it.

Crates.io versions are immutable. A published version cannot be overwritten. If publication partially succeeds or a package is incorrect, fix the repository, increment the version, and publish a new version.

## 1. Verify the release candidate

Run the broad local gate once on the exact release commit:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Run dependency and advisory checks because release inputs are changing:

```bash
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134
```

Run specialized interoperability, Python, performance, or cross-platform checks only when the release contains relevant changes or makes claims that depend on them. The selection policy is in `docs/TESTING.md`.

## 2. Check package metadata

For every crate intended for publication, verify:

- the package version is correct;
- published internal dependencies specify a crates.io version as well as any local path used for workspace development;
- required `description`, `license`, `repository`, `readme`, and include/exclude metadata are present;
- generated package contents do not contain secrets, fixtures, large evidence directories, or development-only artifacts.

Use a dry run for each public crate:

```bash
cargo publish --dry-run -p eggress-cli
```

A dry-run failure is a packaging defect. Fix it before publishing rather than adding CI automation around it.

> **Note:** `cargo publish --dry-run` currently fails because `eggress-cli` depends on internal crates marked `publish = false`. This is expected until crate boundaries are restructured.

## 3. Publish dependency-first

Publish crates in dependency order. Leaf libraries must be available in the crates.io index before crates that depend on them can be published. The CLI or other top-level facade should be published last.

For each crate:

```bash
cargo publish -p eggress-cli
```

Wait for crates.io index propagation before publishing the next dependent crate. Re-run that dependent crate's dry run if resolution is uncertain.

Do not use `--allow-dirty` for a normal release. Do not publish from an unreviewed working tree.

## 4. Verify crates.io installation

Install the published top-level package into a clean temporary location:

```bash
cargo install eggress-cli --version <version> --locked --root /tmp/eggress-release-check
/tmp/eggress-release-check/bin/eggress --version
/tmp/eggress-release-check/bin/pproxy --help
```

Use an equivalent temporary directory on platforms where `/tmp` is unavailable.

If the release includes public libraries, create a minimal temporary consumer project and confirm that Cargo resolves the published versions without workspace paths.

## 5. Optional manual repository bookkeeping

After crates.io verification, a maintainer may create and push a tag:

```bash
git tag -a v<version> -m "Release v<version>"
git push origin v<version>
```

A GitHub Release may also be created manually for notes or separately built binaries:

```bash
gh release create v<version> \
  --title "eggress v<version>" \
  --notes-file <release-notes-file>
```

These commands are optional and must remain manual. Pushing a tag must not start publishing, artifact construction, signing, container pushes, or release creation in GitHub Actions.

## Python distribution

Python publication to PyPI is automated through `.github/workflows/publish-python.yml`. Pushing a `v*` tag triggers production publication after version coherence validation. Manual dispatch targets TestPyPI by default. The workflow builds prebuilt wheels for Linux (x86_64, aarch64), macOS (x86_64, arm64), and Windows (x86_64), plus one source distribution, using the Python stable ABI (`abi3-py39`).

Crates.io publication remains manual and is not coupled to the Python release workflow.

## Roll-forward policy

Crates.io publication is irreversible except for yanking. For a defective release:

1. Yank the affected version when appropriate.
2. Correct the defect.
3. Increment the package version.
4. Repeat dry runs and relevant verification.
5. Publish the corrected version.

Do not delete or retag an existing version to simulate replacement.

## Prohibited automation

The following must not be added back without an explicit project-level decision:

- crates.io token storage or trusted-publishing configuration in GitHub Actions;
- automated GitHub Release creation;
- mandatory release artifact, checksum, SBOM, signature, or container jobs;
- a release workflow that repeats the ordinary CI suite;
- release gates that require generated evidence unrelated to the changed release surface.

Release correctness comes from a clean release commit, proportionate local verification, package dry runs, explicit operator publication, and post-publication installation checks.
