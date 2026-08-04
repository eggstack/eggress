# Lean Runtime and Delivery Roadmap

## Status

**PLANNED**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Audit baseline: `f809c7d8c83e19955e77e02879d2d2f160222efe`
- Product contract: preserve the current Rust CLI, embeddable Rust API, Python package, and bounded `pproxy==2.7.9` compatibility surface.

## Purpose

Reduce avoidable binary and dependency weight, correct the Python wheel delivery gap, and add focused regression coverage for the highest-risk lifecycle paths without adding features, redesigning the proxy, or rebuilding the repository's former verification ceremony.

This is a corrective and reductive line of work. It is not a new parity phase. It must not be used to add protocols, broaden compatibility claims, introduce a plugin framework, reorganize the workspace wholesale, or create a larger routine CI apparatus.

## Governing constraints

1. The current default/full build retains the existing supported feature surface.
2. A smaller build may omit optional protocol and operational groups only through explicit Cargo feature selection.
3. Public Rust and Python APIs remain source-compatible unless an incorrect packaging declaration must be corrected.
4. Default `cargo install --path crates/eggress-cli` continues to install both `eggress` and `pproxy`.
5. Crates.io publication remains manual.
6. PyPI wheel construction and publication may remain in GitHub Actions because platform wheels cannot be produced practically from one local host, but release automation stays limited to wheel/sdist build, clean-install smoke verification, and registry publication.
7. Routine hosted CI remains one Rust smoke workflow and one path-scoped Python smoke workflow. No routine OS, architecture, Python-version, benchmark, fuzz, audit, soak, or differential matrices are added.
8. Existing tests and observability are reused. No test daemon, task registry, custom evidence framework, benchmark system, or generalized fault-injection system is introduced.
9. No new protocol, transport, scheduler, routing primitive, admin endpoint, metrics backend, daemonization mode, or system-integration capability is in scope.
10. Net maintenance complexity must decrease or remain nearly flat. Feature gates that do not materially reduce the linked dependency graph or artifact size are removed rather than retained for theoretical purity.

## Problem statement

The workspace is well decomposed by source responsibility, but the primary build remains broad:

- the workspace enables Tokio's `full` feature set;
- `eggress-runtime` unconditionally depends on admin, metrics, UDP, TLS, Shadowsocks, and reverse-proxy crates;
- `eggress-cli` unconditionally depends on compatibility and system-proxy crates and emits both binaries without feature requirements;
- the root workspace does not define a size-conscious production profile;
- the PyPI workflow currently builds one Ubuntu/Python 3.12 wheel despite declaring Python 3.9 through 3.13 and cross-platform support;
- the Python extension does not currently declare a stable ABI feature, so one CPython 3.12 wheel cannot satisfy the declared Python range;
- HTTP forwarding, UDP association lifecycle, reload, shutdown, and cancellation remain the highest-probability areas for latent lifecycle defects;
- prior cleanup reduced CI and documentation ceremony, and this pass must not recreate it.

## Target state

At closure:

1. The existing full/default build behaves as before.
2. One documented lean local build is composed from a small number of feature groups rather than a large per-capability feature taxonomy.
3. Optional operational and advanced-protocol dependencies are absent from the lean build's dependency graph when cleanly separable.
4. Tokio uses an explicit required feature set instead of `full`.
5. Release profiles provide a normal production build and an optional size-oriented build without imposing unsafe panic/ABI choices on Python embedding.
6. Full and lean artifact sizes are recorded during implementation but do not become routine CI gates.
7. Python release artifacts truthfully support the Python versions and operating systems claimed by package metadata.
8. Production PyPI publication waits for the complete bounded wheel set, sdist, version-coherence check, and clean-install smoke checks.
9. Existing high-risk HTTP, UDP, reload, and shutdown paths have focused deterministic regression coverage.
10. No new permanent completion report, evidence bundle, certification workflow, or broad test matrix is introduced.

## Feature-boundary policy

Use a small set of user-facing feature groups. Exact internal dependency wiring may change during implementation, but the semantic grouping remains bounded:

- `common`: HTTP, SOCKS, direct routing, TLS, and UDP capabilities needed for ordinary local proxy deployment;
- `extended`: Shadowsocks, Trojan, WebSocket, and other already-supported non-common protocol adapters with meaningful dependency cost;
- `operations`: admin, metrics, and system-proxy integration;
- `reverse`: the existing reverse/backward proxy capability;
- `pproxy-compat`: compatibility translation and the `pproxy` binary;
- `full`: the union preserving the current product surface.

The default feature set remains `full`. The documented lean build uses `--no-default-features --features common`. Do not introduce independent public toggles for every parser, scheduler, URI form, metric, or submodule.

If `common` cannot exclude a dependency without invasive conditional types or duplicated runtime state, keep the dependency. Do not create indirection solely to remove a few kilobytes.

## Registered execution plans

