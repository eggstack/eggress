# pproxy Final Corrective Closure Roadmap

> **Historical record.** Phase 0 reset the active contract after this plan
> was completed. Do not use its older `cli.config`, `cli.log`, or BIND gap
> accounting as current requirements; see the active parity matrix and
> `plans/PPROXY_STRICT_PHASE_0_ORACLE_CONTRACT_RESET.md`.

## Status

**IMPLEMENTED — CLOSED**

### Closure record

- **Implementation commit range**: `5a724be..HEAD` (Phases 1-5)
- **Key decisions**:
  - Phase 1: DUMMY restored to callable identity; UDP_LIMIT corrected to 30; prepare_ciphers non-None raises UnsupportedPProxyFeature; plugin-bearing URIs rejected explicitly.
  - Phase 2: cli.config demoted to compatible_with_warning; cli.log demoted to compatible_with_warning; SOCKS4/SOCKS5 BIND reclassified as unsupported (matching pproxy); stale H2/WS/raw/tunnel docs corrected.
  - Phase 3: Temp-file round-trip eliminated; in-memory typed config boundary via `validate_and_compile_toml_with_warnings`; subprocess `eggress` spawn eliminated; shared `run_upstream_test` function. tempfile moved to dev-dependencies.
  - Phase 4: Regex trust model codified (trusted operator config, not hostile network input); fancy_regex backtrack limit explicitly configured via `FANCY_REGEX_BACKTRACK_LIMIT = 1_000_000`; exhaustion and rule-entry overflow tests added; stale Phase 41 verification labels cleaned.
  - Phase 5: Full verification gate passed; manifest validated; all phase plans updated.
- **Focused differential/oracle checks**: Phase 1 behavioral tests (DUMMY callable identity, UDP_LIMIT=30) verified as upstream matches against pinned pproxy 2.7.9. non-`None` `prepare_ciphers` and plugin-bearing URIs are Eggress fail-closed decisions (raises `UnsupportedPProxyFeature`), not upstream-equivalent behavior.
- **Broad Rust result**: 2491 passed, 146 ignored. Format and Clippy clean.
- **Broad Python result**: 2226 passed, 114 skipped. Fresh extension build successful.
- **Environment-limited checks**: External pproxy differential/oracle tests not run (require live pproxy installation); existing checked-in evidence and focused tests cover all changed surfaces.
- **Future reopening threshold**: Future pproxy work requires at least one of: (a) a reproducible user-visible compatibility defect within the bounded product claim, (b) a security/correctness defect in a currently supported protocol/path, (c) an explicit project-level decision to expand product scope, or (d) a new upstream target/version decision. A desire to increase a parity percentage, mirror private pproxy internals, or implement an excluded transport for completeness is insufficient by itself.

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `5a724be68de7080cc6fff21aeb5774491a307dfa`
- Compatibility oracle: Python `pproxy==2.7.9`
- Active product claim: practical and behavioral compatibility for common/documented pproxy workflows, not strict reproduction of every pproxy transport, plugin, legacy cipher, daemon/process behavior, or private implementation detail.

## Purpose

Close the narrow set of correctness, contract, execution-path, and maintenance defects found after the completed practical-parity and corrective-reduction work. This roadmap is intentionally a closure program, not a new parity-expansion program.

The proxy data plane is already strong for HTTP/HTTPS, SOCKS4/4a/5, UDP, TLS, modern Shadowsocks AEAD, Trojan, supported TCP chains, H2/WS/WSS/raw upstreams, routing, and bounded reverse behavior. Remaining work is concentrated at the compatibility boundary and in the repository's maintenance apparatus:

1. Python compatibility helpers still contain a small number of silent-success or exact-contract mismatches.
2. A few capability classifications overstate drop-in/equivalent behavior.
3. Several historical documents remain stale enough to conflict with the active manifest/matrix.
4. The standalone `pproxy` execution path unnecessarily serializes translated config to a temporary file and, for `--test`, launches the sibling `eggress` executable.
5. Compatibility regex fallback uses backtracking without a match-time timeout; the project needs an explicit trusted-configuration boundary rather than speculative sandboxing.
6. The final closure should use focused differential evidence without rebuilding a permanent certification bureaucracy.

