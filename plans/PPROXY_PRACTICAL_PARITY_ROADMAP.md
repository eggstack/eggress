# pproxy 2.7.9 Practical Parity Roadmap

## Status

Reopened — narrow corrective pass required before closure.

Phases 0 through 6 were implemented, but a post-closure source audit found bounded parser, translator, runtime, and Python orchestration defects. Active handoff plan:

`plans/PPROXY_PARITY_NARROW_CORRECTIVE_PASS.md`

This roadmap supersedes `plans/PPROXY_FULL_DROP_IN_ROADMAP.md` for the current line of work. The older roadmap remains useful as a record of the maximal compatibility target, but its certification machinery, exhaustive internal-API replication, and requirement to reproduce every legacy transport are disproportionate to the present project.

## Goal

Make Eggress a reliable replacement for documented, commonly used `pproxy==2.7.9` configurations while preserving Eggress as a focused Rust proxy rather than rebuilding all of pproxy's historical implementation details.

The target includes:

- faithful pproxy URI and CLI parsing;
- matching default routing and scheduler behavior;
- correct translation of per-remote rules;
- compatibility-layer access to protocol implementations Eggress already has;
- a bounded Python namespace and public API that lets normal pproxy applications run unchanged after Eggress is installed;
- closure of small, high-value runtime gaps where existing Eggress primitives make the implementation local and maintainable.

The target does not require reproducing obsolete or unusually costly features merely to increase a parity percentage.

## Frozen reference

All compatibility work targets exactly `pproxy==2.7.9`.

Do not broaden the target to a moving upstream branch. When source, PyPI documentation, and existing repository manifests disagree, determine behavior from a clean pproxy 2.7.9 installation with a small focused probe. Record the result in the existing parity documentation; do not build another general certification framework.

## Product boundary

Eggress continues to expose two related surfaces:

1. The canonical `eggress` API, which remains typed, explicit, Rust-native, and free to provide stronger operational behavior.
2. The pproxy compatibility surface, which reproduces documented pproxy syntax and public contracts where this roadmap requires it.

Compatibility behavior must terminate at a narrow adapter boundary. Protocol parsing and wire handling remain in shared Rust components wherever possible.

## Scope guardrails

The following are explicit non-goals for this roadmap unless a later user decision changes them:

- SSH transport implementation;
- QUIC or HTTP/3 implementation;
- ShadowsocksR;
- legacy Shadowsocks stream ciphers and OTA;
- pproxy obfuscation/plugin families such as `tls1.2_ticket_auth`;
- general-purpose daemonization;
- cross-session connection reuse solely for `--reuse`;
- replication of every private or incidental pproxy Python internal;
- a new compatibility test framework, evidence registry, CI matrix, or release gate;
- broad refactoring of stable native runtime crates when an adapter-layer fix is sufficient.

Unsupported syntax must still parse far enough to produce an accurate, stable diagnostic. Unsupported behavior must not be silently translated into something else.

## Engineering principles

- Correct common behavior before adding obscure protocols.
- Prefer a single parser and typed intermediate representation over scattered special cases.
- Reuse existing admin, routing, transport, and outbound-stream machinery.
- Keep strict pproxy quirks inside the compatibility layer.
- Add only tests that protect the behavior changed in the phase.
- Do not gate routine development on an external pproxy installation.
- Use a small optional oracle smoke set for final compatibility checks.
- Delete or correct stale parity claims rather than adding another competing report.

## Phase sequence

### Phase 0 — Contract and source-of-truth correction

Correct the repository's parity documents and manifests so they describe current code and packaging. Establish one concise list of supported, partially supported, intentionally unsupported, and genuinely missing capabilities.

Detailed plan: `plans/PPROXY_PARITY_PHASE_0_CONTRACT_RESET.md`

### Phase 1 — URI grammar and CLI semantics

Implement the pproxy 2.7.9 URI grammar and canonical CLI option arity, defaults, and argument ownership. Correct no-argument behavior, mixed listeners, authentication placement, fixed-target forms, and the current `--pac`, `--get`, and `--test` parsing errors.

Detailed plan: `plans/PPROXY_PARITY_PHASE_1_URI_AND_CLI.md`

### Phase 2 — Routing and scheduler parity

Match pproxy's default first-available behavior, remote ordering, per-remote rules, and rule-file routing semantics. Keep Eggress-native structured routing available outside compatibility mode.

Detailed plan: `plans/PPROXY_PARITY_PHASE_2_ROUTING.md`

### Phase 3 — Compatibility bridge for existing transports

Expose H2, WebSocket/WSS, raw, and tunnel capabilities already present in the native runtime through the pproxy parser, translator, configuration model, and Python compatibility helpers. This phase must not create new protocol engines.

