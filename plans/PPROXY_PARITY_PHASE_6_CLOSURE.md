# Phase 6 — pproxy Parity Closure, Cleanup, and Final Compatibility Statement

## Status

Proposed.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Depends on

Phases 0 through 5.

## Objective

Close the practical pproxy parity line with a small, trustworthy compatibility matrix, representative oracle checks, cleaned documentation, and removal of stale scaffolding.

This phase is not a release-certification project. It must verify the behaviors implemented by this roadmap without restoring the previous extensive CI, evidence, or compatibility bureaucracy.

## In scope

- final source and documentation audit;
- representative pproxy 2.7.9 oracle comparisons;
- clean-wheel Python compatibility smoke;
- CLI and URI regression set;
- supported protocol/role matrix;
- intentional non-parity list;
- deletion or archival marking of stale compatibility-package references;
- consolidation of active documentation;
- proportional test cleanup;
- final claim language.

## Out of scope

- all-platform exhaustive matrices;
- long-running soak or performance certification;
- retained evidence bundles for every commit;
- required external pproxy tests in routine CI;
- new release gates;
- implementing new features discovered during closure unless they are direct regressions from earlier phases;
- converting intentional exclusions into implementation work;
- historical document rewrites that do not affect current contributor behavior.

## Closure principles

1. Test representative behavior, not every Cartesian combination.
2. Prefer end-to-end public workflows over additional internal unit layers.
3. A skipped optional oracle check is not evidence, but it also should not block ordinary development.
4. Current source and focused tests outrank historical completion records.
5. Every public compatibility claim must name its boundary.
6. Remaining exclusions should be explicit and stable, not hidden behind generic unsupported errors.

## Workstream 6.1 — Build the final compatibility matrix

Create one maintained matrix under `docs/parity/` that lists capabilities by observable user surface rather than internal crate count.

Suggested columns:

| Surface | Capability | CLI | Python | Runtime | Evidence | Status | Notes |
|---|---|---|---|---|---|---|---|

Required sections:

- URI grammar and modifiers;
- CLI options and defaults;
- inbound protocols;
- upstream protocols;
- TCP chains;
- UDP modes;
- routing/rules/schedulers;
- reverse and transparent modes;
- Python package/import surface;
- Python `Connection` and `Server` workflows;
- modern cipher support;
- intentional non-parity.

Use these final labels:

- `matched` — representative oracle comparison or direct interoperability confirms the defined behavior;
- `supported_difference` — usable but a documented difference remains;
- `intentional_non_parity` — explicitly excluded;
- `native_extension` — Eggress-only functionality;
- `platform_limited` — matches where the platform facility exists.

Do not publish aggregate parity percentages.

## Workstream 6.2 — Representative oracle scenarios

Create or consolidate a small optional closure suite covering the highest-value workflows. Keep the suite to roughly 15 to 25 scenarios.

Minimum scenarios:

### CLI and parser

- no-argument mixed listener;
- singular HTTP and SOCKS5 listeners;
- combined listener;
- canonical authentication;
- two-hop chain;
- `--pac` value ownership;
- `--get` value ownership and static response;
- `--test` value ownership and exit behavior;
- first-available default with two remotes;
- per-remote rule selection.

### Runtime

- HTTP CONNECT echo;
- HTTP forward GET/POST;
- SOCKS4a domain target;
- SOCKS5 authenticated CONNECT;
- standalone UDP direct echo;
- one-hop SOCKS5 UDP;
- one modern Shadowsocks or Trojan path;
- one promoted H2/WS/raw path;
- one fixed-target path;
- one Unix upstream on Unix.

### Python

- clean `import pproxy`;
- `await Connection.tcp_connect()` echo;
- supported UDP public API;
- `Server` start/close;
- `Rule`/`DIRECT` public behavior.

Each scenario should produce a compact pass/fail result. Do not retain large stdout, network traces, or environment archives unless a failure requires diagnosis.

## Workstream 6.3 — Clean-wheel and executable smoke

From a clean virtual environment:

1. Build the authoritative Eggress wheel.
2. Confirm only the intended distribution is installed.
3. Confirm `eggress` and `pproxy` namespaces import.
4. Run the public Python closure scenarios.
5. Confirm the `eggress` and compatibility `pproxy` executables are installed as intended.
6. Confirm help/version output identifies Eggress without falsely claiming upstream pproxy ownership.
7. Confirm unsupported families return accurate diagnostics.
8. Uninstall and verify package files are removed cleanly.

Run this locally or manually before release. Do not add a broad wheel matrix beyond the existing PyPI workflow.

## Workstream 6.4 — Documentation consolidation

