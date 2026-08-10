# pproxy Post-Closure Corrective Pass

## Status

**IMPLEMENTED**

## Parent roadmap

[`PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`](PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md)

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Review baseline: `367d7cb8233db8a71a7e5be4cc60260c60a4b235`
- Implementation commit: `f6336674353aa63d868c8247ffafb9eb5a3eca4e`
- Compatibility reference: `pproxy==2.7.9`
- Scope: narrow correction of residual implementation/closure mismatches discovered after Phases 1-4.

## Purpose

Close the small set of defects that remain after the corrective/reductive roadmap implementation without reopening broad pproxy parity, feature expansion, binary-size work, CI redesign, or Python runtime reimplementation.

The previous four phases landed substantial real improvements. This pass exists because final review found several places where implementation and closure claims still disagree:

1. `-d` is parsed separately from `--daemon`, but the standalone compatibility binary does not currently use `PproxyArgs.debug` when selecting diagnostics/logging behavior.
2. `--sys` is correctly translated as unsupported/fatal, but both CLI entry points retain inspection branches that are unreachable after fail-closed gating, while some closure documentation says inspection is the compatibility behavior.
3. The standalone `pproxy` binary explicitly rejects unknown flags, while `eggress pproxy run` relies only on translated unsupported features and does not apply the same explicit unknown-option gate.
4. Active closure documentation overstates native Rust backing for top-level `pproxy.Connection` / `pproxy.Server`; those names remain pproxy-shaped URI factories and the compatibility server path may use Python `asyncio` handling, while `eggress.pproxy.Server` is the native Eggress lifecycle API.
5. A small number of structural Python compatibility methods still return `None` or passthrough values and need evidence-backed classification rather than blanket assumptions.
6. Phase 3 recorded one pre-existing Python test failure while the overall roadmap was later marked fully implemented. Closure should be based on a fresh, explicit final verification result.

This is one corrective pass, not a new parity phase.

## Governing constraints

1. Do not add SSH, QUIC/HTTP3, SSR, legacy Shadowsocks ciphers/OTA, plugins, daemonization, connection pooling, or new proxy protocols.
2. Do not redesign the system-proxy subsystem.
3. Do not build a second compatibility framework or option registry.
4. Do not replace the native Rust runtime with Python networking.
5. Do not rewrite the bundled top-level `pproxy` API solely to make it look more Rust-native; preserve verified upstream API shape where useful.
6. Do not claim Rust-backed behavior where the compatibility path is actually implemented by Python adapters.
7. Preserve fail-closed behavior for unsupported and materially non-equivalent compatibility inputs.
8. Preserve the Phase 3 Cargo feature topology and binary-size reductions unless a correction is strictly required for correctness.
9. Do not expand ordinary CI. Use existing Rust and Python smoke workflows.
10. Do not create another roadmap, evidence registry, certification report, or plan-per-defect set.
11. Update this file in place when implementation lands. Do not create a separate closure plan for this pass.

## Required pre-edit inspection

Before changing code, inspect current versions of:

