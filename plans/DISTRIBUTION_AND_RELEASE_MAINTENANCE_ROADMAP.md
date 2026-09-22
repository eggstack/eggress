# Distribution and Release Maintenance Roadmap

## Status

**READY FOR IMPLEMENTATION — 2026-09-22**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `c493eab803654d1316150a9837fd01ee46b9b87d`
- Workspace release line: `1.0.7`
- Governing constraint: broaden binary Python distribution coverage and reduce release-operator burden without changing Eggress runtime behavior, public Rust/Python/CLI/configuration/protocol surfaces, or the existing manual/operator-driven crates.io publication policy.

## Purpose

Eggress already has the right architectural foundations for much broader distribution:

- the Python extension uses PyO3 `0.29` with `abi3-py39`, so one wheel per OS/architecture can support ordinary GIL-enabled CPython 3.9 and later rather than producing one wheel per Python minor;
- the PyPI workflow already publishes Linux x86_64/AArch64, macOS x86_64/arm64, and Windows x86_64 wheels plus an sdist;
- the binary release workflow already proves that native GitHub-hosted Linux AArch64 runners are viable for this repository;
- all publishable Rust crates move in lockstep and internal registry dependencies use exact same-version pins;
- Cargo now supports workspace-wide package/publish selection, so the hand-maintained publication tier table should no longer be the default source of dependency-order truth.

The remaining work is release engineering, qualification, and documentation. It is not an API or architecture campaign.

## Current-state findings

### Python ABI strategy is already suitable

`crates/eggress-python/Cargo.toml` currently enables:

```toml
pyo3 = { version = "0.29", features = ["extension-module", "abi3-py39"] }
```

and the authoritative `crates/eggress-python/pyproject.toml` also asks maturin for `pyo3/abi3-py39`.

The release workflow validates `cp39-abi3` wheel tags. That is the correct baseline for ordinary CPython 3.9+ and should remain the default ABI family.

Python 3.14 and 3.15 therefore do not require extra ordinary-CPython wheels. They require compatibility qualification, metadata/documentation alignment, and a release test lane using the same artifact.

Free-threaded CPython is a separate ABI problem. PyO3 0.29 supports the newer `abi3t` family for Python 3.15+, while free-threaded Python 3.14 still requires a version-specific wheel. That is intentionally deferred from the baseline expansion so ordinary 3.9–3.15 support does not become coupled to a second ABI family.

### Current PyPI platform matrix is too narrow

`.github/workflows/publish-python.yml` currently builds exactly five wheels:

1. manylinux2014 x86_64;
2. manylinux2014 AArch64;
3. macOS x86_64;
4. macOS arm64;
5. Windows x86_64.

The artifact validator hard-codes exactly five wheels and manually parses wheel filenames. The smoke matrix exercises Linux x86_64, macOS arm64, and Windows x86_64; Linux AArch64 is cross-built but is not installation/runtime-smoked on a native ARM runner.

Current PyPA platform images support additional useful targets:

- manylinux2014: x86_64, i686, aarch64, ppc64le, s390x;
- manylinux_2_31 and newer: armv7l;
- manylinux_2_39: aarch64 and riscv64;
- musllinux_1_2: x86_64, i686, aarch64, ppc64le, s390x, armv7l, riscv64.

Current GitHub-hosted public runners include native Linux ARM64 (`ubuntu-24.04-arm`) and Windows ARM64 (`windows-11-arm`), so those targets do not need to remain cross-build-only.

### SBC support needs explicit qualification rather than SBC-specific artifacts

Raspberry Pi 4/5 and Libre Computer Le Potato-class systems running a 64-bit Linux userland consume ordinary AArch64 Linux wheels. No Raspberry-Pi-specific wheel tag is needed.

The current `manylinux2014_aarch64` wheel is therefore the correct artifact family for 64-bit Pi/Le Potato deployments, but the release process does not currently prove that the wheel installs and runs on that target class.

32-bit Raspberry Pi-class users require an ARMv7 wheel family; `manylinux2014` does not provide armv7l, so the ARMv7 GNU wheel must use a supported newer manylinux floor such as `manylinux_2_31`.

### Python-version testing is endpoint-only

The current compatibility smoke covers only Python 3.9 and 3.13 on Linux x86_64. Metadata and documentation advertise 3.9–3.13.

The intended baseline after this campaign is ordinary CPython 3.9 through 3.15 using the same `cp39-abi3` artifact. Because Python 3.15 is still in release-candidate status at the planning date, pre-release qualification may land before final-release qualification, but production documentation must distinguish those states accurately.

### crates.io release ergonomics are dominated by a hand-maintained graph

`scripts/publish-remaining.sh` manually encodes 28 `eggress-*` crates into dependency tiers and defaults to a 660-second delay after every publish.

This duplicates information already present in Cargo metadata and makes release maintenance sensitive to dependency-edge changes. It also turns normal version publication into a multi-hour operation even when crates.io does not require that fixed delay.

