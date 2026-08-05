# Lean Runtime Phase 3 — Python ABI, Wheels, and Release Closure

## Status

**IMPLEMENTED**

Implemented in commit `b650ab6` on main.

### Artifact set

- `eggress-1.0.1-cp39-abi3-manylinux_2_34_x86_64.whl` (Linux x86_64)
- `eggress-1.0.1-cp39-abi3-manylinux_2_34_aarch64.whl` (Linux aarch64)
- `eggress-1.0.1-cp39-abi3-macosx_11_0_x86_64.whl` (macOS x86_64)
- `eggress-1.0.1-cp39-abi3-macosx_11_0_arm64.whl` (macOS arm64)
- `eggress-1.0.1-cp39-abi3-win_amd64.whl` (Windows x86_64)
- `eggress-1.0.1.tar.gz` (source distribution)

### Verification

- Rust workspace gate: 2423 passed, 146 ignored
- Python test suite: 2170 passed, 114 skipped, 5 warnings
- CI: both `ci.yml` and `python-test.yml` passed on main

## Parent roadmap

[`LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md`](LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md)

## Dependencies

- Phase 1 feature topology is complete and the Python extension explicitly requests the full supported feature set.
- Phase 2 focused reliability tests are complete so the release artifact is built from the corrected runtime surface.

## Objective

Make the published `eggress` Python distribution match its declared Python and operating-system support while retaining the existing bounded release model: one path-scoped Linux/Python smoke workflow for routine changes and one release-only workflow for building, verifying, and publishing wheels and an sdist.

The current release workflow builds one wheel on Ubuntu with Python 3.12. The package declares Python 3.9 through 3.13 and presents itself as operating-system independent despite containing a native PyO3 extension. This phase corrects that mismatch without creating a general release platform or expanding routine CI.

## Governing decisions

1. PyPI publication remains in GitHub Actions because supported platform wheels require multiple build hosts or cross-compilation.
2. Crates.io publication remains manual and is not coupled to this workflow.
3. The Python release workflow is allowed a bounded platform build matrix because it runs only for tags or explicit manual dispatch.
4. Routine `python-test.yml` remains one Ubuntu/Python 3.12 smoke job unless a concrete compatibility defect requires changing the selected smoke version.
5. Prefer a PyO3 stable ABI floor of Python 3.9 (`abi3-py39`) so one wheel per platform can support the declared Python range.
6. Do not fall back automatically to a Python-version-by-platform Cartesian matrix. If current bindings cannot use `abi3-py39`, stop and document the exact blocking APIs before choosing a larger wheel strategy.
7. Production publication must fail on a pre-existing version rather than silently masking it with `skip-existing`.
8. Do not add GitHub Releases, native CLI archives, containers, signatures, checksums, SBOMs, or provenance frameworks.

## Non-goals

Do not use this phase to:

- redesign the Python API;
- split `eggress` and the bundled top-level `pproxy` namespace into separate distributions;
- support installation alongside upstream `pproxy`;
- add a pure-Python fallback;
- publish Rust crates;
- automate version bumping, changelog generation, tagging, or GitHub Releases;
- add release branches or release-candidate workflows;
- build every CPU architecture supported by Rust;
- add musllinux, 32-bit, FreeBSD, Android, iOS, or WebAssembly wheels;
- add a routine Python-version test matrix;
- create custom wheel orchestration when maintained Maturin actions can express the build;
- introduce a permanent artifact-evidence report.

## Target artifact contract

The minimum intended wheel set is:

- Linux x86_64, manylinux-compatible;
- Linux aarch64, manylinux-compatible, because local/SBC deployment is a stated project use case;
- macOS x86_64;
- macOS arm64;
- Windows x86_64;
- one source distribution.

A macOS universal2 wheel may replace the two macOS wheels if Maturin can produce it cleanly and clean-install testing confirms both architectures. Do not retain both universal2 and architecture-specific wheels without a demonstrated reason.

