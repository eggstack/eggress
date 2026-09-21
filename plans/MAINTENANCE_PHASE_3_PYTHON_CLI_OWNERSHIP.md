# Maintenance Phase 3 — Python / CLI Ownership Resolution

## Status

**PLANNED — 2026-09-21**

## Parent

[`MAINTENANCE_CONVERGENCE_ROADMAP.md`](MAINTENANCE_CONVERGENCE_ROADMAP.md)

## Baseline

`fa1d9bf73c5c162c62a6086cf0e7361f1cfb3131`

## Objective

Reassess the remaining `eggress-python -> eggress-cli` dependency and remove it only if the existing stable owner APIs can reproduce the current Python-visible behavior exactly, without introducing duplicated operational logic or adding a new public Rust support API.

A documented retain decision is a valid implementation outcome if the edge cannot be eliminated within these constraints.

## Current state

`crates/eggress-python/src/compat.rs::run_pproxy_test()` currently uses:

- `eggress_cli::parse_pproxy_test_target()`;
- `eggress_cli::run_upstream_test()`.

Those CLI-library functions are also used by the native `eggress upstream test` / compatibility `pproxy --test` flows. The Python binding therefore shares behavior with the CLI rather than maintaining a second tester, but the dependency direction makes a binding facade depend on a presentation-oriented crate.

The previous API-boundary campaign correctly retained this edge because there was no proven existing owner boundary that could absorb it without API expansion.

## Governing constraints

1. Preserve every existing Python function name, argument, return value, exception family, output side effect, timeout, and pproxy compatibility behavior.
2. Preserve the existing public Rust `eggress_cli::parse_pproxy_test_target()`, `run_upstream_test()`, and related CLI paths. This campaign must not remove or move them.
3. Do not add a public Rust helper solely for `eggress-python`.
4. Do not create a new support crate.
5. Do not copy the full upstream tester into `eggress-python`.
6. Do not replace a chain-aware proxy test with a raw endpoint TCP-connect test.
7. Do not change `test_upstream_connect()`; it intentionally probes endpoint reachability and has different semantics.
8. Do not change pproxy parser/gate behavior.
9. Do not change stdout/stderr behavior if the Python helper currently exposes it as an observable side effect.
10. Do not make `eggress-embed` own CLI presentation behavior.
11. If no clean existing owner API can preserve the contract, retain the dependency and document the reason.

## Workstream 1 — Freeze the current Python-visible contract

Before changing dependencies, add or identify focused tests for `run_pproxy_test()` covering:

- default/no-arg parser behavior;
- unknown/invalid pproxy argument failure;
- unsupported execution-gate failure;
- no-upstream behavior;
- a reachable direct/simple proxy-chain case using only local fixtures;
- unreachable/failed upstream exit result;
- URL-shaped target parsing, including default HTTP/HTTPS ports;
- IPv4/IPv6 target handling where existing tests support it;
- any captured stdout/stderr contract that current callers rely on.

The purpose is not to invent a richer API. It is to make the exact behavior movable.

## Workstream 2 — Determine whether existing owner APIs are sufficient

Evaluate existing already-published APIs in this order:

1. `eggress-outbound::OutboundConnector` / `eggress_embed::outbound::OutboundConnector`;
2. existing `eggress-core` chain executor interfaces already consumed by the binding;
3. existing config/runtime structures already directly exposed by the binding.

A replacement is acceptable only if it can:

- test the same configured upstream chain, not merely the first endpoint;
- honor the same target and timeout semantics;
- preserve exit-code/result semantics;
- preserve failure/redaction behavior;
- avoid constructing a listener;
- avoid a TOML or CLI argv round-trip not already required by the compatibility contract.

Do not add methods to any of these crates for this phase.

## Workstream 3 — If exact reuse is possible, invert the dependency

If an existing stable owner API can reproduce the contract:

1. implement the Python helper using that existing API;
2. remove `eggress-cli` from `crates/eggress-python/Cargo.toml`;
3. keep the CLI's existing public helper functions and have the CLI continue using its current implementation;
4. do not copy formatting/probing logic into both places unless the duplicated portion is trivial presentation formatting and has no protocol behavior;
5. confirm `cargo tree -p eggress-python -e features` no longer includes `eggress-cli` through another path.

If removing the edge causes materially more protocol/testing code to live in the binding crate, reject the change.

## Workstream 4 — If exact reuse is not possible, retain deliberately

If the existing owner APIs are insufficient, record a concise architecture note in `architecture/python-bindings.md` explaining:

- the exact two CLI-library helpers consumed;
- why they are operational library functions rather than command parsing/presentation;
- why copying them would create a second chain tester;
- why adding a new public owner API is outside the no-API-change constraint;
- the conditions under which the edge could be removed in a future major/internal-boundary campaign.

Do not create another implementation merely to satisfy a dependency-graph aesthetic goal.

## Workstream 5 — Keep CLI and Python behavior synchronized

Regardless of retain/remove outcome, add a low-maintenance regression proving the CLI-compatible Python test path and the CLI helper agree on representative local cases.

Prefer shared fixture inputs and result/exit semantics, not subprocess output snapshots of every line.

Do not require external network access.

## Verification

Rust/binding:

```bash
cargo check -p eggress-python --locked
cargo tree -p eggress-python -e features
cargo test -p eggress-cli --locked
cargo test -p eggress-python --locked
```

Python:

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest   python/tests/test_pproxy_compat.py   python/tests/test_pproxy_phase6_process.py   python/tests/test_api_boundary_closure.py   python/tests/test_wheel_import_smoke.py -q
```

Run the full Python/compat suite before closure:

```bash
.venv/bin/python -m pytest python/tests tests/compat -q
```

Final gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Decision record required at closure

The closure record must explicitly state one of:

### Outcome A — dependency removed

- existing owner API used;
- exact behavior tests proving equivalence;
- `eggress-cli` absent from direct/transitive binding ownership where expected;
- no new public API or duplicated chain tester.

### Outcome B — dependency retained

- exact missing existing owner capability;
- why removing the edge would require duplication or public API expansion;
- evidence that the retained functions are typed operational helpers rather than arbitrary CLI argv/presentation reach-through.

Do not mark the phase implemented without one of these explicit outcomes.

## Acceptance criteria

- [ ] The current `run_pproxy_test()` contract is covered by focused local regression tests.
- [ ] The existing owner surfaces are evaluated before any new implementation is written.
- [ ] No public Rust API is added, moved, removed, or signature-changed.
- [ ] No second chain-aware upstream tester is introduced.
- [ ] `test_upstream_connect()` remains a distinct raw endpoint probe.
- [ ] If the dependency is removed, Python behavior remains equivalent and `eggress-cli` is removed cleanly from binding ownership.
- [ ] If the dependency is retained, the architectural justification and future removal condition are documented.
- [ ] CLI and Python representative test semantics remain synchronized.
- [ ] Python public names, stubs, exceptions, and wheel behavior are unchanged.
- [ ] Focused and full Python tests pass.
- [ ] Workspace fmt, clippy, and locked tests pass.

## Closure record

Fill in place with Outcome A or Outcome B, implementation/evidence commit, dependency tree evidence, focused test names, and full-suite result.
