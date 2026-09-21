# Maintenance Phase 5 — Documentation and Compatibility Projection Convergence

## Status

**IMPLEMENTED — 2026-09-21**

## Parent

[`MAINTENANCE_CONVERGENCE_ROADMAP.md`](MAINTENANCE_CONVERGENCE_ROADMAP.md)

## Baseline

`fa1d9bf73c5c162c62a6086cf0e7361f1cfb3131`

## Objective

Close the remaining maintained-documentation drift and make the existing pproxy native/TOML projection duality easy to maintain without removing either supported output path or changing compatibility behavior.

This is primarily a documentation/evidence convergence phase. Production refactoring is authorized only when it removes clearly duplicated private projection helpers with no observable behavior change.

## Current state

### Stale maintained Python documentation

`docs/PYTHON_BINDINGS.md` currently states:

- Unix-domain sockets are not supported by the underlying Rust runtime;
- `.pyi` stubs are future work.

Both statements are stale. The current tree contains:

- `UnixListenerConfig`, compiled Unix listener state, and runtime Unix listener handling;
- `python/eggress/py.typed`;
- `python/eggress/_eggress.pyi`;
- `python/eggress/__init__.pyi` and other maintained module stubs.

The limitation text also mixes full runtime capability with what individual Python convenience APIs expose. Those must be distinguished rather than described as one capability boundary.

### Native/TOML pproxy projections

`eggress-pproxy-compat` intentionally supports two projections from shared semantic translation intermediates:

1. native `ConfigFile`/`RuntimeConfig` construction for execution;
2. TOML rendering for migration, diagnostics, `--dump-config`, and Python compatibility output.

This duality is legitimate. Existing `native_equivalence` tests are the primary defense against projection drift.

The maintenance task is to strengthen that authority/evidence model, not to delete TOML rendering or force runtime execution through serialization/reparse.

## Governing constraints

1. Do not change any pproxy capability/tier claim.
2. Do not change parser acceptance, diagnostics, warnings, unsupported classification, or execution-gate behavior.
3. Do not remove TOML output fields or native execution fields.
4. Do not make runtime execution serialize and reparse TOML for convenience.
5. Do not make TOML rendering depend on runtime-only objects.
6. Do not broaden native URI grammar merely to absorb malformed/compat-only pproxy syntax.
7. Preserve credential redaction.
8. Do not change Python public names or stub-visible API.
9. Correct maintained documentation to current behavior; do not rewrite historical milestone/planning descriptions as though they were current docs.
10. Do not convert this phase into feature work for currently unsupported capabilities.

## Workstream 1 — Reconcile Python limitations against current capability sources

Audit and correct at minimum:

- `docs/PYTHON_BINDINGS.md`;
- `architecture/python-bindings.md`;
- `python/README.md`;
- `docs/python/EGRESS_PYTHON_API_CURRENT_STATE.md`;
- `docs/CAPABILITIES.md` only if wording is inconsistent, not to change capability state.

Distinguish three concepts explicitly:

1. capability exists in the Rust runtime;
2. capability is configurable through the embed/Python service facade;
3. capability has a dedicated Python convenience object/API.

For example, a runtime Unix listener capability must not be called absent merely because there is no special Python Unix-listener class.

## Workstream 2 — Remove obsolete stub/typing debt claims

Update maintained docs that still say stubs are future work or that known missing stubs are expected if those statements no longer match the current tree.

Describe the actual current state:

- PEP 561 marker present;
- public/native `.pyi` files present;
- PyO3/private implementation attributes may still have limitations if current tests/type checking prove them;
- runtime/stub agreement is covered by the existing public-export and API-boundary tests.

Do not claim complete static typing quality beyond what is tested.

## Workstream 3 — Document projection authority

Update `architecture/pproxy-compat.md` and any directly related maintained compatibility docs to state clearly:

- semantic translation intermediates are shared;
- native execution projects intermediates directly to typed config/runtime forms;
- TOML is a presentation/migration/compatibility projection;
- native-vs-TOML equivalence tests are the drift gate;
- chain compilation may still use the canonical native URI grammar where that grammar is the intended parser authority.

Avoid language implying that TOML rendering is deprecated or that the native path supersedes Python-visible translation output.

## Workstream 4 — Strengthen projection equivalence tests where there are blind spots

Review `crates/eggress-pproxy-compat/tests/native_equivalence.rs` and related translator tests.

Add focused cases only for fields currently mapped in both projections that are weakly covered, especially:

- listener auth;
- TLS listener material/flags;
- UDP listener mode/fixed target;
- upstream/group scheduler/fallback;
- nested match expressions;
- reverse server/client fields;
- admin/PAC/static content projection;
- empty/None/default distinctions that TOML omission can obscure.