Define current authoritative documents:

- `README.md` — user overview and concise support boundary;
- `docs/parity/README.md` — compatibility policy and document index;
- one final practical compatibility matrix;
- `docs/PYTHON_BINDINGS.md` — native and pproxy-compatible Python use;
- the active roadmap and phase plans as historical execution guidance after completion.

For competing documents:

- mark historical parity roadmaps and completion records as superseded where they could mislead;
- remove references to deleted packages, scripts, or workflows;
- avoid duplicating the same matrix in multiple files;
- preserve useful deep technical documents by linking them from the authoritative index.

Do not delete historical documents solely to reduce file count if they are clearly labeled and no longer referenced as current policy.

## Workstream 6.5 — Test-suite reduction and alignment

Review compatibility tests added across the phases and remove redundant layers.

Keep:

- parser regression cases;
- translator-to-config cases;
- one runtime test per distinct execution mechanism;
- public Python workflow tests;
- optional oracle closure scenarios.

Remove or merge:

- duplicate tests asserting the same generated TOML string at multiple layers;
- structural-only import tests superseded by functional tests;
- stale tests for the deleted compatibility distribution;
- tests whose only purpose is proving a documentation count;
- broad scenario generators that reproduce focused cases without additional failure coverage.

Do not make the full optional oracle suite a required hosted CI job.

## Workstream 6.6 — Final unsupported behavior audit

Confirm stable diagnostics for:

- SSH;
- QUIC/H3;
- SSR;
- legacy Shadowsocks ciphers/OTA;
- unsupported plugins;
- daemonization;
- cross-session reuse;
- unsupported reverse compositions;
- multi-hop UDP combinations still outside scope;
- platform-specific transparent modes not implemented.

Each diagnostic should state:

- what was recognized;
- why it is not supported;
- whether a practical alternative exists;
- whether the exclusion is intentional or platform-limited.

Avoid generic `unknown scheme` when syntax is known.

## Workstream 6.7 — Final public claim

Use qualified language consistent with actual completion.

Permitted claim when all roadmap acceptance criteria pass:

> Eggress provides practical pproxy 2.7.9 compatibility for documented HTTP, SOCKS, modern encrypted-proxy, routing, CLI, and public Python-library workflows, with explicit exclusions for legacy and high-cost transports.

Do not claim:

- universal full pproxy parity;
- support for every optional dependency;
- compatibility with every private pproxy internal;
- SSH, QUIC/H3, SSR, or legacy cipher support;
- all-platform transparent proxy parity unless implemented and verified.

## Final acceptance criteria

The line of work may be closed when:

- the final compatibility matrix is generated from current implementation knowledge and manually reviewed;
- all Phase 1 parser/CLI acceptance cases pass;
- scheduler and per-remote rule behavior matches the oracle in representative cases;
- promoted native transports are reachable through compatibility entry points;
- a clean wheel provides functional top-level `pproxy` imports;
- public `Connection` and `Server` workflows pass local end-to-end tests;
- mandatory Phase 5 runtime gaps are implemented;
- conditional Phase 5 items have explicit implemented or non-parity decisions;
- intentional exclusions produce specific diagnostics;
- stale package and capability claims are removed from current docs;
- redundant compatibility tests and obsolete package tests are removed;
- routine CI remains lean and does not run the external oracle suite;
- the final public claim remains qualified.

## Suggested final verification

Run focused suites first:

```bash
cargo fmt --check
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test cli_tests
cargo test -p eggress-runtime --test integration
python -m pytest python/tests/test_wheel_import_smoke.py
python -m pytest python/tests/test_proxy_connection.py
python -m pytest python/tests/test_server_lifecycle.py
```

If shared runtime code changed across Phase 5, run once:

```bash
cargo test --workspace --locked
```

Run the optional pproxy 2.7.9 closure scenarios from a clean local environment and record only a concise summary in the final compatibility document.

## Failure handling

If closure discovers a mismatch:

- fix it within the phase that owns the behavior when the repair is local;
- downgrade the matrix entry to `supported_difference` when the difference is acceptable and documented;
- record intentional non-parity when repair violates the scope guardrails;
- do not create a new broad corrective roadmap for a small defect;
- create a narrow follow-up plan only if multiple unresolved defects share an architectural cause.

## Handoff guidance

Implement closure in this order:

1. final matrix draft from current source;
2. representative oracle scenarios;
3. clean-wheel smoke;
4. documentation consolidation;
5. test reduction;
6. unsupported diagnostic audit;
7. final matrix and claim review.

The desired closure result is a smaller, clearer repository with more accurate compatibility—not another certification apparatus.
