# Maintenance Corrective Closure Polish

## Status

**IMPLEMENTED — 2026-09-21**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Corrective baseline: `ba4102f68c3965a8cadb7634febd2d610e239e71`
- Parent roadmap: [`MAINTENANCE_CONVERGENCE_ROADMAP.md`](MAINTENANCE_CONVERGENCE_ROADMAP.md)
- Implementation commit under review: `ba4102f68c3965a8cadb7634febd2d610e239e71`

## Purpose

The maintenance convergence implementation landed successfully and the current code state is healthy. This plan closes the remaining evidence/planning defects without reopening runtime architecture, changing public API, or adding capability.

The corrective scope is limited to:

1. reconciling the five phase-plan acceptance checklists with their closure evidence and removing duplicated closure headings;
2. strengthening Phase 3 with one direct native-binding regression for `run_pproxy_test()`, so the plan's stated Python-visible contract is proven directly rather than inferred only from a CLI parser unit test plus process-level `python -m pproxy --test` coverage;
3. recording the successful remote push verification for the implementation commit and making the parent/phase/corrective status agree.

No other implementation work is authorized.

## Why this plan exists

Commit `ba4102f68c3965a8cadb7634febd2d610e239e71` implemented all five maintenance phases:

- runtime listener/service/signal decomposition;
- outbound error/UDP/compat private decomposition;
- deliberate retention of `eggress-python -> eggress-cli` (Phase 3 Outcome B);
- public API contract expansion and outbound `toml` CI slice;
- Python documentation and pproxy projection-equivalence convergence.

Remote push verification for that commit is green:

- GitHub Actions run `35646523487`, workflow `CI`: success;
- GitHub Actions run `35646523343`, workflow `Python smoke`: success;
- Python smoke executed `python/tests tests/compat`: **2308 passed, 115 skipped**;
- Rust CI executed formatting, clippy, locked workspace tests, optional compatibility checks, embed feature boundaries, outbound base/`toml`/pproxy/`ssh`/`ssh+pproxy`/`udp` feature boundaries, OpenSSH runtime regression, and fuzz-target compilation.

Post-implementation review found only closure-state defects.

### Residual A — phase plans say implemented but their criteria remain unchecked

All five phase files are marked `IMPLEMENTED`, but their individual acceptance checkboxes are still `[ ]`.

The parent roadmap marks the corresponding campaign-level criteria `[x]`.

This creates contradictory evidence state even though the implementation and tests exist.

### Residual B — duplicated closure headings

Each maintenance phase currently contains two consecutive:

```text
## Closure record
```

headings.

This is a small documentation defect and should be corrected while reconciling the checklists.

### Residual C — Phase 3 evidence is directionally correct but not as direct as its plan requires

Phase 3 required focused local regression coverage for the Python-visible `run_pproxy_test()` contract.

Current evidence includes:

- `eggress-cli::pproxy_test_target_parsing_contract`, which directly covers the shared target parser;
- `python/tests/test_pproxy_phase6_process.py::test_python_test_mode_uses_native_bridge_without_listener_startup`, which exercises `python -m pproxy --test` through the native bridge;
- the full Python/compat suite, which is green;
- shared implementation: Python `run_pproxy_test()` calls the same `eggress_cli::parse_pproxy_test_target()` and `run_upstream_test()`.

That is strong behavior evidence, but the closure wording currently implies the newly added parser test itself freezes the entire Python helper contract. It does not.

The smallest correction is one direct native-binding test plus precise closure wording.

---

## Governing constraints

1. Do not change `run_pproxy_test()` runtime implementation unless the direct test discovers a real defect.
2. Do not remove the deliberate `eggress-python -> eggress-cli` dependency.
3. Do not add a new public Rust API, helper crate, Python symbol, CLI option, configuration key, protocol/transport capability, or feature.
4. Do not create a second chain-aware upstream tester.
5. Do not replace `run_pproxy_test()` with `test_upstream_connect()`; their semantics remain distinct.
6. Do not change pproxy parsing, translation, execution-gate, timeout, exit-code, redaction, stdout/stderr, or listener-start behavior.
7. Do not change runtime supervisor/outbound decomposition merely because this pass revisits their plans.
8. Do not rewrite historical baseline documents as current-state documentation.
9. Do not add external-network-dependent tests.
10. Do not add new CI tooling or a new evidence framework.
11. A checkbox may be marked `[x]` only when its closure section identifies concrete code/test/doc evidence or the criterion is explicitly satisfied by an unchanged public-surface invariant verified by the existing green gates.
12. If an acceptance criterion cannot be justified from existing evidence plus this narrow test, leave it unchecked and keep the parent corrective status open rather than manufacturing closure.

