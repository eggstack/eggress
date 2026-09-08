# Architecture Convergence Phase 2 — Control-Plane Convergence

## Status

**IMPLEMENTED**

## Baseline

- Repository: `eggstack/eggress`
- Roadmap: `plans/ARCHITECTURE_CONVERGENCE_ROADMAP.md`
- Dependency: Phase 1 correctness closure complete

## Objective

Collapse duplicated startup, reload, translation, and Python async-control paths onto canonical internal operations without changing the supported proxy feature surface.

This phase should make the architecture easier to reason about by ensuring that:

- validated configuration has a single canonical compiled form;
- service startup can consume compiled in-memory configuration directly;
- reload has one transaction regardless of whether configuration originated from a file, string, admin call, or embedding API;
- pproxy translation can produce native structures directly for internal consumers instead of serializing through TOML;
- blocking native operations exposed to Python use one maintained async bridge and close/cancellation model.

The phase is primarily reductive. It must not add new protocol families or broaden parity claims.

---

## Workstream 1 — Make compiled configuration the canonical embed/runtime handoff

### Problem

`EggressConfig::from_toml_str()` currently parses, validates, and compiles input, but the embed object retains the original TOML source as its effective canonical state. Native startup writes that source into a temporary file so the runtime can parse/compile it again. Compatibility startup already demonstrates that `ServiceSupervisor::start_from_config_with_options()` can accept in-memory compiled configuration.

### Target design

`EggressConfig` should own or reference the validated compiled `eggress_config::compile::RuntimeConfig` needed by runtime startup. Retaining source TOML may remain useful for round-trip display/reload convenience, but source text must not be required to start the service.

Preferred shape:

```text
TOML/file/compat input
        |
        v
parse + validate + compile
        |
        v
EggressConfig { compiled, optional_source }
        |
        v
ServiceSupervisor::start_from_config(...)
```

### Implementation tasks

1. Change `EggressConfig` to store compiled runtime configuration as its canonical validated state.
2. Preserve source TOML only where needed by public methods such as `source_toml()` or redacted display. If retaining source is necessary for API compatibility, treat it as ancillary state.
3. Make `EggressService::start()` and `start_blocking()` use the in-memory supervisor startup path.
4. Remove normal native startup dependence on `write_temp_config()`.
5. Preserve file-origin information only when needed for explicit file-backed reload semantics. A service created from an in-memory config should not pretend to have an underlying config file.
6. Review secret lifetime: eliminating temp files should reduce plaintext credential persistence. Do not introduce extra long-lived serialized copies of source TOML merely for convenience.
7. Keep compatibility options orthogonal to configuration. Native and compatibility startup should share the same core startup function with different options rather than duplicate thread/runtime setup.

### Required tests

- `EggressConfig::from_toml_str()` validates once and produces a startable service without filesystem access beyond unrelated runtime needs;
- blocking start succeeds from in-memory config;
- async start succeeds from in-memory config;
- temp directory permission/read-only tests demonstrate service startup does not require writing a temp TOML file;
- status/bound address/shutdown behavior remains unchanged;
- compatibility startup still passes its current behavior tests;
- secret-redaction/public source methods remain source-compatible where they are part of the public API.

### Acceptance criteria

- normal embed startup has no temp-config write/read round trip;
- runtime receives a compiled `RuntimeConfig` directly;
- native and compatibility startup share one core lifecycle implementation apart from explicit compatibility options;
- no user-visible startup regression is introduced.

---

## Workstream 2 — Create one canonical reload transaction

### Problem

File-backed `ServiceSupervisor::reload_config()` and embed `EggressHandle::reload_toml_str()` independently implement reload. Their side effects have diverged: snapshot/router swap, stored runtime config, admin snapshot publication, health restart, H2 pool invalidation, metrics updates, and source-path handling are not guaranteed to happen identically.

### Target design

Introduce one internal method that applies a newly compiled runtime configuration to a running supervisor/state, with all classification and side effects centralized.

Conceptually:

```rust
fn apply_compiled_config(
    &mut self,
    new_config: RuntimeConfig,
) -> ReloadResult
```

The exact ownership/API may differ, but there must be one transaction that owns:

1. reload classification;
2. runtime snapshot compilation against the previous snapshot;
3. runtime-config bookkeeping needed by the next reload;
4. snapshot publication;
5. routing swap;
6. admin snapshot publication;
7. health manager restart/reconfiguration;
8. protocol pool/cache invalidation required by configuration changes;
9. metrics generation/reload recording;
10. failure semantics preserving old state.