Architectures outside this set remain source-build targets unless separately approved. Documentation must distinguish source support from prebuilt-wheel availability.

## Workstream A — Establish a truthful Python ABI contract

### Files to inspect

```text
crates/eggress-python/Cargo.toml
crates/eggress-python/pyproject.toml
crates/eggress-python/src/
python/eggress/
python/pproxy/
python/tests/
tests/compat/
```

### Step A1 — Evaluate `abi3-py39`

Change the PyO3 dependency to request the stable ABI floor while retaining extension-module behavior, for example:

```toml
pyo3 = {
    version = "0.29",
    features = ["extension-module", "abi3-py39"],
}
```

Avoid specifying the same extension feature redundantly in both Cargo and Maturin configuration unless required by the build. Keep one clear source where practical.

Build and test the wheel under at least Python 3.9 and the newest supported Python available in the implementation environment. A local developer does not need every interpreter installed; release artifacts will be checked by wheel tags and selected clean installs.

### Step A2 — Identify stable-ABI blockers narrowly

If compilation or tests fail under `abi3-py39`:

1. identify the exact PyO3 API or binding type requiring a newer/non-limited API;
2. determine whether an equivalent stable-ABI API can replace it without public behavior change;
3. make only that narrow change;
4. rerun the existing Python tests.

Do not redesign classes, convert the package to HPy, or add generated bindings.

If stable ABI remains impossible after narrow substitutions, stop this workstream and record:

- blocking symbols/APIs;
- minimum interpreter-specific wheel set required;
- estimated release-job count;
- whether package metadata should temporarily narrow supported Python versions.

Do not silently create a 25-job matrix.

### Step A3 — Correct package metadata

In `pyproject.toml`:

- retain `requires-python = ">=3.9"` only if the stable-ABI or interpreter-specific artifacts actually support it;
- remove `Operating System :: OS Independent` because the distribution contains a native extension;
- add specific classifiers only when useful and accurate; omission is preferable to a large classifier list;
- keep the declared Python versions synchronized with the tested ABI floor and current support policy;
- preserve the single `eggress` distribution that includes both `eggress.*` and `pproxy.*` packages.

Do not claim architectures in package classifiers.

## Workstream B — Replace the single-wheel release build with a bounded matrix

### Primary file

```text
.github/workflows/publish-python.yml
```

### Required workflow shape

The workflow should have three conceptual stages:

1. version and source validation;
2. wheel/sdist build and artifact smoke verification;
3. one registry publication job that downloads the complete verified artifact set.

Keep workflow logic direct. Do not add a custom release script unless an inline check becomes materially unreadable.

### Step B1 — Validate version coherence before builds

For a tag-triggered production release, require the normalized tag version to equal:

- `[workspace.package].version` in root `Cargo.toml`;
- `[package].version` in `crates/eggress-python/Cargo.toml`;
- `[project].version` in `crates/eggress-python/pyproject.toml`.

The check should accept a leading `v` in the tag and compare normalized semantic versions.

For manual TestPyPI dispatch, version coherence among the three files remains required even without a tag.

Do not add automatic version mutation.

### Step B2 — Use maintained Maturin build actions

Prefer `PyO3/maturin-action` or the currently maintained official/recommended Maturin action rather than hand-written cross toolchains.

Use an explicit matrix with only the target set defined above. A representative shape is:

```yaml
strategy:
  fail-fast: false
  matrix:
    include:
      - os: ubuntu-latest
        target: x86_64
        manylinux: auto
      - os: ubuntu-latest
        target: aarch64
        manylinux: auto
      - os: macos-13
        target: x86_64
      - os: macos-14
        target: aarch64
      - os: windows-latest
        target: x64
```

Exact runner labels and action syntax must be verified against current maintained documentation during implementation. Do not rely on this illustrative YAML blindly.

Linux aarch64 may use Maturin's supported cross/QEMU path. Do not create a custom Docker image or checked-in cross toolchain.

### Step B3 — Build an sdist once