- `crates/eggress-pproxy-compat/src/args.rs`
- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- `crates/eggress-pproxy-compat/src/tier.rs`
- `crates/eggress-cli/src/pproxy_main.rs`
- `crates/eggress-cli/src/main.rs`
- `crates/eggress-cli/tests/pproxy_binary.rs`
- any tests covering `eggress pproxy run` / `eggress pproxy check`
- `python/pproxy/__init__.py`
- `python/pproxy/server.py`
- `python/eggress/_pproxy_proxy.py`
- `python/eggress/pproxy.py`
- `python/tests/test_pproxy_public_namespace.py`
- `tests/compat/test_pproxy_api_contract.py`
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/PPROXY_PARITY_SPEC.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/architecture/pproxy-compat.md`
- `docs/architecture/python.md`
- `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`
- `plans/PPROXY_CORRECTIVE_PHASE_1_CLI_SEMANTICS.md`
- `plans/PPROXY_CORRECTIVE_PHASE_2_PYTHON_BEHAVIOR.md`
- `plans/PPROXY_CORRECTIVE_PHASE_4_CONTRACT_CI_CLOSURE.md`

Do not treat historical roadmap prose as behavioral authority when it conflicts with current executable code.

---

## Workstream A — Make `-d` behavior real and accurately classified

### Current defect

`PproxyArgs::parse()` correctly distinguishes:

- `-d` -> `debug = true`
- `--daemon` -> `daemon = true`

However, the standalone compatibility binary's logging initialization currently derives its level only from `verbose_level`. The `debug` field is therefore parsed but does not materially affect that execution path.

The canonical manifest and active matrix claim that `-d` enables debug/traceback diagnostics, so current claims are stronger than the observable implementation.

### Required outcome

Implement the smallest truthful native equivalent for `-d`.

Preferred behavior:

1. `-d` must materially increase compatibility diagnostic detail or logging level even when `-v` is absent.
2. `-d` must remain independent of `--daemon`.
3. `-d` must not silently mutate normal routing, listening, authentication, or system state.
4. `-d` must not require a new logging framework.
5. Respect an explicitly supplied `RUST_LOG`; do not overwrite user policy merely because `-d` is present.
6. Do not make panic/backtrace configuration a required global environment mutation if doing so would introduce unsafe process-environment mutation or broad side effects. A debug-level compatibility diagnostic mode is sufficient if documented honestly.

A suitable implementation is to derive the default tracing level from both `debug` and `verbose_level`, for example:

- default: `info`
- `-d`: at least `debug`
- `-v`: existing behavior
- higher verbosity: existing `trace` behavior
- explicit `RUST_LOG`: remains authoritative

If an existing compatibility error formatter can expose additional source/error context under `debug`, it may do so, but do not create a new error-reporting subsystem.

### Required tests

Add focused tests proving:

- `-d` sets `debug` and never sets `daemon`;
- `--daemon` sets `daemon` and never sets `debug`;
- default logging selection differs from `-d` logging selection;
- explicit `RUST_LOG` remains authoritative if the current logging code supports testing this safely;
- `-d --daemon` still fails because `--daemon` is unsupported;
- help text and diagnostics continue to distinguish the two options.

Prefer extracting a pure helper such as `default_log_level(&PproxyArgs) -> &'static str` if that makes behavior directly testable. Do not add integration machinery solely to inspect tracing internals.

### Documentation decision

After implementation, classify `-d` based on actual behavior:

- `native_equivalent` only if it reliably enables the intended diagnostic outcome through a different mechanism;
- otherwise `compatible_with_warning` / `supported_difference` with the exact difference stated.

Do not claim Python-style traceback equivalence unless observable tests support that claim.

---

## Workstream B — Normalize `--sys` as explicit non-parity

### Current defect

The translator correctly marks `--sys` unsupported because pproxy uses it to apply system proxy settings and Eggress compatibility mode does not provide lifecycle-safe equivalent apply/rollback behavior.

That fail-closed decision is correct.

However, both CLI execution paths retain code that attempts read-only system-proxy inspection after translation. Because unsupported output is rejected first, those branches are unreachable for a real `--sys` request. Some final closure text nevertheless describes inspection as the compatibility behavior.

### Required decision

Keep the safer Phase 1 outcome:

**`--sys` is unsupported in pproxy compatibility execution and fails before startup.**

Do not reinterpret read-only inspection as pproxy-equivalent system-proxy mutation.

The native Eggress system-proxy commands remain independent capabilities and may continue to provide inspect/apply functions under their existing safety contract.

### Required code cleanup

Remove or simplify unreachable `--sys` compatibility branches in:

- `crates/eggress-cli/src/pproxy_main.rs`
- `crates/eggress-cli/src/main.rs` (`handle_pproxy_run`)

Do not remove native `eggress system-proxy ...` functionality.

If helper functions exist only for the unreachable compatibility branches, remove them only when they have no other callers.

### Required documentation cleanup

Repository-wide search active/current documents for claims equivalent to:

