# Post-Parity Final Corrective Pass

## Status

**PLANNED**

Parent roadmap: `POST_PARITY_CORRECTIVE_AND_REDUCTION_ROADMAP.md`

Planning baseline: `0029194a82ec8bdadc3c89c0944902db1bd3333f`.

## Purpose

Close the remaining defects found after implementation of the four-phase
post-parity corrective/reduction roadmap.

This is a deliberately small closure pass. It does **not** reopen the bounded
`pproxy==2.7.9` parity program and it does not authorize new proxy features,
protocol implementations, compatibility claims, CI matrices, certification
machinery, or binary-size work.

The implementation baseline is otherwise healthy:

- Phase 1 corrected the `serve_connection()` metrics lifecycle so successful and
  failed sessions share one terminal finalization path;
- Phase 2 moved per-diagnostic tier ownership into Rust and removed the Python
  hand-maintained diagnostic/intentional-non-parity tables;
- Phase 3 corrected SOCKS5 RSV validation and removed silent domain truncation;
- Phase 4 kept routine CI small, added the documented lean-build check, reduced
  redundant release verification, measured binary composition, and correctly
  stopped size work when no material no-feature-loss opportunity remained;
- current Rust CI and Python smoke checks pass on the planning baseline.

Two narrow correctness/verification gaps remain, plus roadmap bookkeeping.

## Remaining defects

### Defect A — aggregate compatibility tier is still not single-source

Phase 2 correctly made Rust the executable owner of **per-diagnostic** tier
classification, but aggregate classification policy is still duplicated.

At the planning baseline:

1. `crates/eggress-pproxy-compat/src/tier.rs::classify_aggregate_tier()` has the
   desired warning precedence:

   ```text
   unsupported
   > intentional_non_parity
   > compatible_with_warning
   > native_equivalent
   > drop_in
   ```

   but it immediately returns `unsupported` whenever the `unsupported` feature
   list is non-empty. It therefore does not inspect the already-owned native tier
   for unsupported-feature records such as SSH/SSR/legacy cipher exclusions.
   A known intentional exclusion can therefore aggregate as `unsupported` in the
   Rust/CLI path despite its structured feature tier being
   `intentional_non_parity`.

2. `python/eggress/pproxy.py::_classify_aggregate_tier()` still contains an
   independent aggregate-policy implementation. It correctly distinguishes
   intentional unsupported features from hard unsupported features, but it still
   checks `native_equivalent` before `compatible_with_warning`. Thus a mixed
   report can be upgraded too favorably.

3. The CLI uses the Rust aggregate classifier while Python uses the Python
   aggregate classifier. The same translated feature set can therefore receive
   different aggregate tiers even though individual diagnostic tiers now agree.

This misses the Phase 2 acceptance requirement that retained aggregate tiering be
owned and ordered consistently.

### Defect B — metrics lifecycle regression uses only a test double

Phase 1 fixed the production control-flow bug: once
`record_session_start()` executes, every non-panicking `serve_connection()`
return now reaches exactly one `record_session()` call.

The new `RecordingMetrics` tests prove start/terminal callback balance for
success, authentication failure, malformed protocol, timeout, and route failure.

However, the Phase 1 acceptance criteria also asked for concrete metric semantics
to be pinned where practical:

```text
eggress_connections_active == 0
eggress_connections_total == 1
eggress_connection_failures_total == 1
```

after a failed handshake, with authentication failures additionally incrementing
`eggress_auth_failures_total`.

The concrete `MetricsRegistry` implements those semantics, but the new lifecycle
regressions do not exercise the registry itself. This is a test-coverage gap, not
evidence that the runtime bug remains.

### Defect C — parent roadmap is not formally closed

All four phase plans are marked complete, but
`POST_PARITY_CORRECTIVE_AND_REDUCTION_ROADMAP.md` still reports `PLANNED` and its
phase summaries do not consistently record closure.

Plans remain non-authoritative provenance. This bookkeeping must be corrected
only after the executable work and verification in this pass succeed.

## Objective

Reach one stable post-parity closure state where:

- Rust owns both per-diagnostic and aggregate tier semantics;
- CLI and Python consume the same native aggregate result;
- intentional exclusions remain distinguishable from accidental/unknown
  unsupported features at aggregate level;
- a material compatibility warning cannot be hidden by a
  `native_equivalent` warning;
- concrete Prometheus connection metrics have one focused failed-handshake
  regression;
- the parent roadmap is marked complete only after all checks pass;
- no new pproxy parity scope is opened.

## Non-goals

Do **not** use this pass to:

- implement SSH, QUIC/HTTP3, SSR, legacy Shadowsocks ciphers/OTA, daemonization,
  SOCKS BIND, plugins, macOS PF recovery, backward TLS, or general multi-hop UDP;
