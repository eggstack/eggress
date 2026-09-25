# pproxy Maintenance Phase 1 — Contract Truth and Optional Feature Gates

## Status

**IMPLEMENTED**

## Closure

Implemented in `8c10331`. Verification: contract/manifest truth checks,
bounded optional-compat compile gate, and the workspace gate green on the
corrective head (`6171f73` plus this closure record).

## Parent roadmap

[`PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md`](PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md)

## Baseline

- Repository: `eggstack/eggress`
- Planning baseline: `6a5d5b7e6c37d0794775fc2e2c848a3aa948b219`
- Oracle: `pproxy==2.7.9` / `09d4752f17ed6787e1a073c93980eec019887ee3`
- Existing active contract: `docs/parity/pproxy_capability_manifest.toml`
- Maintained human matrix: `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- Existing routine CI: `.github/workflows/ci.yml` and path-scoped `.github/workflows/python-test.yml`

## Objective

Establish one truthful compatibility contract for the current implementation and close the verification hole around product-relevant optional compatibility features, without creating a new parity framework, another workflow, or a broad CI matrix.

This phase is intentionally first because structural refactoring should not proceed while active documentation disagrees about what the code is supposed to do.

## Confirmed problems

### A. Active compatibility documentation has drifted

`AGENTS.md` says the capability manifest and executable evidence are authoritative. The practical matrix is the maintained human-readable companion. Despite that, `docs/PPROXY_MIGRATION.md` currently repeats detailed supported/unsupported feature tables and contains contradictions relative to the manifest/matrix and current implementation.

Known examples to verify and correct include:

- Trojan is described as client-only in one section while another section says inbound/server support exists; current `docs/CAPABILITIES.md` and implementation support Trojan client/server roles.
- Compatibility `--daemon` is described as simply unsupported even though the maintained contract exposes a Linux opt-in `pproxy-daemon` implementation.
- Compatibility `--sys` is described as unsupported even though current runtime/manifest behavior applies and restores system proxy state on supported platforms.
- H3/QUIC, SSR/legacy crypto, and other optional surfaces are easy to phrase ambiguously because default CLI `full` intentionally does not enable every optional legacy/transport feature.

The problem is not merely stale prose. Repeated detailed status tables create additional de facto sources of truth and will continue to drift.

### B. Optional compatibility features are not exercised by the ordinary default feature path

`eggress-cli` currently defines:

```text
default = ["full"]
full = ["common", "extended", "operations", "reverse", "pproxy-compat"]
```

while SSH, QUIC/H3, `pproxy-legacy`, legacy crypto, and pproxy daemon behavior remain separate explicit features.

The ordinary Rust CI runs format, clippy, default workspace tests, and fuzz-target compilation. This is deliberately lean, but it means a source-level break that appears only when one of those optional compatibility features is enabled may not be detected on an ordinary pull request.

Cargo's resolver-v2 feature model makes this expected: default workspace tests do not substitute for explicit optional-feature selection. A product-relevant explicit feature bundle is therefore the appropriate bounded check.

## Governing constraints

1. Do not create a new capability manifest or generated parity report.
2. Do not create a new GitHub Actions workflow.
3. Do not add a routine OS matrix.
4. Do not turn external pproxy/shadowsocks interoperability suites into ordinary CI.
5. Do not add `cargo-hack`, `cargo-nextest`, or another CI dependency solely for this phase.
6. Do not use indiscriminate `--all-features` if that activates intentionally insecure/test-only combinations such as `insecure-quic`.
7. Do not change compatibility behavior merely to make documentation easier to state. Documentation follows actual behavior.
8. Do not broaden the frozen oracle beyond pproxy 2.7.9.
9. Keep historical plans and certification documents historical. Do not line-edit every old artifact.
10. The phase may edit active documentation, manifest validation/tests, and the existing CI workflow only.

## Workstream 1 — Reduce compatibility truth to the canonical contract plus maintained matrix

### Required audit

Inspect these active/current files against the canonical manifest and current source behavior:

- `README.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/CAPABILITIES.md`
- `docs/PPROXY_PARITY_SPEC.md`
- `docs/parity/README.md`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/CI_STATUS.md`
- `architecture/pproxy-compat.md`
- `architecture/cli.md`

Do not mechanically update historical files under `plans/` or release evidence unless an active document directly links to one as current authority.

### Required changes

1. Correct every demonstrated contradiction in active documentation, starting with Trojan, `--daemon`, and `--sys`.
2. Keep `docs/parity/pproxy_capability_manifest.toml` as the machine authority and the practical matrix as the detailed human status table.
3. Reduce `docs/PPROXY_MIGRATION.md` so it explains migration mechanics and important boundaries but does **not** maintain a second exhaustive supported/unsupported feature table.
4. Where the migration guide needs a feature example, link or refer to the maintained matrix rather than duplicating a long status inventory.
5. Keep `README.md` high level. It may summarize major capability groups and intentional boundaries but should not duplicate per-capability tier details that belong in the manifest/matrix.
6. Reconcile `docs/CAPABILITIES.md` language with the compatibility distinction: native Eggress capability does not automatically imply exact pproxy compatibility.
7. If `docs/PPROXY_PARITY_SPEC.md` is still an active source for vocabulary, ensure it describes current tier semantics only; historical implementation status must be clearly marked historical or removed from the active section.
8. Preserve the frozen oracle commit and version in all active compatibility documents that identify the reference implementation.

### Do not solve this with brittle prose parsing