### Implementation tasks

1. Extract the existing supervisor reload transaction into a reusable internal API.
2. Ensure mutation ordering preserves existing atomicity guarantees. No externally visible new generation may be published before all state needed by that generation is ready.
3. Route file reload through `load/compile -> apply_compiled_config`.
4. Route embed string/file reload through `parse/compile -> apply_compiled_config` rather than independently swapping state.
5. Preserve distinction between parse/validation failure, restart-required rejection, snapshot-build failure, and successfully applied reload.
6. Normalize metrics behavior so both entry points record success/failure consistently.
7. Ensure H2 or other transport-pool invalidation occurs through one canonical hook rather than only one reload path.
8. Ensure admin snapshot data and public status derive from the same newly published snapshot.

### Required tests

Create table-driven or shared-harness tests that apply the same logical change through both file and embed reload entry points and compare outcomes.

At minimum:

- routing-only accepted reload;
- upstream chain change requiring pool invalidation;
- health configuration change;
- listener change rejected according to Phase 1 policy;
- malformed TOML fails before state mutation;
- snapshot/compile failure preserves generation;
- metrics generation/reload counters agree across reload entry points;
- admin/status reflects the same generation and configuration as routing after success.

### Acceptance criteria

- only one implementation owns runtime state mutation for reload;
- file and embed reload differ only in how new configuration is obtained;
- side effects cannot silently diverge between entry points;
- all rejected/failed reloads preserve the prior active generation and runtime behavior.

---

## Workstream 3 — Remove TOML as an internal pproxy translation IR

### Problem

`eggress-pproxy-compat` reasonably needs its own compatibility AST because pproxy syntax includes modifiers and legacy semantics not identical to native Eggress URI syntax. The avoidable part is serializing translated compatibility input to TOML and then reparsing/recompiling it for consumers such as outbound connectors or Python connection wrappers.

### Target design

Keep the compatibility parser/AST, but introduce a native compilation result that can be consumed directly by runtime/embed code.

Preferred conceptual flow:

```text
pproxy argv / URI
      |
      v
PproxyArgs / PproxyUri
      |
      v
Compat compile
      |
      +--> native RuntimeConfig / ProxyChainSpec / compatibility options
      |
      +--> optional TOML renderer for --dump-config / migration / debugging
```

Do not force every input through TOML merely because TOML is a convenient serialization format.

### Implementation tasks

1. Identify the minimal native outputs needed by current internal consumers:
   - compiled runtime configuration for listener/service mode;
   - proxy chain specification for outbound connector mode;
   - compatibility runtime options/warnings/unsupported-feature diagnostics.
2. Refactor compatibility translation so semantic translation produces typed/native intermediate structures before rendering.
3. Keep `TranslationOutput.toml` or equivalent user-facing TOML generation where public compatibility APIs require it, but make it a renderer over the semantic/native result rather than the only result.
4. Update `OutboundConnector::from_pproxy_uri()` to consume a native chain/config result directly.
5. Update PyO3/Python construction paths that currently translate -> TOML -> `EggressConfig::from_toml_str()` to consume the typed result where practical.
6. Preserve warnings, structured unsupported-feature diagnostics, redaction, and compatibility-tier semantics.
7. Do not merge `PproxyUri` into `eggress-uri` if doing so would lose pproxy-specific syntax or create a bloated native AST. The goal is a clean conversion boundary, not one parser for every dialect.

### Required tests

- for representative HTTP, SOCKS4/5, TLS-wrapped, Shadowsocks/Trojan, chain, fixed-target, local-bind, and rule examples, direct native compilation and TOML-render/reparse produce equivalent compiled semantics;
- unsupported features and warnings are identical across direct and TOML-render paths;
- credentials are not exposed in diagnostics;
- outbound connector no longer needs TOML serialization to establish a supported pproxy-style chain;
- `--dump-config`/translation APIs still emit valid TOML when requested.

### Acceptance criteria

- TOML is presentation/configuration input-output, not mandatory internal IR for pproxy-to-native conversion;
- `OutboundConnector::from_pproxy_uri()` avoids the serialize/parse round trip;
- compatibility syntax remains isolated to the compatibility crate rather than leaking into native config layers.

---

