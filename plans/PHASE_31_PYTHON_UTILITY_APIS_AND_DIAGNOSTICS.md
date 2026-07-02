# Phase 31 Plan: Python Utility APIs, Diagnostics, and Test Fixtures

## Purpose

Phase 31 fills in the Python utility layer around the server lifecycle wrapper. pproxy users do not only start servers; they also parse/translate proxy URIs, inspect compatibility, run route/test helpers, and handle errors from application code.

This phase exposes Python APIs for those utility workflows while reusing the Rust compatibility parser, manifest classifications, diagnostics taxonomy, and config-generation code. The goal is to prevent Python compatibility from becoming a divergent implementation of the CLI.

## Scope

This phase covers:

- Python URI parsing and translation helpers.
- Python pproxy compatibility check helpers.
- Structured diagnostics and exception classes.
- Route/config explanation helpers.
- Generated TOML/config inspection helpers.
- Lightweight stats/status helpers for embedded servers.
- Shared fixture tests between CLI and Python helpers.
- Documentation and manifest updates for utility APIs.

## Non-goals

Do not implement full pproxy internals or protocol classes.

Do not expose unstable Rust internal types directly to Python.

Do not add separate Python parsers for proxy URI grammar when Rust already has a parser.

Do not implement system proxy manipulation or daemon helpers unless already supported by Rust and manifest evidence.

## Work items

### 31.1 Define Python utility API surface

Based on Phase 29 inventory and Phase 30 lifecycle implementation, define the initial stable helper API.

Potential functions:

```python
from eggress import pproxy

pproxy.translate_uri("socks5://127.0.0.1:1080")
pproxy.translate_args(["-l", "socks5://127.0.0.1:1080"])
pproxy.check_uri("ss://...")
pproxy.check_args(["-l", "...", "-r", "..."])
pproxy.redact_uri("http://user:pass@host:8080")
pproxy.explain_config(...)
pproxy.supported_features()
```

Define return types using dataclasses or pydantic-free simple classes:

```python
@dataclass(frozen=True)
class CompatibilityResult:
    tier: str
    feature_ids: tuple[str, ...]
    warnings: tuple[Diagnostic, ...]
    errors: tuple[Diagnostic, ...]
    generated_toml: str | None
```

Avoid new heavy Python dependencies.

### 31.2 Expose Rust compatibility parser to Python

Use PyO3 bindings to call the Rust URI/CLI parser and translator.

Requirements:

- Python helpers use the same Rust parser as CLI compatibility commands.
- Every structured diagnostic includes stable code, message, feature ID if known, tier, and suggestion if known.
- Credentials are redacted before crossing into Python exception strings unless the API explicitly returns structured secret-safe fields.
- Unsupported schemes produce the same classification as CLI.
- H2/WS/Raw remain protocol-crate-only/refused for runtime config unless a later phase changes that.

Tests should compare Python helper results against CLI fixture expectations.

### 31.3 Python diagnostics classes

Create a clean Python diagnostics model.

Suggested classes:

```python
@dataclass(frozen=True)
class Diagnostic:
    code: str
    message: str
    tier: str | None = None
    feature_id: str | None = None
    suggestion: str | None = None

class EggressError(Exception): ...
class CompatibilityError(EggressError): ...
class UnsupportedFeatureError(CompatibilityError): ...
class InvalidUriError(CompatibilityError): ...
class ConfigValidationError(EggressError): ...
```

Requirements:

- Exceptions can be serialized to dict/JSON.
- `str(exc)` is redacted.
- `repr(exc)` is redacted.
- Tests cover credentials in userinfo, Shadowsocks passwords, Trojan passwords, and query strings if any are parsed.

### 31.4 Shared fixture runner for CLI and Python

Add a Python test runner that reads existing CLI/URI fixtures.

Inputs:

```text
tests/compat/fixtures/pproxy_uri_corpus.toml
tests/compat/fixtures/pproxy_cli_cases/*.toml
```

Tests:

- Python `check_uri` matches corpus tier.
- Python `redact_uri` matches expected redacted display.
- Python `translate_args` generated TOML contains required fixture snippets.
- Unsupported fixtures produce expected diagnostic codes.
- Credential cases never leak secret values.

Suggested file:

```text
python/tests/test_pproxy_utility_fixtures.py
```

This reuses the Rust fixture corpus instead of duplicating expectations in Python.

### 31.5 Config explanation helpers

Expose helpers that let Python users inspect generated Eggress config.

Potential functions:

```python
pproxy.translate_args([...]).generated_toml
pproxy.explain_args([...])
pproxy.explain_uri("...")
```

The explanation should include:

- listeners generated;
- upstreams generated;
- route rules generated;
- compatibility tier;
- warnings/errors;
- unsupported/deferred features;
- security notes for reverse/plaintext and transparent privileges.

Do not expose raw internal Rust structs unless they are stable.

### 31.6 Embedded server status/stat helpers

If Phase 30 has a Python `Server`, expose minimal status methods.

Potential methods/properties:

```python
server.is_ready
server.addresses
server.listener_info()
server.metrics_text()
server.admin_snapshot()
server.stop_gracefully(timeout=...)
```

Requirements:

- stable simple Python return types;
- no accidental credential leaks;
- clear behavior before start/after close;
- tests with live server.

Do not overbuild a full admin client unless needed.

### 31.7 Route and upstream test helpers

Expose Python equivalents for common CLI utility checks if useful.

Potential helpers:

```python
pproxy.route_explain(config_or_args, target="example.com:443")
pproxy.test_upstream("socks5://127.0.0.1:1080")
```

Requirements:

- reuse Rust routing/config code;
- support JSON/dict output;
- deterministic diagnostics;
- no network call unless function name clearly implies it.

If the Rust CLI path is not cleanly reusable, document as deferred.

### 31.8 Manifest and evidence updates

Add/refine manifest entries:

```text
python_translate_uri
python_translate_args
python_check_uri
python_check_args
python_redact_uri
python_diagnostics_model
python_config_explain
python_server_status
python_metrics_text
python_route_explain
python_upstream_test
```

Evidence should be synthetic unless tested against pproxy's Python API or CLI oracle. Do not mark these compatible simply because they are useful Eggress APIs.

### 31.9 Documentation updates

Create/update:

```text
docs/python/UTILITY_APIS.md
docs/python/DIAGNOSTICS.md
docs/python/FIXTURE_COMPATIBILITY.md
docs/python/README.md
docs/COMPATIBILITY_EVIDENCE.md
docs/PARITY_MATRIX.md
README.md
```

Docs should include copy-paste examples for:

- translating pproxy args;
- checking support;
- catching unsupported feature errors;
- redacting URIs;
- starting a server and reading status;
- using fixture-based compatibility checks in downstream tests.

## Testing strategy

Python tests:

```bash
python -m pytest python/tests/test_pproxy_utility_fixtures.py -q
python -m pytest python/tests/test_pproxy_diagnostics.py -q
python -m pytest python/tests/test_server_lifecycle.py -q
```

Rust/PyO3 tests:

```bash
cargo test -p eggress-python
cargo test -p eggress-pproxy-compat
cargo test -p eggress-testkit corpus
cargo test -p eggress-testkit manifest
```

Gated pproxy oracle:

```bash
EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/compat/test_pproxy_api_oracle.py -q
```

## Acceptance criteria

Phase 31 is complete when:

- Python exposes URI/arg translation helpers backed by Rust compatibility code.
- Python exposes structured diagnostics and redacted exceptions.
- Python helper tests reuse the existing URI/CLI fixture corpus.
- Generated TOML/config explanations are inspectable from Python.
- Embedded server status helpers exist if Phase 30 lifecycle is implemented.
- Manifest and docs classify Python utility APIs conservatively.
- No secret-bearing URI fixture leaks credentials through Python string/repr/JSON outputs.

## Handoff notes

The main risk is parser drift. If Python helper outputs differ from CLI compatibility output, the Python layer will become a second compatibility product. Keep the Rust parser/classifier as the source of truth and make Python mostly a typed presentation layer.
