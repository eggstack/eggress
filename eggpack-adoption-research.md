# Eggpack producer-side adoption research

**Scope:** assess whether Eggpack should replace or augment Eggress's prebuilt CLI release packaging. Research only; no release workflow or source changes are proposed or applied here.

## Executive finding

Eggpack's producer contract fits Eggress's four Unix .tar.gz artifacts, including the existing two-binary archive layout. It is **not a drop-in replacement for the five-target release today**: Eggress's Windows artifact is a .zip, while Eggpack's current finalizer accepts only tar+gzip archives. Changing Windows to tar.gz would alter an established installer and self-updater contract, so it should not be done implicitly.

**Recommendation:** keep the current release workflow authoritative. If Eggpack adoption is worth pursuing, first run a pinned, non-publishing shadow pilot for the four Unix artifacts, compare its outputs and qualification against the current pipeline, and leave Windows and the public release uploader unchanged. Do not migrate production until Eggpack supports the current Windows ZIP contract or an explicit product decision approves a Windows format change. Keep producer tooling separate from Eggress's runtime Eggup dependencies.

## Eggress's existing contract

The release workflow builds the eggress and pproxy CLI binaries for five targets: Linux x86_64 and AArch64, macOS x86_64 and AArch64, and Windows x86_64 MSVC. Each archive contains the two sibling executables. Four assets are named eggress-<target>.tar.gz; Windows is eggress-x86_64-pc-windows-msvc.zip. Each has an adjacent .sha256 sidecar. The tag supplies the version namespace, so the filenames intentionally do not embed the release version. These names and target mappings are also encoded in the updater (crates/eggress-cli/src/update/target.rs:8-15,63-77).

The workflow is more than a pack command. It is tag-triggered (or manually dispatched for an existing tag), builds from that exact tag, verifies versions and CLI startup/help behavior, runs listener-bind smoke checks, verifies the complete five-archive/checksum set, stages both installer scripts, and then creates or updates the GitHub Release and uploads assets (.github/workflows/release-binaries.yml:7-16,45-89,115-153,371-440,443-526). The release preflight enforces the vX.Y.Z tag, workspace/Python/compat version lockstep, internal exact-version pins, tagged-commit identity, and a clean tree (scripts/release-preflight.sh:12-23,152-176).

The Windows installer explicitly downloads the .zip (packaging/install.ps1:24-42). The updater consumes the same target-specific archive and checksum naming, verifies the archive before extraction, and updates eggress and pproxy together (crates/eggress-cli/src/update/eggup.rs:1-23; crates/eggress-cli/src/update/mod.rs:1-15). Its current trust path has no Eggpack manifest dependency.

## Eggpack fit and limits

Eggpack separates a schema-v1 distribution contract from producer execution. The contract can describe target triples/aliases, asset and checksum names, archive member paths, and installed names. Its asset forms include direct assets, bundles, and one archive containing multiple named members. The sample archive fixture demonstrates two executables as members of one .tar.gz plus a checksum sidecar; it is an illustrative fixture, not an Eggress qualification result.

The producer core plans builds, binds logical outputs to Cargo packages/binaries, runs bounded builds and qualification, then finalizes candidate bytes under contract names and creates SHA-256 sidecars and a manifest. Its README describes candidate-format/architecture checks and identity-bound qualification evidence. The archive finalizer currently exposes only ArchiveEncoding::TarGzip, and requires the contract archive name to end in .tar.gz. It does not provide ZIP output. Thus, the contract can express Eggress's member layout, but the current producer cannot finalize the Windows archive without a format change or an out-of-band packaging path.

Eggpack's local operator flow builds and finalizes files; local commands do not publish. Workflow generation/checking and GitHub staging are distinct CI capabilities, so adopting the producer need not grant it authority to publish a production release. Eggress would still need to retain or explicitly replace its own version preflight, exact tag/source checks, target matrix, compatibility-floor claims, runtime smoke checks, full-set/checksum gate, installer staging, and GitHub Release upload behavior. Eggpack qualification should not be treated as equivalent to those Eggress-specific release policies until tested.

