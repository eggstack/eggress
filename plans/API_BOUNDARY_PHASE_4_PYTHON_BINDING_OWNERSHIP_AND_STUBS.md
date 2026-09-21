# API Boundary Phase 4 — Python Binding Ownership and Stub Convergence

## Status

**READY FOR IMPLEMENTATION — 2026-09-21**

## Parent

[`API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md)

## Baseline

`2c9d064794a2b74831979f7f36fb25e4f5992707`

## Objective

Reduce unnecessary architectural reach-through from `egress-python`, repair runtime/type-stub/documentation drift, and make capability metadata mechanically consistent while preserving the existing Python API and wheel behavior.

## Current dependency boundary

`egress-python` directly depends on:

- `egress-embed`;
- `egress-pproxy-compat`;
- `egress-config`;
- `egress-routing`;
- `egress-core`;
- `egress-uri`;
- `egress-system-proxy`;
- `egress-cli`;
- `egress-runtime`.

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

- service lifecycle/outbound service facade → `egress-embed`;
- pproxy parsing/translation/diagnostics → `egress-pproxy-compat`;
- system proxy application → `egress-system-proxy`;
- direct lower-level crates only where Python intentionally exposes that lower-level concept.

## Workstream 2 — Remove reach-through only when an existing owner can absorb it

If `egress-python` uses `egress-cli` only to access reusable non-CLI logic, move/reuse that logic from its natural existing owner rather than making the bindings depend on CLI presentation code.

Likewise, eliminate direct `egress-config`, `egress-routing`, `egress-core`, `egress-uri`, or `egress-runtime` edges only when:

1. the binding does not intentionally expose that crate's type/concept;
2. an existing owner facade already provides the required behavior;
3. moving the logic does not add a new public API.

Do not create an `egress-python-support` crate.

Do not funnel everything through `egress-embed` if that would make embed depend on CLI-only or Python-only concerns.

After each dependency removal, use `cargo tree -p eggress-python -e features` to verify no heavier replacement path was introduced.

## Workstream 3 — Repair native extension stubs

Compare `crates/egress-python/src/lib.rs` module registration and each `#[pymethods]` block against `python/egress/_egress.pyi`.

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

- `python/egress/__init__.py`;
- `python/egress/__init__.pyi`;
- `python/egress/exceptions.py`;
- `python/egress/exceptions.pyi`;
- other module `.py/.pyi` pairs.

Add a lightweight test that detects missing maintained exports without trying to assert every private helper.

The test should explicitly tolerate conditional native-import fallback behavior while validating a built wheel/native test environment.

Preserve every existing public name even if it is awkward or redundant.

## Workstream 5 — Capability metadata consistency

`egress.capabilities()` currently returns a stable dictionary with hard-coded lists.

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
- `egress-pproxy-compat` remains the only package owning top-level `pproxy`;
- no `sys.modules` aliasing is introduced;
- source tree does not shadow the built extension under pytest importlib mode.

## Verification

```bash
cargo check -p eggress-python --locked
cargo tree -p eggress-python -e features

(cd crates/egress-python && ../../.venv/bin/maturin develop)
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
2. make `egress-embed` own CLI presentation or Python-specific behavior;
3. increase the dependency graph materially through a heavier facade;
4. change a Python return type, exception, or capability value.

Retain the justified dependency and document why it is architectural rather than accidental.

## Acceptance criteria

- [ ] Every direct internal `egress-python` dependency has a documented owner/use justification.
- [ ] Unnecessary reach-through edges are removed without adding a new crate.
- [ ] Removed dependencies do not return indirectly through a heavier facade without justification.
- [ ] `_egress.pyi` matches registered native classes/functions/methods for the maintained surface.
- [ ] Public `.py/.pyi` pairs and `__all__` agree.
- [ ] Existing Python public names and import locations are preserved.
- [ ] `capabilities()` retains its public contract and is protected against metadata drift.
- [ ] Wheel/import smoke and full Python suite are green.
- [ ] Current Python architecture documentation no longer describes obsolete gaps as current.