| Order | Plan | Purpose | Dependency |
|---|---|---|---|
| 1 | [`LEAN_RUNTIME_PHASE_1_FEATURE_BOUNDARIES.md`](LEAN_RUNTIME_PHASE_1_FEATURE_BOUNDARIES.md) | Add bounded Cargo feature groups, reduce Tokio features, add release profiles, and measure full versus lean artifacts. | None |
| 2 | [`LEAN_RUNTIME_PHASE_2_FOCUSED_RELIABILITY.md`](LEAN_RUNTIME_PHASE_2_FOCUSED_RELIABILITY.md) | Add only missing deterministic regression coverage for HTTP framing, UDP lifecycle, reload, cancellation, and resource cleanup. | Phase 1 feature topology stable |
| 3 | [`LEAN_RUNTIME_PHASE_3_PYTHON_DELIVERY.md`](LEAN_RUNTIME_PHASE_3_PYTHON_DELIVERY.md) | Correct Python ABI/platform packaging and make the existing release-only workflow build and verify the bounded wheel set. | Phases 1-2 complete |

These three plans are the complete implementation set for this roadmap. Do not split them into plan-per-test, plan-per-platform, or plan-per-crate files.

## In scope

- workspace and crate Cargo feature definitions;
- optional dependency wiring and narrowly necessary `cfg` boundaries;
- explicit Tokio feature selection;
- normal and size-oriented release profiles;
- full/lean artifact and dependency-tree measurement;
- `pproxy` binary `required-features` handling while preserving default installation behavior;
- Python stable-ABI evaluation and adoption when compatible with the existing bindings;
- a bounded PyPI wheel/sdist build set for the operating systems the project claims to support;
- package metadata correction where current declarations are inaccurate;
- tag/package version-coherence checks;
- clean wheel installation and import/startup smoke checks;
- focused tests for currently under-covered lifecycle invariants;
- concise updates to active build, installation, testing, and release documentation.

## Out of scope

- new proxy protocols or transport roles;
- full `pproxy` parity expansion;
- SOCKS BIND, SSH, QUIC/HTTP/3, SSR, TLS interception, daemonization, or connection-pool work;
- crate merging or a workspace-wide architecture rewrite;
- a generic dependency-injection, plugin, backend, or capability registry;
- replacing boxed stream boundaries with a generic type architecture;
- a new CLI configuration language or API redesign;
- per-protocol micro-features beyond the bounded groups above;
- automatic crates.io publication;
- GitHub Releases, standalone native binary distribution, containers, checksums, signatures, SBOMs, or provenance systems;
- routine CI operating-system or architecture matrices;
- code coverage, benchmark, cargo-bloat, audit, fuzz, soak, or external-oracle gates;
- new generated parity reports or completion/evidence documents;
- performance optimization unrelated to demonstrated binary/dependency weight or a defect exposed by focused tests.

## Required measurements

Measurements are implementation evidence, not permanent CI gates. Record them in the implementation commit or pull-request summary:

```bash
cargo tree -p eggress-cli -e features
cargo build -p eggress-cli --release
cargo build -p eggress-cli --release --no-default-features --features common
ls -lh target/release/eggress target/release/pproxy
```

Use isolated target directories or clean between configurations. `cargo bloat` may be used interactively if already installed, but must not become a repository dependency or required workflow step.

Retain the lean feature boundary only when at least one is true:

- the lean binary is at least 5% smaller than the full binary;
- one or more substantial optional dependency families are absent from `cargo tree`;
- the lean build materially reduces compile time or platform build complexity and this is documented.

If none is true, revert unnecessary `cfg` complexity and keep only low-risk Tokio/profile changes.

## Verification policy

During implementation, run the narrowest affected crate tests. At roadmap closure, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

For Python-facing changes, build a wheel into a clean virtual environment and run the existing Python and compatibility suites as described in `AGENTS.md`.

External interoperability, certification, benchmark, fuzz, soak, and audit commands remain opt-in and are not closure requirements unless the implementation changes the corresponding protocol parser, compatibility claim, or dependency set.

## Roadmap acceptance criteria

This roadmap is complete only when all are true:

- all three registered plans are implemented or explicitly closed as unnecessary with evidence;
- full/default behavior and supported public APIs remain intact;
- the lean build is documented and materially leaner, or non-beneficial feature complexity was removed;
- Tokio no longer uses the workspace `full` feature set;
- Python package metadata, ABI, wheel set, and publication behavior agree;
- PyPI publication is blocked on clean artifact verification while crates.io remains manual;
- targeted lifecycle regressions are covered without broad new test infrastructure;
- the repository still has only the existing two routine smoke workflows;
- no new completion report or evidence bundle is added;
- active documentation accurately states full versus lean build behavior and supported Python distribution targets.

## Closure record

When implementation is complete, update this file in place:

- change `PLANNED` to `IMPLEMENTED`;
- add the implementation commit range;
- summarize full/lean size and dependency results;
- link the focused test locations and final workflow file;
- record any explicitly rejected optimization and why.

Do not create a separate roadmap-completion document.