## Governing constraints

1. Preserve the current native Eggress runtime and the existing bounded `pproxy==2.7.9` compatibility target.
2. Do **not** add SSH, QUIC/HTTP/3, ShadowsocksR, legacy Shadowsocks stream ciphers/OTA, plugin execution, daemonization, per-client auth reuse, generalized multi-hop UDP, macOS PF recovery, or other previously excluded scope.
3. Do not convert private upstream implementation details into new public Eggress architecture merely to increase a parity count.
4. Unsupported compatibility behavior must fail explicitly before side effects. Silent no-ops, ignored metadata, pass-through substitutions, and accidental success are defects.
5. Keep one active machine-readable compatibility contract (`docs/parity/pproxy_capability_manifest.toml`) and one maintained human matrix (`docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`). Historical plans/specifications may remain, but they must not compete as active sources of truth.
6. Hosted CI remains deliberately small: one Rust smoke workflow, one path-scoped Python smoke workflow, and release-only Python publication. Do not add routine oracle, benchmark, fuzz, audit, artifact-size, or cross-platform matrices.
7. Binary-size changes are measurement-driven and secondary to simplification. Do not replace Tokio, rustls, Clap, Serde/TOML, or the crate structure for speculative byte savings.
8. Do not add a planning registry, certification registry, evidence database, generated completion-report tree, or plan-per-test hierarchy.
9. Prefer typed in-process Rust interfaces over temporary files and subprocess composition where the existing architecture allows a small clean change.
10. Regex/rule configuration is local operator configuration unless an existing public API explicitly makes it untrusted remote input. Do not design an isolated regex worker process unless evidence demonstrates that the threat model requires it.
11. Update the roadmap and phase plan status in place when implementation lands. Do not create another closure plan unless review discovers a genuinely new defect class.

## Confirmed findings owned by this roadmap

### Python compatibility semantic defects

- `python/pproxy/server.py::prepare_ciphers()` returns the original reader/writer unchanged for a non-`None` cipher even though upstream pproxy initializes plugins/cipher stream wrappers. A compatibility helper must not report apparent success while omitting the requested behavior.
- `python/pproxy/server.py::_proxy_by_uri()` parses comma-delimited plugin metadata into `_plugins` and then discards it. Unsupported plugin execution must fail explicitly rather than being ignored.
- `python/pproxy/server.py::DUMMY` is an `object()` while pproxy 2.7.9 exposes a callable identity helper. Code that calls `DUMMY(value)` therefore breaks.
- `python/pproxy/server.py::UDP_LIMIT` is `64` while pproxy 2.7.9 uses `30`. If the public compatibility namespace exposes this constant, the value should match unless a deliberate incompatibility is documented and tested.
- Existing strict helper tests emphasize symbol existence/signatures and do not fully guard these behavioral/value properties.

### Contract classification defects

- `-f/--config` is classified as `drop_in` even though Eggress accepts its own configuration schema rather than pproxy's configuration schema. Same flag purpose is not unchanged-input compatibility.
- `--log <PATH>` is classified as native-equivalent while compatibility mode does not write to the requested path; it relies on tracing/stderr/shell redirection. This is a supported difference or warning, not equivalent CLI behavior.
- SOCKS4 BIND taxonomy is internally awkward: the manifest states pproxy also does not implement it while labeling Eggress's matching refusal as intentional non-parity. Classification should describe observable compatibility, not implementation preference.
- Any similar contradictions discovered by the focused manifest audit may be corrected, but the phase must not become an unrestricted rewrite of all capability tiers.

### Documentation/source-of-truth drift

Known stale material includes:

- outdated upstream repository references;
- stale H2/WS/raw integration status;
- incorrect scheduler statements despite pproxy supporting `fa`, `rr`, `rc`, and `lc`;
- stale dependency-policy examples that still show rustls logging features already removed from the actual workspace;
- historical Python API inventories that no longer describe the promoted runtime surface.