- change the existing bounded Python compatibility claim;
- add a parity percentage or new compatibility taxonomy;
- redesign the compatibility manifest;
- move compatibility policy into Python;
- parse the TOML manifest at runtime merely to classify one invocation;
- redesign `SessionMetrics` or `MetricsRegistry`;
- add an RAII session framework now that the control flow is already balanced;
- add a new metrics counter;
- add new CI workflows, OS matrices, oracle jobs, soak tests, fuzzing, benchmarks,
  `cargo audit`, or `cargo deny` to routine push CI;
- resume binary-size optimization;
- alter the manual crates.io release policy or the PyPI wheel matrix;
- rewrite historical plans beyond the one parent-roadmap closure update required
  here.

## Workstream 1 — make aggregate tier native and authoritative

### 1.1 Correct native unsupported-feature aggregation

Primary file:

```text
crates/eggress-pproxy-compat/src/tier.rs
```

Supporting classification source:

```text
crates/eggress-pproxy-compat/src/diagnostics.rs
```

`classify_aggregate_tier()` must evaluate the native tier of every unsupported
feature rather than treating a non-empty unsupported list as automatically
`unsupported`.

The aggregate precedence must remain, worst to best:

```text
unsupported
intentional_non_parity
compatible_with_warning
native_equivalent
drop_in
```

Required semantics:

- if any unsupported feature is classified `unsupported`, aggregate is
  `unsupported`;
- otherwise if any unsupported feature or warning is
  `intentional_non_parity`, aggregate is `intentional_non_parity`;
- otherwise if any warning is `compatible_with_warning`, aggregate is
  `compatible_with_warning`;
- otherwise if any warning is `native_equivalent`, aggregate is
  `native_equivalent`;
- otherwise aggregate is `drop_in`.

Unknown unsupported feature IDs must continue to fail closed to `unsupported`.
Unknown warning categories must continue to fail closed to `unsupported`.

Do not duplicate the unsupported-feature tier mapping in `tier.rs`. Reuse the
native classification function already owned by `diagnostics.rs`, or factor a
small shared native helper if required to avoid a module-cycle problem.

### 1.2 Ensure aggregate classification is usable from the Python binding

Likely files:

```text
crates/eggress-pproxy-compat/src/lib.rs
crates/eggress-python/src/lib.rs
python/eggress/pproxy.py
python/eggress/pproxy.pyi
```

Preferred design: expose the native aggregate tier directly on the PyO3
translation result, for example as a read-only `tier` property, or expose one
small native aggregate-classification function whose arguments are the existing
native translation result.

The installed Python compatibility path must not recreate aggregate policy from
strings.

Prefer this shape conceptually:

```python
result = _translate_pproxy_args(...)
tier = result.tier
```

rather than:

```python
tier = _classify_aggregate_tier(result.warnings, result.unsupported)
```

The precise PyO3 API may differ, but Rust must be the normal executable owner.

### 1.3 Delete Python aggregate-policy duplication

Remove:

```text
python/eggress/pproxy.py::_classify_aggregate_tier
```

once the native aggregate tier is available.

Do not replace it with another Python severity table, set, or mapping.

If a compatibility fallback is absolutely required for extension-version skew,
it must be:

- narrowly version-gated;
- fail-closed;
- explicitly documented as a fallback only;
- absent from the normal same-wheel installation path.

The preferred solution is no fallback because the Python package and native
extension ship together.

### 1.4 Keep CLI on the same native classifier

Likely file:

```text
crates/eggress-cli/src/main.rs
```

The CLI already calls the native aggregate classifier. Preserve that architecture.
Do not add a CLI-specific tier layer.

After fixing the native function, CLI `pproxy check` and Python
`check_pproxy_args()` must agree for the same translated arguments.

## Workstream 2 — add focused aggregate-tier regressions

### Rust unit tests

Primary file:

```text
crates/eggress-pproxy-compat/src/tier.rs
```

Add or correct focused cases proving at least:

```text
no diagnostics
    -> drop_in

native-equivalent warning only
    -> native_equivalent

compatible warning only
    -> compatible_with_warning

native_equivalent + compatible_with_warning
    -> compatible_with_warning

SSH/SSR/legacy-cipher intentional exclusion only
    -> intentional_non_parity

intentional_non_parity + compatible_with_warning
    -> intentional_non_parity

hard unsupported feature + intentional_non_parity
    -> unsupported

unknown warning category
    -> unsupported

unknown unsupported feature ID
    -> unsupported

SOCKS4 BIND
    -> unsupported

SOCKS5 BIND
    -> unsupported
```

Rename stale tests whose names encode the previous incorrect precedence. A test
named as though native-equivalent "beats" compatible warning must not remain even
if its assertion was corrected.

