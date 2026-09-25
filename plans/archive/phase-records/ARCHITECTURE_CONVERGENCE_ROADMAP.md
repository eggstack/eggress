# Architecture Convergence Roadmap

## Status

**IMPLEMENTED**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Baseline commit: `93a205c387b502db1ceba8c9fd1a50c5de9e8e37`
- Product contract: preserve the existing Rust CLI, embeddable Rust API, Python package, bounded `pproxy==2.7.9` compatibility surface, and current supported proxy protocols unless a plan explicitly says otherwise.

## Purpose

The current implementation is feature-rich and broadly mature, but several concepts are now represented in more than one layer: listener reload state, URI/config translation, redaction, runtime startup/reload, Python async bridging, and metrics ownership. Some of those overlaps have already diverged into observable defects.

This roadmap closes the demonstrated correctness gaps first, then reduces duplicate control-plane paths and oversized orchestration modules, and only after that fills two high-value capability gaps that are already implied by existing public APIs or subsystems.

The goal is convergence, not expansion. Do not use this roadmap to add new protocol families, new workflow infrastructure, new evidence systems, generalized plugin abstractions, or broad architecture rewrites.

## Confirmed problem set

The implementation pass must address the following observed conditions.

1. Listener reload classification currently allows changes to settings such as protocol lists, authentication, TLS, Shadowsocks/Trojan configuration, connection limits, fixed targets, local bind, and some UDP settings even though accept loops capture those values at startup. A reload may therefore publish a new snapshot generation while the running listener continues to apply old settings.
2. `eggress-embed` contains a second URI-redaction path using a hard-coded scheme whitelist. That whitelist can miss credential-bearing schemes supported elsewhere, making `to_redacted_toml()` weaker than the canonical URI redactor.
3. `python/eggress/_asyncio_adapter.py` has compatibility defects in `CompatibleStreamReader`: EOF behavior for `readline()` is inconsistent with `asyncio.StreamReader`, and `__aiter__` is declared asynchronously rather than returning the iterator directly.
4. `scripts/publish-remaining.sh` publishes crates with `--no-verify`, despite the active release policy requiring package dry-run verification. A recent package-only `include_str!` defect demonstrated that workspace tests do not substitute for package verification.
5. Native embed startup validates/compiles TOML, discards the compiled result, writes the source to a temporary file, then asks the supervisor to load/compile it again. The compatibility startup path already proves that in-memory supervisor startup is possible.
6. File-backed runtime reload and embed reload implement separate reload transactions and do not perform identical side effects.
7. pproxy compatibility translation commonly uses generated TOML as an internal intermediate representation before native compilation, even when the consumer only needs a compiled/native chain or runtime configuration.
8. Python async APIs do not consistently use the repository's maintained `AsyncBridge`/close semantics.
9. Several crates are externally decomposed but internally monolithic, especially runtime supervision, metrics, pproxy translation, server accept/execute, config validation, and PyO3 bindings.
10. Metrics state is partly canonical in protocol subsystems and partly duplicated/bridged into `MetricsRegistry`, including delta bookkeeping.
11. Compatibility diagnostics and parity documentation have multiple partially overlapping models and have drifted from implementation in at least some cases.
12. The listener-free outbound API exposes a UDP-association concept but does not implement it.
13. Native reverse control transport lacks a first-class TLS/mTLS option even though reverse mode is otherwise substantial and the repository already has TLS transport support.

## Governing constraints