Do not add a script that tries to parse arbitrary Markdown sentences to determine whether documentation is correct. The robust solution is to eliminate duplicated detailed status tables, not to create another parser for prose.

Existing machine-contract validation may be extended only for machine-readable invariants, such as:

- unique capability IDs;
- valid tier/status vocabulary;
- implementation/test references present where policy requires them;
- fixed oracle version/commit coherence;
- no known impossible combination such as a row simultaneously claiming unsupported runtime and matched behavior unless explicitly defined by the schema.

If existing `scripts/validate_pproxy_parity_manifest.py` and `eggress-testkit::canonical_manifest` already cover an invariant, reuse them rather than adding a second validator.

## Workstream 2 — Resolve the dependency-audit/CI wording mismatch

`docs/CAPABILITIES.md` currently describes dependency auditing as an in-CI security property, while `AGENTS.md` describes `cargo deny` / `cargo audit` as dependency-change and release-preparation checks rather than ordinary CI.

Determine which statement is current policy from `AGENTS.md`, `docs/CI_STATUS.md`, and `.github/workflows/ci.yml`.

Preferred outcome:

- keep routine CI lean as current policy requires;
- change capability/security wording so it does not imply `cargo deny` or `cargo audit` runs on every ordinary CI job if it does not;
- retain the documented dependency/advisory commands for dependency changes/releases;
- do not add an audit workflow merely to preserve stale prose.

Acceptance is truthful policy, not more automation.

## Workstream 3 — Add one bounded optional compatibility compile gate

### Why an explicit bundle

The product-relevant optional compatibility surfaces are intentionally not all enabled by the default `full` group. A default workspace test therefore cannot prove they continue to compile.

At this baseline, the relevant CLI-facing optional features are:

- `ssh`
- `quic`
- `pproxy-legacy`
- `legacy-crypto`
- `pproxy-daemon`

The existing `full` group should remain unchanged unless independent product semantics require otherwise. These features are optional by design; this phase is about verification, not making them defaults.

### Required CI change

Add **one** compile-only step to the existing Ubuntu Rust job in `.github/workflows/ci.yml`, after the ordinary workspace checks or at another sensible point:

```bash
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon \
  --bins
```

The implementer must validate that this exact bundle is internally compatible on the Linux CI host. If one feature cannot legally coexist with another, split the check into the minimum number of commands necessary **within the same existing job** and document the incompatibility. Do not create a matrix.

Do not include `insecure-quic` in the product compatibility gate. It is an explicit insecure compatibility/testing option, not a normal supported product feature that should define routine release confidence.

If another product-relevant opt-in feature is discovered during implementation, include it only if it backs an active README/matrix capability claim and can be compiled cheaply in the same job.

### Scope of the gate

This is compile verification, not another full test suite. Do not run all workspace tests again under every optional feature combination. Focused optional-feature tests may remain local/manual unless the feature is fragile enough that one tiny deterministic test is necessary to make the compile gate meaningful.

## Workstream 4 — Align CI and testing documentation

Update `docs/CI_STATUS.md`, `docs/TESTING.md`, and `AGENTS.md` only as necessary to record the new bounded compile step and its purpose.

Required wording should distinguish:

- ordinary default workspace tests;
- optional product-feature compile coverage;
- path-scoped Python smoke;
- opt-in external interoperability/certification;
- dependency/advisory checks for dependency changes/releases.

Do not introduce the language of "full certification" for ordinary CI.

## Required tests and verification

During implementation:

```bash
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
cargo test -p eggress-testkit canonical_manifest
cargo test -p eggress-pproxy-compat
```

Validate the optional bundle locally on Linux-compatible tooling:

```bash
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon \
  --bins
```

At closure:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Python smoke is required only if Python compatibility documentation changes are accompanied by Python code changes; documentation-only corrections do not require rebuilding the wheel.

External pproxy differential tests are required only if implementation semantics or a compatibility tier materially changes. Pure correction of obviously stale prose to already-proven behavior does not require rerunning the full external oracle suite.

## Acceptance criteria

Phase 1 is complete only when all are true:

- `docs/PPROXY_MIGRATION.md` no longer contains contradictory Trojan, daemon, system-proxy, or optional-transport status claims;
- detailed per-feature compatibility truth lives in the canonical manifest and maintained practical matrix rather than being duplicated in multiple active Markdown tables;
- README/migration/capability documents accurately distinguish native capability from pproxy compatibility tier;
- all active references identify pproxy 2.7.9 / `09d4752f17ed6787e1a073c93980eec019887ee3` as the compatibility oracle where an oracle is named;
- dependency-audit wording matches actual current CI/release policy;
- no new parity manifest/report/dashboard/prose-validation system has been introduced;
- the existing `.github/workflows/ci.yml` contains a bounded optional compatibility compile check;
- the check covers SSH, QUIC/H3, pproxy legacy/SSR, legacy crypto, and Linux pproxy-daemon code through the CLI feature forwarding path, or documents a minimal split if a combination cannot compile together;
- `insecure-quic` is not accidentally promoted into the routine product feature gate;
- no new workflow or OS matrix exists;
- routine CI documentation describes the new gate accurately;
- focused manifest/compatibility tests and the broad repository gate pass.

## Explicit non-goals

Do not implement PF, new ciphers, SSR UDP, external plugins, QUIC UDP, reverse TLS wire compatibility, Trojan UDP, MASQUE, TPROXY, certificate reload, or post-2.7.9 upstream features in this phase.
