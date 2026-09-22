# Manual crates.io Publishing Simplification

## Status

**IMPLEMENTED — 2026-09-22**

Qualification showed `cargo publish --workspace` is nightly-only on the
pinned stable toolchain (Cargo 1.89), so Outcome B landed:
`scripts/publish-crates.py` derives order from `cargo metadata`, supports
`--list`/`--dry-run`/`--execute`, resumes by skipping already-published
versions, and uses bounded reactive backoff instead of the fixed 660-second
delay. `scripts/publish-remaining.sh` is now only a compatibility wrapper;
`docs/release/RELEASE_PROCESS.md`, the release skill, `AGENTS.md`, and the
architecture index agree on the one-command path. Graph/recovery/rate-limit
behavior is covered by `tests/scripts/test_publish_crates.py` (no crates.io
writes). No GitHub Actions crates.io publication was added.

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `c493eab803654d1316150a9837fd01ee46b9b87d`
- Parent roadmap: [`DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md`](DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md)
- Workspace release line: `1.0.7`
- Current helper: `scripts/publish-remaining.sh`
- Current policy authority: `docs/release/RELEASE_PROCESS.md`

## Objective

Reduce crates.io release operator burden and remove hand-maintained dependency-order duplication while keeping crates.io publication manual, local, explicit, verified, and resumable.

The desired operator experience is one deliberate local command from a clean release checkout, not 28 manually supervised `cargo publish -p ...` operations.

This plan does not authorize automated crates.io publication from GitHub Actions.

## Current state

### Workspace shape

The root workspace currently contains 28 publishable `eggress-*` crates plus the root `eggress-bench` package, which has `publish = false`.

All internal published dependencies are declared from the workspace with exact same-version requirements such as:

```toml
eggress-core = { version = "=1.0.7", path = "crates/eggress-core" }
```

This is a strong fit for workspace-wide release tooling because the dependency graph and lockstep version are already machine-readable.

### Existing helper

`scripts/publish-remaining.sh`:

- manually lists every crate in a tiered dependency order;
- documents dependency-edge exceptions in shell comments;
- publishes one crate at a time;
- waits for crates.io visibility;
- defaults to `PUBLISH_DELAY_SECONDS=660` after every successful publication;
- has a `--dry-run` mode;
- correctly refuses `--no-verify`.

The script solved an older Cargo/tooling gap, but the tier list now duplicates Cargo metadata and is expensive to maintain.

### Modern Cargo capability

Current Cargo supports:

```bash
cargo publish --workspace
cargo publish --workspace --exclude <package>
cargo publish --workspace --dry-run
```

Workspace publishing verifies the selected packages before upload and publishes according to the workspace dependency graph.

For Eggress, the root non-publishable package means the relevant native command must exclude `eggress-bench` explicitly.

Workspace publication is not atomic. If publication stops after some crates have reached crates.io, rerunning the exact same command may encounter already-published versions. Therefore native workspace publication must be qualified together with a partial-release recovery strategy before replacing the current helper.

## Governing constraints

1. crates.io publication stays manual and operator-driven.
2. Do not add tag-triggered or branch-triggered crates.io publication.
3. Do not add a GitHub Actions crates.io token, trusted publisher, or crates.io OIDC configuration.
4. Do not merge crates, make existing published crates private, or change public Rust API solely to reduce release count.
5. Do not change lockstep workspace versioning in this campaign.
6. Do not weaken exact internal same-version pins.
7. Do not use `--no-verify`.
8. Do not use `--allow-dirty` in the normal release path.
9. Do not encode the dependency graph manually in a new language or new table.
10. Use Cargo's own workspace graph or data from `cargo metadata`.
11. The normal release path must be safe to rerun after an interrupted/partial publication.
12. Do not silently skip a crate whose published artifact differs from the intended release commit; crates.io versions are immutable, so any mismatch is a roll-forward defect.
13. Avoid a new third-party release dependency unless Cargo + a small repository-local helper cannot satisfy the requirements.

## Phase 1 — Qualify native workspace publication

On the current clean workspace, qualify Cargo's native multi-package packaging/publish behavior before changing the helper.

Required commands:

```bash
cargo publish --workspace --exclude eggress-bench --locked --dry-run
cargo package --workspace --exclude eggress-bench --locked
```

If the installed Cargo version does not support one of those exact option combinations, record the actual supported invocation rather than dropping verification semantics.

Confirm:

1. exactly the intended 28 crates are selected;
2. `eggress-bench` is excluded because `publish = false`;
3. dependency ordering is derived successfully from the workspace;
4. all packages verify without `--no-verify`;
5. internal path+version dependencies are rewritten/resolved correctly for packaging;
6. optional normal/build dependencies are handled correctly;
7. path-only dev-dependencies do not incorrectly constrain publication;
8. the full workspace can be validated before any external upload.

Capture the Cargo version used for qualification.

### Acceptance

