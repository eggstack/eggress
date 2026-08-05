# pproxy Corrective Phase 4 — Contract, CI, and Planning Closure

## Status

**PLANNED**

## Parent roadmap

[`PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`](PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md)

## Dependencies

Phases 1 through 3 must be complete. This phase documents and validates final behavior; it must not guess the outcome of unresolved CLI, Python, or feature-topology decisions.

## Objective

Establish a small, internally consistent source-of-truth set for practical `pproxy==2.7.9` compatibility, repair stale planning status and links, make Python smoke CI cover all transitive implementation paths, and close the corrective line without adding new verification ceremony.

This phase is documentation and workflow correction backed by executable tests. It is not another implementation phase, certification campaign, or release redesign.

## Current problems to resolve

- The repository names multiple parity manifests, matrices, specifications, freezes, inventories, scenario indexes, completion records, and historical roadmaps as though they are concurrently authoritative.
- `docs/parity/pproxy_2_7_9_strict_manifest.toml` contains stale option arity and support claims.
- `docs/PPROXY_PARITY_SPEC.md` contains an incorrect upstream source reference and stale claims about Trojan, least-connections scheduling, and `--reuse`.
- historical freeze/version records refer to old package versions and old unsafe-code policy wording.
- current code uses workspace `unsafe_code = "deny"`; active documents must not claim a different policy.
- practical parity documents correctly avoid a full aggregate claim, but older maximal-parity files remain easy to mistake for current acceptance gates.
- the lean-runtime roadmap remains marked planned although its implementation phase files are marked implemented; phase files contain a broken/non-existent parent filename.
- `.github/workflows/python-test.yml` omits several transitive Rust crates from its path filter.
- ordinary Rust CI is already a single Ubuntu smoke job and is not the over-engineered part of the repository.
- the release-only Python wheel matrix is required for native artifacts and should not be collapsed into a single-host release.

## Target authority model

At closure, active compatibility authority is limited to:

1. **One human-readable matrix**
   - `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
   - explains supported, native-equivalent, warning, intentional exclusion, and unsupported behavior;
   - contains no aggregate parity percentage.

2. **One machine-readable capability manifest**
   - `docs/parity/pproxy_capability_manifest.toml`
   - contains the current claim for each maintained capability;
   - references executable test identifiers or focused evidence where useful;
   - is validated by existing repository tooling/tests.

3. **Executable upstream baseline fixtures**
   - `compat/pproxy-2.7.9/cli-baseline.json` and other pinned fixtures;
   - record facts about upstream, not Eggress support claims;
   - feed parser/semantic tests.

4. **Executable tests**
   - parser, translator, CLI exit, Python contract, and runtime tests are behavioral authority.

The strict manifest may remain only if code requires it as a derived test fixture. It must no longer be listed as an independent active contract. Prefer generating or mechanically deriving it from the canonical manifest and upstream baseline. If that is disproportionate, mark it historical/derived and remove it from active source-of-truth lists.

Do not create another manifest or closure matrix.

## Workstream A — Correct the canonical compatibility contract

### Canonical manifest

Update `docs/parity/pproxy_capability_manifest.toml` after Phases 1 and 2 settle behavior.

At minimum review and correct entries for:

- `-d` debug behavior;
- `--daemon`;
- `--reuse`;
- `--auth`;
- `--sys`;
- `--pac` arity and behavior;
- `--get` arity and behavior;
- `--test` arity and behavior;
- scheduler `lc`;
- Trojan listener/upstream support;
- raw/tunnel roles;
- H2/WS/WSS actual supported role boundaries;
- SSH, QUIC/H3, SSR, legacy cipher, and plugin exclusions;
- Python `Connection` and `Server` factory semantics;
- Python runtime-shaped methods corrected in Phase 2;
- default/full versus common feature availability.

Every entry must distinguish:

- upstream capability fact;
- Eggress status;
- compatibility tier;
- platform boundary where applicable;
- exact test or source reference when maintained.

Do not encode a percentage or score.

### Human matrix

Update `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` from final code/tests.

Required sections:

- common replacement workflows that are behaviorally supported;
- native equivalents with meaningful differences;
- compatibility inputs that fail closed;
- platform-specific behavior;
- Python factory versus lifecycle distinction;
- intentional exclusions;
- feature-build distinction where a capability is present only in default/full.

Use direct language. Do not claim "drop-in replacement" without the bounded qualifier.

### Upstream baseline

Correct `compat/pproxy-2.7.9/cli-baseline.json` only when a fact about upstream is wrong. Never edit it merely to align with Eggress.

Add or retain a test that consumes the baseline's option arity and aliases. The baseline should prove facts such as:

- `-d` and `--daemon` are different options;
- `--reuse` meaning;
- value-taking options;
- scheduler names.

Do not add a network oracle dependency to routine CI.

## Workstream B — Demote or consolidate stale parity documents

Review these active-looking documents:

- `docs/PPROXY_PARITY_SPEC.md`
- `docs/REAL_PPROXY_PARITY_ROADMAP.md`
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `docs/python/PPROXY_API_INVENTORY.md`
- `docs/python/PPROXY_EMBEDDED_USAGE_PATTERNS.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/python/MIGRATION_FROM_PPROXY.md`
- `docs/release/MIGRATION_FROM_PPROXY_FINAL.md`
- parity freeze/final evidence files under `docs/parity/`;
- historical phase and completion records under `plans/`.

For each file choose one action:

### Keep active

Keep only when it serves a distinct current user need. Correct stale facts and add a clear link to the canonical matrix/manifest.

### Reduce to a pointer

When content duplicates the canonical matrix, replace the active claim sections with a short purpose statement and links. Preserve migration examples that remain useful.

### Mark historical

Add a visible header:

> Historical implementation record. This file is not a current compatibility contract. See ...

Do not rewrite every historical detail. The objective is authority clarity, not archaeology.

### Delete

Delete only generated or wholly redundant files that have no historical or user value and are not referenced by tooling. Check references before deletion.

Do not delete old plans merely to reduce repository size.

## Workstream C — Correct specific stale facts

Search the entire repository for the following stale statements and correct or mark historical:

- `-d` described as daemon mode;
- `--reuse` described as connection pooling or cross-session reuse;
- `--auth` described as supported when ignored;
- `--sys` described as equivalent when inspection-only;
- `--pac`, `--get`, or `--test` described with wrong argument arity;
- Trojan inbound described as rejected if current code supports it;
- least-connections described as unavailable if current code supports `lc`;
- wrong upstream repository/source URL;
- version `0.1.0` or other obsolete freeze values presented as current;
- `unsafe_code = "forbid"` or another policy presented as current when workspace policy is `deny`;
- strict/full parity language presented without bounded exclusions;
- a separate `eggress-pproxy-compat` Python distribution;
- unsupported top-level Python methods presented as operational;
- common feature build described as excluding dependencies that remain linked.

Use repository-wide search, but edit only active documents or add historical banners where appropriate.

## Workstream D — Planning status and registration cleanup

### Register this roadmap

Update `plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md` near the status section to state:

- the practical parity roadmap remains completed for its original scope;
- post-audit corrective/reductive work is governed by `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`;
- the new roadmap does not reopen excluded transports or strict full parity.

Do not rewrite the historical phase sequence.

### Lean-runtime status

Review:

- `plans/LEAN_RUNTIME_DELIVERY_ROADMAP.md`
- `plans/LEAN_RUNTIME_PHASE_1_FEATURE_BOUNDARIES.md`
- `plans/LEAN_RUNTIME_PHASE_2_FOCUSED_RELIABILITY.md`
- `plans/LEAN_RUNTIME_PHASE_3_PYTHON_DELIVERY.md`

When implementation evidence is present:

- update the roadmap status from `PLANNED` to `IMPLEMENTED` or the precise partial status;
- correct broken parent links from `LEAN_RUNTIME_AND_DELIVERY_ROADMAP.md` to the actual roadmap filename;
- avoid creating a replacement lean roadmap;
- reference Phase 3 of the new corrective roadmap for any follow-up topology work.

### AGENTS source-of-truth list

Update `AGENTS.md` only after the final authority model is settled:

- list the canonical matrix and capability manifest;
- list pinned baseline fixtures as upstream facts, not support contracts;
- remove the strict manifest from active authority if demoted;
- register the new corrective roadmap as the active follow-on;
- retain the statement that `plans/` files are historical after implementation.

Keep the list short.

## Workstream E — Make manifest drift executable

Use existing testkit/validation code. Add at most the following focused checks:

1. every known CLI option in the parser has a canonical manifest entry;
2. every value-taking option in the upstream baseline is tested for missing-value failure;
3. canonical manifest test references point to existing test identifiers or files where the current validator already supports references;
4. no active capability has contradictory statuses in the canonical manifest;
5. the practical matrix generation/check, if one already exists, consumes the canonical manifest rather than a separate hand-maintained status table.

Do not create a new report generator if none exists. A direct Rust test or existing validation script is sufficient.

Do not gate routine development on an upstream installation.

## Workstream F — Correct Python smoke CI path coverage

### Current problem

`.github/workflows/python-test.yml` watches only selected binding/package paths. Changes in runtime, server, config, routing, protocol, transport, compatibility, or other transitive Rust crates can change the built extension while bypassing Python smoke.

### Required simplification

Prefer a broad, maintainable path boundary over an exhaustive dependency list:

```yaml
paths:
  - 'crates/**'
  - 'python/**'
  - 'tests/compat/**'
  - 'Cargo.toml'
  - 'Cargo.lock'
  - 'pyproject.toml'
  - '.github/workflows/python-test.yml'
