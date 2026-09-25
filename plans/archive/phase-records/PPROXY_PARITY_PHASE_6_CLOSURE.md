# Phase 6 — pproxy Parity Closure, Cleanup, and Final Compatibility Statement

## Status

Completed — narrow corrective pass verified.

The narrow corrective pass repaired canonical fixed-target parsing, echo parser
reachability, independent fixed-target TCP/UDP translation, local-bind wiring,
`httponly` role classification, and Python upstream handshake orchestration.

Completed corrective handoff:

`plans/PPROXY_PARITY_NARROW_CORRECTIVE_PASS.md`

The maintained practical matrix has been re-reviewed against the focused parser,
translation, runtime, and Python tests listed in the handoff summary.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Depends on

Phases 0 through 5 and the narrow corrective pass linked above.

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
- final claim language;
- re-review of the narrow corrective pass's affected matrix rows.

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
7. A parser or generated-TOML assertion does not close a runtime path by itself.

## Corrective re-entry gate

Before continuing the original closure sequence, complete and verify:

- canonical pproxy fixed-target syntax;
- `echo://` parser-to-runtime reachability;
- independent fixed-target TCP and UDP configuration;
- compatibility local-bind propagation into native chains;
- upstream-only `httponly` role enforcement;
- exactly one HTTP/SOCKS upstream handshake in the Python server path.

The detailed tasks, tests, stop conditions, and scope limits are in `plans/PPROXY_PARITY_NARROW_CORRECTIVE_PASS.md`.

## Workstream 6.1 — Build the final compatibility matrix

Maintain one matrix under `docs/parity/` that lists capabilities by observable user surface rather than internal crate count.

Suggested columns:

| Surface | Capability | CLI | Python | Runtime | Evidence | Status | Notes |
|---|---|---:|---:|---:|---|---|---|

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

Use these labels:

- `matched` — representative oracle comparison or direct interoperability confirms the defined behavior;
- `supported_difference` — usable but a documented difference remains;
- `intentional_non_parity` — explicitly excluded;
- `native_extension` — Eggress-only functionality;
- `platform_limited` — matches where the platform facility exists.

Do not publish aggregate parity percentages.

For the corrective pass, re-review only rows concerning fixed-target grammar, echo, fixed-target TCP/UDP, local bind, `httponly`, and Python server routing through upstream proxies.

## Workstream 6.2 — Representative oracle scenarios

Keep a small optional closure suite covering the highest-value workflows. The suite should remain roughly 15 to 25 scenarios and must not become a routine hosted-CI gate.

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
- per-remote rule selection;
- canonical fixed-target form;
- `echo://` recognition;
- `httponly` listener role rejection.

### Runtime

- HTTP CONNECT echo;
- HTTP forward GET/POST;
- SOCKS4a domain target;
- SOCKS5 authenticated CONNECT;
- standalone UDP direct echo;
- one-hop SOCKS5 UDP;
- one modern Shadowsocks or Trojan path;
- one promoted H2/WS/raw path;
- fixed-target TCP;
- bounded fixed-target UDP;
- local source bind;
- Unix upstream on Unix.

### Python

- clean `import pproxy`;
- `await Connection.tcp_connect()` echo;
- supported UDP public API;
- `Server` start/close;
- `Rule`/`DIRECT` public behavior;
- server through HTTP upstream with one handshake;
- server through SOCKS5 upstream with one greeting/connect sequence.

Each scenario should produce a compact pass/fail result. Do not retain large stdout, network traces, or environment archives unless a failure requires diagnosis.

## Workstream 6.3 — Clean-wheel and executable smoke

From a clean virtual environment:

1. Build the authoritative Eggress wheel.
2. Confirm only the intended distribution is installed.
3. Confirm `eggress` and `pproxy` namespaces import.
4. Run the public Python closure scenarios.
5. Confirm the Rust `eggress` and compatibility `pproxy` executables remain
   Cargo-installed binaries. The Python wheel intentionally contains the
   `eggress` and top-level `pproxy` namespaces but does not add pip console
   scripts; verify that boundary rather than introducing a second CLI package.
6. Confirm help/version output identifies Eggress without falsely claiming upstream pproxy ownership.
7. Confirm unsupported families return accurate diagnostics.
8. Uninstall and verify package files are removed cleanly.

Run this locally or manually before release. Do not add a broad wheel matrix beyond the existing PyPI workflow.

## Workstream 6.4 — Documentation consolidation

Current authoritative documents are:

- `README.md` — user overview and concise support boundary;
- `docs/parity/README.md` — compatibility policy and document index;
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` — maintained practical matrix;
- `docs/parity/PPROXY_CLOSURE_SCENARIOS.md` — optional representative scenario index;
- `docs/PYTHON_BINDINGS.md` — native and pproxy-compatible Python use;
- the active roadmap and corrective plan.

For competing documents:

- mark historical parity roadmaps and completion records as superseded where they could mislead;
- remove references to deleted packages, scripts, or workflows;
- avoid duplicating the same matrix in multiple files;
- preserve useful deep technical documents by linking them from the authoritative index.

Do not delete historical documents solely to reduce file count if they are clearly labeled and no longer referenced as current policy.

## Workstream 6.5 — Test-suite reduction and alignment

Review compatibility tests and keep only tests that protect distinct behavior.

Keep:

- parser regression cases;
- translator-to-config cases;
- one runtime test per distinct execution mechanism;
- public Python workflow tests;
- optional oracle closure scenarios;
- handshake-counting fixtures for the corrected Python server path.

Remove or merge:

- duplicate tests asserting the same generated TOML string at multiple layers;
- structural-only import tests superseded by functional tests;
- stale tests for the deleted compatibility distribution;
- tests whose only purpose is proving a documentation count;
- broad scenario generators that reproduce focused cases without additional failure coverage;
- noncanonical fixed-target examples presented as proof of pproxy syntax.

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
- platform-specific transparent modes not implemented;
- `httponly` listener role;
- local bind on incompatible connection types.

Each diagnostic should state:

- what was recognized;
- why it is not supported;
- whether a practical alternative exists;
- whether the exclusion is intentional or platform-limited.

Avoid generic `unknown scheme` when syntax is known.

## Workstream 6.7 — Final public claim

Use qualified language consistent with actual completion.

Permitted claim when all roadmap and corrective acceptance criteria pass:

> Eggress provides practical pproxy 2.7.9 compatibility for documented HTTP, SOCKS, modern encrypted-proxy, routing, CLI, and public Python-library workflows, with explicit exclusions for legacy and high-cost transports.

Do not claim:

- universal full pproxy parity;
- support for every optional dependency;
- compatibility with every private pproxy internal;
- SSH, QUIC/H3, SSR, or legacy cipher support;
- all-platform transparent proxy parity unless implemented and verified.

## Final acceptance criteria

The line of work may be closed when:

- all acceptance criteria in `plans/PPROXY_PARITY_NARROW_CORRECTIVE_PASS.md` pass;
- the final compatibility matrix is reviewed against current source and focused runtime tests;
- all Phase 1 parser/CLI acceptance cases pass, including canonical fixed-target and echo syntax;
- scheduler and per-remote rule behavior matches the oracle in representative cases;
- promoted native transports are reachable through compatibility entry points;
- a clean wheel provides functional top-level `pproxy` imports;
- public `Connection` and `Server` workflows pass local end-to-end tests;
- HTTP and SOCKS upstream server paths perform one handshake each;
- mandatory Phase 5 runtime gaps are implemented or accurately downgraded;
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
python -m pytest python/tests/test_pproxy_public_namespace.py
python -m pytest python/tests/test_pproxy_route_through.py
```

If shared runtime code changed across the corrective pass, run once:

```bash
cargo test --workspace --locked
```

Run the optional pproxy 2.7.9 closure scenarios from a clean local environment and record only a concise summary in the final compatibility document. A skipped external run is not a pass.

## Local verification record — 2026-08-03

- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test -p eggress-pproxy-compat` passed (288 tests).
- `cargo test --workspace --locked` passed (2,390 passed, 146 ignored).
- `maturin develop` completed, then `python/tests` and `tests/compat` passed
  (2,169 passed, 114 skipped, 5 existing warnings).
- Clean-wheel smoke passed for the bundled `eggress` and `pproxy` namespaces,
  absence of pip console scripts, Cargo binary version output, and manifest
  validation (148 capabilities).
- The optional external pproxy oracle suite was not executed because no local
  `pproxy==2.7.9` oracle environment was available; it remains outside routine
  hosted CI.

## Failure handling

If closure discovers a mismatch:

- fix it within the owning parser, translator, runtime, or Python adapter when the repair is local;
- downgrade the matrix entry to `supported_difference` when the difference is acceptable and documented;
- record intentional non-parity when repair violates the scope guardrails;
- do not create a new broad roadmap for a small defect;
- do not mark the phase complete from parser-only or construction-only tests.

## Handoff guidance

Complete the narrow corrective plan before resuming closure documentation. Then proceed in this order:

1. affected runtime and Python behavior tests;
2. focused optional oracle comparisons where available;
3. clean-wheel smoke;
4. practical matrix correction;
5. unsupported diagnostic review;
6. final closure status and claim review.

The desired result remains a smaller, clearer repository with accurate compatibility—not another certification apparatus.
