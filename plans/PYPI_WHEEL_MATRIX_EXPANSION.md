# PyPI Wheel Matrix Expansion

## Status

**IMPLEMENTED — 2026-09-22**

The Tier A matrix landed: ten `cp39-abi3` families (Linux x86_64/aarch64/armv7l
GNU + musllinux x86_64/aarch64/armv7l, macOS x86_64/arm64, Windows
x86_64/ARM64) plus one sdist, with a data-driven build matrix, a
`packaging`-based fail-closed validator
(`scripts/validate_release_artifacts.py`, covered by
`tests/scripts/test_validate_release_artifacts.py`), native AArch64/Windows
ARM64 smokes, musl/ARMv7 execution smokes, exhaustive 3.9–3.15 compat smoke
(3.15 RC-qualified), pinned maturin (`v1.14.1`), and `--locked` release
builds. Classifiers and install/bindings docs now state 3.9–3.15. Tier B
remains deferred per sequencing (each target still needs a build proof plus
executable smoke before joining the required set); physical SBC
target-class qualification is documented as a pending one-time procedure;
free-threaded wheels stay explicitly separate. No API/runtime change.

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `c493eab803654d1316150a9837fd01ee46b9b87d`
- Parent roadmap: [`DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md`](DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md)
- Workspace release line: `1.0.7`
- Authoritative Python package entrypoint: `crates/eggress-python/pyproject.toml`
- Release workflow: `.github/workflows/publish-python.yml`

## Objective

Expand the canonical PyPI wheel set from the current five platform artifacts to a broad, maintainable platform matrix with explicit ARM/SBC coverage and ordinary CPython compatibility through 3.15, while preserving the existing `cp39-abi3` strategy and avoiding a Python-version-by-platform artifact explosion.

This is packaging/release engineering. It must not change the Python API, Rust API, runtime behavior, protocol behavior, pproxy compatibility, or feature defaults.

## Current state

### Stable ABI

The binding crate already uses:

```toml
pyo3 = { version = "0.29", features = ["extension-module", "abi3-py39"] }
```

and the authoritative pyproject asks maturin for:

```toml
features = ["pyo3/extension-module", "pyo3/abi3-py39"]
```

The release artifact validator requires `cp39-abi3`.

Keep that design. One ordinary CPython wheel per OS/architecture supports GIL-enabled CPython 3.9 and newer.

### Existing artifacts

The current release matrix produces:

| Platform | Build target/policy | Native release smoke |
|---|---|---|
| Linux x86_64 | manylinux2014 | yes |
| Linux AArch64 | manylinux2014 | no; cross-build only in Python workflow |
| macOS x86_64 | x86_64 | yes |
| macOS arm64 | arm64 | yes |
| Windows x86_64 | x64 | yes |

The workflow then asserts exactly five wheels and one sdist.

Compatibility smoke currently installs the Linux x86_64 wheel only under Python 3.9 and 3.13.

### Current support metadata

Both maintained pyproject files currently list classifiers only through Python 3.13, and maintained installation/binding documentation states 3.9–3.13.

`requires-python = ">=3.9"` already permits later Python versions and should remain unchanged.

## Target matrix

### Tier A — canonical required wheel families

The first implementation target is this required matrix:

| Family | Target | Policy | Why |
|---|---|---|---|
| Linux GNU | x86_64 | manylinux2014 | retain current baseline |
| Linux GNU | aarch64 | manylinux2014 | retain; first-class 64-bit Pi/Le Potato-class target |
| Linux GNU | armv7l | manylinux_2_31 or the oldest currently supported PyPA armv7l image that actually builds Eggress | 32-bit Raspberry Pi-class support |
| Linux musl | x86_64 | musllinux_1_2 | Alpine/static-oriented deployments |
| Linux musl | aarch64 | musllinux_1_2 | ARM64 Alpine/SBC deployments |
| Linux musl | armv7l | musllinux_1_2 | 32-bit Alpine/SBC deployments |
| macOS | x86_64 | native macOS wheel | retain |
| macOS | arm64 | native macOS wheel | retain |
| Windows | x86_64 | win_amd64 | retain |
| Windows | ARM64 | win_arm64 | current hosted native runner exists; qualify and add |

Do not invent an `armv7l manylinux2014` tag. PyPA's supported armv7l floor is newer; use a real supported image/tag.

### Tier B — extended wheels

After Tier A is green, add extended Linux architectures when the dependency graph and maturin/PyPA tooling support them cleanly:

| Family | Target | Candidate policy |
|---|---|---|
| Linux GNU | i686 | manylinux2014 |
| Linux GNU | ppc64le | manylinux2014 |
| Linux GNU | s390x | manylinux2014 |
| Linux GNU | riscv64 | manylinux_2_39 |
| Linux musl | i686 | musllinux_1_2 |
| Linux musl | ppc64le | musllinux_1_2 |
| Linux musl | s390x | musllinux_1_2 |
| Linux musl | riscv64 | musllinux_1_2 |

Tier B is part of the intended broad-coverage line, but a target may be deferred individually if an Eggress dependency cannot be built for it without source/API/runtime changes disproportionate to distribution value.

Any deferral must record the exact dependency/toolchain blocker rather than silently omitting the target.

### Explicitly not required

Do not add wheel families for:

- 32-bit Windows unless there is a demonstrated user need;
- Android/iOS;
- WASI;
- PyPy-specific native ABI wheels;
- free-threaded CPython in this baseline plan.

## Phase 1 — Make the wheel matrix data-driven

Refactor `.github/workflows/publish-python.yml` so target definitions are authoritative in one matrix rather than duplicated between build logic and the artifact validator.

Each matrix entry should carry enough metadata to drive:

- runner;
- maturin target;
- compatibility policy (`manylinux2014`, `manylinux_2_31`, `musllinux_1_2`, native macOS/Windows);
- expected normalized platform family;
- whether a native runtime smoke is required;
- whether emulation/cross-smoke is the best available evidence.

The artifact collector must derive the expected wheel count/set from maintained target data or from a small canonical validation table. Do not leave an unexplained literal `len(wheels) != 5`.

### Wheel filename validation

Replace fragile manual `name.split("-")` parsing with standards-aware wheel parsing using the Python `packaging` library or equivalent maintained PyPA tooling.

Validate at minimum:

- distribution name is `eggress`;
- wheel version equals the release version;
- ordinary wheels use `cp39-abi3`;
- every required platform family appears exactly once;
- no unexpected platform family appears in a production publish;
- sdist version matches wheel version;
- no debug artifact is present.

The validator must understand both `manylinux` and `musllinux` platform tags.

### Acceptance

- [ ] Build and validation target knowledge is no longer spread across multiple unrelated hard-coded conditionals.
- [ ] Artifact validation is standards-aware and fail-closed.
- [ ] Adding/removing an approved target requires changing one canonical target definition plus any target-specific build mechanics.
- [ ] The existing five wheels still validate during the transition.

## Phase 2 — Native AArch64 release qualification

Move the canonical Linux AArch64 Python wheel build or at least its installation/runtime qualification onto a native GitHub ARM64 runner such as `ubuntu-24.04-arm`.

Preferred outcome: build and smoke the manylinux AArch64 artifact from the native ARM64 job where maturin/PyPA tooling permits the required manylinux floor.

If the manylinux container build still needs cross/container mechanics, keep those mechanics but install and execute the resulting wheel natively on the ARM64 runner before publication.

The current QEMU-only setup and special `CFLAGS_aarch64_unknown_linux_gnu=-D__ARM_ARCH=8` workaround should be re-evaluated. Remove it only if the native path makes it unnecessary and the `ring`-dependent build remains green.

Required smoke:

```text
create clean venv
install selected wheel
install opt-in pproxy compat package with --no-deps
run scripts/release_artifact_smoke.py
```

Also record:

```text
platform.machine()
sys.version
wheel filename/tag
glibc version
```

### Acceptance

- [ ] Linux AArch64 has a native install/import/runtime smoke before PyPI publication.
- [ ] The wheel remains compatible with the documented manylinux floor.
- [ ] Any old AArch64 compiler workaround retained in the workflow has a current, demonstrated reason.
- [ ] No SBC-specific code path is introduced.

## Phase 3 — ARMv7 and musllinux families

Add the Tier A ARMv7 and musllinux artifacts.

Use maturin/PyPA-supported platform images rather than hand-assigning wheel tags.

Required additions:

- GNU armv7l using a supported manylinux armv7l policy;
- musllinux_1_2 x86_64;
- musllinux_1_2 aarch64;
- musllinux_1_2 armv7l.

Cross-building and QEMU are acceptable where GitHub has no native hosted runner, but every new architecture/libc family must have at least one actual install/import/runtime smoke before first production publication.

For QEMU/container smokes, ensure the test executes the built extension for the target architecture rather than merely checking that a file was created.

### Stop conditions