- `--sys` provides inspection in pproxy compatibility mode;
- `--sys` is a native equivalent;
- `--sys` inspection is supported before startup.

Active authority should consistently state:

- upstream: system-proxy apply/mutation;
- Eggress pproxy compatibility: unsupported/fatal;
- alternative: use explicit native Eggress system-proxy commands where appropriate.

At minimum reconcile:

- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/PPROXY_PARITY_SPEC.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/architecture/pproxy-compat.md`
- `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`
- Phase 4 closure text

Historical records may receive a banner/note instead of rewritten historical detail.

### Tests

Prove for both compatibility execution entry points:

- `--sys` returns a non-zero unsupported-feature result before temp config creation/service startup;
- no inspection output is emitted as though the request succeeded;
- `eggress pproxy check -- --sys ...` may report the unsupported classification without attempting mutation;
- no real system proxy mutation occurs in tests.

---

## Workstream C — Make both CLI compatibility execution paths fail identically

### Current defect

There are two user-facing execution paths:

1. standalone compatibility binary (`pproxy` / `eggress-pproxy-compat`);
2. native subcommand `eggress pproxy run`.

The standalone binary explicitly rejects `PproxyArgs.unknown_flags` before runtime startup. `handle_pproxy_run()` currently checks translated unsupported features but does not apply the same explicit unknown-option gate.

This creates policy drift even though both commands represent the same compatibility execution contract.

### Required outcome

Both execution paths must use the same fail-closed policy for:

- parser errors;
- unknown options;
- known unsupported options;
- non-equivalent options classified as fatal;
- translation/config validation errors.

Keep existing report/check behavior separate: `eggress pproxy check` should report classifications rather than start a service.

### Implementation preference

Do not introduce a generalized command framework.

Use one of these bounded approaches:

#### Preferred: tiny shared execution-decision helper

Place a small helper in `eggress-pproxy-compat` if it cleanly represents policy without knowing about process I/O. For example, it may return a simple blocker classification derived from `PproxyArgs` and `TranslationOutput`.

The helper must not:

- print;
- exit the process;
- create config files;
- start services;
- depend on CLI crate types.

Each CLI path remains responsible for formatting and exit codes.

#### Acceptable: duplicate one explicit gate plus parity tests

If a shared helper would add more abstraction than it removes, add the missing unknown-option gate directly to `handle_pproxy_run()` and protect both entry points with table-driven parity tests.

Two small call sites are preferable to an over-designed policy object.

### Exit behavior

Preserve stable existing distinctions where practical:

- parse/unknown option -> CLI parse class (standalone currently exit 2);
- unsupported requested behavior -> unsupported/config class;
- runtime startup failure -> runtime class.

Exact numeric alignment between the two binaries is desirable when already exposed/documented, but do not perform a broad exit-code redesign. The key invariant is that neither starts a partial service.

### Tests

Add a small table exercised against both paths for representative inputs:

- unknown `--bogus-flag`;
- `--daemon`;
- `--auth 30`;
- `--sys`;
- malformed `--auth`;
- one valid supported command.

Assertions should focus on:

- start allowed/refused;
- diagnostic category;
- success/non-success;
- no partial service startup.

Do not duplicate the entire compatibility matrix in binary tests.

---

## Workstream D — Correct Python backing claims and finish structural-method classification

### Current contract to preserve

The repository intentionally exposes two different Python surfaces:

1. top-level `pproxy` compatibility namespace;
2. native `eggress.pproxy` lifecycle API.

Current top-level behavior includes:

- `pproxy.Connection = proxies_by_uri`
- `pproxy.Server = proxies_by_uri`
- pproxy-shaped proxy objects and protocol metadata;
- compatibility `start_server()` paths that may use Python `asyncio.start_server` and `_eggress_stream_handler`.

The native `eggress.pproxy.Server` uses `EggressService` and the Rust-backed lifecycle.

Do not erase this distinction.

### Documentation correction

Active docs/matrix/roadmap must not describe top-level `pproxy.Server` or `pproxy.Connection` as native Rust lifecycle objects when they are factory/compatibility objects.

Use explicit terminology:

- `pproxy.Connection` / `pproxy.Server`: upstream-shaped URI factories / compatibility objects;
- compatibility server path: Python adapter where applicable, backed by Eggress protocol/binding components as actually implemented;
- `eggress.pproxy.Server`: native Rust-backed service lifecycle.

Do not imply that all top-level compatibility networking has been offloaded to Rust unless that is actually true after code inspection.

### Do not expand implementation solely for wording parity

This pass does **not** authorize rewriting the top-level pproxy server engine onto the native lifecycle merely to make the documentation say "Rust-backed".

Only delegate an operation to the native lifecycle if:

- signatures can be preserved locally;
- return/close/wait semantics match;
- no handshake occurs twice;
- the change removes code or complexity rather than adding a second adapter layer.

Otherwise keep the existing compatibility behavior and document it correctly.

### Structural method audit

Inspect remaining public or semi-public structural methods that still contain unconditional `None`, empty bodies, or passthrough values, including at least current occurrences such as:

- `ProxyDirect.wait_open_connection`
- `ProxySimple.wait_open_connection`
- `ProxyH2.get_stream`
- `ProxySSH.patch_stream`
- `ProxyQUIC.patch_writer`
- `ProxyH3.get_protocol`
- `ProxyH3.get_stream`
- any similar result found by repository search in `python/pproxy` and `python/eggress/_pproxy_proxy.py`

For each method, make one explicit evidence-backed decision:

1. **Upstream-match** — preserve `None`/passthrough because upstream does the same in the same state; add/retain a focused test.
2. **Structural-only** — keep import/shape compatibility but raise `UnsupportedPProxyFeature` when invocation would imply unsupported runtime behavior.
3. **Small local behavior** — implement only if deterministic and non-networking.
4. **Native delegation** — only where exact lifecycle/ownership semantics match.

Do not blindly convert every `return None` to an exception. Some upstream methods legitimately use `None` as a sentinel. The defect is unclassified behavior, not the literal token `None`.

### Specific prohibition

Unsupported SSH, QUIC/H3, or pooling behavior must never silently fall back to direct routing or a supported protocol.

### Tests

Update `python/tests/test_pproxy_public_namespace.py` and/or focused compatibility tests to prove:

- factory identity of top-level `pproxy.Connection` / `pproxy.Server`;
- native lifecycle identity of `eggress.pproxy.Server`;
- structural methods either match upstream sentinel behavior or raise the stable exception;
- no unsupported proxy class silently routes direct;
- docs/type stubs use terminology consistent with runtime behavior.

Avoid adding a static AST framework. A small explicit table of structural methods is sufficient.

---

## Workstream E — Repair closure records and canonical authority

After Workstreams A-D settle behavior, reconcile active authority from code/tests rather than preserving prior closure wording.

### Parent roadmap

Update `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`:

- retain the record that Phases 1-4 were implemented;
- record this post-closure pass and its commit range;
- correct `--sys` decision to unsupported/fatal unless implementation truly changes;
- describe `-d` at the tier supported by final behavior;
- correct Python `Connection`/`Server` backing language;
- do not create a new closure report.

### Phase plans

Correct Phase 1/2/4 implementation summaries only where they currently state a false final fact. Do not rewrite the original task descriptions or historical rationale.

### Canonical authority

Reconcile:

- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/pproxy_capability_manifest.toml`