### Cross-surface tests

Add a small number of real translation/report tests that exercise the public
surfaces rather than only constructing `CompatWarning` by hand.

At minimum prove:

1. one invocation producing both a `native_equivalent` warning and a
   `compatible_with_warning` warning aggregates to
   `compatible_with_warning` in Rust/CLI semantics;
2. an SSH listener or upstream reports aggregate `intentional_non_parity` rather
   than generic `unsupported` while still being non-runnable / `ok == false`;
3. a genuinely unsupported feature remains aggregate `unsupported`;
4. Python returns the same aggregate value as the native result.

Use stable translator inputs already covered by the repository. Good candidates
include `--reuse` for `native_equivalent`, `--log` or `-d` for
`compatible_with_warning`, and SSH for `intentional_non_parity`. Do not create
synthetic CLI flags solely for testing.

### Python tests

Likely file:

```text
python/tests/test_pproxy_dropin.py
```

Add explicit assertions for real `check_pproxy_args()` results, including a mixed
warning case. Tests should detect if Python ever reintroduces the old
native-before-compatible ordering.

Do not test a private Python aggregate helper after that helper is deleted.

## Workstream 3 — pin concrete `MetricsRegistry` lifecycle semantics

### 3.1 Add one concrete failed-handshake regression

Preferred locations, in order:

```text
crates/eggress-runtime/tests/observability.rs
crates/eggress-server/tests/            # only if concrete registry wiring is cleaner
crates/eggress-metrics/tests/           # only if the server path can be exercised without awkward layering
```

Exercise a real connection through the existing runtime/server path using the
actual `eggress_metrics::MetricsRegistry`, not only a trait test double.

The smallest useful case is a malformed SOCKS/HTTP handshake or deliberate
handshake timeout. After the connection terminates, render or inspect the actual
Prometheus metrics and assert:

```text
eggress_connections_active == 0
eggress_connections_total == 1
eggress_connection_failures_total == 1
```

Metric-name handling should reuse existing Prometheus parsing helpers where
possible. Do not introduce another metrics parser if
`crates/eggress-runtime/tests/observability.rs` already has appropriate helpers.

### 3.2 Add auth assertion only if cheap

If the same fixture makes an authentication-failure case straightforward, also
assert:

```text
eggress_auth_failures_total == 1
```

This is desirable but must not turn the closure pass into a broad metrics test
rewrite. The mandatory concrete regression is one failed handshake proving
active/total/failure accounting.

### 3.3 Preserve existing trait-boundary tests

Do not delete the Phase 1 `RecordingMetrics` tests. They prove structural
exactly-once callback behavior and complement the concrete registry test.

Do not alter listener semaphore/concurrency behavior.

## Workstream 4 — close planning records only after executable closure

Files:

```text
plans/POST_PARITY_CORRECTIVE_AND_REDUCTION_ROADMAP.md
plans/POST_PARITY_PHASE_2_COMPATIBILITY_TAXONOMY_SINGLE_SOURCE.md
plans/POST_PARITY_FINAL_CORRECTIVE_PASS.md
```

After implementation and all required checks pass:

- mark this corrective pass `COMPLETE`;
- mark the parent roadmap `COMPLETE` or equivalent closed status;
- make the parent phase summary accurately state that Phases 1–4 and this final
  correction are complete;
- if useful, add one short closure note naming the final defect corrected.

Do not append command transcripts, artifact inventories, generated evidence,
large implementation summaries, or another follow-up plan.

Do not rewrite historical completed plans merely for stylistic consistency.

## Required verification

### Focused Rust

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-testkit canonical_manifest
cargo test -p eggress-server metrics_lifecycle
cargo test -p eggress-runtime observability
```

If the actual test names differ, run the smallest equivalent crate/test targets.

### Python

Build/use the current native extension in the repository's normal clean test
flow, then run:

```bash
python -m pytest python/tests/test_pproxy_dropin.py -q
python -m pytest python/tests tests/compat -q
```

### Standard Rust gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check -p eggress-cli --no-default-features --features common
```

### Manifest

Run the maintained canonical manifest validator using the repository's existing
command/path and require zero hard errors.

Do not add a new validator.

### Hosted CI

The existing Rust CI and path-scoped Python smoke workflows should remain the
only routine hosted push checks involved in this pass. Both must be green on the
implementation commit before closure.

No external pproxy oracle run is required to close this defect because aggregate
classification is an Eggress reporting invariant over already-classified native
translation results.

## Explicit acceptance criteria

This corrective pass is complete only when all of the following are true:

1. Rust remains the single normal executable owner of per-diagnostic tier
   semantics.