- [ ] Native Cargo identifies the complete intended publish set without a manual crate list.
- [ ] Workspace dry-run/package succeeds with normal verification.
- [ ] No package metadata defect is hidden behind `--no-verify`.
- [ ] The result demonstrates that the shell tier list is not required for ordinary dependency ordering.

## Phase 2 — Define the preferred one-command manual path

Preferred Outcome A is native Cargo:

```bash
cargo publish --workspace --exclude eggress-bench --locked
```

Use this as the normal release command if qualification demonstrates that it:

- publishes the Eggress graph in a valid dependency order;
- waits adequately for same-release dependencies to become resolvable;
- does not require fixed sleeps;
- behaves predictably with the user's normal crates.io registry configuration;
- provides actionable failure output.

Do not pass `--registry crates-io` merely for verbosity if local Cargo source replacement/mirror configuration makes that behavior less reliable. Use the simplest default crates.io configuration that resolves correctly in qualification.

### Outcome A acceptance

- [ ] One native Cargo command publishes the full intended workspace from a clean release checkout.
- [ ] No fixed inter-crate sleep exists in the normal path.
- [ ] No hand-maintained topological order exists in the normal path.
- [ ] Release documentation includes exact preflight and recovery instructions.

If Outcome A cannot meet the resume/recovery requirements cleanly, implement Outcome B.

## Phase 3 — Outcome B: graph-derived resumable helper

If native workspace publication is not sufficiently resumable for Eggress, replace `scripts/publish-remaining.sh` with a small repository-local helper whose graph is derived from `cargo metadata`.

Preferred implementation language is Python 3 standard library because the release process already uses Python for metadata/preflight logic and this avoids adding a new Rust release-tool crate solely for orchestration.

A Rust `xtask` is acceptable only if repository conventions strongly favor it and it does not become another separately published crate.

### Required interface

A suggested interface:

```text
scripts/publish-crates.py --list
scripts/publish-crates.py --dry-run
scripts/publish-crates.py --execute
```

Default invocation without `--execute` must not mutate crates.io.

### Graph construction

Use:

```bash
cargo metadata --format-version 1
```

or an equivalently authoritative Cargo source.

Derive:

- workspace packages;
- `publish = false` exclusions;
- internal package dependency edges;
- release version;
- manifest paths.

The helper must not contain a manually ordered array of crate names.

For publication order, include internal dependencies that Cargo must resolve from the registry when packaging a crate, including optional normal/build dependencies. Exclude path-only dev-dependency edges where Cargo excludes them from the published dependency graph.

Topologically sort the selected publishable packages and fail on any cycle or inconsistent metadata rather than inventing an order.

### Version/pin validation

Before any upload:

1. run `scripts/release-preflight.sh --check-versions-only` or reuse its logic;
2. verify every selected package version equals the workspace release version;
3. verify every published internal dependency's registry requirement resolves to the same intended exact version;
4. verify the working tree is clean;
5. verify the operator is on the intended release commit/tag according to the maintained release procedure.

Do not duplicate version logic unnecessarily. Prefer calling the existing preflight script.

### Registry state and resume behavior

Before publishing each crate/version, query crates.io for that exact package/version.

States:

- not published -> eligible to publish when internal prerequisites are visible;
- already published -> skip only after confirming that this is the expected version in a partial-release resume;
- unavailable/transient registry error -> retry conservatively or stop with an actionable message.

The helper cannot prove remote crate bytes equal the local checkout merely from version existence. Therefore the release documentation must treat an already-published same version as immutable external state: if there is any reason to believe it came from the wrong commit, stop and roll forward to a new patch version rather than continuing.

### Publish behavior

For each unpublished ready crate:

```bash
cargo publish -p <crate> --locked
```

Requirements:

- no `--no-verify`;
- no `--allow-dirty`;
- use normal Cargo authentication;
- let Cargo perform its own package verification;
- after success, confirm the exact version is visible before moving to dependents;
- do not sleep a fixed 660 seconds unconditionally.

### Rate limiting and transient failure

Do not encode the historical new-crate-name cooldown as a permanent delay for every normal version release.

On HTTP 429 or a clear registry throttle:

1. honor an explicit retry delay if Cargo/registry output supplies one;
2. otherwise use bounded backoff;
3. re-check whether the crate/version became visible before retrying;
4. stop after a bounded retry budget with instructions to rerun `--execute`.

For network/5xx failures, use similarly bounded retry.

For manifest/package/compile failures, do not retry automatically; those are release defects.

### Resume behavior

A second `--execute` invocation after interruption must:

1. recompute the graph from current Cargo metadata;
2. verify version/preflight again;
3. identify already-published exact versions;
4. skip them;
5. continue with the first unpublished dependency-ready package.

This is the primary advantage of Outcome B over a bare workspace publish command.

### Acceptance