1. Correctness fixes precede structural refactoring and feature work.
2. Preserve public Rust/Python behavior unless the current behavior is demonstrably incorrect or falsely claims support.
3. Keep listener topology restart-required. Do not invent a generalized dynamic-listener subsystem during the correctness pass.
4. Prefer one canonical internal operation over duplicated file/string/compatibility variants.
5. Do not introduce another configuration language, another parity manifest, another metrics backend, or another async runtime abstraction.
6. Do not add crates merely to split source files. Internal modules are preferred unless a real public/package boundary exists.
7. Existing `eggress-uri`, compiled config, routing snapshot, `AsyncBridge`, TLS transport, UDP subsystem, and testkit should be reused rather than replicated.
8. Routine CI remains the current Rust smoke workflow and path-scoped Python smoke workflow. No new routine matrix or certification workflow is required.
9. Crates.io publication remains manual. The helper may be corrected, but publication must not become automatic.
10. New feature work in this roadmap is limited to listener-free outbound UDP and reverse control-channel TLS/mTLS. Other capability gaps remain explicitly deferred.
11. Every phase must leave the workspace buildable and testable; do not require a multi-phase flag day.
12. Documentation must describe actual behavior. Where implementation is intentionally limited, classify it as restart-required or unsupported rather than simulating support.

## Target state

At closure:

- a successful reload never reports listener behavior that the running data plane has not actually adopted;
- all credential-bearing URI/TOML redaction funnels through one canonical tolerant URI redactor;
- Python stream adapters satisfy the relevant `asyncio` contracts and use one maintained async bridging model where blocking native work is involved;
- Rust package publication helpers cannot silently bypass Cargo package verification;
- the embed API can start from validated/compiled in-memory configuration without a temporary TOML round trip;
- runtime reload has one canonical transaction used by file-backed and embed-facing entry points;
- pproxy compatibility can compile directly into native configuration/chain structures for internal consumers, while TOML rendering remains an explicit presentation/migration output;
- the largest orchestration files are split by responsibility without changing crate topology or public API unnecessarily;
- metrics have one defined ownership model and no unnecessary delta mirroring;
- compatibility diagnostics use one typed issue representation and active parity documentation no longer contradicts runtime behavior;
- listener-free outbound UDP is implemented using existing UDP primitives;
- reverse control channels can opt into TLS, with mTLS when configured, without inventing reverse-specific cryptography;
- no unrelated protocol expansion or verification ceremony is introduced.

## Execution sequence

| Order | Plan | Purpose | Dependency |
|---|---|---|---|
| 1 | [`ARCHITECTURE_CONVERGENCE_PHASE_1_CORRECTNESS.md`](ARCHITECTURE_CONVERGENCE_PHASE_1_CORRECTNESS.md) | Close reload, redaction, Python stream, release-helper, and documentation correctness defects. | None |
| 2 | [`ARCHITECTURE_CONVERGENCE_PHASE_2_CONTROL_PLANE.md`](ARCHITECTURE_CONVERGENCE_PHASE_2_CONTROL_PLANE.md) | Unify in-memory startup/reload, pproxy-to-native compilation, and Python async bridging. | Phase 1 complete |
| 3 | [`ARCHITECTURE_CONVERGENCE_PHASE_3_INTERNAL_STRUCTURE.md`](ARCHITECTURE_CONVERGENCE_PHASE_3_INTERNAL_STRUCTURE.md) | Decompose oversized modules and consolidate metrics/diagnostic ownership without new product behavior. | Phase 2 canonical paths stable |
| 4 | [`ARCHITECTURE_CONVERGENCE_PHASE_4_CAPABILITY_COMPLETION.md`](ARCHITECTURE_CONVERGENCE_PHASE_4_CAPABILITY_COMPLETION.md) | Implement listener-free outbound UDP and reverse TLS/mTLS only. | Phases 1-3 complete |

These four plans are the complete implementation set for this line of work. Do not split them into plan-per-file or plan-per-test documents unless implementation uncovers a genuinely independent blocker that cannot be handled within the owning phase.

## Explicitly deferred work

The following are not closure requirements for this roadmap:

- MASQUE / CONNECT-UDP;
- generalized HTTP/3 proxy feature expansion;
- full Linux TPROXY support;
- IPv6 transparent original-destination work;
- Trojan UDP unless independently required by an existing documented compatibility contract;
- TLS certificate hot reload;
- additional reverse-proxy protocol modes beyond securing the existing control transport;
- workspace crate merging;
- independent versioning of all workspace crates;
- binary-distribution redesign;
- generalized plugin or capability registries;
- replacing `BoxStream` with a generic stream architecture;
- new hosted CI workflows, benchmark gates, fuzz gates, soak gates, audit gates, or generated evidence bundles.