## Workstream 4 — Standardize Python async bridging and lifecycle semantics

### Problem

The Python package has a maintained `AsyncBridge`/`CloseWaiter` implementation with explicit loop affinity, cancellation and close invariants. Other async wrappers, notably outbound stream/connector paths, call `run_in_executor()` directly and can therefore diverge in cancellation, context propagation, loop affinity, error normalization, and shutdown behavior.

### Implementation direction

1. Inventory all Python methods that expose blocking native calls through asyncio.
2. Classify operations into:
   - truly nonblocking native async operation;
   - blocking PyO3/native operation requiring executor/bridge;
   - synchronous lightweight accessor not requiring executor work.
3. Route blocking async wrappers through the maintained bridge helper or a single small primitive extracted from it.
4. Avoid one thread-pool submission per trivial operation if the native binding can safely expose a proper async method without architectural expansion. Prefer minimal changes and measure complexity.
5. Standardize loop-affinity behavior for stateful async wrappers.
6. Standardize close/wait semantics so cancellation of one waiter cannot corrupt object state.
7. Preserve user-visible exception taxonomy where already established.
8. Do not create a second bridge abstraction specifically for outbound streams.

### Required tests

- cross-loop misuse for stateful async objects fails predictably;
- cancellation of connect/read/drain/wait operations does not cause double resolution;
- close and wait are idempotent;
- multiple waiters complete consistently;
- contextvars behavior remains as documented where the bridge promises preservation;
- no event-loop thread blocking is introduced;
- ResourceWarning/finalizer behavior remains bounded and nonblocking.

### Acceptance criteria

- Python async wrappers use one maintained bridging pattern for blocking native work;
- direct `run_in_executor()` usage is limited to the canonical bridge implementation or explicitly documented exceptional cases;
- cancellation/close behavior is contract-tested across `AsyncConnection` and outbound stream paths.

---

## Workstream 5 — Normalize configuration/result API boundaries

This is a small cleanup workstream performed only after the preceding convergence lands.

1. Review constructors named `from_toml`, `from_file`, `from_pproxy_uri`, and internal `compile_*` helpers for duplicated validation or semantic conversion.
2. Ensure each public facade performs input parsing once and hands a typed object to lower layers.
3. Prefer explicit internal result structs over tuples carrying unrelated options/warnings.
4. Do not add public builders solely for aesthetic consistency.
5. Update `architecture/embed.md`, `architecture/runtime.md`, `architecture/config.md`, `architecture/pproxy-compat.md`, and `architecture/python-bindings.md` to show the new canonical flows.

### Acceptance criteria

- architecture docs describe actual typed boundaries;
- no active facade relies on hidden temp-file or TOML round trips after typed compilation unless the user explicitly supplied a file and file semantics are intended.

---

## Phase verification

