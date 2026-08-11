# pproxy Final Phase 2 — Contract and Documentation Reduction

## Status

**PLANNED**

## Parent roadmap

[`PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md`](PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md)

## Objective

Make the active pproxy compatibility contract accurately describe observable behavior, then reduce maintenance risk by ensuring only the canonical capability manifest and practical compatibility matrix function as live parity authorities.

This phase is deliberately reductive. It must correct known overstated classifications and stale active facts without turning historical documents into another synchronization burden.

## Active authority

Per `AGENTS.md`, the active compatibility sources are:

1. `docs/parity/pproxy_capability_manifest.toml` — canonical machine-readable capability contract.
2. `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` — maintained human-facing matrix.
3. executable tests/differential harnesses — behavioral evidence.

`docs/parity/pproxy_2_7_9_strict_manifest.toml`, older parity specifications, API inventories, phase plans, completion records, and certification documents are historical/derived unless a current source-of-truth document explicitly says otherwise.

## Confirmed classification/document drift

At planning baseline `5a724be68de7080cc6fff21aeb5774491a307dfa`:

### `-f/--config`

The capability manifest labels this `drop_in` while also stating Eggress uses a different configuration schema. A flag with the same purpose but incompatible file contents is not unchanged-input/drop-in behavior.

Expected correction: classify as a supported difference / compatibility warning tier consistent with the manifest's existing vocabulary. Do not implement a pproxy config-file parser solely to preserve the old tier.

### `--log <PATH>`

The manifest labels the option `native_equivalent`, while the standalone compatibility help describes the outcome as tracing/stderr and shell redirection. If Eggress does not write to the supplied path, this is not equivalent behavior.

Expected correction: classify as a supported difference / warning, with explicit note that the compatibility parser recognizes the option but does not reproduce pproxy's file-writing mechanism. If implementation instead adds a tiny direct file sink using existing tracing infrastructure without new complexity, re-evaluate based on observable behavior; this phase does not require such implementation.

### SOCKS4 BIND

The manifest says pproxy itself does not implement SOCKS4 BIND but labels Eggress's matching refusal as `intentional_non_parity`. This taxonomy confuses a mutually unsupported operation with an Eggress-specific exclusion.

Expected correction: classify according to observable equivalence/refusal. Do not implement SOCKS4 BIND.

### Known stale documentation

Known examples that must be addressed if they still read as active/current:

- incorrect upstream repository/source references;
- scheduler text claiming pproxy lacks `fa`, `rr`, `rc`, or `lc` despite pproxy 2.7.9 implementing all four;
- stale H2/WS/raw/tunnel promotion status;
- stale Python API inventory claims such as missing SOCKS4 upstream/runtime integrations that have since landed;
- stale dependency-policy examples showing rustls/tokio-rustls `logging` features that are no longer enabled in the actual workspace.

The objective is not to modernize every historical line. The objective is to prevent stale files from masquerading as current authority.

## Governing rules

1. Classify capabilities by **observable user behavior**, not by whether Eggress has an analogous internal mechanism.
2. `drop_in` should mean the same input shape can be used without a materially different result on the bounded surface being claimed.
3. `native_equivalent` should require the requested outcome to actually occur, even if implemented differently.
4. When an option is accepted but behavior differs, use the existing warning/supported-difference tier rather than inflating equivalence.
5. Matching rejection of an operation that upstream also rejects should not be counted as Eggress-specific non-parity unless the failure semantics materially differ in a user-visible way.
6. Do not optimize for a higher parity percentage. The project explicitly avoids aggregate percentages.
7. Keep exactly one active machine contract and one active human matrix.
8. Historical documents may remain for provenance. Prefer a short top banner pointing to active sources over line-by-line maintenance.
9. Do not create a new generated compatibility report, registry, dashboard, or classifier artifact to solve document drift.
10. Do not delete historical plans merely to make the repository smaller unless they are demonstrably misleading and no longer provide useful provenance.
11. Do not weaken executable tests to make a tier easier to justify.

## Likely files

Primary:

- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/README.md`
- `AGENTS.md` only if source-of-truth wording needs a narrow clarification

Known stale candidates to inspect:

- `docs/PPROXY_PARITY_SPEC.md`
- `docs/python/PPROXY_API_INVENTORY.md`
- `docs/DEPENDENCY_POLICY.md`
- `docs/REAL_PPROXY_PARITY_ROADMAP.md`
- `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION_TRACK_BC.md`
- other files surfaced by targeted search for stale upstream URL, old runtime status, or obsolete scheduler claims

Tests/scripts only if required to validate the canonical manifest:

- `scripts/validate_pproxy_parity_manifest.py`
- existing manifest/compatibility tests

## Workstream A — Establish tier semantics before editing entries

Read the tier definitions used by the canonical manifest/matrix. If definitions are ambiguous, make one concise correction in `docs/parity/README.md` or the manifest header so future maintainers can distinguish:

- unchanged/drop-in compatibility;
- supported behavior with a meaningful implementation/CLI difference;
- native extension;
- intentional exclusion;
- unsupported behavior.

Do not invent new tier names if the existing vocabulary can express the required distinctions.

The implementation summary must state the applied rule for at least:

```text
same flag + different config schema
accepted flag + different output destination
same unsupported operation on both implementations
```

## Workstream B — Correct known capability classifications

### `cli.config`

Audit pproxy 2.7.9's `-f/--config` accepted file formats/schema against Eggress's compatibility path.

If an upstream pproxy config file cannot be used unchanged, demote `drop_in` to the appropriate supported-difference/warning tier. Update notes to explain exactly what is compatible:

- flag ownership/arity;
- Eggress accepts a config path;
- schema differs;
- migration/translation expectations if any.

Do not state that the option is drop-in merely because both programs accept `-f`.

### `cli.log`

Verify current runtime behavior with a focused process test or code inspection:

- Does `--log PATH` create/write `PATH`?
- Is the value currently only parsed/classified while output remains on tracing/stderr?

If no file is written, classify it as a supported difference/warning and state the actual alternative. Add or update one test proving the current behavior/classification so it cannot silently drift back to `native_equivalent`.

Do not add a logging subsystem in this phase.

### SOCKS4 BIND

Verify pproxy 2.7.9 behavior from pinned source/test evidence. If upstream also does not provide SOCKS4 BIND, reclassify Eggress's refusal so the manifest does not call a matched unsupported surface intentional non-parity.

Preserve Eggress's secure refusal. No listener-opening behavior is authorized.

### Adjacent targeted audit

Search the canonical manifest for entries whose notes directly contradict their tier, focusing on terms such as:

```text
different schema
not reproduced
unsupported in both
parsed but unused
no differential coverage
```

Correct only clear contradictions. Stop after the bounded pass; do not re-litigate every protocol tier already backed by differential/runtime tests.

## Workstream C — Reconcile the maintained practical matrix

Update `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` so it agrees with the corrected canonical manifest and Phase 1 Python behavior.

Requirements:

- no aggregate parity percentage;
- no new matrix dimension merely to capture implementation trivia;
- unsupported plugin/cipher-helper internals remain explicitly bounded;
- corrected CLI classifications use plain language understandable to a pproxy user;
- stable intentional exclusions remain unchanged unless Phase 1 produces a direct contract correction.

The matrix should remain concise. Do not duplicate every manifest field.

## Workstream D — Demote stale historical documents instead of maintaining them as live specs

For each stale candidate, determine whether it is:

1. **still active/documentation policy** — correct the stale fact;
2. **historical but useful** — add or strengthen a short banner near the top stating that it is historical/derived and pointing to the canonical manifest + practical matrix;
3. **obsolete duplicate with no useful provenance** — deletion may be considered, but only if references are updated and the removal clearly lowers confusion/maintenance.

Preferred treatment for large old parity/API documents is a banner, not line-by-line synchronization.

A suitable banner should communicate that:

- the file records a historical implementation/audit state;
- it must not be used to infer current support;
- current support lives in the canonical manifest and practical matrix.

Do not add a banner to every plan under `plans/`; `AGENTS.md` already defines those as historical implementation records.

## Workstream E — Correct genuinely active non-parity documentation

Some files outside the parity docs remain active operational policy and therefore must be factually correct.

### `docs/DEPENDENCY_POLICY.md`

Synchronize rustls/tokio-rustls examples with the actual root `Cargo.toml`. If `logging` has been removed, the policy must not show it as required.

Re-run the production dependency verification commands if dependency text changes materially:

```bash
cargo tree -i aws-lc-sys -e normal
cargo tree -i cmake -e normal
cargo tree -i openssl-sys -e normal
```

The expected result should continue to demonstrate that prohibited native production dependencies do not enter deliverable binaries.

### Upstream source reference

Where active docs identify the pproxy source, use the actual pinned upstream repository/source associated with the checked-in oracle. Do not retain an incorrect repository URL just because older plans used it.

## Workstream F — Simplify validation ownership

Inspect `scripts/validate_pproxy_parity_manifest.py` and related validation tests.

Retain checks that catch structural corruption or contradictions in the canonical manifest. Remove or avoid adding checks whose only purpose is to keep historical/derived documents synchronized with the canonical manifest.

The validator should not require every historical API inventory, roadmap, completion document, and strict manifest to echo the active contract.

Do not build a document generator to solve this phase unless one already exists and deleting duplicate hand-maintained content is clearly simpler than maintaining it manually.

## Workstream G — Targeted tests

Add or update tests sufficient to freeze the corrected contract:

- canonical manifest parses/validates;
- `cli.config` cannot be reported as unchanged/drop-in if the schema remains incompatible;
- `cli.log` classification agrees with actual file-output behavior;
- SOCKS4 BIND classification agrees with pinned upstream evidence;
- active matrix and manifest do not directly contradict the corrected entries.

Prefer simple targeted assertions over snapshotting entire documentation files.

## Verification

Focused manifest/docs checks:

```bash
python3 scripts/validate_pproxy_parity_manifest.py
```

Run any existing focused parity metadata tests identified by repository search. If code changes are required for `--log`/classification behavior, run the associated Rust/Python tests.

Broad gate for substantial changes:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Python suite is required if Phase 1 classifications/namespace docs are touched in a way coupled to Python behavior:

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

No external pproxy oracle run is required for facts already pinned in checked-in fixtures/source; use it only if a classification depends on uncertain upstream behavior.

## Acceptance criteria

Phase 2 is complete only when all are true:

- the canonical manifest's tier definitions are sufficiently clear to distinguish unchanged compatibility from supported differences;
- `cli.config` no longer claims drop-in/unchanged compatibility while requiring a different config schema;
- `cli.log` no longer claims equivalent file-output behavior unless an actual test proves the requested log path is written;
- SOCKS4 BIND classification accurately reflects the fact that pproxy 2.7.9 also lacks the operation, while Eggress continues to refuse it safely;
- any adjacent manifest tier corrected by the bounded audit has a direct evidence/reason note and was not changed merely to improve a parity score;
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` agrees with the canonical manifest for all changed entries;
- Phase 1 Python plugin/cipher-helper boundaries are represented accurately in the active matrix/manifest;
- active documentation uses the correct pinned upstream source reference;
- active H2/WS/raw/tunnel and scheduler statements no longer contradict current runtime behavior or pproxy 2.7.9;
- `docs/DEPENDENCY_POLICY.md` examples match the actual rustls/tokio-rustls feature configuration;
- large historical parity/API/certification documents are either clearly bannered as historical/derived or removed only when doing so reduces confusion without losing useful provenance;
- no new active parity document, generated certification artifact, registry, or aggregate parity percentage is introduced;
- manifest validation remains focused on the active contract rather than forcing historical documents to stay synchronized;
- targeted contract tests pass;
- broad Rust/Python gates pass when affected by implementation changes;
- this plan is updated in place to `IMPLEMENTED` with the changed classifications, files demoted/corrected, implementation commit(s), and verification summary.