The objective is not to make every historical phase document current. The objective is to make active documents correct and make historical documents unmistakably non-authoritative.

### Execution-path complexity

The standalone `pproxy` binary currently translates arguments to TOML, writes a temporary file, and then asks `ServiceSupervisor` to parse that file back into runtime configuration. `--test` additionally resolves and executes a sibling `eggress` process. This introduces avoidable filesystem/process failure modes and keeps `tempfile`/subprocess glue in a path that can be in-process.

The target is a shared typed execution path where practical, while preserving `pproxy translate` and any user-visible serialized-config output.

### Regex threat-model ambiguity

Compatibility rules use the fast Rust `regex` engine with `fancy_regex` fallback for constructs needed to approximate Python `re`. The fallback can backtrack and currently has no match-time timeout. For pproxy compatibility this input originates from operator configuration/rule files, not unauthenticated network payloads. The repository should codify that trust boundary and retain validation/limits without adding an elaborate evaluator service.

## Execution sequence

| Order | Plan | Purpose | Dependency |
|---|---|---|---|
| 1 | [`PPROXY_FINAL_PHASE_1_PYTHON_SEMANTIC_CLOSURE.md`](PPROXY_FINAL_PHASE_1_PYTHON_SEMANTIC_CLOSURE.md) | Remove the remaining Python silent-success behaviors and exact public-contract mismatches. | None |
| 2 | [`PPROXY_FINAL_PHASE_2_CONTRACT_AND_DOCUMENTATION_REDUCTION.md`](PPROXY_FINAL_PHASE_2_CONTRACT_AND_DOCUMENTATION_REDUCTION.md) | Correct overstated tiers, reconcile active sources, and demote stale documentation without creating replacement bureaucracy. | Phase 1 behavior classifications stable |
| 3 | [`PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md`](PPROXY_FINAL_PHASE_3_EXECUTION_PATH_SIMPLIFICATION.md) | Replace avoidable temporary-file/subprocess composition with shared typed/in-process execution where cleanly feasible; measure any resulting size/dependency benefit. | Phase 2 contract stable |
| 4 | [`PPROXY_FINAL_PHASE_4_REGEX_AND_VERIFICATION_BOUNDARY.md`](PPROXY_FINAL_PHASE_4_REGEX_AND_VERIFICATION_BOUNDARY.md) | Codify regex trust/resource boundaries and prune verification/documentation ceremony without reducing high-value tests or hosted smoke CI. | Can begin after Phase 2; finalize after Phase 3 |
| 5 | [`PPROXY_FINAL_PHASE_5_DIFFERENTIAL_CLOSURE.md`](PPROXY_FINAL_PHASE_5_DIFFERENTIAL_CLOSURE.md) | Run focused changed-surface oracle/interoperability checks, reconcile final active claims, and close the line. | Phases 1-4 complete |

## Phase boundaries

### Phase 1 — Python semantic closure

Own only the remaining top-level `pproxy` Python compatibility defects. Prefer exact behavior when it is trivial and local (`DUMMY`, constants). Where actual behavior would require intentionally excluded plugin/cipher/runtime replication, raise the stable compatibility exception before side effects. Do not recreate pproxy's asyncio runtime engine.

### Phase 2 — Contract and documentation reduction

Audit only active claims plus known stale documents that are still likely to be read as current. Correct tier semantics using observable behavior. Keep the active compatibility authority small. Historical records may be given a short banner or cross-reference rather than rewritten line-by-line.

### Phase 3 — Execution-path simplification

Introduce or reuse a typed runtime/config entry point so compatibility execution need not round-trip through a temporary TOML file. Make `--test` call shared Rust functionality rather than launching a sibling process if this can be done without duplicating CLI behavior or creating a new abstraction framework. Retain serialized TOML only as an output/diagnostic form. Measure dependency/artifact effects but do not chase size beyond the architectural simplification.

### Phase 4 — Regex and verification boundary