- [ ] No hard-coded tier list remains.
- [ ] No fixed unconditional 660-second sleep remains.
- [ ] A partial release can be resumed safely with the same command.
- [ ] Dependency order comes from Cargo metadata.
- [ ] Package verification is never bypassed.
- [ ] Registry throttling is handled reactively rather than by multi-hour unconditional waiting.

## Phase 4 — Dry-run semantics

The current `--dry-run` loops over packages independently. That can be misleading for dependent same-version workspace crates if the registry does not yet contain the version.

Use Cargo's workspace-wide dry-run/package capability as the authoritative pre-publication package verification:

```bash
cargo publish --workspace --exclude eggress-bench --locked --dry-run
```

If Outcome B is implemented, its `--dry-run` should:

1. run the workspace-wide Cargo dry-run first;
2. print the computed graph/order;
3. query current registry state without uploading;
4. identify which crates would publish vs skip;
5. exit nonzero on version/pin/metadata/graph defects.

Do not simulate success by invoking per-crate dry-runs in an order that cannot resolve unpublished same-version dependencies.

### Acceptance

- [ ] One dry-run validates the complete publishable workspace before mutation.
- [ ] The dry-run reports the actual publish set and dependency order.
- [ ] Dry-run never requires an existing same-version partial publication to succeed.

## Phase 5 — Release documentation and policy cleanup

Update `docs/release/RELEASE_PROCESS.md` to make the crates.io path short and explicit.

The maintained release flow should read approximately:

```text
1. verify release candidate
2. run version/preflight checks
3. run workspace-wide publish dry-run
4. execute one manual local publish command
5. verify top-level cargo install / representative public libraries
6. tag/push for PyPI + binary workflows
```

Keep the explicit policy that crates.io publication is manual.

Remove obsolete statements that require a maintainer to publish every crate individually or wait a fixed interval after every crate.

Document partial-publication recovery.

Keep the roll-forward rule: if a published artifact is incorrect, increment the version; never overwrite or retag.

### Acceptance

- [ ] Release docs contain one canonical crates.io command path.
- [ ] Manual/operator-driven publication policy is unchanged.
- [ ] Recovery from partial publication is documented.
- [ ] No crates.io automation is introduced into GitHub Actions.

## Phase 6 — Helper compatibility and regression tests

If a graph-derived helper is implemented, add focused tests that do not touch crates.io.

Fixtures/tests should prove:

- `publish = false` root package is excluded;
- all 28 current publishable crates are discovered;
- the computed order places every internal dependency before its consumer;
- optional normal/build internal dependencies constrain order;
- path-only dev-dependencies do not create false release edges;
- a simulated already-published subset is skipped;
- the next unpublished crate becomes eligible only after prerequisites;
- cycle/inconsistent-version detection fails closed;
- dry-run cannot invoke mutation;
- `--execute` command construction never includes `--no-verify` or `--allow-dirty`;
- rate-limit retry is bounded and testable without sleeping real minutes.

Use dependency injection/mocked subprocess/HTTP boundaries rather than live crates.io writes.

### Acceptance

- [ ] The release helper's graph logic is deterministically tested.
- [ ] Tests do not publish crates or require crates.io credentials.
- [ ] No test introduces long real sleeps.

## Verification

Required before closure:

```bash
./scripts/release-preflight.sh --check-versions-only
cargo publish --workspace --exclude eggress-bench --locked --dry-run
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 check --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
```

If a new Python helper/test is added, run its focused unit tests/linting according to repository conventions.

Do not perform a real crates.io publication solely to prove the implementation plan. The next actual release is the production qualification of upload behavior; use dry-run plus mocked recovery/rate-limit tests beforehand.

## Final acceptance criteria

This plan is complete only when:

- [ ] the normal crates.io release is one deliberate local command after preflight;
- [ ] the set of publishable crates is discovered from Cargo/workspace metadata;
- [ ] hand-maintained dependency tiers are removed from the active release path;
- [ ] workspace-wide dry-run verifies all selected packages before external mutation;
- [ ] no normal release path uses `--no-verify` or `--allow-dirty`;
- [ ] no fixed unconditional 660-second per-crate delay remains;
- [ ] partial publication has a tested/documented resume path;
- [ ] already-published versions are never overwritten;
- [ ] a wrong published artifact requires a new version/roll-forward;
- [ ] crates.io credentials remain local to the operator;
- [ ] no GitHub Actions crates.io publication/trusted-publisher configuration is added;
- [ ] public crates, versions, APIs, and dependency pins remain unchanged except for normal future release-version bumps;
- [ ] release documentation and planning state are internally consistent.

## Expected implementation footprint

Likely files:

- `scripts/publish-remaining.sh` (remove, reduce to compatibility wrapper, or replace);
- optionally `scripts/publish-crates.py` plus focused tests;
- `docs/release/RELEASE_PROCESS.md`;
- `AGENTS.md`, `docs/CI_STATUS.md`, or `docs/TESTING.md` only where they describe the old publication process;
- parent/registry planning files at closure.

No Cargo workspace restructuring is expected.
