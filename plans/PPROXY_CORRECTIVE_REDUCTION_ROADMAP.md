# pproxy Compatibility Corrective and Reduction Roadmap

## Status

**IMPLEMENTED — FINAL CONTRACT/REPORTING CLOSURE PASS COMPLETE**

All four original phase plans are implemented. The post-closure corrective
pass that closed the residual `-d`, `--sys`, execution-gate, Python
claim/classification, and verification mismatches is also implemented.

The final contract/reporting closure pass is implemented in `bd10467`. The
follow-up contract metadata reconciliation is implemented in
`cef6851cc7275aca3cb8f9e27cc3dc8b4f7abff6` under Outcome 1. Its focused and
changed-surface gates and full workspace gate pass. The focused GET fail-closed
evidence and closure records are in `cf448e7`; the bounded test-only
observability synchronization correction is in `eeaa411`.

Implementation commit range: see the post-closure entry in the closure
record below. Do not reopen the completed phase scope beyond that
narrow follow-up (no new scope was introduced).

Final decisions recorded by the post-closure corrective pass:

- `-d` is wired to a debug-level default tracing filter via the shared
  `default_log_level` helper. Independent of `-v` and `--daemon`. An
  explicit `RUST_LOG` environment variable remains authoritative.
- `--sys` is unsupported and fatal before startup in pproxy compatibility
  mode. The native `eggress system-proxy inspect` and `apply` subcommands
  remain independent capabilities under their own safety contract.
- The standalone `pproxy` binary and `eggress pproxy run` apply the same
  fail-closed policy through the shared `eggress_pproxy_compat::evaluate_execution_gate`
  helper. Unknown, unsupported, and unsupported-by-architecture flags
  cannot start a partial service from either entry point.
- Top-level `pproxy.Connection` / `pproxy.Server` are pproxy-shaped URI
  factories (aliases for `proxies_by_uri`). They are not the native
  `eggress.pproxy.Server` lifecycle class. The compatibility server path
  is a Python adapter using `asyncio.start_server` and
  `_eggress_stream_handler`.
- The remaining structural Python methods (`ProxyDirect.wait_open_connection`,
  `ProxySimple.wait_open_connection`, `ProxyH2.get_stream`, `ProxySSH.patch_stream`,
  `ProxyQUIC.patch_writer`, `ProxyH3.get_protocol`, `ProxyH3.get_stream`) have
  explicit test-backed classifications in `python/tests/test_pproxy_public_namespace.py`.
  Sentinel `None` returns are preserved where upstream uses the same
  sentinel; unsupported pooling/lifecycle hooks raise `UnsupportedPProxyFeature`.

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Audit baseline: `6a812f02a50f202955402b60f16361310e39d7e9`
- Compatibility reference: Python `pproxy==2.7.9`
- Product boundary: practical replacement for documented/common pproxy workflows, not exhaustive reproduction of every legacy transport, plugin, or private implementation detail.

## Purpose

Close the remaining high-confidence compatibility defects found after the practical-parity and lean-runtime work, make unsupported behavior fail honestly, remove silent Python compatibility stubs, make the lean feature graph materially truthful, and reduce documentation/verification maintenance cost.

This is a corrective and reductive roadmap. It must not become another parity expansion program. The main engineering problem is no longer broad protocol coverage; it is that several compatibility claims, option semantics, Python facades, feature boundaries, and planning artifacts disagree with the current implementation or with `pproxy==2.7.9`.

## Governing constraints

1. Preserve the current native Eggress product and the existing bounded pproxy compatibility target.
2. Do not add SSH, QUIC/HTTP/3, ShadowsocksR, legacy Shadowsocks stream ciphers/OTA, obfuscation plugins, generalized daemonization, or private pproxy internals.
3. Correct behavior at the compatibility boundary before changing shared runtime architecture.
4. Unsupported or non-equivalent inputs must produce structured, actionable failures. They must not be accepted and silently ignored.
5. The default/full Cargo build must retain the current supported feature surface.
6. Binary-size work must be measurement-driven. Do not retain `cfg` complexity that neither removes a meaningful dependency family nor produces a measurable artifact benefit.
7. Routine hosted CI remains small. Preserve one Rust smoke workflow, one Python smoke workflow, and the release-only Python wheel workflow unless consolidation clearly reduces maintenance without weakening coverage.
8. Crates.io release remains manual. PyPI wheel publication remains release-only CI because it produces multiple native platform artifacts.
9. Preserve high-value network correctness tests for framing, lifecycle, cancellation, reload, and hostile input handling. Reduce duplicated documentary evidence and redundant compatibility ceremony instead.
10. Do not create a new evidence registry, certification framework, completion-report tree, plan-per-test hierarchy, or permanent benchmark gate.
11. Use the pinned upstream source or focused probes only where semantics remain uncertain. The checked-in `compat/pproxy-2.7.9` baseline should become executable authority for stable CLI facts.
12. Update roadmap and plan status in place when work lands. Do not create separate closure plan files unless a genuinely new defect class is discovered.