The project quickstart demonstrates installing the CLI from a Git checkout at a configured exact revision; its sample revision is explicitly a placeholder that must be replaced by an approved immutable commit. At research time, the crates.io API returned 404 for eggpack-cli and eggpack-contract, so producer-tool availability appears Git-pinned rather than registry-published. Recheck this before any implementation. Pinning a reviewed immutable revision and the workflow's third-party actions is a prerequisite, not an optional polish step.

## Relationship to Eggup

Eggress already uses eggup-core and eggup-archive for the updater's transactional install and safe archive extraction. eggup-eggpack is a separate, optional **consumer-side adapter**: it projects an Eggpack manifest into Eggup acquisition/deployment inputs. It is not the archive producer or a tool for creating Eggress release archives. Adding it to Eggress solely to adopt producer packaging would mix runtime and release-tooling concerns without solving the ZIP gap. A future decision to make the updater consume Eggpack manifests should be evaluated separately.

## Low-risk adoption path

1. Preserve the current release workflow as the production source of truth and keep all existing asset names, target mappings, installer inputs, updater behavior, and permissions unchanged.
2. If a pilot is approved, create a shadow contract and generated/checkable workflow pinned to a reviewed Eggpack commit. Give it no production release credentials or public-upload step. Start with the four Unix tarballs; keep the Windows ZIP on the existing path.
3. Compare the pilot to the current artifacts: exact filenames and sidecars, both binary members and their names, checksums, executable identity/version, listener-bind behavior, Linux glibc floor, and target/source identity. Keep Eggress's release preflight and qualification gates unless Eggpack proves equivalent coverage.
4. Require full matrix parity, including an Eggpack ZIP finalizer (or a separately approved Windows-format change), before considering production replacement. Then review workflow permissions, immutable pins, failure behavior, and manifest/consumer implications as a distinct change.

## Decision

**Do not replace the five-target production pipeline with Eggpack yet.** A shadow pilot is technically feasible for the four Unix tarballs and can establish value without changing the public contract. Full adoption is blocked by Windows ZIP support or an explicit, separately reviewed decision to change that contract.

## Sources

Local Eggress contracts:
- .github/workflows/release-binaries.yml:7-16,45-89,115-153,371-440,443-526
- crates/eggress-cli/src/update/target.rs:8-15,63-77
- crates/eggress-cli/src/update/eggup.rs:1-23
- crates/eggress-cli/src/update/mod.rs:1-15
- packaging/install.ps1:24-42
- scripts/release-preflight.sh:12-23,152-176

Eggpack upstream material reviewed (accessed 2026-11-19):
- [Quickstart](https://github.com/eggstack/eggpack/blob/main/docs/quickstart.md)
- [Contract schema and conformance](https://github.com/eggstack/eggpack/blob/main/crates/eggpack-contract/README.md)
- [Producer core](https://github.com/eggstack/eggpack/blob/main/crates/eggpack-core/README.md)
- [Archive finalizer implementation](https://github.com/eggstack/eggpack/blob/main/crates/eggpack-core/src/finalization.rs)
- [CLI/workflow command reference](https://github.com/eggstack/eggpack/blob/main/crates/eggpack-cli/README.md)
- [Illustrative two-member archive fixture](https://github.com/eggstack/eggpack/blob/main/crates/eggpack-contract/tests/fixtures/egress-archive.toml)
- [eggpack-cli registry endpoint](https://crates.io/api/v1/crates/eggpack-cli) and [eggpack-contract registry endpoint](https://crates.io/api/v1/crates/eggpack-contract) (both returned 404 during this check)
- [Published eggup-eggpack 0.2.0 consumer adapter](https://crates.io/crates/eggup-eggpack/0.2.0)