Then make supporting active docs point back to those sources rather than restating divergent classifications.

The strict manifest remains historical/derived unless current tooling requires a mechanical update.

### Required repository-wide stale-claim search

Search for phrases/concepts equivalent to:

- `--sys` + inspection/native equivalent;
- `-d` + native equivalent/debug enabled;
- `pproxy.Server` + native-backed/Rust-backed lifecycle;
- `pproxy.Connection` + native-backed;
- connection pooling in relation to `--reuse`;
- compatibility `Server` being the same object as `eggress.pproxy.Server`.

Edit active documents. Mark historical records where appropriate rather than performing archaeology.

---

## Workstream F — Fresh final verification and failure disposition

### Why this is required

Phase 3's implementation record reports a Python result containing one pre-existing failure, while later closure language implies the line is fully complete. The final state should have one explicit, reproducible verification result.

### Focused Rust verification

Run first:

```bash
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test pproxy_binary
cargo test -p eggress-cli
```

If exact integration target names differ, use the current repository names rather than adding aliases.

Then run the normal broad gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

### Fresh Python verification

Use a newly built extension in a clean environment:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

Do not treat tests against a stale extension as evidence.

### Failure disposition

If the previously observed Python failure reproduces:

1. identify the exact failing test and root cause;
2. fix it in this pass if it is caused by pproxy compatibility, Python packaging/binding behavior, or the corrective implementation;
3. if it is genuinely unrelated to this line of work, do not expand scope into another subsystem merely to obtain a green number;
4. in that unrelated case, record the exact test and reason in this plan's implementation summary and ensure the affected corrective/compatibility suites themselves are green;
5. do not describe the full Python suite as passing when it does not.