Detailed plan: `plans/PPROXY_PARITY_PHASE_3_TRANSPORT_BRIDGE.md`

### Phase 4 — Bounded Python drop-in surface

Ship a top-level `pproxy` namespace from the single Eggress distribution and implement the documented public contracts used by ordinary pproxy applications: `Connection`, `Server`, `Rule`, `DIRECT`, and the primary `server`, `proto`, and `cipher` modules. Delegate network behavior to Rust and avoid reproducing unused internals.

Detailed plan: `plans/PPROXY_PARITY_PHASE_4_PYTHON_DROP_IN.md`

### Phase 5 — Remaining high-value runtime gaps

Implement only the missing runtime features that are both part of documented pproxy behavior and reasonably local to Eggress's existing architecture: HTTP-only upstream mode, echo endpoints, fixed-target UDP, Unix upstreams, outbound local bind, and bounded transparent/reverse composition work. Preserve explicit non-parity where implementation cost would require a new subsystem.

Detailed plan: `plans/PPROXY_PARITY_PHASE_5_RUNTIME_GAPS.md`

### Phase 6 — Closure, cleanup, and truthful compatibility statement

Run a small representative oracle comparison, resolve documentation drift, remove obsolete compatibility scaffolding, and publish a final supported/unsupported matrix. Do not expand CI or introduce release certification machinery.

Detailed plan: `plans/PPROXY_PARITY_PHASE_6_CLOSURE.md`

### Narrow corrective pass — Parser/runtime wiring closure

Repair only the post-closure defects confirmed in canonical fixed-target syntax, echo parser reachability, fixed-target TCP/UDP separation, local-bind translation, `httponly` role classification, and Python upstream handshake orchestration.

This pass does not reopen excluded transports or authorize broader compatibility work.

Detailed plan: `plans/PPROXY_PARITY_NARROW_CORRECTIVE_PASS.md`

## Dependency order

```text
Phase 0
  └─ Phase 1
       ├─ Phase 2
       ├─ Phase 3
       └─ Phase 4
            └─ Phase 5
                 └─ Phase 6
                      └─ Narrow corrective pass
                           └─ Phase 6 closure re-review
```

Phase 2 and Phase 3 may proceed in parallel after the Phase 1 grammar representation is stable. Phase 4 should consume the stable parser and translator rather than implement its own URI handling.

## Compatibility modes

Do not create a second runtime. The same Rust engine serves both modes.

- Native mode uses Eggress configuration and Eggress defaults.
- Compatibility mode is selected by the `pproxy` executable or top-level `pproxy` Python namespace.
- Compatibility mode may preserve pproxy defaults or return types that differ from the native API.
- Compatibility mode must not weaken native defaults globally.

## Lightweight verification policy

Each phase should add:

- focused parser or unit tests for every corrected syntax rule;
- one or two runtime tests per newly wired behavior;
- a small Python test for each public contract changed;
- at most a handful of optional paired pproxy probes for behavior that cannot be established from source or existing tests.

Routine verification should remain proportional:

```bash
cargo fmt --check
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test cli_tests
python -m pytest python/tests/<focused files>
```

Run wider workspace tests only when shared runtime code changes. Do not add mandatory external-service tests or platform matrices.

## Definition of completion

This roadmap is complete when:

- documented common pproxy URIs parse and translate correctly;
- canonical fixed-target and echo forms reach their intended runtime paths;
- fixed-target TCP and UDP roles retain independent configuration;
- outbound local binding reaches the native socket path;
- `httponly` is accepted only in its supported compatibility role;
- Python compatibility servers perform each upstream handshake once;
- no CLI option consumes the wrong number of arguments;
- default remote selection and rule routing match pproxy;
- already-supported native transports are reachable through compatibility entry points;
- normal `import pproxy` applications using the documented public server and connection APIs run against Eggress without source edits;
- the bounded Phase 5 runtime gaps are either implemented or explicitly retained as intentional non-parity with accurate diagnostics;
- current documentation contains no stale top-level-package, protocol, or drop-in claims;
- a concise final matrix clearly distinguishes matched behavior from deliberate exclusions;
- Phase 6 has been re-reviewed after the narrow corrective pass.

Completion does not authorize the claim that Eggress reproduces every legacy cipher, plugin, optional transport, or private pproxy implementation detail. The appropriate public claim is:

> Eggress provides practical pproxy 2.7.9 compatibility for documented HTTP, SOCKS, modern encrypted-proxy, routing, CLI, and public Python-library workflows, with explicit exclusions for legacy and high-cost transports.
