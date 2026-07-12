# Phase C1 — Python public API contract freeze

## Objective

Create an executable, versioned contract for the public Python API exposed by pinned `pproxy==2.7.9`. The contract must inventory symbols, signatures, defaults, coroutine behavior, exceptions, object attributes, representations, and lifecycle semantics. It becomes the source of truth for Track C and prevents import-only compatibility from being mistaken for drop-in behavior.

## Dependencies

- A1 canonical parity classifications.
- A2 composition and feature IDs.
- A3 oracle/reporting infrastructure.
- Existing Eggress Python package, stubs, service APIs, and test matrix.

## Scope

Inventory at minimum:

- top-level `pproxy` exports;
- public submodules;
- classes and constructors;
- functions and coroutine functions;
- constants, registries, and aliases;
- protocol/cipher factory objects;
- `Connection` and `Server` surfaces;
- exception hierarchy;
- documented callbacks and plugin hooks;
- context manager and async context manager behavior;
- attributes created after construction/start;
- `repr`, `str`, equality, and truthiness where observable;
- argument acceptance, defaults, positional/keyword behavior, and errors.

## Workstream 1: extractor

Build a deterministic extractor that installs/imports pinned pproxy in an isolated environment and records:

- module and qualified symbol name;
- symbol kind;
- `inspect.signature` output;
- coroutine/generator/async-generator classification;
- class bases and MRO;
- public methods/properties/descriptors;
- constants and safe serializable values;
- docstring-derived hints only as non-authoritative metadata;
- import aliases and identity relationships.

Avoid executing arbitrary networking behavior during extraction. Store the generated contract under `python/compat/` or `docs/python/` with a schema version and pproxy version.

## Workstream 2: behavioral probes

Add controlled probes for surfaces that static inspection cannot capture:

- constructor-created attributes;
- default object state;
- exceptions for invalid arguments;
- start/close/wait behavior;
- callback invocation shape;
- coroutine return types;
- idempotence of close/wait;
- object representations with secrets redacted;
- registry lookup and aliases.

Run probes in isolated child processes with timeouts and bounded output.

## Workstream 3: classification

Classify every public entry as:

- `exact_target`: must match directly;
- `adapted_target`: same use case via compatibility wrapper with documented semantic mapping;
- `unsupported_release_blocker`: required for strict Python drop-in parity;
- `intentional_non_parity`: only with explicit rationale and release naming impact;
- `internal_observed`: publicly reachable but not intended as stable API.

Each entry must reference an Eggress implementation location or planned phase.

## Workstream 4: contract validator

Create tests that compare Eggress’s compatibility module against the contract:

- symbol presence;
- import paths and aliases;
- signatures and defaults;
- coroutine classification;
- exception inheritance;
- method/property availability;
- selected behavioral probes;
- stub/runtime agreement.

Support an allowlist of explicit divergences with stable IDs, rationale, owner phase, and expiration/review status. Reject undocumented drift.

## Workstream 5: packaging and namespace strategy

Decide and document how Eggress provides the compatibility namespace:

- `import pproxy` from the Eggress wheel;
- optional separate distribution if namespace conflict is unsafe;
- executable entry point ownership;
- coexistence behavior when original pproxy is installed;
- version metadata identifying Eggress compatibility implementation;
- import order and editable-install behavior.

Test clean environments and conflict environments. Never silently shadow an installed original package without documented packaging behavior.

## Deliverables

- versioned machine-readable API contract;
- extractor and behavioral probe runner;
- classification report mapped to Track C phases;
- runtime contract tests;
- `.pyi`/runtime consistency checks;
- namespace/import decision record;
- CI job against supported Python versions.

## Acceptance criteria

- Every public pproxy symbol is inventoried and classified.
- Signatures, defaults, coroutine status, aliases, and exception bases are machine-tested.
- Existing Eggress compatibility helpers are mapped honestly rather than treated as replacements for missing low-level objects.
- Missing `Connection`, `Server`, protocol, cipher, and plugin contracts are explicit release blockers.
- Generated contract is reproducible from pinned pproxy.
- CI rejects unreviewed API drift and stale stubs.
- Reports contain no secrets or unstable memory addresses.

## Out of scope

- implementing missing objects;
- full network behavioral equivalence;
- performance optimization;
- latest-pproxy compatibility beyond a non-blocking comparison lane.

## Recommended commit sequence

1. schema and extractor;
2. behavioral probes;
3. generated pinned contract;
4. classification mapping;
5. Eggress validator/tests;
6. namespace decision and packaging tests;
7. CI and documentation.