Certificate hot reload is deliberately deferred because the current listener-reload ownership problem must be resolved first. If a future project explicitly requires hot listener material, introduce that as a separate design with per-listener dynamic state and data-plane tests rather than weakening the restart-required contract in this roadmap.

## Verification policy

Use narrow affected tests during implementation. Each phase names its required checks. At roadmap closure run the existing broad gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

For Python-facing changes, build/install the extension and run the active Python suites from the repository root as documented in `AGENTS.md`:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q
```

External pproxy/shadowsocks interoperability is required only when a phase changes the corresponding compatibility behavior or manifest claim. Release dry-runs are required only for the publication/package surfaces changed in Phase 1.

## Roadmap acceptance criteria

This roadmap is complete only when all are true:

- all four registered plans are implemented or explicitly closed as unnecessary with code-backed rationale;
- Phase 1 defects have regression tests proving the corrected behavior;
- no reload operation can publish a listener configuration that the running listener is known not to use;
- `eggress-embed` no longer has an independent incomplete URI redaction scheme whitelist;
- Python compatibility stream EOF and async-iteration semantics are contract-tested;
- manual crates.io helper paths do not use `cargo publish --no-verify`;
- native embed startup does not require a temp config file merely to call the runtime;
- one internal reload transaction performs routing/snapshot/admin/health/pool side effects consistently;
- internal pproxy consumers do not need a TOML serialize/parse round trip to obtain native configuration;
- async Python wrappers use the maintained bridge/close semantics consistently for blocking native operations;
- large-module decomposition reduces responsibility concentration without adding crates or public abstractions without demonstrated need;
- metrics ownership is explicit and duplicate counter synchronization is reduced rather than expanded;
- active capability/compatibility docs and diagnostics agree with implemented behavior;
- listener-free outbound UDP has deterministic functional tests for direct and supported upstream cases plus cancellation/close behavior;
- reverse TLS/mTLS has deterministic tests for successful TLS, certificate rejection, and client-certificate policy where enabled;
- current routine CI topology remains unchanged unless a separate project-level decision explicitly changes it;
- no deferred expansion item is pulled into scope merely because adjacent code is being touched.

## Closure record

- Implementation commit range: phases 1–3 closed in prior `plans: close ...` commits through `b906010`; phase 4 implemented in `de460fc` (`convergence phase 4: listener-free outbound UDP and native reverse TLS/mTLS`).
- No scope item closed as unnecessary; all four registered plans implemented with code-backed tests.
- Principal regression/acceptance tests: phase 1 reload/redaction/stream/release-helper defects; phase 2 in-memory startup, canonical reload transaction, direct pproxy-to-native compilation, AsyncBridge convergence; phase 3 internal decomposition with metrics/diagnostic ownership; phase 4 `eggress-embed --lib udp` (direct + SOCKS5, lifecycle), `eggress-protocol-reverse --test tls` (server-auth, mTLS policy, SNI, reconnect, shutdown, redaction), `eggress-config --lib reverse` (TLS compile), `eggress-runtime --test reverse_runtime` (+ TLS spawn smoke) and `reverse_interop` ungated. Canonical paths: `EggressConfig::parse_validate_compile` → `startup_in_memory` → `apply_compiled_config`; `compile_chain_to_native` (no TOML round trip); shared TLS builders; `udp_capability` + `open_socks5_udp_upstream`.
- Deferred items remain out of scope: MASQUE/CONNECT-UDP, H3 expansion, TPROXY, IPv6 transparent OD, Trojan UDP, TLS hot reload, extra reverse modes, crate merges, independent versioning, distribution redesign, plugin registries, BoxStream replacement, new hosted CI gates. Routine CI topology unchanged; crates.io remains manual.

Do not create a separate completion/evidence document.