2. Rust also becomes the single normal executable owner of aggregate tier
   semantics.
3. `classify_aggregate_tier()` no longer treats every non-empty unsupported list
   as automatically generic `unsupported`.
4. Native aggregation consults the native classification of unsupported feature
   IDs.
5. A known intentional exclusion such as SSH aggregates to
   `intentional_non_parity` when no harder unsupported feature is present.
6. SSH remains non-runnable / unsupported operationally; changing its aggregate
   reporting does not implement or enable SSH.
7. SSR and legacy-cipher intentional exclusions retain their existing native
   per-feature classification.
8. A genuinely unsupported feature aggregates to `unsupported`.
9. An unknown unsupported feature ID fails closed to `unsupported`.
10. An unknown warning category fails closed to `unsupported`.
11. `compatible_with_warning` dominates `native_equivalent` in aggregate
    severity.
12. A real mixed invocation containing one native-equivalent warning and one
    compatible warning aggregates to `compatible_with_warning`.
13. `intentional_non_parity` dominates `compatible_with_warning` and
    `native_equivalent`.
14. `unsupported` dominates all other tiers.
15. SOCKS4 BIND remains aggregate `unsupported`.
16. SOCKS5 BIND remains aggregate `unsupported`.
17. The Rust CLI compatibility report uses the corrected native aggregate result.
18. Python `check_pproxy_args()` uses the corrected native aggregate result.
19. `python/eggress/pproxy.py::_classify_aggregate_tier()` is removed from the
    normal implementation.
20. Python does not introduce a replacement hand-maintained severity table,
    category map, or intentional-exclusion set.
21. The same representative translated arguments produce the same aggregate tier
    in native/Rust and Python reporting.
22. Per-diagnostic `--log` classification remains
    `compatible_with_warning`.
23. Per-diagnostic SOCKS BIND classifications remain aligned with the canonical
    manifest.
24. Existing canonical manifest/reporting cross-checks continue to pass.
25. No second full capability manifest is introduced in Rust or Python.
26. No runtime TOML-manifest parsing is added to ordinary compatibility checks.
27. A concrete `MetricsRegistry` regression exercises a failed connection through
    the real server/runtime lifecycle.
28. After that failed connection terminates,
    `eggress_connections_active` is exactly back at baseline/zero for the isolated
    test.
29. The same regression proves `eggress_connections_total` increments exactly
    once.
30. The same regression proves `eggress_connection_failures_total` increments
    exactly once.
31. If an auth-failure concrete case is added, `eggress_auth_failures_total`
    increments exactly once.
32. Existing `RecordingMetrics` exactly-once lifecycle tests remain in place or
    are replaced only by stronger equivalent structural tests.
33. Listener admission/semaphore behavior is unchanged.
34. No new metric is added.
35. No new protocol, proxy feature, public parity claim, or compatibility tier is
    introduced.
36. No new routine CI workflow or matrix is introduced.
37. No external pproxy oracle, fuzz, soak, benchmark, audit, or release evidence
    job becomes a routine push gate.
38. The lean-build compile check added in Phase 4 remains intact.
39. PyPI and crates.io release policies are unchanged.
40. No further binary-size architecture work is performed.
41. Focused `eggress-pproxy-compat` tests pass.
42. Focused canonical-manifest tests pass.
43. Focused metrics/runtime tests pass.
44. Python pproxy compatibility tests pass.
45. `python -m pytest python/tests tests/compat -q` passes in the normal native
    extension test environment.
46. `cargo fmt --all -- --check` passes.
47. `cargo clippy --workspace --all-targets -- -D warnings` passes.
48. `cargo test --workspace --locked` passes.
49. `cargo check -p eggress-cli --no-default-features --features common` passes.
50. The maintained canonical manifest validator reports zero hard errors.
51. The existing Rust CI workflow is green on the implementation commit.
52. The existing Python smoke workflow is green when triggered by the changed
    Python/native files.
53. This plan is marked complete only after the executable criteria above pass.
54. `POST_PARITY_CORRECTIVE_AND_REDUCTION_ROADMAP.md` is then marked complete and
    no additional closure/certification plan is created.

## Stop conditions

If aggregate tiering can be exposed directly through the existing PyO3
translation-result wrapper, do that and stop. Do not create a registry,
serialization protocol, generated Python module, or runtime manifest reader.

If the concrete metrics assertion can be added to the existing runtime
observability test support, reuse it and stop. Do not create a new metrics
harness.

If implementation discovers a separate protocol defect, security issue, or
feature-parity gap, record it independently. Do not expand this final corrective
pass to absorb unrelated work.

When the acceptance criteria above pass, close the parent roadmap and end this
line of work. Future pproxy compatibility changes should require a new
reproducible defect or an explicit product-scope decision.