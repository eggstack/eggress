# pproxy Final Phase 5 — Differential Closure

## Status

**PLANNED**

## Parent roadmap

[`PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md`](PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md)

## Objective

Perform one bounded final verification pass over the surfaces changed by Phases 1-4, reconcile the active compatibility contract with observed behavior, and close the `pproxy==2.7.9` parity line without creating another permanent certification/reporting layer.

This phase verifies closure. It does not authorize additional parity scope.

## Entry conditions

Do not begin final closure until:

- Phase 1 Python semantic closure is implemented;
- Phase 2 contract/documentation reduction is implemented;
- Phase 3 execution-path simplification is implemented or explicitly closed under its stop conditions;
- Phase 4 regex/verification boundary is implemented;
- no phase has introduced unresolved failures in the normal Rust or Python smoke suites.

If a phase is explicitly `CLOSED — NO CHANGE REQUIRED`, its plan must already contain the evidence/stop-condition reasoning before Phase 5 treats it as complete.

## Governing rules

1. Test the **changed surfaces**, not every historical Cartesian combination of pproxy features.
2. Use the pinned pproxy 2.7.9 source/oracle as authority for upstream semantics.
3. A skipped oracle test is not evidence of parity.
4. Do not change production behavior merely to make an overbroad historical certification script green if the active bounded product contract says that behavior is intentionally excluded.
5. Do not add aggregate parity percentages.
6. Do not generate a new permanent certification report, evidence bundle, dashboard, registry, or completion document.
7. Record final verification in the roadmap/phase plans and implementation commit/PR summary.
8. Preserve intentional exclusions: SSH, QUIC/H3, SSR, legacy ciphers/OTA, plugin execution, daemonization, `--auth`, pproxy-mode `--sys`, general multi-hop UDP, unsupported reverse compositions, and unavailable platform transparent facilities.
9. Fail closure if an active document claims behavior that the final executable evidence contradicts.
10. Environment-limited checks may be marked not-run only when the limitation is concrete and the affected claim already has adequate checked-in/local evidence; do not convert not-run into a match claim.

## Closure matrix

The implementer should verify at least the following changed surfaces.

### Phase 1 — Python public/compatibility helpers

Verify against pinned pproxy 2.7.9 source or paired oracle observations:

- `pproxy.server.DUMMY` callability and identity behavior;
- `pproxy.server.UDP_LIMIT` exact value;
- `prepare_ciphers(None, ...)` sentinel behavior;
- non-`None` `prepare_ciphers` behavior is explicitly unsupported in Eggress rather than silently pass-through, with the active contract documenting that difference;
- plugin-bearing compatibility URI is explicitly rejected rather than accepted while dropping the plugin;
- plugin-free URI factory behavior remains unchanged.

The unsupported Eggress result need not equal pproxy's successful plugin/cipher result; the closure requirement is **honest classification and fail-closed behavior**, not false equivalence.

### Phase 2 — CLI/contract classifications

Verify enough observable behavior to justify the final tiers:

- `-f/--config`: whether an actual pproxy config file is accepted unchanged by Eggress; if not, the final tier must not be drop-in;
- `--log PATH`: whether Eggress writes the requested path; if not, the final tier must describe a supported difference/warning rather than equivalence;
- SOCKS4 BIND: confirm pproxy 2.7.9 behavior and Eggress refusal so the final taxonomy is accurate;
- active scheduler text recognizes pproxy's `fa`, `rr`, `rc`, and `lc` algorithms;
- active H2/WS/raw/tunnel status matches current runtime support;
- active dependency-policy TLS feature examples match `Cargo.toml`.

### Phase 3 — Execution path

Verify behavior rather than internal helper names:

- standalone `pproxy` starts a representative supported listener from translated args;
- `eggress pproxy run` starts the equivalent representative configuration;
- both paths apply the same unsupported/unknown execution gate;
- `pproxy --test TARGET` preserves the exact target and stable success/failure class;
- if temp-file/subprocess indirection was removed, tests prove the new in-process path works without requiring sibling-executable resolution;
- if a Phase 3 stop condition retained either mechanism, the plan contains the concrete rationale and associated regression coverage;
- native file-backed config startup/reload remains unchanged.

### Phase 4 — Regex boundary

Verify locally:

- fast regex path;
- fancy regex fallback path;
- pattern-length bound;
- rule-entry bound;
- unsupported-pattern structured error;
- no changed call site exposes arbitrary remote payload matching contrary to the documented trust model.

A catastrophic-backtracking stress corpus is not required for closure and must not become a routine gate.

## Workstream A — Focused external pproxy differential checks

Use the existing documented differential harness where a live pproxy process/installation materially increases confidence.

At minimum run or add focused differential tests for changed CLI/Python semantics that are suitable for paired execution. Existing representative wire tests for core HTTP/SOCKS data-plane behavior need not be multiplied solely because this is a closure phase.

Examples, adapting to actual test names:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
  cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