## Workstream 1 — Reconcile all five phase acceptance checklists

Update:

- `MAINTENANCE_PHASE_1_RUNTIME_SUPERVISOR_DECOMPOSITION.md`;
- `MAINTENANCE_PHASE_2_OUTBOUND_INTERNAL_DECOMPOSITION.md`;
- `MAINTENANCE_PHASE_3_PYTHON_CLI_OWNERSHIP.md`;
- `MAINTENANCE_PHASE_4_PUBLIC_API_AND_FEATURE_QUALIFICATION.md`;
- `MAINTENANCE_PHASE_5_DOCUMENTATION_AND_COMPAT_PROJECTION_CONVERGENCE.md`.

For every acceptance criterion:

1. map the criterion to concrete implementation or verification evidence already present at `ba4102f68c3965a8cadb7634febd2d610e239e71` or added by this corrective pass;
2. change `[ ]` to `[x]` only after that mapping exists;
3. where a criterion is conditional (for example Phase 3 Outcome A vs Outcome B), mark it satisfied only through the selected Outcome B branch and phrase the closure mapping accordingly;
4. do not claim tests that were not actually executed.

The closure section for each phase should contain a concise evidence map, not merely a prose assertion that the phase landed.

### Required Phase 1 evidence mapping

At minimum map:

- private listener ownership -> `supervisor/listeners.rs`;
- reverse/admin separation -> `supervisor/services.rs`;
- readiness/SIGHUP ownership -> `supervisor/signals.rs`;
- canonical reload -> `RuntimeState::apply_compiled_config()`;
- ordered shutdown -> `shutdown_ordered(ShutdownPlan)`;
- remote Rust CI + lifecycle-focused test evidence.

### Required Phase 2 evidence mapping

At minimum map:

- stable facade -> `eggress-outbound/src/lib.rs`;
- typed errors -> `connect_error.rs`;
- UDP lifecycle -> `udp.rs`;
- compatibility redaction -> `compat.rs`;
- TCP execution still single-sourced in `connector.rs`;
- outbound feature-slice CI including `toml`;
- embed compatibility re-export contract.

### Required Phase 3 evidence mapping

At minimum map:

- Outcome B architectural justification -> `architecture/python-bindings.md`;
- retained dependency -> `crates/eggress-python/Cargo.toml` / dependency tree;
- target parser contract -> `pproxy_test_target_parsing_contract`;
- Python direct binding contract -> Workstream 2 of this plan;
- process-level no-listener bridge -> `test_python_test_mode_uses_native_bridge_without_listener_startup`;
- distinction from `test_upstream_connect()`;
- full Python/compat verification.

### Required Phase 4 evidence mapping

At minimum map:

- embed/public compile contracts -> `crates/eggress-embed/tests/public_api.rs`;
- server public contract -> `crates/eggress-server/tests/public_api.rs`;
- outbound `toml` slice -> `.github/workflows/ci.yml`;
- maintained feature map -> `AGENTS.md`;
- public-surface policy -> `docs/RUST_API.md`;
- no mandatory semver/nightly tooling.

### Required Phase 5 evidence mapping

At minimum map:

- corrected Unix/stub language -> `docs/PYTHON_BINDINGS.md`;
- package-facing corrections -> `python/README.md`;
- historical-state qualification -> `docs/python/EGRESS_PYTHON_API_CURRENT_STATE.md`;
- native/TOML authority -> maintained architecture docs;
- strengthened `native_equivalence.rs` coverage;
- compatibility suite evidence;
- no compatibility claim change.

## Workstream 2 — Add one direct native-binding `run_pproxy_test()` regression

Add a focused Python test that imports the native binding function directly:

```python
from eggress._eggress import run_pproxy_test
```

The test must exercise the Python-visible function itself without external network dependency.

Required cases:

1. **No-upstream fast path**
   - pass arguments that compile to no upstreams;
   - assert return value `0`;
   - this proves direct native-binding invocation and the documented no-upstream contract without performing network I/O.

2. **Invalid/unknown argument**
   - pass one strict-parser violation or invalid value;
   - assert `ValueError` and the established `pproxy argument error` / unknown-option family as applicable.

3. **Unsupported execution gate**
   - use an already-known unsupported pproxy argument combination that fails before side effects;
   - assert `UnsupportedFeatureError`.

4. **Chain-aware execution evidence**
   - retain the existing process-level `test_python_test_mode_uses_native_bridge_without_listener_startup` as the chain-aware/no-listener behavioral proof;
   - do not add an internet target;
   - if an existing reusable local proxy fixture makes a direct successful chain case trivial, it may be added, but it is not required if it would introduce substantial fixture machinery.

The direct test may live in the most relevant existing Python compatibility test module. Do not create another test framework.

### Evidence wording after the test

Phase 3 must distinguish:

- **direct binding evidence**: parser/gate/no-upstream contract of `run_pproxy_test()`;
- **shared-owner evidence**: target parser unit test;
- **process behavioral evidence**: native bridge, chain-aware failure result, and no listener startup;
- **full-suite evidence**: package/import/compatibility regression.

Do not state that the target-parser unit test alone covers the entire Python helper.

## Workstream 3 — Clean closure headings and record exact remote provenance

For all five phase files:

- reduce duplicate consecutive `## Closure record` headings to one;
- retain one authoritative closure block.

Update the parent roadmap so its closure record contains:

- implementation commit `ba4102f68c3965a8cadb7634febd2d610e239e71`;
- remote CI workflow run ID `35646523487`;
- remote Python smoke workflow run ID `35646523343`;
- Python full-suite result `2308 passed, 115 skipped`;
- statement that the corrective pass changed only tests/planning/docs unless a real defect was discovered.

After this corrective pass itself is pushed, record the new corrective commit's remote CI/Python run IDs before final closure. Do not rely only on the older implementation runs for the final corrective commit.

## Workstream 4 — Reconcile parent/corrective status

During execution:

1. keep the parent roadmap marked as corrective polish open while this plan is incomplete;
2. register this plan as the final row in the parent execution table;
3. once every criterion below is satisfied, mark:
   - this plan `IMPLEMENTED`;
   - parent roadmap `IMPLEMENTED`;
   - all five phase plans `IMPLEMENTED` with reconciled checked acceptance criteria;
4. do not create another evidence-polish plan if this pass closes cleanly.

## Verification

### Focused Phase 3 proof

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)