During implementation run focused crate tests. At phase closure run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-config
cargo test -p eggress-runtime
cargo test -p eggress-embed
cargo test -p eggress-pproxy-compat
cargo test -p eggress-python
cargo test --workspace --locked
```

Build/install the Python extension and run:

```bash
.venv/bin/python -m pytest python/tests tests/compat -q
```

No external oracle suite is required unless semantic compatibility output changes rather than merely changing its internal representation.

## Phase acceptance criteria

Phase 2 is complete only when:

- compiled runtime configuration is the canonical embed/runtime handoff;
- native embed startup no longer requires temp TOML files;
- one canonical reload transaction owns all state mutation and side effects;
- file and embed reload reuse that transaction;
- internal pproxy consumers can obtain native configuration/chain structures without TOML serialization;
- Python blocking async wrappers use one maintained bridging model;
- public behavior and supported compatibility semantics remain unchanged except for correctness fixes already approved in Phase 1;
- architecture documentation reflects the converged paths.

## Closure record

Implemented in commit `edca5ab` (single commit on `main`, ahead of the
roadmap baseline `93a205c` and after Phase 1 closure `cf1aba1`).

Canonical startup: `startup_in_memory(rt_config, options)` in
`crates/eggress-embed/src/lib.rs` (shared by native `start()` /
`start_blocking()` and compat `start_blocking_with_compatibility_options()`;
only `CompatibilityOptions` differ) via
`ServiceSupervisor::start_from_config_with_options(rt, None, options)`.
`EggressConfig` stores compiled `RuntimeConfig` (canonical) + ancillary
source TOML via shared `parse_validate_compile`; `_config_path=None`,
SIGHUP disabled, no `write_temp_config` round trip.

Canonical reload transaction:
`RuntimeState::apply_compiled_config(&RuntimeConfig) -> ReloadResult` in
`crates/eggress-runtime/src/supervisor.rs` owns classification (snapshot
authoritative), snapshot build (Arc reuse), snapshot/routing/admin publish,
`restart_health_probes()`, `H2_POOL_REGISTRY.clear()`, and metrics
(`set_config_generation` + `record_reload` on success *and* failure).
`ServiceSupervisor::reload_config()` (file), SIGHUP handling, and embed
`reload_toml_str` / `reload_toml_file` / `reload_compiled` differ only in
config acquisition; supervisor updates stored `rt_config` only on `Applied`.

Direct compatibility compilation (no TOML string in native paths):
- `compile_chain_to_native(&PproxyChain) -> ProxyChainSpec` in
  `crates/eggress-pproxy-compat/src/translate.rs` (outbound; validation
  mirrors remote handling, `build_chain_config_uri` → `parse_proxy_chain`).
- Shared `build_intermediates()` produces typed `TranslationIntermediates`;
  `generate_toml()` (presentation/`--dump-config`) and
  `intermediates_to_config_file()` (direct `ConfigFile` mapping, no
  `to_string`/`from_str`) are renderers over it.
- `translate_to_runtime_config()` → `NativeTranslation { runtime, warnings,
  unsupported }` and `translate_pproxy_args_to_native()` →
  `CombinedTranslation { toml, runtime, warnings, unsupported }`
  (single intermediates build for both renderers).
- `OutboundConnector::from_pproxy_uri()` consumes `compile_chain_to_native`
  into a minimal `RuntimeConfig` (no TOML); `PyConnection::new` consumes
  `CombinedTranslation` via `EggressConfig::from_compiled` (TOML retained
  only for `config` display).

Python bridge: `AsyncBridge.run` / `wrap_blocking_call` + `CloseWaiter` in
`python/eggress/_asyncio.py`. `AsyncOutboundStream` (bridge loop-affinity +
waiter idempotent close/wait, no executor for trivial wait),
`OutboundConnector.aconnect_tcp`, `Connection.aclose`/`await_closed`, and
`CompatibleStreamWriter.drain` route via the bridge. Direct
`run_in_executor` remains only inside the bridge itself plus the documented
`plugin.py` user-callback timeout exception.

Regression/equivalence tests:
- reload: `crates/eggress-embed/tests/reload_convergence.rs` (6 tests:
  routing accept via string+file, upstream/H2 clear, health via
  string+compiled, listener reject on all entry points, malformed metrics,
  admin/status generation agreement);
- translation: `crates/eggress-pproxy-compat/tests/native_equivalence.rs`
  (5 tests: HTTP/SOCKS variants, TLS/SS/Trojan, fixed-target/local-bind/rule
  parity handling, outbound chain match, identical warnings/unsupported);
- async: `python/tests/test_asyncio_bridge_convergence.py` (6 passed,
  1 skipped: no direct executor outside bridge, bridge/waiter lifecycle,
  close idempotent/multi-waiter, wait cancellation safety, contextvars,
  cross-loop `LoopAffinityError`, bounded `ResourceWarning` finalizer);
- embed unit: `from_toml_str_validates_once`, in-memory startup with no
  `eggress-embed-*.toml` + `_config_path=None`, async start.

Verification at closure: `cargo fmt --all -- --check`, `cargo clippy
--workspace --all-targets -- -D warnings`, `cargo test --workspace --locked`
(2704 passed, 151 ignored), rebuilt PyO3 (`maturin develop`) +
`pytest python/tests tests/compat` (2273 passed, 115 skipped).
No external oracle suite (compatibility tiers unchanged; internal
representation only). README/AGENTS.md reviewed: no temp-file/TOML-IR claims
to prune, no changes needed. Architecture docs (`embed`, `runtime`,
`config`, `pproxy-compat`, `python-bindings`) and skills (`config-reload`,
`python-bindings`, `rust-proxy-dev`, `testing`) updated to canonical flows.

No scope expansion: no new protocols, manifests, workflows, or crates;
`eggress-routing` added to `eggress-embed` deps only to name
`RouteActionSpec::Direct` for the minimal outbound `RuntimeConfig`.