```

Adjust the metadata filename to the repository's actual layout.

This intentionally runs Python smoke for any crate change. The repository is small enough that this is simpler and safer than maintaining transitive path knowledge.

Do not:

- add an operating-system matrix;
- add a Python-version matrix to routine smoke;
- add pull-request-only certification jobs;
- add external pproxy installation;
- add wheel construction to every push;
- duplicate the release artifact workflow.

### Keep the Python smoke job narrow

Retain:

- one Ubuntu runner;
- one supported development Python version;
- one native `maturin develop` build;
- `python/tests` and `tests/compat`;
- existing concurrency cancellation and timeout.

Remove only redundant setup or duplicate test invocations proven unnecessary.

## Workstream G — Preserve proportionate Rust CI

`.github/workflows/ci.yml` already runs:

- format;
- workspace Clippy;
- workspace tests;
- one Ubuntu job;
- push-to-main/manual triggers.

Do not remove the workspace test gate as "over-engineering." It is the primary check for a multi-crate network project.

Do not add:

- routine OS/architecture matrices;
- MSRV matrix;
- coverage gates;
- benchmark gates;
- cargo-bloat gates;
- audit/deny on every push;
- fuzzing;
- soak tests;
- external interoperability;
- parity certification;
- generated evidence artifacts.

If runtime exceeds the current bounded timeout after Phases 1-3, first identify duplicated/ignored/slow tests. Do not split the workflow into many jobs merely to mask inefficiency.

## Workstream H — Preserve release-only wheel verification

The multi-platform Python publish workflow is justified because the project distributes native wheels for Linux, macOS, and Windows.

Retain release gates for:

- approved wheel platform/architecture set;
- stable ABI tag;
- sdist;
- version coherence;
- clean artifact installation;
- native import;
- top-level `pproxy` import;
- representative startup/lifecycle smoke;
- TestPyPI before production policy where documented;
- OIDC trusted publication.

Simplify only duplicated implementation details, such as multiple scripts checking the same artifact list. Prefer one release artifact smoke script as the executable authority.

Do not move crates.io publication into CI. Do not add GitHub Release automation, signatures, SBOMs, provenance, or container publishing in this phase.

## Workstream I — Verification documentation

Reconcile:

- `docs/CI_STATUS.md`
- `docs/TESTING.md`
- `docs/release/RELEASE_PROCESS.md`
- `AGENTS.md`

Required policy:

### Routine hosted smoke

- Rust format/Clippy/workspace tests;
- Python 3.12 or current selected development-version smoke for all crate/package changes.

### Release-only

- platform wheel matrix;
- sdist;
- clean installed-artifact tests;
- publication.

### Local/affected-subsystem only

- dependency audits for dependency/release work;
- external pproxy differential tests;
- protocol interoperability;
- fuzzing;
- soak/performance;
- binary-size measurements;
- exhaustive feature checks.

Remove instructions requiring screenshots, evidence bundles, copied transcripts, or completion reports for ordinary changes.

## Final verification

Run focused contract checks first:

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-testkit
cargo test -p eggress-cli --test pproxy_binary
python -m pytest tests/compat -q
```

Use actual test target names where they differ.

Validate workflow syntax through review and, where repository tooling exists, the existing workflow linter. Do not add a linter dependency solely for this phase.

Final repository gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Fresh Python gate:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

No full external certification run is required unless Phases 1-2 left a specific upstream semantic uncertainty.

## Acceptance criteria

Phase 4 is complete only when all are true:

- one human matrix and one machine capability manifest are the active Eggress compatibility contract;
- upstream baseline fixtures are clearly separated from Eggress support claims;
- the strict manifest is derived, demoted, or removed from active authority;
- all confirmed stale flag, protocol, scheduler, version, unsafe-policy, package, Python, and feature claims are corrected or marked historical;
- the practical parity roadmap registers the corrective roadmap without reopening old scope;
- lean-runtime roadmap status and parent links are accurate;
- `AGENTS.md` names a short, correct source-of-truth set;
- manifest/parser/baseline drift is protected by focused executable checks;
- Python smoke runs for `crates/**` or an equivalently complete simple path boundary;
- routine Python smoke remains one job and one Python version;
- ordinary Rust CI remains one smoke job with format, Clippy, and workspace tests;
- release-only wheel verification remains multi-platform and installed-artifact based;
- crates.io publication remains manual;
- active verification documentation no longer requires routine evidence ceremony;
- the parent corrective roadmap is updated to `IMPLEMENTED` with commit range and final decisions;
- no new closure report, manifest, workflow matrix, certification system, or release product is added.

## Handoff notes for the implementer

- Do not start documentation edits until Phases 1-3 have final behavior.
- Use repository-wide searches for stale phrases, then distinguish active files from historical records.
- Prefer adding one historical banner over rewriting a large completed plan.
- Prefer one broad Python path filter over a long transitive crate list.
- Treat release wheel verification as necessary product delivery, not routine CI overkill.
- Update this plan and the parent roadmap in place at closure. Do not create a Phase 5 or separate evidence document unless implementation discovers a new functional defect outside the four planned phases.