Compare semantic compiled/config structures rather than raw TOML formatting where possible.

Do not build a second generic snapshot framework.

## Workstream 5 — Consolidate private mapping helpers only when obvious

If review finds repeated private conversion of the same semantic field in both native and TOML projection code, extract a private semantic helper only when:

- it does not change ordering/default/omission behavior;
- both projections can consume the helper naturally;
- it reduces actual duplicated rules, not merely duplicated syntax;
- existing equivalence tests prove unchanged behavior.

Do not attempt to force all projection code through one generic serializer abstraction.

## Workstream 6 — Repository-wide stale current-state search

Search maintained docs for current-state statements involving:

```text
not supported
not yet supported
future release
future work
missing stub
no .pyi
Unix domain
Python limitation
listener limitation
```

Review hits against current capability/runtime code.

Historical plans and explicitly historical reports may retain old baseline descriptions. Active architecture, API reference, README, and capability docs may not.

## Verification

Compatibility translator:

```bash
cargo test -p eggress-pproxy-compat --locked
cargo test -p eggress-pproxy-compat --locked --test native_equivalence
cargo test -p eggress-pproxy-compat --locked --test uri_syntax_equivalence
```

Python docs/API agreement:

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest   python/tests/test_public_exports.py   python/tests/test_wheel_import_smoke.py   python/tests/test_api_boundary_closure.py   python/tests/test_docs_examples.py -q
```

If compatibility implementation code changes, also run:

```bash
.venv/bin/python -m pytest python/tests tests/compat -q
```

Final repository gate for code changes:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

For a documentation-only final patch after all code is already qualified, use the normal narrow documentation/example checks rather than inventing additional evidence tooling.

## Stop conditions

Do not refactor projection code if:

1. the only gain is fewer lines without reducing duplicated semantic rules;
2. TOML omission/default behavior would become less explicit;
3. diagnostics/warning ordering could change;
4. credential-redaction handling would move farther from its compatibility boundary;
5. a new generic mapping framework would be harder to audit than the explicit projections.

In those cases, keep explicit code and rely on equivalence tests.

## Acceptance criteria

- [ ] Maintained Python docs no longer say Unix-domain sockets are absent from the Rust runtime.
- [ ] Maintained Python docs no longer describe `.pyi` support as future work.
- [ ] Runtime capability, service-facade exposure, and dedicated Python convenience API are described as distinct layers.
- [ ] `architecture/pproxy-compat.md` identifies the shared semantic intermediate model and both supported projections accurately.
- [ ] Native/TOML equivalence tests cover the important dual-mapped field families.
- [ ] Any private helper extraction reduces duplicated semantic rules and preserves output exactly.
- [ ] No pproxy parser, diagnostic, tier, warning, TOML, runtime, or Python API behavior changes.
- [ ] No compatibility capability claim changes.
- [ ] Current-state stale-doc search has been reviewed and maintained docs are reconciled.
- [ ] Focused compatibility/Python documentation tests pass.
- [ ] If code changed, workspace fmt, clippy, locked tests, and full Python/compat suite pass.

## Closure record

## Closure record

Implementation (docs + equivalence only; no compat behavior/claim change):

- Corrected `docs/PYTHON_BINDINGS.md` (native/TOML dual projection, Unix three-layer distinction, stubs shipped + `py.typed`, compat framing as runtime/facade/convenience layers).
- Corrected `python/README.md` non-parity (multi-hop, generic TOML listeners, opt-in `legacy-crypto`/`ssh`/`pproxy-daemon`, Unix/redir/standalone UDP supported).
- Marked `docs/python/EGRESS_PYTHON_API_CURRENT_STATE.md` as Phase 29 historical snapshot with current-state corrections (alias, `__all__`, gaps, native tables).
- `architecture/pproxy-compat.md` + `architecture/python-bindings.md` already authoritative (shared intermediates, no TOML round-trip, equivalence gate) — no change.
- Strengthened `native_equivalence.rs`: extended `assert_runtime_equivalent` with listener auth/TLS/UDP + group scheduler; added `listener_auth_and_multi_remote_group_equivalent` (auth presence, warnings/unsupported agreement, redacted views). No private helper extraction (explicit projections retained per stop conditions).
- Stale current-state search reviewed; historical plans/reports retain baselines by policy.

Evidence: `cargo test -p eggress-pproxy-compat --locked --test native_equivalence` (6 passed), `--test uri_syntax_equivalence`; `cargo test -p eggress-pproxy-compat --locked`.

(End of file - total 229 lines)
