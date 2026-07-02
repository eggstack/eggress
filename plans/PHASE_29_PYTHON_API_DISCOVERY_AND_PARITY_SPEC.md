# Phase 29 Plan: Python API Discovery and Parity Specification

## Purpose

Phase 29 begins the Python/PyPI compatibility block. The repo already has Python bindings and pproxy-oriented helper functions, but the long-term goal is stronger: users should be able to use Eggress from Python as a practical Rust-backed replacement for common `pproxy` library workflows.

This phase does not implement the full drop-in API. It captures the real public Python API surface of `pproxy==2.7.9`, classifies each symbol and behavior, and creates an evidence-backed specification that later phases can implement without guessing.

## Goals

- Build a source-backed inventory of pproxy's Python library API.
- Distinguish public, semi-public, and internal APIs.
- Identify common embedded usage patterns from examples/docs/tests.
- Map pproxy objects/functions to existing Eggress Python/Rust concepts.
- Define target compatibility tiers for each Python API surface.
- Add initial Python API manifest entries and documentation.
- Create compatibility fixtures that later phases can execute against pproxy and Eggress.

## Non-goals

Do not implement full `pproxy.Server()` compatibility in this phase.

Do not rename the Eggress package or replace top-level imports yet.

Do not claim Python drop-in compatibility from documentation alone.

Do not support undocumented internals unless there is strong evidence users depend on them.

## Work items

### 29.1 Pin and vendor a Python API oracle fixture

Create a repeatable pproxy oracle environment.

Tasks:

- Pin `pproxy==2.7.9` in a Python compatibility requirements file.
- Add a small script that imports pproxy and records introspection output.
- Capture:
  - module-level attributes;
  - public classes;
  - public functions;
  - constructor signatures;
  - coroutine/sync function status;
  - docstrings where useful;
  - version metadata;
  - import side effects.

Suggested paths:

```text
python/compat/requirements-pproxy.txt
python/compat/inspect_pproxy_api.py
python/compat/fixtures/pproxy_api_snapshot_2_7_9.json
```

The snapshot should be generated deterministically enough for code review. Avoid embedding excessive implementation text.

### 29.2 Static source audit of pproxy API

Inspect pproxy source for public library surfaces that introspection may miss.

Document:

- `Server` class behavior;
- connection/server lifecycle APIs;
- parser helpers;
- URI helpers;
- protocol classes or constants exposed to users;
- plugin/cipher helpers exposed at module level;
- scheduler helpers;
- event-loop assumptions;
- error types;
- logging/stat APIs;
- CLI entry points that are importable.

Output:

```text
docs/python/PPROXY_API_INVENTORY.md
```

For each entry, include:

- symbol name;
- kind: class/function/constant/module;
- signature;
- async/sync semantics;
- target compatibility tier;
- Eggress mapping;
- notes/gaps;
- test fixture ID.

### 29.3 Classify API tiers

Use these Python API-specific tiers:

- **Drop-in target**: intended to match pproxy API shape closely enough for common code to run with minimal/no changes.
- **Shim-compatible**: Eggress exposes a compatibility wrapper but semantics differ in documented ways.
- **Eggress-native alternative**: no pproxy-shaped API; provide a clear replacement.
- **Intentional non-parity**: deliberately not implemented due to security, obsolete behavior, or architectural mismatch.
- **Unsupported**: not implemented yet, no final decision.

Add these tiers to Python docs and, where appropriate, map them to the broader compatibility manifest statuses.

### 29.4 Identify common embedded usage patterns

Collect likely user flows for pproxy as a library.

At minimum, document:

- starting a local proxy server from Python;
- creating a server with one or more listen URIs;
- creating chained upstream remotes;
- running in an existing asyncio loop;
- starting/stopping from a script;
- reading selected bind ports for ephemeral listeners;
- using auth/ciphers/schedulers;
- configuring PAC/static/helper behavior if exposed;
- using pproxy in test fixtures.

Add examples in:

```text
docs/python/PPROXY_EMBEDDED_USAGE_PATTERNS.md
```

Each pattern should include a pproxy example and an intended Eggress equivalent or gap.

### 29.5 Map pproxy lifecycle to Eggress runtime model

Create a lifecycle model that later implementation phases can use.

Map:

- pproxy object creation;
- async start;
- background tasks;
- listener readiness;
- graceful shutdown;
- signal handling;
- reload/config mutation;
- exception propagation;
- event loop ownership;
- process/thread ownership;
- logging and stats.

Output:

```text
docs/python/PYTHON_LIFECYCLE_PARITY.md
```

This document should explicitly define what it means for Eggress to run Rust networking from Python:

- blocking vs async entry points;
- background Tokio runtime ownership;
- Python asyncio interop boundaries;
- cancellation behavior;
- context-manager support;
- thread-safety rules;
- object lifetime and finalizer behavior.

### 29.6 Design compatibility fixture cases

Create fixture files for Python API parity tests.

Suggested path:

```text
tests/compat/fixtures/python_api_cases.toml
```

Each case should include:

```toml
[[cases]]
id = "server_http_listen_direct"
pproxy_code = "..."
eggress_target = "Server/listen/direct"
expected_tier = "drop_in_target"
requires_network = true
requires_external_pproxy = true
notes = "Starts HTTP proxy on ephemeral local port and relays TCP echo."
```

Initial cases:

- module import;
- version metadata;
- `Server` constructor shape;
- start/close lifecycle;
- HTTP listener;
- SOCKS5 listener;
- listen + remote chain;
- auth rejection;
- ephemeral port access;
- failure on unsupported scheme;
- cancellation/shutdown;
- repeated start/stop.

### 29.7 Add pproxy-vs-Eggress Python oracle test harness skeleton

Build a test harness skeleton without requiring full implementation yet.

Suggested paths:

```text
python/tests/compat/test_pproxy_api_oracle.py
python/tests/compat/oracle.py
```

Behavior:

- import real `pproxy` when `EGRESS_REQUIRE_PPROXY_PYTHON_API=1`;
- run selected fixture code against pproxy;
- run matching code against Eggress wrappers when implemented;
- record pass/fail/skip with clear status;
- skip gracefully when pproxy is missing or env gate is unset;
- produce enough output to promote features in the manifest later.

### 29.8 Update manifest for Python API surfaces

Add or refine entries in:

```text
tests/compat/pproxy_manifest.toml
```

Potential feature IDs:

```text
python_module_import
python_version_metadata
python_server_constructor
python_server_lifecycle_async
python_server_lifecycle_blocking
python_server_context_manager
python_listen_uri_api
python_remote_uri_api
python_chain_api
python_auth_api
python_stats_api
python_error_types
python_event_loop_integration
python_shutdown_semantics
```

Initial evidence should be conservative: mostly `unsupported`, `partial`, or `implemented_synthetic` unless an actual wrapper already passes tests.

### 29.9 Audit existing Eggress Python package

Inspect current Python package state and classify it against the API inventory.

Audit:

- `python/eggress/__init__.py`;
- `python/eggress/pproxy.py`;
- pyo3 exports in `crates/eggress-python/src/lib.rs`;
- existing pytest suite;
- wheel metadata;
- import path behavior;
- error type exposure;
- sync/async helper shape.

Output:

```text
docs/python/EGGRESS_PYTHON_API_CURRENT_STATE.md
```

Include a gap table and direct implementation notes for Phases 30-32.

### 29.10 Documentation and README updates

Add/update:

- `docs/python/README.md`;
- `docs/python/PPROXY_API_INVENTORY.md`;
- `docs/python/PPROXY_EMBEDDED_USAGE_PATTERNS.md`;
- `docs/python/PYTHON_LIFECYCLE_PARITY.md`;
- `docs/python/EGGRESS_PYTHON_API_CURRENT_STATE.md`;
- `docs/COMPATIBILITY_EVIDENCE.md`;
- `docs/PARITY_MATRIX.md`;
- README Python section.

Docs must state that Phase 29 is discovery/specification and does not imply full Python API parity.

## Validation commands

```bash
python -m pip install "pproxy==2.7.9"
python python/compat/inspect_pproxy_api.py --write python/compat/fixtures/pproxy_api_snapshot_2_7_9.json
python -m pytest python/tests
cargo test -p eggress-python
cargo test -p eggress-testkit manifest
```

If the Python API oracle harness is added:

```bash
EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/compat/test_pproxy_api_oracle.py -q
```

## Acceptance criteria

Phase 29 is complete when:

- A pinned pproxy API snapshot exists.
- Public/semi-public pproxy Python symbols are inventoried.
- Common embedded usage patterns are documented.
- Eggress current Python API gaps are documented.
- Python lifecycle semantics are specified.
- Python API fixture cases exist.
- Oracle harness skeleton exists and skips cleanly when gated dependencies are unavailable.
- Manifest has Python API entries with conservative evidence levels.
- Docs clearly state that full Python API compatibility is not yet implemented.

## Handoff notes

This phase is about removing ambiguity. Later implementation should not rely on memory of pproxy or guesses from CLI behavior. Treat `pproxy==2.7.9` as the oracle, and preserve all observed deviations in fixtures and docs.