Build one sdist in a small Linux job or the validation job. Ensure it contains:

- Rust sources required to compile the extension;
- Python `eggress` and `pproxy` packages;
- `.pyi` files and `py.typed`;
- Cargo manifests/lockfile needed by the package;
- license and readme metadata.

Do not build duplicate sdists per platform.

### Step B4 — Use unambiguous artifact names

Upload each wheel artifact with target-specific names, then download all into one publication directory. Avoid artifact-name collisions.

Before publication, list and validate the directory so it contains:

- exactly one wheel for each approved platform target, or the approved universal2 substitution;
- exactly one sdist;
- no debug artifacts, duplicate wheels, test files, or unrelated native binaries.

A small shell/Python assertion is sufficient. Do not generate an artifact manifest file unless the publishing action requires it.

## Workstream C — Clean-install artifact verification

### Native wheel smoke checks

For each natively executable build runner:

1. create a clean virtual environment;
2. install the built wheel without the source tree on `PYTHONPATH`;
3. import `eggress`;
4. import the bundled top-level `pproxy`;
5. assert the version exposed by the package matches the release version where such an API exists;
6. start a port-0 HTTP or SOCKS listener through the public `EggressService` API;
7. inspect bound addresses/status;
8. shut it down cleanly.

Use one short script or existing smoke test. Do not run the complete Python suite on every release matrix target.

### Python compatibility-range smoke

Because `abi3-py39` is intended to cover multiple CPython versions, add a small release-only compatibility check on one native platform using:

- Python 3.9;
- Python 3.13 or the newest currently declared/supported version.

Each installs the same platform wheel and performs imports plus the port-0 startup/shutdown smoke.

This may be two short steps or two small jobs. It must not become the routine Python smoke matrix.

### Cross-built Linux aarch64 handling

If the wheel is cross-built and not natively executable on the runner:

- rely on the maintained action's manylinux/cross build path;
- validate wheel tags and archive contents;
- optionally execute under the action's supported emulation only if this requires no custom infrastructure and remains reliable.

Do not add a bespoke emulation framework. If aarch64 runtime smoke cannot be made reliable, document that release verifies build/tag integrity and that native functionality is covered by the same source under other targets.

### Sdist smoke

On one Linux runner:

1. install the sdist into a clean virtual environment with Rust available;
2. import `eggress` and `pproxy`;
3. perform the same short startup/shutdown smoke.

This proves the source fallback is installable. Do not run the full test suite from the sdist.

## Workstream D — Publication safety

### PyPI

The production publication job must:

- depend on every required build and verification job;
- use OIDC trusted publishing through the existing `pypi` environment;
- publish the complete artifact directory once;
- not use `skip-existing: true` for production;
- never publish from a pull request or ordinary branch push;
- never publish Rust crates or create a GitHub Release.

A `v*` tag may continue to trigger production publication after version coherence passes.

### TestPyPI

Manual dispatch may continue to target TestPyPI. It should use the same built artifact set and validation path.

`skip-existing` may be retained only if TestPyPI iteration genuinely requires it, but a clearer approach is to require a unique test version. Do not let TestPyPI behavior weaken production failure semantics.

### Environments and permissions

Keep:

```yaml
permissions:
  contents: read
  id-token: write
```

Use `id-token: write` only in publishing jobs if practical. Build jobs need read-only contents.

Do not add package-write, release-write, or repository-write permissions.

## Workstream E — Keep routine Python CI lean

### Primary file

```text
.github/workflows/python-test.yml
```

Retain:

- path filtering;
- one Ubuntu runner;
- Python 3.12 unless a concrete reason justifies selecting another single middle/new version;
- Maturin development build;
- existing `python/tests` and `tests/compat` execution, unless measured runtime shows a specific redundant subset should move to release/manual verification.

Add only a missing import assertion for the top-level `pproxy` package if the current suite does not already prove it.

Do not add operating-system, architecture, or Python-version matrices to this workflow.

