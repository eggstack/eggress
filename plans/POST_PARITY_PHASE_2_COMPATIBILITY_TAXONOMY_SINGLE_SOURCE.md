# Post-Parity Phase 2 — Compatibility Taxonomy Single Source

## Status

**PLANNED**

Parent roadmap: `POST_PARITY_CORRECTIVE_AND_REDUCTION_ROADMAP.md`

Depends on: Phase 1 only for execution order; there is no code dependency.

## Problem statement

The repository has one canonical capability manifest, but compatibility tier
semantics are still hand-maintained in multiple executable locations.

At the planning baseline:

- the maintained contract classifies `--log` as
  `compatible_with_warning` because Eggress recognizes the path but does not
  reproduce pproxy's file-writing behavior;
- `python/eggress/pproxy.py` still maps the `log-file` diagnostic category to
  `native_equivalent`;
- the Rust compatibility tier mapper also maps `log-file` to
  `native_equivalent`;
- Python still lists `socks4-bind` and `socks5-bind` as
  `intentional_non_parity` even though the corrected canonical contract no
  longer treats matching upstream BIND refusal as Eggress-specific intentional
  non-parity.

This is a source-of-truth architecture defect, not merely a documentation typo.

The aggregate classifier also currently treats a `native_equivalent` warning as
worse/more dominant than `compatible_with_warning`. That can allow a materially
degraded option to be hidden by another feature with better compatibility.

## Objective

Make compatibility classification deterministic and owned by one executable
native mapping, with the canonical manifest validating that mapping and Python
consuming it instead of maintaining an independent semantic table.

Keep the existing five-tier vocabulary:

```text
drop_in
native_equivalent
compatible_with_warning
intentional_non_parity
unsupported
```

Do not introduce another taxonomy.

## Likely files

Primary:

```text
crates/eggress-pproxy-compat/src/tier.rs
crates/eggress-pproxy-compat/src/diagnostics.rs
crates/eggress-pproxy-compat/src/warnings.rs
crates/eggress-python/src/lib.rs
python/eggress/pproxy.py
docs/parity/pproxy_capability_manifest.toml
docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md
crates/eggress-testkit/src/canonical_manifest.rs
python/tests/
tests/compat/
```

Inspect before editing:

```text
docs/parity/README.md
crates/eggress-pproxy-compat/src/lib.rs
```

## Design requirements

### 1. Choose one executable tier owner

Prefer the Rust compatibility crate as the executable owner because:

- CLI translation already originates there;
- PyO3 can expose structured results;
- Python should adapt native classification rather than recreate it.

Provide a small stable function/data field that answers the tier for a known
diagnostic/feature classification.

Do not parse the TOML manifest at runtime merely to answer every compatibility
check. The manifest is the canonical contract and validation target, not a
runtime configuration file.

### 2. Remove Python's independent tier lookup tables

Delete or reduce Python-owned sets such as:

```text
INTENTIONAL_NON_PARITY_FEATURE_IDS
native_equivalent = {...}
compatible_with_warning = {...}
```

where the native report can provide the tier directly.

Python may retain a compatibility fallback only if required for binary-version
skew, and that fallback must be minimal, explicitly bounded, and tested. The
normal installed-wheel path must use native classification.

### 3. Correct known drift

At minimum reconcile:

```text
log-file
socks4-bind
socks5-bind
```

with the canonical contract.

`--log PATH` must not report `native_equivalent` while Eggress does not write the
requested file.

For BIND, classify according to the actual maintained manifest semantics. Do not
implement SOCKS BIND in this phase.

### 4. Correct aggregate severity semantics

If aggregate tiering is retained, define severity as user-visible degradation,
not implementation novelty.

Required dominance order from worst to best:

```text
unsupported
intentional_non_parity
compatible_with_warning
native_equivalent
drop_in
```

Thus, a report containing both:

```text
native_equivalent + compatible_with_warning
```

must aggregate to `compatible_with_warning`.

If aggregate tiering is not required by public API compatibility and removing it
would be simpler, it may be replaced with explicit per-feature classifications
plus an `ok`/hard-failure indicator. However, do not break documented public
Python APIs without first checking current tests and docs. The lower-risk default
is to retain the field and fix ordering.

### 5. Make the manifest test executable semantics

The canonical manifest validator currently checks internal manifest consistency.
Add a focused cross-check for entries that correspond to executable diagnostics
so a future change cannot produce:

```text
manifest tier != Rust reporter tier
```

Do not attempt to encode all 149 capabilities as hard-coded Rust test cases.
Cover the diagnostic categories/feature IDs that the reporter directly exposes,
using a compact mapping or representative generated cases from existing data.

### 6. Keep human documentation derived and concise

Update the maintained practical matrix only where the observable classification
changes or a stale statement is corrected.

Do not update historical parity plans line-by-line.

Do not add a third public compatibility table.

## Focused tests

Rust:

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-testkit canonical_manifest
```

Python after building/developing the extension:

```bash
python -m pytest python/tests tests/compat -q
```

At minimum add explicit assertions for:

```text
--log -> compatible_with_warning
socks4-bind -> canonical/current classification
socks5-bind -> canonical/current classification
native_equivalent + compatible_with_warning
    -> compatible_with_warning aggregate
unknown category -> fail closed / unsupported
```

Also preserve redaction tests for any structured diagnostic containing a URI or
credential-bearing value.

## Explicit acceptance criteria

Phase 2 is complete only when:

1. There is one normal executable owner for compatibility tier semantics.
2. Python's normal compatibility-report path does not independently maintain the
   complete diagnostic-category → tier mapping.
3. Python's normal compatibility-report path does not independently maintain the
   complete intentional-non-parity feature-ID set when native results already
   contain that information.
4. `--log` reports `compatible_with_warning` consistently in Rust.
5. `--log` reports `compatible_with_warning` consistently in Python.
6. The canonical manifest and maintained human matrix agree with that `--log`
   result.
7. SOCKS4 BIND classification is identical between manifest, Rust report, and
   Python report.
8. SOCKS5 BIND classification is identical between manifest, Rust report, and
   Python report.
9. No BIND implementation is added.
10. A mixed report containing `native_equivalent` and
    `compatible_with_warning` aggregates to `compatible_with_warning`, if the
    aggregate field is retained.
11. `unsupported` remains dominant over all non-hard-failure tiers.
12. `intentional_non_parity` remains distinguishable from an unknown/accidental
    unsupported feature.
13. Unknown diagnostic categories fail closed rather than being silently upgraded.
14. The canonical manifest validator has at least one executable cross-check
    preventing reporter/manifest tier drift.
15. The cross-check does not require maintaining a second full manifest in Rust
    source.
16. No new parity percentage, dashboard, generated report, or public taxonomy is
    introduced.
17. Existing public Python compatibility APIs retain their documented shape
    unless an intentional compatibility-preserving migration is documented and
    tested.
18. Credential-bearing diagnostic output remains redacted.
19. `cargo test -p eggress-pproxy-compat` passes.
20. `cargo test -p eggress-testkit canonical_manifest` passes.
21. Clean Python compatibility tests pass.
22. `cargo fmt --all -- --check` passes.
23. `cargo clippy --workspace --all-targets -- -D warnings` passes.
24. `cargo test --workspace --locked` passes.

## Stop condition

If a fully native tier is already present on every PyO3 warning/unsupported
object and Python can simply consume it, do not introduce a new registry or
serialization layer. Make the smallest deletion/refactor that removes duplicate
policy.
