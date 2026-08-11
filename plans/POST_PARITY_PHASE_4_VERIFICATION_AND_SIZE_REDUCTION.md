# Post-Parity Phase 4 — Verification and Size Reduction

## Status

**PLANNED**

Parent roadmap: `POST_PARITY_CORRECTIVE_AND_REDUCTION_ROADMAP.md`

Depends on: Phases 1-3 should be complete so final measurements describe the
corrected product.

## Problem statement

Routine CI has already been reduced to a proportionate shape:

- one Ubuntu Rust smoke job for fmt, clippy, and locked workspace tests;
- one path-scoped Ubuntu/Python 3.12 smoke job;
- one release-only PyPI workflow for the native wheel matrix and publication.

The remaining excess is mostly outside routine CI:

- historical parity/certification records are numerous and should not be product
  acceptance dependencies;
- manifest validation has accumulated checks that can become self-referential if
  it validates planning prose instead of executable behavior;
- the PyPI workflow performs both strong install-smoke validation and weaker
  archive/filename bookkeeping that partly duplicates it;
- further binary-size work has previously produced modest gains, so no additional
  feature splitting should occur without current linked-code measurements.

The goal is reduction, not a new optimization program.

## Objective

Keep the behavioral gates that catch real defects, remove redundant verification
that maintains descriptions of descriptions, and perform one evidence-based
binary-size pass with explicit stop conditions.

## Workstream A — Protect the current CI floor

Do not remove:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

from `.github/workflows/ci.yml`.

Do not broaden the Rust smoke workflow into OS matrices, external oracle runs,
fuzzing, benchmarks, audits, or soak tests.

Keep the Python smoke workflow path-scoped and single-version unless a concrete
Python compatibility defect requires otherwise.

### Lean-build contract

The README documents a supported common-only build. Add at most one cheap compile
check if current CI does not exercise it:

```bash
cargo check -p eggress-cli --no-default-features --features common
```

Prefer adding this to the existing Rust job rather than creating another
workflow/job.

Only keep it if it protects a public build command and adds negligible runtime.

## Workstream B — Remove planning-prose verification coupling

Search tests/scripts for assertions against:

```text
plans/
completion record wording
implementation commit placeholders
historical certification prose
command transcript text
```

Tests may verify that active source-of-truth files exist and obey machine-readable
contracts. They should not require historical plan prose to remain synchronized
with runtime behavior.

Preserve validation for:

- canonical manifest schema/version;
- unique capability IDs;
- valid tier/status values;
- contradictions that would make a compatibility claim impossible;
- executable reporter/manifest agreement established in Phase 2.

Remove or demote checks that only prove historical records contain expected
sentences.

Do not delete historical plans wholesale. They are provenance, not product
inputs.

## Workstream C — Simplify PyPI release verification without reducing delivery

The release workflow must continue to produce:

```text
Linux x86_64 wheel
Linux aarch64 wheel
macOS x86_64 wheel
macOS arm64 wheel
Windows x86_64 wheel
one sdist
```

and retain OIDC trusted publishing.

Preserve strong evidence:

1. version coherence before publication;
2. all five wheels build;
3. representative installed-wheel smoke on Linux, macOS, and Windows;
4. stable-ABI compatibility smoke at the declared low/high Python boundaries;
5. clean sdist installation and smoke test;
6. production publication fails on an already-existing version.

Candidates for removal/reduction if they are redundant after install smoke:

- hand-written parsing of exact wheel filename token counts;
- archive content `grep` checks for files whose absence would already make the
  clean install/import smoke fail;
- duplicate artifact-set bookkeeping that does not catch a failure outside the
  build/install steps.

Do not remove target coverage merely to shorten YAML.

## Workstream D — Measure binary composition

Before changing dependencies/features, capture isolated measurements from the
same toolchain and clean target directories:

```bash
rustc --version
cargo --version

CARGO_TARGET_DIR=target/measure-full \
  cargo build -p eggress-cli --release
stat -c '%n %s' target/measure-full/release/eggress \
                 target/measure-full/release/pproxy

CARGO_TARGET_DIR=target/measure-small \
  cargo build -p eggress-cli --profile release-cli-small
stat -c '%n %s' target/measure-small/release-cli-small/eggress \
                 target/measure-small/release-cli-small/pproxy

CARGO_TARGET_DIR=target/measure-common \
  cargo build -p eggress-cli --release \
  --no-default-features --features common
stat -c '%n %s' target/measure-common/release/eggress
```

Use the platform-equivalent byte-count command on macOS if needed.

Also inspect:

```bash
cargo tree -p eggress-cli -e normal
cargo tree -d
```