## Confirmed problem set

### CLI compatibility defects

- `-d` is currently parsed as daemon mode. In `pproxy==2.7.9`, `-d` enables debug tracebacks while `--daemon` is the daemon option.
- `--reuse` is documented and diagnosed as cross-session connection pooling. Upstream uses it for listener `SO_REUSEPORT` behavior on supported systems.
- `--auth <seconds>` is accepted by the parser but is not consumed by translation or runtime configuration.
- `--sys` currently inspects and prints system proxy settings. Upstream uses it to apply system proxy configuration; inspection is not equivalent behavior.
- Unknown and unsupported options can reach service startup after warnings. Compatibility mode therefore risks running a materially different configuration from the one requested.
- Help text, compatibility diagnostics, manifests, and checked-in baseline facts disagree on several option meanings.

### Python compatibility defects

- The top-level `pproxy` namespace exposes several structural facades whose methods either return `None`, sleep forever without checking state, or raise generic `NotImplementedError` only when invoked.
- Some classes are useful metadata adapters, while other methods imply operational compatibility that is not present.
- Silent `None` results from lifecycle and connection methods are particularly hazardous because callers may treat them as successful no-op behavior.
- The native `eggress.pproxy` lifecycle API is stronger than parts of the top-level compatibility namespace, but delegation boundaries are not consistently enforced or documented.

### Feature and binary-size defects

- The implemented `common` feature build is only modestly smaller and still retains admin and metrics as required dependencies.
- Broad crate-level dependencies mean some nominally optional groups remain linked transitively.
- The existing full build already uses release LTO, one codegen unit, and symbol stripping, so additional full-binary reductions must come from removing unused feature families or narrowly disabling costly optional functionality.
- Crate merging is not a primary binary-size strategy and should not be used as one.

### Documentation and verification defects

- The capability and strict manifests contain stale or contradictory flag classifications.
- Historical parity specifications include incorrect upstream references and claims that no longer match current code.
- The practical matrix correctly avoids an aggregate parity claim, but multiple older files still appear authoritative.
- The lean-runtime roadmap remains marked `PLANNED` while its phase files are marked implemented, and some phase files link to a non-existent parent filename.
- Python smoke CI is path-filtered but omits several transitive Rust crates whose changes can alter the extension's behavior.
- Ordinary Rust CI is already small; the main simplification opportunity is reducing duplicate governance artifacts, not deleting core smoke checks.

## Target state

At closure:

1. `-d`, `--daemon`, `--reuse`, `--auth`, and `--sys` have accurate, tested compatibility semantics or explicit non-parity failures.
2. Compatibility execution fails before starting the service when any requested behavior is unsupported, ignored, malformed, or non-equivalent.
3. Help text, baseline data, parser tests, translation diagnostics, and public compatibility documentation agree.
4. Top-level Python compatibility methods either delegate to a working implementation or raise a stable, specific compatibility exception before side effects.
5. No operational method returns `None` merely to preserve API shape unless upstream itself defines that return behavior and a test proves it.
6. The default/full build retains its existing feature surface.
7. The lean build removes meaningful optional dependency families from `cargo tree`; otherwise the ineffective feature boundary is simplified rather than elaborated.
8. Full and lean artifact measurements are recorded in implementation commits, not enforced as routine CI gates.
9. One maintained human compatibility matrix, one machine-readable manifest, and executable tests form the active compatibility authority.
10. Historical plans remain available but are clearly non-authoritative; stale active-roadmap status and broken parent links are corrected in place.
11. Python smoke CI runs for all Rust paths that can change the extension or compatibility namespace without introducing a broad matrix.
12. No additional routine workflows, release certifications, evidence bundles, or completion documents are added.

## Execution sequence