Cargo now supports `cargo publish --workspace` with package selection/exclusion and performs workspace-wide verification before upload. Workspace publication is still not atomic, however, so interrupted publication needs an explicit recovery story.

## Registered execution plans

| Order | Plan | Status | Purpose |
|---|---|---|---|
| 1 | [`PYPI_WHEEL_MATRIX_EXPANSION.md`](PYPI_WHEEL_MATRIX_EXPANSION.md) | Ready | Expand wheel/platform coverage, qualify ordinary CPython 3.9–3.15, add ARM/SBC evidence, and make artifact validation matrix-driven. |
| 2 | [`MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md`](MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md) | Ready | Replace the hand-maintained publish tier table/fixed delay with native workspace publication or a graph-derived resumable manual helper while keeping crates.io publication local and operator-driven. |

## Sequencing

The two implementation plans are operationally independent and may be developed in parallel.

The PyPI plan should land before the next release that claims expanded Python/platform support. It changes only packaging, CI/release workflow, tests, and maintained support documentation unless qualification exposes a real target-specific source defect.

The crates.io plan should first qualify Cargo's native workspace publisher against the exact Eggress workspace. Only after that evidence exists should the old tier table be removed or reduced to a fallback.

## Governing constraints

1. Preserve all existing public Rust, Python, CLI, configuration, URI, protocol, compatibility, feature-gate, and runtime behavior.
2. Keep `abi3-py39` as the baseline ordinary-CPython wheel ABI unless direct qualification proves it cannot support a required target.
3. Do not multiply ordinary wheels by Python minor version.
4. Do not make free-threaded Python support a prerequisite for ordinary CPython 3.9–3.15 support.
5. Do not claim a platform/Python combination as supported without an install/import/runtime smoke appropriate to that claim.
6. Prefer native runners where GitHub provides them, especially Linux AArch64 and Windows ARM64.
7. Keep release CI proportionate: do not form the full Cartesian product of every Python version and every target.
8. Do not merge, hide, or unpublish existing Rust crates merely to simplify release mechanics.
9. crates.io publication remains manual and operator-driven from a trusted local checkout.
10. Do not add crates.io trusted publishing, GitHub Actions crates.io credentials, tag-triggered crate publication, or automatic crates.io publication in this campaign.
11. Do not pass `--no-verify` or `--allow-dirty` in the normal crates.io release path.
12. Replace hand-maintained dependency-order knowledge with Cargo workspace behavior or `cargo metadata`-derived ordering.
13. An interrupted crates.io release must have a documented roll-forward/resume procedure; never overwrite or retag an already-published version.
14. Do not reopen pproxy compatibility claims merely because packaging coverage changes.

## Global acceptance criteria

This campaign is complete only when:

1. ordinary CPython 3.9–3.15 compatibility is represented and tested through the existing stable ABI rather than per-minor wheels;
2. the canonical PyPI matrix includes materially broader Linux ARM/SBC coverage, musl coverage, and Windows ARM64 where qualification passes;
3. Linux AArch64 is natively install/runtime-smoked in hosted CI;
4. first-class ARM/SBC claims have target-class evidence and clearly stated OS/libc/architecture requirements;
5. the Python release artifact validator no longer assumes exactly five hard-coded wheels;
6. a release can still fail closed if any required wheel/sdist is absent or mislabeled;
7. crates.io publication remains a deliberate local command;
8. the release operator no longer maintains a manual 28-crate dependency tier table as the primary ordering authority;
9. fixed unconditional ~11-minute sleeps are removed from the normal crates.io version-publish path;
10. partial crates.io publication has a deterministic resume/roll-forward procedure;
11. release documentation, `plans/README.md`, and `docs/ROADMAP.md` agree on the final state;
12. no runtime/API/capability regression is introduced.

## Explicit non-goals

This roadmap does not authorize:

- runtime/protocol feature work;
- API removal or crate merging;
- a Python minimum-version increase;
- dropping the current five wheel families;
- automatic crates.io publication;
- GitHub-stored crates.io credentials or crates.io OIDC;
- mandatory self-hosted runners for every release;
- a permanent full Python-version × platform test matrix;
- free-threaded Python 3.14/3.15 wheel publication in the baseline phase;
- changing the CLI binary release matrix except where shared documentation must remain accurate;
- new parity claims or pproxy behavior changes.

## Research references

Implementation should re-check current upstream documentation when it lands. The planning baseline was informed by:

- Cargo Book: `cargo publish` workspace/package selection and registry polling behavior;
- PyO3 0.29 building/distribution guidance for `abi3` and `abi3t`;
- maturin stable-ABI guidance;
- PyPA manylinux/musllinux supported image architectures;
- GitHub-hosted runner documentation for Linux ARM64 and Windows ARM64.

Do not copy external target claims into maintained Eggress support documentation without successfully building and testing the actual Eggress artifact.