If `cargo-bloat` is already available or can be installed locally without
changing repository requirements:

```bash
cargo bloat -p eggress-cli --release --bin eggress --crates
cargo bloat -p eggress-cli --release --bin pproxy --crates
```

The measurement results belong in the implementation commit/PR summary, not a new
permanent evidence report.

## Workstream E — Conditional size fixes only

A size change is authorized only when measurement identifies a concrete linked
dependency/code family that can be removed from a build without reducing the
selected feature set.

Prioritize low-risk cases:

- accidental default-feature activation on internal dependencies;
- a dependency linked but unused by the selected feature group;
- a CLI-only code path that can use the existing `release-cli-small` profile;
- duplicated compatibility/reporting machinery eliminated by Phase 2;
- operational/export code accidentally retained in `common` despite current
  documented feature boundaries.

Do not:

- create per-protocol micro-features;
- split every metric counter behind cfg gates;
- replace rustls/Tokio/h2/crypto libraries solely for size;
- merge crates for binary size;
- add allocator experiments;
- use UPX;
- require nightly Rust;
- use `panic = "abort"` for the Python extension or embeddable library.

### Materiality threshold

Treat a change as worth keeping when it achieves either:

- >=5% reduction in the affected distributed binary with no feature loss; or
- >=250 KiB reduction from a clearly unnecessary dependency/code path with a
  trivial maintenance cost.

A smaller result may be kept only when the code simplification itself is clearly
valuable. Do not retain architecture complexity for a sub-threshold size win.

## Documentation

Update only active docs affected by actual changes:

```text
docs/CI_STATUS.md
docs/TESTING.md
docs/release/RELEASE_PROCESS.md
README.md
docs/architecture/*
AGENTS.md
```

Do not create a new verification manual or binary-size dashboard.

## Explicit acceptance criteria

Phase 4 is complete only when:

1. The existing Rust fmt/clippy/workspace-test smoke gate remains intact.
2. The existing path-scoped Python smoke gate remains intact.
3. No routine CI job requires an external pproxy installation/oracle.
4. No routine CI job requires soak tests.
5. No routine CI job requires fuzzing.
6. No routine CI job requires benchmarks.
7. No routine CI job requires `cargo audit`/`cargo deny` for unrelated pushes.
8. If the documented common-only build is otherwise unprotected, one cheap
   `cargo check -p eggress-cli --no-default-features --features common` check is
   added to an existing job rather than a new matrix.
9. Tests no longer assert historical plan/completion prose solely to keep records
   synchronized with implementation.
10. Canonical manifest validation still checks machine-readable contract
    integrity and reporter agreement.
11. Historical plan files remain non-authoritative provenance.
12. PyPI release still builds exactly the approved five platform wheels and one
    sdist.
13. PyPI release still smoke-installs wheels on representative Linux, macOS, and
    Windows runners.
14. PyPI release still verifies stable-ABI compatibility at the supported Python
    boundary versions.
15. PyPI release still performs a clean sdist install/smoke.
16. PyPI publication still uses OIDC trusted publishing.
17. Production publication still fails rather than skipping an existing release.
18. Redundant artifact-name/archive-content checks are removed only where the
    retained build/install smoke provides an equal or stronger guarantee.
19. Fresh full/default, CLI-small, and common-only binary byte counts are
    recorded during implementation.
20. Dependency-tree inspection is performed before any size code change.
21. If `cargo-bloat` is locally available, its crate-level output is used to guide
    changes; it is not added as a repository requirement.
22. No size change removes a default/full feature.
23. No size change alters the bounded pproxy compatibility contract.
24. No size change introduces a new public feature group without a demonstrated
    necessity.
25. Any kept size optimization reports before/after byte counts using comparable
    builds.
26. Any optimization below the materiality threshold is rejected unless it also
    clearly simplifies code.
27. If no worthwhile size reduction is found, the phase closes without
    speculative architecture changes.
28. Active CI/release documentation matches the resulting workflows.
29. `cargo fmt --all -- --check` passes.
30. `cargo clippy --workspace --all-targets -- -D warnings` passes.
31. `cargo test --workspace --locked` passes.
32. Python smoke tests pass if Python/release packaging files change.

## Stop conditions

Stop binary-size work immediately when the next candidate requires any of:

- a runtime snapshot redesign;
- a second parallel metrics architecture;
- widespread conditional compilation through protocol algorithms;
- replacement of mature crypto/network dependencies;
- a new supported build matrix;
- a feature reduction.

At that point, record the measured result in the implementation summary and close
the phase. The roadmap values simplicity over a smaller number in `ls -l`.