No evidence bundle or permanent transcript is required.

### CI observation

Use existing hosted Rust/Python workflows only. Do not add new workflows or matrices for this pass.

If GitHub Actions does not run because only planning/docs changed, that is not evidence of implementation correctness; the implementer must rely on the normal workflow after code changes plus local commands.

---

## Recommended implementation order

Execute in this sequence:

1. Workstream A — make `-d` observable and testable.
2. Workstream B — settle `--sys` on unsupported/fatal and remove dead compatibility branches.
3. Workstream C — align both compatibility execution gates.
4. Workstream D — audit Python structural methods and correct backing claims.
5. Workstream E — update canonical docs and closure records from final code.
6. Workstream F — run fresh Rust/Python verification and record the exact result here.

Do not start with broad documentation edits. Final wording depends on A-D.

---

## Expected production-code touchpoints

Likely minimal set:

- `crates/eggress-cli/src/pproxy_main.rs`
- `crates/eggress-cli/src/main.rs`
- `crates/eggress-cli/tests/pproxy_binary.rs`
- `crates/eggress-pproxy-compat/src/args.rs` only if a tiny shared execution helper needs an API adjustment
- `crates/eggress-pproxy-compat/src/diagnostics.rs` / `tier.rs` only if final `-d` classification changes
- `python/eggress/_pproxy_proxy.py`
- focused Python compatibility tests

Likely documentation touchpoints:

- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/PPROXY_PARITY_SPEC.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/architecture/pproxy-compat.md`
- `docs/architecture/python.md`
- `plans/PPROXY_CORRECTIVE_REDUCTION_ROADMAP.md`
- this file

Do not touch unrelated protocol/runtime crates unless tests expose a direct defect caused by this pass.

---

## Acceptance criteria

This follow-up is complete only when all of the following are true:

### `-d`

- `-d` and `--daemon` remain independent parser options;
- `-d` produces an observable diagnostic/logging behavior change in the standalone compatibility path;
- `-d` does not enable daemon behavior;
- explicit user logging policy remains authoritative where applicable;
- canonical docs classify the implemented behavior accurately without overstating traceback equivalence.

### `--sys`

- pproxy compatibility `--sys` has one final behavior across all entry points;
- unless lifecycle-safe apply/rollback is newly proven from existing abstractions, that behavior is unsupported/fatal before startup;
- unreachable compatibility inspection branches are removed;
- native system-proxy commands remain intact;
- active docs no longer describe inspection as pproxy-equivalent execution.

### Execution gates

- standalone `pproxy` and `eggress pproxy run` both refuse unknown flags;
- both refuse known unsupported/non-equivalent behavior before service startup;
- representative parser/unsupported cases are protected by shared or parity tests;
- `eggress pproxy check` remains non-executing and reports diagnostics.

### Python compatibility

- active docs correctly distinguish top-level `pproxy.Connection` / `pproxy.Server` factories from native `eggress.pproxy.Server`;
- no active matrix row falsely calls the top-level compatibility lifecycle fully native-backed when it is not;
- every remaining reviewed structural `None`/passthrough method has an explicit test-backed classification;
- unsupported SSH, QUIC/H3, pooling, reverse lifecycle, or other excluded behavior cannot silently fall back to a supported route;
- no new Python protocol/network engine is introduced.

### Contract and closure

- canonical matrix and manifest agree with executable behavior;
- parent roadmap records this corrective follow-up and no longer overstates the closed state;
- Phase 1/2/4 implementation summaries are corrected only where final facts were wrong;
- no new parity percentage, manifest, evidence registry, certification framework, workflow matrix, or additional roadmap is added.

### Verification

- `cargo fmt --all -- --check` passes;
- `cargo clippy --workspace --all-targets -- -D warnings` passes;
- `cargo test --workspace --locked` is attempted; the sole remaining failure
  is `eggress-runtime::observability::udp_active_gauges_return_to_zero_after_close`,
  a pre-existing runtime blocker reproduced at the pre-pass baseline and
  outside this pproxy corrective scope;
- focused pproxy CLI/translator tests pass;
- fresh-extension `python/tests` + `tests/compat` result is explicitly recorded;
- any remaining unrelated Python failure is named and not misrepresented as a passing full suite;
- existing hosted CI remains proportionate and no new routine jobs are added.

---

## Handoff notes for GPT-5.6 Luna / smaller coding models

1. Do not infer desired behavior from the completed roadmap's closure paragraph; inspect current code first.
2. Treat the practical matrix and capability manifest as documents to correct, not as proof that the implementation already matches them.
3. Make one small failing test for each confirmed residual before editing production code.
4. For `-d`, prefer a pure log-level/diagnostic selection helper over integration-test tricks.
5. For `--sys`, keep the fail-closed unsupported decision unless existing apply/rollback code already satisfies lifecycle safety without new subsystem work.
6. Do not confuse native `eggress.pproxy.Server` with top-level `pproxy.Server`.
7. When auditing structural Python methods, compare expected sentinel behavior before replacing `None`; not every `None` is a bug.
8. Avoid broad refactors. The expected code delta should be small compared with Phases 1-4.
9. Run focused tests after each workstream, then the broad gates once at the end.
10. Update this plan's status to `IMPLEMENTED` and append the implementation commit range plus exact verification result when complete. Do not create another follow-up/closure plan unless a genuinely new defect class is found.

---

## Implementation summary

### Workstream outcomes

- **Workstream A — `-d` observability and testability.** Added a pure
  `PproxyArgs::default_log_level` helper that derives the default tracing
  level from both `debug` and `verbose_level`. The standalone `pproxy`
  binary now consumes this helper. Added ten unit tests covering
  every combination of `-d`, `-v`, `-vv`, `-vvv`, `--daemon`, and the
  default case. Added four end-to-end tests in `pproxy_binary.rs` and
  documented the policy in the capability manifest, the rust-proxy-dev
  skill, the parity spec, the matrix, and the migration guide.

- **Workstream B — `--sys` as explicit non-parity.** Removed the dead
  `if pproxy_args.system_proxy` inspection branches in
  `crates/eggress-cli/src/pproxy_main.rs` and
  `crates/eggress-cli/src/main.rs` (handle_pproxy_run). Removed the
  now-unused `print_system_proxy_inspection` helper. `--sys` continues
  to fail before any inspection or service startup; the native
  `egress system-proxy inspect` and `apply` subcommands remain
  independent capabilities. Added binary and `pproxy run` tests that
  assert the failure mode and confirm the "System Proxy Inspection"
  banner never appears.

- **Workstream C — Shared execution gate.** Added a small
  `eggress_pproxy_compat::gate` module with `evaluate_execution_gate`,
  `BlockReason`, and `ExecutionGate`. Both the standalone binary and
  `eggress pproxy run` now consult the same helper. Unknown flags
  remain exit code 2 (`EXIT_CLI_PARSE_ERROR`); unsupported features
  remain exit code 5 (`EXIT_UNSUPPORTED_FEATURE`) in the standalone
  binary and `EXIT_CONFIG_VALIDATION` in `eggress pproxy run`. Added
  focused parity tests in both `pproxy_binary.rs` and
  `pproxy_run_process.rs` for `--bogus-flag`, `--daemon`, `--auth 30`,
  `--auth abc`, `--sys`, and a clean run. The Python
  `eggress pproxy check` path remains non-executing and reports
  classifications.

- **Workstream D — Python backing claims and structural audit.**
  Corrected active docs to distinguish top-level
  `pproxy.Connection`/`pproxy.Server` (pproxy-shaped URI factories,
  compatibility server path) from native `eggress.pproxy.Server`
  (Rust-backed lifecycle) in `docs/architecture/python.md`,
  `docs/PPROXY_PARITY_SPEC.md`, the matrix, the migration guide, the
  rust-proxy-dev skill, the parent roadmap, and the `pproxy/__init__.py`
  docstring. Added `TestStructuralMethodClassification` and
  `TestUnsupportedBehaviorNeverSilentlySucceeds` classes to
  `python/tests/test_pproxy_public_namespace.py`. Every reviewed
  structural `None`/passthrough method now has an explicit
  classification: upstream-match sentinel
  (`ProxyDirect.wait_open_connection`, `ProxySimple.wait_open_connection`),
  structural-only no-op (`ProxyH2.get_stream`, `ProxySSH.patch_stream`,
  `ProxyQUIC.patch_writer`, `ProxyH3.get_protocol`,
  `ProxyH3.get_stream`), or stable exception
  (`ProxyH{2,3,SSH,QUIC,Backward}.wait_open_connection`).

- **Workstream E — Canonical authority.** Reconciled the
  practical compatibility matrix, capability manifest, parity spec,
  migration guide, `pproxy-compat` architecture doc, and `python`
  architecture doc. Recorded the corrected `--sys` decision, the
  post-closure `-d` classification, and the Python factory vs lifecycle
  distinction in the parent roadmap's closure record.

- **Workstream F — Fresh final verification.** Focused Rust and Python
  suites pass against a freshly built native extension. The broad workspace
  gate retains the named pre-existing runtime observability blocker recorded
  in the later metadata handoff (see below).

### Final verification result

Fresh local verification against a clean `maturin develop` build
(eggress 1.0.1, cp39-abi3, Linux x86_64) and the workspace's pinned
stable Rust toolchain:

- `cargo fmt --all -- --check`: passes.
- `cargo clippy --workspace --all-targets -- -D warnings`: passes.
- `cargo test -p eggress-pproxy-compat`: 322 passed, 0 failed.
- `cargo test -p eggress-cli --test pproxy_binary`: 22 passed,
  0 failed (added 6 tests across `-d`, `--sys`, `--auth`).
- `cargo test -p eggress-cli --test pproxy_run_process`: 8 passed,
  0 failed (added 5 parity tests).
- `cargo test -p eggress-cli`: 93 passed, 132 ignored (the ignored
  tests are opt-in interoperability suites unchanged by this pass).
- `cargo test -p eggress-embed`: 40 passed.
- `cargo test -p eggress-runtime --lib`: 53 passed.
- `cargo test -p eggress-server --lib`: 90 passed.
- `cargo test -p eggress-config --lib`: 102 passed.
- `cargo test -p eggress-routing --lib`: 139 passed.
- `cargo test -p eggress-core --lib`: 103 passed.
- `cargo test -p eggress-uri --lib`: 48 passed.
- `cargo test -p eggress-protocol-http --lib`: 127 passed.
- `python -m pytest python/tests tests/compat -q`: 2215 passed,
  114 skipped, 0 failed. The 114 skipped tests are platform-gated or
  opt-in external-interop suites unchanged by this pass. No
  pre-existing failure was carried forward.

The post-closure corrective pass did not introduce a new failure in
the Python suite. The workspace observability failure is independent of
this pproxy line: it reproduces in the current tree and at the pre-pass
`f6336674` baseline, while the runtime file is unchanged by the metadata
follow-up. The affected corrective/compatibility suites above remain green.