## Documentation updates

Update only current documentation, likely:

```text
README.md
python/README.md
docs/PYTHON_BINDINGS.md
docs/release/RELEASE_PROCESS.md
AGENTS.md
```

Documentation must state separately:

- supported Python language versions;
- prebuilt wheel platforms/architectures;
- source-build fallback expectations;
- that the wheel owns both `eggress` and bounded `pproxy` import namespaces;
- that upstream `pproxy` must be uninstalled first;
- that PyPI publication is automated through the release workflow while crates.io remains manual.

Do not create a wheel matrix document or generated compatibility page.

## Required local verification

Before workflow changes are considered ready:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin build --release --out ../../target/wheels)
.venv/bin/python -m pip install --force-reinstall target/wheels/eggress-*.whl
.venv/bin/python -c "import eggress, pproxy; print(eggress, pproxy)"
.venv/bin/python -m pytest python/tests tests/compat -q
```

Add the short public-API startup/shutdown smoke to an existing test location if one does not exist.

## Workflow verification

Use manual TestPyPI dispatch or a non-publishing branch/workflow test mechanism permitted by the repository to prove matrix builds before the first production tag. Do not publish a fake production version.

Check artifact filenames and wheel tags. For stable ABI, wheels should carry an `abi3` tag with the intended Python floor rather than `cp312-cp312`.

## Acceptance criteria

Phase 3 is complete only when:

1. The Python ABI strategy truthfully supports the declared Python version range.
2. `Operating System :: OS Independent` is removed from native-extension metadata.
3. The release workflow builds the bounded approved wheel set and one sdist.
4. Linux x86_64, Linux aarch64, macOS x86_64, macOS arm64, and Windows x86_64 have wheels, or an explicitly verified universal2 wheel replaces the two macOS artifacts.
5. Production publication waits for all required artifact and smoke checks.
6. Production publication fails rather than silently skipping an existing version.
7. Version coherence among tag, workspace, binding crate, and Python project is enforced.
8. Clean wheel installs can import both `eggress` and bundled `pproxy` and can start/shut down a port-0 proxy through the public API.
9. One sdist clean-install smoke succeeds.
10. Routine Python CI remains one path-scoped Ubuntu/Python job.
11. Crates.io remains manual and no GitHub Release or native-binary publishing is added.
12. Active documentation accurately distinguishes Python versions, wheel platforms, and source builds.
13. No custom cross image, release framework, generated artifact report, or completion document is added.
14. The Rust workspace gate and existing Python suites pass.

## Stop conditions

Stop and narrow this phase when:

- `abi3-py39` is blocked by a public behavior that cannot be replaced narrowly;
- Linux aarch64 requires a custom image/toolchain rather than maintained Maturin support;
- universal2 requires invasive platform-specific code;
- a proposed smoke test duplicates the full Python suite on every target;
- workflow logic begins handling Rust crates, native CLI archives, GitHub Releases, or signing;
- the target matrix expands beyond the approved artifact contract without a concrete user requirement.

When a stop condition is reached, correct metadata to the support that can actually be delivered and record the blocked target in the roadmap closure. Do not preserve inaccurate claims.

## Handoff sequence

Prefer a small sequence:

1. `build(python): adopt stable ABI and correct metadata`
2. `ci(python): build bounded release wheel matrix and sdist`
3. `test(python): verify clean wheel and source installs`
4. `docs: document Python artifact support and release boundary`
5. update this plan and the parent roadmap in place at closure

Do not create one commit per platform.

## Roadmap closure duties

After this phase succeeds:

- update this file to `IMPLEMENTED` with commit range and artifact set;
- update the parent roadmap to `IMPLEMENTED` if Phases 1 and 2 are also complete;
- record full/lean binary measurements from Phase 1;
- record focused reliability test locations from Phase 2;
- record final wheel tags and supported platforms;
- record any rejected optionalization or packaging target with its reason.

Do not create a separate release-certification or roadmap-completion document.