Do not weaken TLS/crypto behavior, disable existing default binding functionality, or alter public capability solely to make a target compile.

If `ring`, `russh`, platform verifier, or another dependency blocks a target:

1. record the exact compile/link/runtime blocker;
2. confirm whether the blocker affects default Python-wheel features;
3. attempt a normal supported toolchain configuration;
4. defer only that target if solving it would require capability/API regression or a large dependency redesign.

### Acceptance

- [ ] GNU armv7l wheel builds with a legitimate supported manylinux tag.
- [ ] musllinux x86_64/AArch64/armv7l wheels build.
- [ ] Each new family receives an install/import/runtime smoke appropriate to available execution infrastructure.
- [ ] No wheel is published merely because cross-compilation succeeded.

## Phase 4 — Windows ARM64

Add a native Windows ARM64 build/smoke job using the current GitHub-hosted Windows ARM64 runner.

Qualify:

- Rust target/toolchain;
- maturin/PyO3 extension build;
- wheel tag;
- clean-venv installation;
- `release_artifact_smoke.py`;
- any Windows-specific system-proxy import/runtime initialization that executes during smoke.

Do not infer Windows ARM64 support from Linux/macOS ARM64.

### Acceptance

- [ ] `win_arm64` wheel is produced.
- [ ] The wheel installs and executes on a native Windows ARM64 runner.
- [ ] Existing `win_amd64` behavior remains green.

## Phase 5 — Ordinary CPython 3.9–3.15 qualification

Keep one `cp39-abi3` artifact per platform.

On Linux x86_64, install the exact collected release wheel under every ordinary CPython minor:

```text
3.9
3.10
3.11
3.12
3.13
3.14
3.15
```

Run the existing release artifact smoke for every interpreter.

Do not build a new wheel for each interpreter.

For Python 3.15:

- at the planning date, 3.15 is still pre-release;
- qualify against the newest available 3.15 release candidate immediately;
- when final 3.15 is available, replace/augment the pre-release lane with final-release evidence before documenting unqualified final support;
- do not delay all other target work solely because the final interpreter has not shipped yet.

Update maintained metadata after evidence exists:

- `crates/eggress-python/pyproject.toml`;
- `python/pyproject.toml`;
- relevant README/install/bindings documentation.

Add classifiers for 3.14 and 3.15 when the corresponding support state is accurate.

Keep:

```toml
requires-python = ">=3.9"
```

### Cross-platform Python-minor policy

Do not run all seven Python minors on every platform.

Required evidence shape:

- Linux x86_64: exhaustive 3.9–3.15 ordinary-CPython compatibility smoke;
- every other canonical target: one current representative interpreter native/emulated runtime smoke;
- optionally one oldest/newest ABI endpoint on native AArch64 if inexpensive.

This proves ABI-span compatibility without multiplying CI cost unnecessarily.

### Acceptance

- [ ] The same `cp39-abi3` Linux x86_64 wheel installs/runs on every maintained ordinary CPython 3.9–3.15 interpreter.
- [ ] Python 3.14 metadata/docs are updated from evidence.
- [ ] Python 3.15 support wording distinguishes RC qualification from final qualification until final 3.15 is actually tested.
- [ ] No per-minor ordinary wheel matrix is introduced.

## Phase 6 — SBC target-class qualification

Before calling AArch64/ARMv7 wheels first-class SBC support, run a one-time target-class qualification on representative real hardware or an equivalent native machine.

Preferred evidence:

### 64-bit ARM

At least one Raspberry Pi-class machine and/or Libre Computer Le Potato-class board running a supported 64-bit GNU/Linux distribution:

```bash
python -m venv /tmp/eggress-wheel-test
/tmp/eggress-wheel-test/bin/python -m pip install --upgrade pip
/tmp/eggress-wheel-test/bin/python -m pip install ./eggress-<version>-cp39-abi3-<aarch64-tag>.whl
/tmp/eggress-wheel-test/bin/python -c "import eggress; print(eggress.__version__)"
```

Run `scripts/release_artifact_smoke.py` where practical.

### 32-bit ARM

Run equivalent validation on a real ARMv7 userland before claiming 32-bit Raspberry Pi-class support.

Record, in a compact maintained support note or release evidence:

- board/model class;
- architecture from `uname -m`;
- distribution;
- glibc/musl version;
- Python version;
- wheel filename;
- smoke outcome.

Do not commit bulky logs.

This is first-target qualification, not a requirement to keep a permanent self-hosted Pi online for every release.

### Acceptance

