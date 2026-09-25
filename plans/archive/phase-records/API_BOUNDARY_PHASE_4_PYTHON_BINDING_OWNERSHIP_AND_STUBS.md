# API Boundary Phase 4 — Python Binding Ownership and Stub Convergence

## Status

**IMPLEMENTED — 2026-09-21**

Implementation landed in `c15c60b189d0588d254011e877b78b2ca8a6b3a9`. Residual post-implementation findings were closed by [`API_BOUNDARY_CORRECTIVE_CLOSURE.md`](API_BOUNDARY_CORRECTIVE_CLOSURE.md).

## Parent

[`API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md)

## Baseline

`2c9d064794a2b74831979f7f36fb25e4f5992707`

## Objective

Reduce unnecessary architectural reach-through from `eggress-python`, repair runtime/type-stub/documentation drift, and make capability metadata mechanically consistent while preserving the existing Python API and wheel behavior.

## Current dependency boundary

`eggress-python` directly depends on:

- `eggress-embed`;
- `eggress-pproxy-compat`;
- `eggress-config`;
- `eggress-routing`;
- `eggress-core`;
- `eggress-uri`;
- `eggress-system-proxy`;
- `eggress-cli`;
- `eggress-runtime`.

Not every edge is necessarily wrong. This phase must prove liveness and ownership before deleting or moving anything.

## Workstream 1 — Build a binding dependency ownership map

For each direct internal dependency, record:

- importing binding module;
- exact symbols consumed;
- whether the symbol is data-plane, compatibility, diagnostics, system integration, or lifecycle;
- natural owner crate;
- whether the dependency is required in the final wheel's default feature set;
- whether a stable owner API already exists.

Use compiler/import evidence, not assumptions from crate names.

The intended high-level boundary is:

- service lifecycle/outbound service facade → `eggress-embed`;
- pproxy parsing/translation/diagnostics → `eggress-pproxy-compat`;
- system proxy application → `eggress-system-proxy`;
- direct lower-level crates only where Python intentionally exposes that lower-level concept.

## Workstream 2 — Remove reach-through only when an existing owner can absorb it

If `eggress-python` uses `eggress-cli` only to access reusable non-CLI logic, move/reuse that logic from its natural existing owner rather than making the bindings depend on CLI presentation code.

Likewise, eliminate direct `eggress-config`, `eggress-routing`, `eggress-core`, `eggress-uri`, or `eggress-runtime` edges only when:

1. the binding does not intentionally expose that crate's type/concept;
2. an existing owner facade already provides the required behavior;
3. moving the logic does not add a new public API.

Do not create an `eggress-python-support` crate.

Do not funnel everything through `eggress-embed` if that would make embed depend on CLI-only or Python-only concerns.

After each dependency removal, use `cargo tree -p eggress-python -e features` to verify no heavier replacement path was introduced.

## Workstream 3 — Repair native extension stubs

Compare `crates/eggress-python/src/lib.rs` module registration and each `#[pymethods]` block against `python/eggress/_eggress.pyi`.

At minimum review current drift around:

- `PyEggressService.start_with_compatibility_options`;
- all registered exception classes;
- outbound methods/properties;
- system-proxy objects/functions;
- compatibility helper functions;
- static methods and argument defaults.

Update stubs to describe the runtime that exists. Do not add runtime functions merely because a stub mentions them.

## Workstream 4 — Repair public package stubs and __all__ agreement

Compare:

- `python/eggress/__init__.py`;
- `python/eggress/__init__.pyi`;
- `python/eggress/exceptions.py`;
- `python/eggress/exceptions.pyi`;
- other module `.py/.pyi` pairs.

Add a lightweight test that detects missing maintained exports without trying to assert every private helper.

The test should explicitly tolerate conditional native-import fallback behavior while validating a built wheel/native test environment.

Preserve every existing public name even if it is awkward or redundant.

## Workstream 5 — Capability metadata consistency

`eggress.capabilities()` currently returns a stable dictionary with hard-coded lists.

Do not expand or reorder that public output opportunistically in this phase.

Instead:

1. identify the authoritative source for each current value;
2. derive values internally when doing so produces exactly the same public result, or
3. add consistency tests documenting that the public list is intentionally a stable subset.

Avoid creating a second generated capability manifest. The existing pproxy capability manifest remains authoritative for compatibility claims; `capabilities()` is an Eggress Python API contract.

## Workstream 6 — Documentation convergence

Update current-state docs that still describe obsolete gaps or old async implementation details.

Review at minimum:

- `architecture/python-bindings.md`;
- `docs/PYTHON_BINDINGS.md`;
- `docs/python/EGRESS_PYTHON_API_CURRENT_STATE.md`;
- `docs/python/PYTHON_LIFECYCLE_PARITY.md`;
- crate/package READMEs touched by dependency changes.

Do not rewrite historical milestone plans.

## Workstream 7 — Wheel/import qualification

Build the extension/wheel using the normal abi3 path and verify:

- native import succeeds;
- pure-Python package imports all documented symbols;
- `eggress-pproxy-compat` remains the only package owning top-level `pproxy`;
- no `sys.modules` aliasing is introduced;
- source tree does not shadow the built extension under pytest importlib mode.

## Verification

```bash
cargo check -p eggress-python --locked
cargo tree -p eggress-python -e features

(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest   python/tests/test_wheel_import_smoke.py   python/tests/test_errors.py   python/tests/test_service.py   python/tests/test_pproxy_public_namespace.py   tests/compat -q

.venv/bin/python -m pytest python/tests tests/compat -q

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

If a dependency changes, also run the repository's normal dependency audit/release checks required by `AGENTS.md`.

## Stop conditions

Do not remove a direct binding dependency if doing so would:

1. require a new public Rust API solely for Python;
2. make `eggress-embed` own CLI presentation or Python-specific behavior;
3. increase the dependency graph materially through a heavier facade;
4. change a Python return type, exception, or capability value.

Retain the justified dependency and document why it is architectural rather than accidental.

## Acceptance criteria

- [x] Every direct internal `eggress-python` dependency has a documented owner/use justification.
- [x] Unnecessary reach-through edges are removed without adding a new crate.
- [x] Removed dependencies do not return indirectly through a heavier facade without justification.
- [x] `_eggress.pyi` matches registered native classes/functions/methods for the maintained surface.
- [x] Public `.py/.pyi` pairs and `__all__` agree.
- [x] Existing Python public names and import locations are preserved.
- [x] `capabilities()` retains its public contract and is protected against metadata drift.
- [x] Wheel/import smoke and full Python suite are green.
- [x] Current Python architecture documentation no longer describes obsolete gaps as current.

## Closure evidence (2026-09-21 polish)

- Ownership map: `architecture/python-bindings.md## Binding ownership` justifies every direct `eggress-python` edge (embed lifecycle/facade, pproxy-compat parsing/translation, config/routing/core route-explain, uri redaction, system-proxy binding, cli upstream-test helper, runtime startup hooks). No new crate was added; retained edges are recorded as live architectural edges rather than accidental reach-through. `cargo tree -p eggress-python -e features` shows no heavier replacement path.
- Stubs: `python/eggress/_eggress.pyi` matches `crates/eggress-python/src/lib.rs` module registration and `#[pymethods]` (including removal of accidental `write_blocking_for_sync`, private `_submit_write` absent from the maintained public stub surface per `test_private_submit_exists_but_not_public`); `python/eggress/exceptions.py`/`exceptions.pyi` identity covered by `TestExceptionIdentityMatrix::test_stubs_agree_with_runtime`.
- Exports: `python/tests/test_public_exports.py::test_all_public_exports_are_bound_in_native_test_environment` validates `__all__`/`.pyi` agreement with conditional native-fallback tolerance; existing public names/import locations preserved (no renames).
- Capabilities: `eggress.capabilities()` stable shape/values protected by `test_capabilities_contract_is_stable` and `test_wheel_import_smoke.py::test_capabilities`; pproxy manifest remains authoritative for compat claims.
- Wheel/import: `python/tests/test_wheel_import_smoke.py`, `test_pproxy_public_namespace.py`, `tests/compat` verify native import, pure-Python symbols, sole `pproxy` ownership by `eggress-pproxy-compat`, no `sys.modules` aliasing, importlib-mode non-shadowing.
- Docs: `architecture/python-bindings.md`, `docs/PYTHON_BINDINGS.md`, `docs/python/EGRESS_PYTHON_API_CURRENT_STATE.md`, `docs/python/PYTHON_LIFECYCLE_PARITY.md` no longer describe obsolete async gaps as current.
- Broad gate: full `python/tests tests/compat` (2308 passed), `cargo test --workspace --locked` (2947 passed), fmt/clippy green.