State explicitly that compatibility regex/rule files are trusted local/operator configuration. Preserve parser validation, entry-count/input-size limits, and fail-closed unsupported-pattern diagnostics. Do not add a regex subprocess/sandbox solely because `fancy_regex` can backtrack. Separately, ensure repository policy and scripts distinguish routine tests from specialized oracle/certification work.

### Phase 5 — Differential closure

Run only the focused upstream checks needed for the behavior changed in Phases 1-4 plus the existing broad workspace/Python gates. Correct final active claims in place. Mark this roadmap and phase plans implemented with commit/test references. Do not create a new certification document or aggregate parity percentage.

## Global non-goals

- strict 100% pproxy parity;
- matching every private function/class implementation in upstream pproxy;
- SSH or nested SSH chains;
- QUIC or HTTP/3;
- SSR or legacy stream ciphers;
- plugin execution/obfuscation framework;
- daemon/process-manager implementation;
- implicit system proxy mutation in compatibility mode;
- generalized connection reuse/pooling beyond existing supported protocol behavior;
- general multi-hop UDP;
- a regex worker process or sandbox absent a changed threat model;
- replacing the async runtime, TLS stack, CLI framework, serialization stack, or workspace crate structure;
- new routine CI matrices or release automation;
- deleting useful high-value tests simply to reduce test count;
- rewriting every historical plan/spec to current terminology.

## Verification policy

Use focused tests during implementation. The broad Rust gate remains:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Python-facing changes additionally require a fresh extension build and the relevant compatibility suites:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

Use the pproxy 2.7.9 oracle only for changed or uncertain semantics. Representative commands are documented in `docs/DIFFERENTIAL_TESTING.md`; external oracle installation must not become a routine CI dependency.

Artifact/dependency measurement is informational:

```bash
cargo tree -p eggress-cli -e features
CARGO_TARGET_DIR=target/size-final-full cargo build -p eggress-cli --release
CARGO_TARGET_DIR=target/size-final-small cargo build -p eggress-cli --profile release-cli-small
ls -lh target/size-final-full/release/eggress target/size-final-full/release/pproxy
```

Run `cargo bloat` only interactively when it is already available. Do not add it to repository dependencies or CI.

## Roadmap acceptance criteria

This roadmap is complete only when all are true:

- every phase plan is implemented or explicitly closed as unnecessary using code/test/measurement evidence;
- `prepare_ciphers()` no longer silently reports success for behavior Eggress does not implement;
- parsed plugin metadata cannot be silently discarded by the Python compatibility URI factory;
- public `DUMMY` behavior and `UDP_LIMIT` match the pinned pproxy 2.7.9 contract or have an explicit, justified, tested incompatibility classification;
- `-f/--config`, `--log`, SOCKS4 BIND, and any directly adjacent audited classifications describe observable behavior accurately;
- the active capability manifest and practical matrix agree with tests and with each other;
- stale historical documents no longer appear to override the active sources of truth;
- compatibility service startup no longer requires a temporary TOML round-trip when a clean typed in-process path is available;
- `--test` no longer requires launching a sibling `eggress` executable when shared Rust functionality can execute the same operation directly;
- any retained temporary-file/subprocess path has a documented stop-condition reason rather than being preserved accidentally;
- compatibility regex/rule configuration has an explicit trust/resource model with existing bounded validation retained;
- no regex sandbox, new runtime framework, protocol family, plugin system, or large dependency is added;
- routine hosted CI remains the current lean Rust/Python smoke shape;
- specialized differential/certification work remains opt-in and changed-surface focused;
- default/full supported runtime behavior is unchanged outside the intended compatibility corrections;
- the final changed-surface differential checks and broad Rust/Python gates pass;
- no aggregate parity percentage or new permanent certification artifact is introduced;
- the roadmap and all five phase plans are updated in place to `IMPLEMENTED` (or explicitly `CLOSED — NO CHANGE REQUIRED`) with the implementation commit(s) and verification summary.

## Closure rule

After Phase 5, treat the bounded `pproxy==2.7.9` parity program as closed. Future compatibility work should be driven by a reproducible user-visible defect or an intentional product-scope decision, not by reopening broad feature enumeration against pproxy internals.
