# pproxy Maintenance Convergence Roadmap

## Status

**IMPLEMENTED**

## Closure

Implemented as committed: Phase 1 -> `8c10331`, Phase 2 -> `5962d47`,
Phase 3 -> `f011b66`, final corrective closure -> `6171f73`.
Verification on the corrective head: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --locked`, `cargo check --manifest-path fuzz/Cargo.toml --bins`,
bounded optional-compat compile gate, and Python smoke
(`python/tests` + `tests/compat`) all green. The line of work is closed;
remaining pproxy differences are documented compatibility boundaries.

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `6a5d5b7e6c37d0794775fc2e2c848a3aa948b219`
- Compatibility oracle: `pproxy==2.7.9`, commit `09d4752f17ed6787e1a073c93980eec019887ee3`
- Parent context: the architecture-convergence roadmap and its final corrective closure are implemented. This roadmap must not reopen already-closed work unless a current source-level defect proves that a closure invariant has regressed.

## Purpose

Eggress has reached broad practical compatibility with the frozen pproxy 2.7.9 target. The remaining high-value work is no longer another protocol-parity expansion. It is maintenance convergence: reduce the number of independently maintained representations of the same compatibility facts and URI/protocol semantics, ensure important optional compatibility code remains buildable, isolate pproxy-specific policy from the generic runtime where practical, and reduce responsibility concentration in the largest remaining modules without changing crate topology.

This roadmap is intentionally reductive. It should make the existing supported surface cheaper to maintain and harder to misrepresent. It must not be used as justification to implement obscure pproxy tails or unrelated modern proxy features.

## Research basis

### Current repository findings

At the planning baseline:

1. `docs/parity/pproxy_capability_manifest.toml` is the canonical machine-readable compatibility contract and `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` is the maintained human matrix, per `AGENTS.md`.
2. Other active documentation still duplicates detailed compatibility status and has drifted. `docs/PPROXY_MIGRATION.md`, for example, simultaneously describes Trojan as client-only and as supporting inbound/server use, and contains stale statements about compatibility `--daemon` / `--sys` behavior relative to the maintained matrix and current code.
3. `eggress-uri` and `eggress-pproxy-compat` each contain substantial URI parsing/redaction/endpoint machinery. The second parser is partly justified by pproxy-only syntax (`+in`, backward/bind/listen/rebind forms, plugin metadata, auth fragments), but both currently own lexical tasks that can drift independently.
4. Protocol identity is represented at several levels: syntax-oriented `eggress_uri::ProtocolSpec`, runtime-oriented `eggress_core::ProtocolId`, and compatibility string/token classification. The semantic distinction is legitimate, but recognition and conversion rules are distributed.
5. `ServiceSupervisor` still receives `CompatibilityOptions`, including fields that are not intrinsically runtime lifecycle concerns. Some pproxy-specific behavior genuinely requires post-bind/runtime integration; other policy can be lowered earlier.
6. The recent architecture-convergence work already decomposed several large modules and established canonical startup/reload/metrics/diagnostic paths. Those changes are treated as constraints, not invitations for another rewrite.
7. Remaining large maintenance hotspots include `eggress-routing/src/lib.rs`, `eggress-config/src/compile.rs`, `eggress-runtime/src/supervisor.rs`, and compatibility URI/translation modules. File size alone is not a defect; only coherent responsibility boundaries justify extraction.
8. Routine Rust CI intentionally runs a small Ubuntu smoke gate. The CLI default `full` feature excludes several explicitly optional compatibility features such as SSH, QUIC/H3, pproxy legacy/SSR, legacy crypto, and pproxy daemon support, so the ordinary default workspace path is not a complete compile check for those feature-gated claims.

### External/tooling research

Cargo's feature model is additive and its resolver-v2 behavior makes selected-package feature activation explicit. Cargo provides `--features`, `--no-default-features`, and `--all-features` specifically because ordinary default builds do not exercise every optional feature. For Eggress, a targeted explicit compatibility feature bundle is preferable to indiscriminate `--all-features`: it validates the claimed optional surface while avoiding intentionally unsafe/test-only combinations such as `insecure-quic` and avoiding a new CI matrix.

References:

- Cargo features: <https://doc.rust-lang.org/cargo/reference/features.html>
- Cargo resolver v2: <https://doc.rust-lang.org/cargo/reference/resolver.html>
- `cargo check` feature selection: <https://doc.rust-lang.org/cargo/commands/cargo-check.html>
- frozen upstream oracle: <https://github.com/qwj/python-proxy/tree/09d4752f17ed6787e1a073c93980eec019887ee3>

## Governing constraints

1. Preserve the frozen pproxy 2.7.9 compatibility target. Do not chase upstream `master` merely because later commits exist.
2. Preserve current public Rust, CLI, Python, and compatibility behavior unless a current behavior is demonstrably incorrect or a compatibility claim is false.
3. Treat `docs/parity/pproxy_capability_manifest.toml` plus executable evidence as authoritative. Human documentation must follow that contract.
4. Do not create a new capability manifest, parity percentage, certification framework, generated evidence bundle, or dashboard.
5. Do not create a new GitHub Actions workflow. Any CI change in this roadmap must be a bounded step in the existing Rust smoke workflow.
6. Do not add an OS matrix for routine compatibility verification.
7. Do not use `cargo --all-features` blindly if it activates intentionally insecure, test-only, mutually awkward, or non-product feature combinations. Prefer an explicit product-relevant optional feature bundle.
8. Do not merge crates. Existing crate boundaries are retained unless a separate project-level decision explicitly changes them.
9. Do not add a dynamic protocol/plugin registry. Static typed mappings and exhaustive tests are preferred.
10. Do not make `eggress-uri` understand pproxy-only reverse/plugin syntax merely to eliminate the compatibility parser. Compatibility-specific grammar remains compatibility-owned.
11. Do not replace boxed stream boundaries or introduce broad generic stream plumbing.
12. Do not reopen completed startup/reload, metrics ownership, Python async bridge, listener-free UDP, reverse TLS/mTLS, or other architecture-convergence work without a demonstrated regression.
13. Internal extraction must follow coherent responsibility boundaries; no arbitrary file-size targets.
14. Add dependencies only when unavoidable. This roadmap should be implementable with the existing dependency set.
15. Specialized external/oracle suites remain opt-in and should run only when a phase changes the corresponding compatibility behavior or claim.

## Explicitly deferred / rejected feature work

The following are not required for this roadmap and should remain documented boundaries unless a separate user requirement appears:

- macOS PF original-destination recovery;
- the four unavailable legacy cipher names (`cast5-cfb`, `idea-cfb`, `rc2-cfb`, `seed-cfb`);
- SSR UDP or generalized external/SIP003 plugin execution;
- QUIC/H3 UDP-association expansion;
- pproxy backward/reverse TLS wire composition;
- Trojan UDP;
- MASQUE / CONNECT-UDP;
- Linux TPROXY;
- Linux IPv6 original-destination recovery;
- TLS certificate hot reload;
- generalized Happy Eyeballs/address-selection infrastructure;
- new proxy protocols or transport families;
- post-2.7.9 pproxy `httpadmin` parity. Eggress already has a richer native admin plane, so reproducing that later upstream surface is not part of the frozen compatibility contract.

## Execution sequence

| Order | Plan | Primary outcome |
|---|---|---|
| 1 | [`PPROXY_MAINTENANCE_PHASE_1_CONTRACT_AND_FEATURE_GATES.md`](PPROXY_MAINTENANCE_PHASE_1_CONTRACT_AND_FEATURE_GATES.md) | Active docs become truthful and non-duplicative; product-relevant optional compatibility features receive one bounded compile gate. |
| 2 | [`PPROXY_MAINTENANCE_PHASE_2_URI_PROTOCOL_CONVERGENCE.md`](PPROXY_MAINTENANCE_PHASE_2_URI_PROTOCOL_CONVERGENCE.md) | Shared URI lexical/endpoint/redaction primitives and exhaustive protocol-token mappings reduce semantic duplication without collapsing the two grammars. |
| 3 | [`PPROXY_MAINTENANCE_PHASE_3_RUNTIME_POLICY_AND_INTERNAL_OWNERSHIP.md`](PPROXY_MAINTENANCE_PHASE_3_RUNTIME_POLICY_AND_INTERNAL_OWNERSHIP.md) | Compatibility-only policy is pushed outward from generic runtime where appropriate, and the highest-value remaining monolithic modules are decomposed only along proven responsibility boundaries. |

Do not start Phase 2 before Phase 1 establishes a truthful baseline. Do not start Phase 3 until the parser/protocol boundaries in Phase 2 are stable enough that runtime refactoring will not mix with grammar churn.

## Verification policy

Use focused tests while implementing each phase. For substantial Rust changes, the repository's existing broad gate remains:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

`fuzz/` remains a standalone workspace and should be checked only when affected code or phase acceptance criteria require it:

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Python smoke is required only when Python-facing behavior or the top-level compatibility package changes. External pproxy oracle/differential tests are required only when observable compatibility semantics are changed, not for source-only refactors proven equivalent by existing contract tests.

## Roadmap acceptance criteria

This roadmap is complete only when all of the following are true:

- the maintained compatibility manifest and practical matrix agree with active user-facing documentation about current supported/unsupported/difference boundaries;
- active migration documentation no longer contains stale contradictory protocol/CLI status tables that can silently become a third parity authority;
- no new compatibility source-of-truth artifact has been introduced;
- the existing Rust CI workflow compiles the product-relevant optional compatibility feature bundle on Linux without creating another workflow or matrix;
- the optional compile gate deliberately excludes non-product unsafe/test-only features and documents why;
- native and pproxy-compatible URI parsing share endpoint/userinfo/top-level-delimiter/redaction primitives where semantics are actually identical;
- pproxy-specific syntax remains represented explicitly rather than being smuggled into the native URI AST;
- supported protocol-name and alias mappings are exhaustive and regression-tested across syntax/runtime/compatibility boundaries;
- adding or removing a protocol token requires changing one obvious native recognition point plus explicit compatibility-only mappings, rather than several independent string whitelists;
- `ServiceSupervisor` no longer carries compatibility-only logging/debug policy that can be resolved before runtime startup;
- any compatibility options that remain in runtime have a documented reason tied to runtime/post-bind behavior and cannot be expressed cleanly by ordinary compiled configuration;
- routing/config source decomposition, if performed, reduces responsibility concentration without changing crate topology or public semantics;
- no deferred protocol expansion or CI certification program is pulled into scope;
- focused phase tests and the broad repository gate pass at closure.

## Stop condition

After these three phases, do not automatically create another pproxy-completeness roadmap. Remaining gaps should be evaluated only from concrete user demand, demonstrated interoperability failures, security defects, or maintainability regressions. The default stance after closure is maintenance rather than feature expansion.