- [ ] AArch64 SBC support is backed by target-class installation/runtime evidence.
- [ ] ARMv7 support is not claimed until real ARMv7 evidence exists.
- [ ] Documentation explains that SBC compatibility is determined by architecture/libc/Python ABI, not board branding.

## Phase 7 — Extended architecture pass

After Tier A is stable, attempt Tier B architectures.

Prefer adding a target when:

- PyPA provides a maintained image;
- maturin supports the target;
- the Rust dependency graph builds without feature reduction;
- a reasonable QEMU/native smoke can execute the extension.

Do not make all Tier B targets blocking for Tier A publication until each has demonstrated sufficient reliability.

Once an extended target has been advertised as a supported production artifact, treat absence/failure as a release failure unless the support policy is deliberately revised.

### Acceptance

- [ ] Every feasible Tier B target is either published or has a documented concrete blocker.
- [ ] Extended targets do not weaken the Tier A release gate.
- [ ] No unsupported wheel tag is fabricated.

## Phase 8 — Release-tooling reproducibility and cleanup

While editing the workflow:

1. pin or deliberately version maturin rather than relying on an unbounded moving toolchain;
2. add `--locked` to release builds where supported;
3. keep PyPI OIDC trusted publication as-is;
4. preserve TestPyPI manual dispatch;
5. preserve production failure on duplicate version rather than silently skipping;
6. keep the sdist clean-install smoke;
7. avoid introducing third-party release orchestration unless necessary;
8. update `docs/INSTALLATION.md`, `docs/PYTHON_BINDINGS.md`, release docs, root README support statements, and CI/testing docs that describe the old matrix.

### Free-threaded Python

Do not enable `abi3t` in this implementation plan.

Create a separate future handoff only after the ordinary matrix is stable if free-threaded support is desired. Such a plan would need to address:

- version-specific `cp314t` wheel(s) for Python 3.14t;
- `abi3t-py315` wheel(s) for Python 3.15+;
- maturin >= 1.14;
- PyO3 free-threading safety declarations/qualification;
- additional wheel-family validation.

Keeping this separate prevents ordinary 3.9–3.15 support from tripling the artifact matrix.

## Verification

Minimum release-workflow verification before closure:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 check --workspace --locked
```

Python/package validation should include:

```text
authoritative pyproject metadata check
maturin build for every Tier A target
artifact-set validator
sdist build + clean install
Linux x86_64 cp39-abi3 install/runtime smoke on Python 3.9–3.15
native Linux AArch64 smoke
native macOS x86_64/arm64 smoke
native Windows x86_64/ARM64 smoke
ARMv7/musl target execution smoke through native hardware or QEMU/container as appropriate
TestPyPI dry publication before first expanded production release
```

Run the ordinary CI suite normally after push. Do not add unrelated protocol/differential suites to the release workflow.

## Final acceptance criteria

This plan is complete only when:

- [ ] Tier A produces ten validated ordinary-CPython wheel families plus one sdist, unless a specific target has been formally deferred with a concrete blocker;
- [ ] existing five wheel families remain supported;
- [ ] Linux AArch64 is natively smoke-tested;
- [ ] GNU ARMv7 and musllinux x86_64/AArch64/ARMv7 have executable smoke evidence;
- [ ] Windows ARM64 has native smoke evidence;
- [ ] the Linux x86_64 `cp39-abi3` artifact passes ordinary CPython 3.9–3.15 compatibility testing;
- [ ] support metadata/documentation agrees with the evidence;
- [ ] the artifact collector is matrix-driven and fail-closed rather than fixed at five wheels;
- [ ] a real ARM SBC qualification exists for every SBC architecture claimed as first-class;
- [ ] feasible Tier B targets are added or their blockers are documented;
- [ ] free-threaded Python remains explicitly separate;
- [ ] no API, runtime, feature-default, protocol, parity, or compatibility behavior changes.

## Expected implementation footprint

Likely files:

- `.github/workflows/publish-python.yml`;
- `crates/eggress-python/pyproject.toml`;
- `python/pyproject.toml`;
- `docs/INSTALLATION.md`;
- `docs/PYTHON_BINDINGS.md`;
- `docs/release/RELEASE_PROCESS.md`;
- `docs/CI_STATUS.md` and/or `docs/TESTING.md` if they enumerate packaging gates;
- root `README.md` if it states the supported matrix;
- small release validation scripts if needed.

Rust/Python runtime source changes are not expected. If a target requires runtime/source changes, isolate and justify them before proceeding.
