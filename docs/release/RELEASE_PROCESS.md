# Manual Release Process

Egress releases are operator-driven. GitHub Actions does not publish crates, build release bundles, create GitHub Releases, push container images, or react to ordinary pushes.

The one automated publishing path is Python: `.github/workflows/publish-python.yml` fires on every `v*` tag push and publishes the `eggress` wheel and sdist to PyPI through the protected `pypi` GitHub environment. Pushing a version tag is therefore a deliberate release action, not bookkeeping.

No release cadence is encoded in the repository. A maintainer releases when the code and version are ready.

## Release target

The primary Rust release channel is crates.io using local `cargo publish` commands. Git tags trigger the Python publish workflow (see below); a GitHub Release may optionally be created manually after publication.

Python/PyPI distribution is a separate manual operation and must not be coupled to the Rust release workflow.

## Prerequisites

- A clean local checkout of the intended commit.
- Rust stable and the repository MSRV available.
- crates.io credentials configured with `cargo login` or `CARGO_REGISTRY_TOKEN`.
- The intended version committed in all affected manifests and user-visible version constants. The version moves in lockstep across: `[workspace.package]` `version` in the root `Cargo.toml`, every internal `=x.y.z` pin under `[workspace.dependencies]`, `crates/eggress-python/pyproject.toml`, and `python-pproxy-compat/pyproject.toml`.
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
cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071
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
cargo publish --dry-run -p <crate-name>
```

A dry-run failure is a packaging defect. Fix it before publishing rather than adding CI automation around it.

## 3. Publish dependency-first

Publish crates in dependency order. Leaf libraries must be available in the crates.io index before crates that depend on them can be published. The CLI or other top-level facade should be published last.

For each crate:

```bash
cargo publish -p <crate-name>
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

## 5. Tagging and the Python publish workflow

A `v*` tag push is a release action: it triggers `.github/workflows/publish-python.yml`.

The workflow:
1. Hard-fails unless the tag equals the workspace version (`v<version>` where `<version>` is `[workspace.package]` `version` in the root `Cargo.toml`).
2. Builds five abi3 wheels (Linux x86_64/aarch64, macOS x86_64/arm64, Windows x86_64) plus one sdist.
3. Smoke-tests each artifact in a clean environment (`scripts/release_artifact_smoke.py`, which imports both `eggress` and the top-level `pproxy` namespace from the opt-in compat package).
4. Publishes to PyPI through the protected `pypi` GitHub environment via OIDC trusted publishers. TestPyPI is available only through manual workflow dispatch with `publish_target=testpypi`.

```bash
git tag -a v<version> -m "Release v<version>"
git push origin v<version>
```

Verify publication:

```bash
gh run list --workflow=publish-python.yml --limit=1
curl -s https://pypi.org/pypi/eggress/<version>/json | python -m json.tool | head -5
```

Do not push a production tag solely to test the workflow; use manual dispatch against TestPyPI with a version that is safe to reuse there.

## 6. Optional manual repository bookkeeping

After crates.io verification, a maintainer may create a GitHub Release for notes or separately built binaries:

```bash
gh release create v<version> \
  --title "eggress v<version>" \
  --notes-file <release-notes-file>
```

This step is optional and must remain manual.

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

- crates.io publishing on a tag or branch push;
- crates.io tokens or trusted-publishing configuration in GitHub Actions;
- automated GitHub Release creation;
- publishing anything other than the canonical `eggress` wheel/sdist from the tag-triggered Python workflow (the `eggress-pproxy-compat` distribution is published manually);
- mandatory release artifact, checksum, SBOM, signature, or container jobs;
- a release workflow that repeats the ordinary CI suite;
- release gates that require generated evidence unrelated to the changed release surface.

Release correctness comes from a clean release commit, proportionate local verification, package dry runs, explicit operator publication, and post-publication installation checks.
