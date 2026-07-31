# Phase 0 — pproxy Contract and Source-of-Truth Reset

## Status

Proposed.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Objective

Make the repository's compatibility claims describe the current implementation, package layout, and actual pproxy 2.7.9 behavior before further code is added.

This phase is intentionally documentation- and manifest-heavy but process-light. It must correct existing sources of truth rather than create another compatibility framework.

## Why this phase is required

Current repository material contains mutually incompatible claims, including:

- a separate top-level `pproxy` compatibility distribution that was removed;
- compatibility-manifest entries that claim H2, WS/WSS, raw, and tunnel are reachable through the translator even though current translator allowlists reject them;
- inconsistent descriptions of rule-file support;
- Python symbols classified as drop-in based only on importability or structure;
- old full-parity plans that require SSH, QUIC/H3, SSR, legacy ciphers, large certification matrices, and extensive retained evidence despite the project's current bounded scope;
- CLI descriptions of `--get`, `--pac`, and `--test` that do not match pproxy 2.7.9 argument behavior.

Implementation work performed against these contradictory documents is likely to close the wrong gaps.

## In scope

- `README.md` compatibility and limitation statements;
- `docs/PPROXY_PARITY_SPEC.md` and parity-related documentation under `docs/parity/`;
- `docs/PARITY_MATRIX.md`;
- `docs/PYTHON_BINDINGS.md` and Python migration/install documentation;
- `docs/parity/pproxy_capability_manifest.toml`;
- `docs/parity/pproxy_2_7_9_strict_manifest.toml` only where its current records contradict package or code state;
- old roadmap status and cross-references;
- a concise, human-readable current gap table.

## Out of scope

- new runtime features;
- parser changes;
- new test harnesses;
- new CI workflows;
- regenerating or preserving large historical evidence bundles;
- deleting historical completion records unless they actively mislead current contributors;
- changing intentional non-parity decisions.

## Required source hierarchy

Use this order when sources disagree:

1. Observable behavior from a clean `pproxy==2.7.9` installation.
2. Current Eggress source code.
3. Current focused tests.
4. Current user-facing README and reference documentation.
5. Generated or historical parity reports.
6. Old completion documents and plans.

Historical plans must never override current source.

## Workstream 0.1 — Record the current compatibility boundary

Create or update one concise section in `docs/parity/README.md` that states:

- the frozen target is pproxy 2.7.9;
- Eggress currently ships one Python distribution named `eggress`;
- current users import the compatibility module as `eggress.pproxy` until Phase 4 restores the top-level namespace;
- the native runtime is broader than the current compatibility translator;
- practical parity is the active target;
- SSH, QUIC/H3, SSR, legacy Shadowsocks, plugin replication, daemonization, and cross-session reuse are not part of the active roadmap.

Do not describe planned behavior as already implemented.

## Workstream 0.2 — Correct the capability manifest

Audit each capability entry that is affected by the current findings. At minimum, correct:

- `python.importable_package` and all references to the deleted compatibility distribution;
- H2, WS, WSS, raw, and tunnel translator/CLI layers;
- `cli.get` semantics;
- `cli.pac` argument shape;
- `cli.test` argument shape;
- default scheduler behavior;
- rule-file and per-remote rule behavior;
- no-argument mixed-listener behavior;
- Python `Connection` and `Server` compatibility status;
- structural protocol/cipher facades that are not wire-functional through their Python methods.

Use only these practical states:

- `matched` — behavior matches the oracle for the defined scenario;
- `partial` — usable but observably different;
- `missing` — not implemented;
- `intentional_non_parity` — explicitly excluded by the active roadmap;
- `native_extension` — Eggress feature with no pproxy equivalent.

If the existing schema cannot accept these literal states without broad validator work, retain its current vocabulary but map entries honestly. Do not redesign the schema in this phase.

## Workstream 0.3 — Reconcile the strict manifest

The strict manifest may remain as an inventory, but it must not call structural-only symbols drop-in. Update the small set of records affected by current package changes:

- top-level `pproxy` namespace is absent from the current wheel;
- `pproxy.server`, `pproxy.proto`, and `pproxy.cipher` are not installed as top-level modules;
- `eggress.Connection` is not pproxy's `Connection` contract;
- `ProxyConnection` uses a different sync/async shape and stream object;
- Python protocol classes that are construction-only must remain structural or partial;
- removed package paths and tests must not be referenced as current evidence.

Do not expand this into a complete re-inventory of private pproxy internals.

## Workstream 0.4 — Correct user-facing documentation

Update the README and migration docs so they consistently say:

- common HTTP/SOCKS and modern encrypted-proxy workflows are supported;
- strict full drop-in parity is not yet achieved;
- `import pproxy` is not currently available from the Eggress wheel;
- unsupported transports and legacy families are deliberate exclusions;
- known ordinary compatibility gaps are URI grammar, CLI arity/semantics, routing defaults/rules, compatibility bridge wiring, and Python public API behavior.

Remove stale examples that install or import the deleted compatibility package.

## Workstream 0.5 — Mark the active plan line

Add a short supersession note to relevant old roadmap documents or their index references:

- `plans/PPROXY_FULL_DROP_IN_ROADMAP.md` is historical/maximal;
- `plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md` governs current execution;
- old milestone plans remain historical and are not acceptance gates for the bounded roadmap.

Do not rewrite every historical plan.

## Suggested implementation order

1. Read current package metadata and translator source.
2. Correct `docs/parity/README.md`.
3. Correct the main capability manifest.
4. Correct only directly affected strict-manifest records.
5. Align README and Python documentation.
6. Add supersession references to the old roadmap line.
7. Run the existing manifest validator and focused documentation searches.

## Acceptance criteria

Phase 0 is complete when all of the following are true:

- no current document claims a separate compatibility distribution exists;
- no current document claims top-level `import pproxy` works before Phase 4;
- H2/WS/WSS/raw/tunnel are described as native-runtime-complete but compatibility-bridge-missing where that remains true;
- `--get`, `--pac`, and `--test` descriptions match pproxy 2.7.9;
- default scheduler behavior is described accurately;
- per-remote rule routing is not marked complete before Phase 2;
- Python `Connection` and `Server` are not called drop-in based on aliases or importability;
- intentional exclusions are listed consistently;
- the active roadmap is unambiguous;
- the existing manifest validator still passes, or any validator adjustment is narrowly limited to corrected records.

## Lightweight verification

Use existing tools only:

```bash
python3 scripts/validate_pproxy_parity_manifest.py \
  docs/parity/pproxy_capability_manifest.toml

rg -n "eggress-pproxy-compat|pip install pproxy|real pproxy namespace" \
  README.md docs plans

rg -n "--get|--pac|--test|round-robin|first-available" \
  README.md docs/parity docs/PARITY_MATRIX.md
```

Review search results manually; historical completion documents may retain old statements if clearly marked historical.

## Handoff notes

This phase should be one small documentation/manifest commit or a few logically separated commits. Avoid touching runtime code. When uncertainty remains about an oracle behavior, add one tiny probe script or run an isolated Python command; do not build generalized observation infrastructure.