.venv/bin/python -m pytest   python/tests/test_pproxy_compat.py   python/tests/test_pproxy_phase6_process.py   python/tests/test_api_boundary_closure.py -q
```

Use the actual test file containing the direct binding regression if it differs from the list above.

### Full Python contract

```bash
.venv/bin/python -m pytest python/tests tests/compat -q
```

### Planning-state checks

```bash
rg -n '^## Closure record$' plans/MAINTENANCE_PHASE_*.md
rg -n '\- \[ \]' plans/MAINTENANCE_PHASE_*.md plans/MAINTENANCE_CORRECTIVE_CLOSURE_POLISH.md
```

Expected at final closure:

- exactly one closure heading per maintenance phase;
- no unexplained unchecked acceptance criteria in an `IMPLEMENTED` maintenance plan.

### Repository gates

Because the corrective pass should be test/documentation-only, no runtime behavior needs special new qualification. The pushed corrective commit must nevertheless pass the repository's normal remote workflows:

- `CI`;
- `Python smoke`.

If any Rust source changes unexpectedly become necessary, also run locally before push:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

No external pproxy/shadowsocks oracle run is required unless actual compatibility behavior changes, which is outside this plan.

## Stop conditions

Stop and leave this plan open rather than expanding scope if:

1. the direct binding regression reveals a real runtime behavior defect;
2. satisfying Phase 3 requires changing `run_pproxy_test()` semantics or moving the shared tester;
3. any phase acceptance criterion is not supported by actual evidence;
4. correcting a documentation claim would require changing a compatibility tier/capability claim;
5. a proposed fix touches runtime supervisor/outbound behavior rather than evidence state.

A newly discovered runtime defect should receive a separate corrective implementation plan.

## Acceptance criteria

This corrective polish is complete only when:

- [x] All five maintenance phase files contain exactly one `## Closure record` heading.
- [x] Every checked phase acceptance criterion has explicit supporting evidence in its closure record.
- [x] No `IMPLEMENTED` maintenance phase contains unexplained unchecked acceptance criteria.
- [x] Phase 1 criteria are reconciled against the private runtime decomposition and lifecycle evidence.
- [x] Phase 2 criteria are reconciled against outbound private ownership, re-export, redaction, UDP, and feature-slice evidence.
- [x] A direct native-binding test exercises `eggress._eggress.run_pproxy_test()`.
- [x] The direct test covers no-upstream return, invalid/unknown argument failure, and unsupported execution-gate failure without external network access.
- [x] The existing process-level `--test` case remains the chain-aware/no-listener behavioral proof.
- [x] Phase 3 closure wording no longer implies the CLI target-parser unit test alone proves the full Python helper contract.
- [x] Phase 3 Outcome B remains unchanged: `eggress-python -> eggress-cli` is deliberately retained with no duplicated tester or public API addition.
- [x] Phase 4 criteria are reconciled against public compile contracts and actual CI feature slices, including outbound `toml`.
- [x] Phase 5 criteria are reconciled against corrected maintained docs and native/TOML equivalence evidence.
- [x] Parent roadmap records implementation commit `ba4102f68c3965a8cadb7634febd2d610e239e71` and remote workflow runs `35646523487` / `35646523343`.
- [x] Full Python/compat suite passes after the new direct binding test.
- [x] The final corrective commit's remote `CI` and `Python smoke` workflows both succeed.
- [x] No Rust/Python/CLI/config/protocol/transport public surface or capability changes.
- [x] No new closure plan is needed after this pass.

## Closure record

Corrective pass (tests/planning/docs only; no runtime/public-surface change):

- direct `run_pproxy_test()` test: `python/tests/test_pproxy_compat.py::TestRunPproxyTestNativeBinding` — `test_no_upstream_fast_path_returns_zero_without_network` (listener-only → `0`, no I/O), `test_invalid_argument_raises_value_error` (`-s invalid` → `ValueError/pproxy argument error`; `--bogus-flag` → `ValueError/unknown option`), `test_unsupported_execution_gate_raises_unsupported_feature` (ssh-upstream → `UnsupportedFeatureError`, gate blocks before side effects). Process proof retained: `test_python_test_mode_uses_native_bridge_without_listener_startup`.
- reconciliation counts: Phase 1 (12/12), Phase 2 (9/9), Phase 3 (11/11 via Outcome B branch), Phase 4 (10/10), Phase 5 (11/11); each phase has exactly one `## Closure record` heading and explicit evidence maps.
- docs: `architecture/python-bindings.md` agreement paragraph now distinguishes direct-binding vs shared-owner vs process evidence; README/`AGENTS.md`/skills required no pruning (already current: outbound `toml` slice, feature maps, test inventory).
- local gates: `cargo fmt --check` pass; `cargo clippy --workspace --all-targets -- -D warnings` pass; focused Rust (`eggress-runtime` lifecycle 18, `eggress-outbound` 15, `eggress-cli --lib` 11) pass; outbound base/`toml`/pproxy/`ssh`/`ssh+pproxy`/`udp` + embed `ssh`/pproxy/`ssh+pproxy` compile slices pass; focused Python (`test_pproxy_compat` 15, `test_pproxy_phase6_process` + `test_api_boundary_closure` 32) pass; full Python/compat `2311 passed, 115 skipped` (implementation was `2308 passed`; +3 new).
- implementation provenance (unchanged): commit `ba4102f68c3965a8cadb7634febd2d610e239e71`, remote CI `35646523487` success, Python smoke `35646523343` success (`2308 passed, 115 skipped`).
- corrective commit remote verification: corrective code commit `dc9a76f21d1a11806987050c75051758613be64d` — remote `CI` run `35655704749` success, remote `Python smoke` run `35655704713` success.
- no Rust/Python/CLI/config/protocol/transport public-surface or capability changes; no new closure plan needed.