```

If that target is broad and expensive, use exact test filters for the relevant cases.

For Python strict paired observations, use the existing oracle/candidate mechanism documented by `docs/DIFFERENTIAL_TESTING.md` rather than creating another probe format.

### Existing full certification script

`./scripts/run_pproxy_certification.sh` may be run once as an informational final cross-check if:

- its environment dependencies are available;
- it does not force excluded/historical claims as release blockers;
- it completes without requiring new permanent CI/setup work.

It is **not required** if focused differential tests plus the active manifest/matrix validation cover every changed claim. Do not modify production scope merely to satisfy stale certification assumptions; fix/demote the stale assumption instead.

## Workstream B — Broad Rust verification

Run the repository's normal substantial-change gate from a clean-enough workspace:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Failures must be triaged as:

- regression caused by this roadmap — fix before closure;
- pre-existing reproducible failure — document and decide whether it invalidates a changed claim;
- environment-only specialized failure — do not disguise it as a passing check.

No new mandatory check should be added to `ci.yml` as part of closure.

## Workstream C — Broad Python verification

Build the extension freshly and run the maintained Python/compatibility suites:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

If strict paired-oracle tests require separate observation directories or an alternate environment, run only the changed-surface strict cases after the normal Python suite.

Do not add every supported Python version/platform to routine closure verification. Release-only wheel matrices remain in the existing publication workflow.

## Workstream D — Manifest and matrix reconciliation

After executable verification, inspect every active compatibility entry changed by this roadmap.

Requirements:

- `docs/parity/pproxy_capability_manifest.toml` describes the final behavior/tier/evidence accurately;
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` agrees with it in user-facing language;
- unsupported plugin/cipher internals are not described as runtime-backed;
- `-f/--config` and `--log` tiers agree with observed behavior;
- SOCKS4 BIND taxonomy agrees with pinned pproxy behavior;
- no stale active text reintroduces strict-full-parity language;
- intentional exclusions remain explicit.

Run the existing manifest validator after final edits:

```bash
python3 scripts/validate_pproxy_parity_manifest.py
```

Use the actual supported invocation if the script requires arguments/environment.

## Workstream E — Final architecture/size sanity record

If Phase 3 changed production dependencies or startup composition, record the final state in the Phase 3 implementation summary and briefly reference it in the roadmap closure record.

Required information when applicable:

- whether normal pproxy startup is in-memory/typed or still file-backed under stop conditions;
- whether `--test` is in-process or still subprocess-backed under stop conditions;
- whether `tempfile` remains a production CLI dependency;
- before/after artifact sizes measured during Phase 3;
- any complexity proposed and rejected because it did not justify itself.

Do not rerun a large binary-size optimization program in Phase 5.

## Workstream F — Close planning records in place

When verification is green/adequate:

1. update this plan from `PLANNED` to `IMPLEMENTED`;
2. update Phases 1-4 to `IMPLEMENTED` or `CLOSED — NO CHANGE REQUIRED` as appropriate;
3. update `PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md` to `IMPLEMENTED — CLOSED`;
4. add a concise closure record to the roadmap containing:
   - implementation commit range/SHAs;
   - key decisions;
   - focused differential/oracle checks actually run;
   - broad Rust result;
   - broad Python result;
   - any environment-limited check and why it is not required for an active claim;
5. do **not** create a new closure/certification/evidence file.

If `AGENTS.md` enumerates active/completed roadmap lineage and would otherwise mislead future agents, add at most one concise reference to the new final closure roadmap. Do not turn `AGENTS.md` into a chronological commit log.

## Workstream G — Define future reopening threshold

The roadmap closure record must state that future pproxy work requires at least one of:

- a reproducible user-visible compatibility defect within the bounded product claim;
- a security/correctness defect in a currently supported protocol/path;
- an explicit project-level decision to expand product scope;
- a new upstream target/version decision.

A desire to increase a parity percentage, mirror private pproxy internals, or implement an excluded transport for completeness is insufficient by itself.

## Failure handling

### If a focused differential test disagrees with the active claim

Fix the implementation if the behavior is within current scope and the change is bounded. Otherwise demote/correct the claim and ensure fail-closed behavior.

Do not automatically implement a large excluded feature to restore a tier.

### If core wire tests regress

Treat regressions in supported HTTP/SOCKS/UDP/TLS/Shadowsocks/Trojan/chaining behavior as release-blocking for this roadmap. The closure work must not degrade the data plane.

### If only a historical document fails a synchronization check

Remove/demote the stale synchronization requirement rather than rewriting production behavior to match history.

## Acceptance criteria

Phase 5 and the parent roadmap are complete only when all are true:

- Phases 1-4 are implemented or explicitly closed under their documented stop conditions;
- focused pproxy 2.7.9 source/oracle evidence confirms the final `DUMMY`, `UDP_LIMIT`, config/log, SOCKS4 BIND, and other changed upstream facts;
- Eggress's intentionally unsupported non-`None` `prepare_ciphers`/plugin behavior fails explicitly and is classified honestly rather than presented as a match;
- representative standalone `pproxy` and `eggress pproxy run` supported startup paths work after the execution refactor;
- both compatibility execution entry points still fail closed for unsupported/unknown requests;
- `--test` exact-target behavior and failure/success classification pass focused tests;
- regex backend/boundary tests pass and active documentation states the trusted-configuration/no-hard-timeout model accurately;
- the canonical manifest validator passes;
- canonical manifest and practical matrix agree for every changed capability;
- the broad Rust gate (`fmt`, Clippy with `-D warnings`, workspace locked tests) passes, or any pre-existing/environment-limited exception is explicitly shown not to invalidate an active changed claim;
- a fresh extension build followed by `python/tests` + `tests/compat` passes, with any strict oracle tests run separately as needed;
- existing high-value core wire/runtime tests remain intact;
- no new routine CI job, platform matrix, parity percentage, certification registry, evidence bundle, benchmark gate, or size gate has been added;
- no SSH, QUIC/H3, SSR, legacy cipher, plugin execution, daemonization, general multi-hop UDP, or other excluded scope has been added;
- final Phase 3 size/dependency observations are recorded informationally if applicable;
- all five phase plans and the roadmap are updated in place with final status and implementation/verification references;
- no additional closure plan/report is created;
- the roadmap closure record explicitly states the future reopening threshold;
- after this commit, bounded `pproxy==2.7.9` parity work is considered closed unless a reproducible defect or explicit scope decision reopens it.