| Order | Plan | Purpose | Dependency |
|---|---|---|---|
| 1 | [`PPROXY_CORRECTIVE_PHASE_1_CLI_SEMANTICS.md`](PPROXY_CORRECTIVE_PHASE_1_CLI_SEMANTICS.md) | Correct option meanings, make translation exhaustive, and fail closed before runtime startup. | None |
| 2 | [`PPROXY_CORRECTIVE_PHASE_2_PYTHON_BEHAVIOR.md`](PPROXY_CORRECTIVE_PHASE_2_PYTHON_BEHAVIOR.md) | Remove silent facade behavior and align the bounded Python namespace with real runtime capability. | Phase 1 diagnostic taxonomy stable |
| 3 | [`PPROXY_CORRECTIVE_PHASE_3_FEATURE_TOPOLOGY_AND_SIZE.md`](PPROXY_CORRECTIVE_PHASE_3_FEATURE_TOPOLOGY_AND_SIZE.md) | Make optional feature boundaries truthful and perform a measured, low-risk binary-size pass. | Phases 1-2 may run in parallel; shared API changes settled before closure |
| 4 | [`PPROXY_CORRECTIVE_PHASE_4_CONTRACT_CI_CLOSURE.md`](PPROXY_CORRECTIVE_PHASE_4_CONTRACT_CI_CLOSURE.md) | Consolidate source-of-truth documents, repair CI path coverage, and close the line without new ceremony. | Phases 1-3 complete |
| Follow-up | [`PPROXY_POST_CLOSURE_CORRECTIVE_PASS.md`](PPROXY_POST_CLOSURE_CORRECTIVE_PASS.md) | Correct residual `-d`, `--sys`, execution-gate, Python claim/classification, and verification mismatches found after Phase 4. | Phases 1-4 landed |

The four numbered phases remain the completed implementation set. The post-closure follow-up is a bounded corrective pass for defects found during review and must not be split into additional parity phases.

## Phase boundaries

### Phase 1 — CLI semantics and fail-closed execution

Correct the compatibility parser and translator at the narrow boundary. The phase owns option arity, aliases, classifications, diagnostics, startup gating, and platform-specific `SO_REUSEPORT` handling. It does not authorize daemonization or broad system-proxy redesign.

### Phase 2 — Python behavioral honesty

Inventory every public method exported through the bundled top-level `pproxy` namespace. Preserve behavior-backed methods, delegate to the native Eggress binding where the mapping is local, and replace misleading no-ops with a stable unsupported-feature exception. Do not recreate upstream's internal asyncio server engine in Python.

### Phase 3 — Feature topology and artifact reduction

Trace actual dependency edges from the CLI, runtime, server, metrics, admin, embed, and Python crates. Remove broad unconditional edges only where a small shared interface can preserve runtime invariants. Retain bounded feature groups; do not create per-protocol micro-features. Use `cargo tree` and artifact measurements to decide which changes survive.

### Phase 4 — Contract and CI closure

Regenerate active compatibility claims from corrected behavior, demote historical records, correct roadmap status/link drift, and make Python CI path coverage truthful. Do not add a new closure report; update this roadmap and active documents in place.

### Post-closure corrective pass

Review of the landed Phase 1-4 implementation found a small residual set: `-d` is parsed but not consumed by standalone logging selection; `--sys` is fail-closed yet unreachable inspection code and stale closure prose remain; the two compatibility execution paths do not apply identical unknown-option gating; Python closure text overstates native backing for top-level factory objects; and a small number of structural Python methods still need test-backed classification. The follow-up plan owns only those items and final verification.

## Global non-goals

- strict 100% pproxy parity;
- expanding the public claim beyond practical `pproxy==2.7.9` compatibility;
- new proxy protocols, transports, schedulers, routing primitives, admin APIs, metrics backends, or daemon managers;
- a new system-proxy abstraction beyond what is required to classify or locally support `--sys`;
- cross-session upstream connection pooling;
- generalized Python reimplementation of pproxy's private runtime;
- deleting historical plans solely to make the tree smaller;
- workspace-wide crate merging;
- unsafe socket code when a safe maintained API is available;
- routine cargo-bloat, benchmark, audit, fuzz, soak, differential, or external-service CI gates;
- automatic crates.io publication;
- additional platform/Python matrices in ordinary smoke CI.

## Verification policy

Use focused tests during each phase. The final broad Rust gate remains:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

For Python-facing work:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

Use the pinned upstream oracle only for the small set of semantics that are not already proven by `compat/pproxy-2.7.9/cli-baseline.json` or source inspection. Do not make upstream installation a routine test dependency.

Binary-size evidence is local and informational:

```bash
cargo tree -p eggress-cli -e features
CARGO_TARGET_DIR=target/full cargo build -p eggress-cli --release
CARGO_TARGET_DIR=target/lean cargo build -p eggress-cli --release --no-default-features --features common
ls -lh target/full/release/eggress target/full/release/pproxy target/lean/release/eggress
```

`cargo bloat` may be used interactively but must not become a repository dependency or CI requirement.

## Roadmap acceptance criteria

This roadmap is complete only when all are true:

- all four phase plans are implemented or explicitly closed as unnecessary with code- or measurement-backed reasoning;
- the post-closure corrective pass is implemented or explicitly closes every review finding with code/test-backed reasoning;
- the five confirmed CLI semantic defects are corrected or fail explicitly before startup;
- unsupported and unknown compatibility inputs cannot silently alter execution;
- every public Python facade method has an evidence-backed behavior classification;
- silent compatibility no-ops are removed or proven to match upstream sentinel behavior;
- default/full feature behavior is unchanged;
- the lean graph demonstrably omits meaningful optional families or ineffective gates are removed;
- active compatibility documentation and manifests agree with executable tests;
- historical plans are clearly non-authoritative and active roadmap status/link drift is repaired;
- Python smoke CI covers all transitive Rust implementation paths without a new matrix;
- existing Rust smoke and PyPI release boundaries remain proportionate;
- no new protocol scope, certification framework, completion document, or permanent binary-size gate has been added.

## Closure record

### Implementation commit range

Phases 1-3: `f08b8d0..6cb43dd`
Phase 4: `367d7cb`
Post-closure corrective: `f6336674353aa63d868c8247ffafb9eb5a3eca4e`.
Final contract/reporting closure pass: `bd10467`; metadata reconciliation:
`cef6851cc7275aca3cb8f9e27cc3dc8b4f7abff6`; test-only workspace correction:
`eeaa411`; see `PPROXY_FINAL_CONTRACT_REPORTING_CLOSURE_PASS.md` for focused
and workspace verification. PAC and verbose are both
`compatible_with_warning`; `cli.get` config/runtime/CLI layers are complete.
Focused GET fail-closed evidence and the final closure-record update are in
`cf448e7`.

### Phase 4 review corrections

The Phase 4 closure record below reflects the state that was intended at
implementation time. The post-closure corrective pass resolved the
specific statements identified at final review:

- `-d` is wired to a debug-level default tracing filter via the shared
  `default_log_level` helper. Independent of `-v` and `--daemon`.
- `--sys` is unsupported and fatal before startup in pproxy compatibility
  mode. Inspection-only behavior is not presented as equivalent to
  pproxy's `--sys` mutation.
- The standalone `pproxy` binary and `eggress pproxy run` apply the same
  fail-closed policy through the shared `eggress_pproxy_compat::evaluate_execution_gate`
  helper.
- Top-level `pproxy.Connection` / `pproxy.Server` are pproxy-shaped URI
  factories and must not be conflated with native `eggress.pproxy.Server`.
- Remaining structural Python sentinel/no-op methods have explicit
  test-backed classifications in `python/tests/test_pproxy_public_namespace.py`.
- Final Python verification result is recorded in the post-closure
  corrective pass.

### Final CLI compatibility decisions

- `-d` enables a debug-level default tracing filter via the shared
  `default_log_level` helper. Independent of `-v` and `--daemon`.
- `--daemon` is fatal before startup (exit code 5).
- `--reuse` configures SO_REUSEPORT on listener sockets (not connection pooling).
- `--sys` is unsupported in pproxy compatibility mode; fails before startup.
- `--pac`, `--get`, `--test` are value-taking options; values are not reclassified as positional.
- Unknown flags are fatal in both the standalone compatibility binary and
  `eggress pproxy run`. The shared `evaluate_execution_gate` helper
  enforces the same classification in both entry points.
- Trojan is supported for both inbound and upstream.

### Python methods

- Top-level `pproxy.Connection` and `pproxy.Server` are pproxy-shaped URI
  factories (aliases for `proxies_by_uri`). The compatibility server
  path is a Python adapter using `asyncio.start_server` and
  `_eggress_stream_handler`. It is NOT the native `eggress.pproxy.Server`
  lifecycle class.
- `eggress.pproxy.Server` is the native Rust-backed service lifecycle.
- Known unsupported concrete operations use the stable compatibility
  exception hierarchy.
- Structural `proto.*`, `cipher.*`, and proxy-class helper behavior has
  explicit test-backed classifications recorded in the post-closure
  corrective pass.

### Full/lean results

- Default/full build retains existing feature surface
- Lean build (`common` feature) omits operations, extended protocols, reverse
- Feature boundaries documented in `AGENTS.md`

### Active authority

- Matrix: `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`
- Manifest: `docs/parity/pproxy_capability_manifest.toml`
- Strict manifest: `docs/parity/pproxy_2_7_9_strict_manifest.toml` (historical/derived)
- CI: `.github/workflows/ci.yml`, `.github/workflows/python-test.yml`
- Release: `.github/workflows/publish-python.yml`

### Rejected optimizations

- Per-protocol micro-features: maintenance cost outweighs binary-size benefit
- Workspace-wide crate merging: not a primary binary-size strategy
- Routine OS/architecture matrices: disproportionate for a small project

Do not create a separate corrective-closure roadmap or evidence report.
