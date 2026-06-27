# Phase 16 Detailed Plan: Python Library Parity with Common `pproxy` Use Cases

## Purpose

Phase 16 makes the Python package useful as a practical replacement for Python users who currently run or embed `pproxy`. The goal is not to reimplement pproxy in Python. The goal is to expose migration helpers and high-level Python APIs that configure and control the Rust Eggress runtime.

Python callers should be able to:

- translate pproxy-style arguments or URIs;
- start an Eggress service from that translation;
- inspect warnings/unsupported features;
- use sync and async-friendly lifecycle patterns;
- proxy local traffic through Rust;
- access metrics/status/reload;
- avoid subprocess management for common cases.

---

# Prerequisites

Required:

- Phase 13 Rust embed API complete;
- Phase 14 Python bindings complete;
- Phase 15 wheel/package workflow complete or at least local wheel build works;
- Phase 8 pproxy compatibility translator exists in Rust;
- corrective parity audit complete, including Shadowsocks TCP downgrade.

---

# Non-goals

Do not implement:

- new proxy protocols;
- standard Shadowsocks TCP rework;
- public-internet-dependent tests;
- OS/system proxy installation;
- daemon/service manager integration;
- Python-side proxy forwarding;
- unbounded pproxy CLI compatibility beyond documented supported subset.

---

# Workstream 1: Python pproxy translation helpers

## Goal

Expose Rust pproxy-compat translation through Python.

## API sketch

```python
from eggress import translate_pproxy_args, EggressConfig, EggressService

result = translate_pproxy_args([
    "-l", "socks5://127.0.0.1:1080",
    "-r", "http://proxy:8080",
])

print(result.toml)
print(result.warnings)
print(result.unsupported)
```

## Types

```python
@dataclass(frozen=True)
class TranslationWarning:
    category: str
    message: str

@dataclass(frozen=True)
class UnsupportedFeature:
    feature: str
    message: str

@dataclass(frozen=True)
class TranslationResult:
    toml: str
    warnings: list[TranslationWarning]
    unsupported: list[UnsupportedFeature]

    @property
    def ok(self) -> bool: ...

    def config(self) -> EggressConfig: ...
```

## Required functions

```python
def translate_pproxy_args(args: Sequence[str]) -> TranslationResult: ...
def translate_pproxy_uri(local: str, remotes: Sequence[str] = ()) -> TranslationResult: ...
def check_pproxy_args(args: Sequence[str]) -> TranslationResult: ...
```

## Acceptance criteria

- Python can translate supported pproxy-style arguments without invoking CLI subprocesses.
- Unsupported features are structured, not just printed strings.
- Credentials are redacted in warnings/errors.

---

# Workstream 2: Start service from pproxy-style arguments

## Goal

Expose a convenience API for common migration cases.

## API sketch

```python
svc = EggressService.from_pproxy_args([
    "-l", "socks5://127.0.0.1:0",
    "-r", "http://proxy:8080",
])

with svc.start() as handle:
    print(handle.bound_addresses)
```

Alternative direct helper:

```python
with eggress.start_pproxy(args) as handle:
    ...
```

## Behavior

- returns warnings/unsupported features before start if caller requests check mode;
- refuses to start if unsupported required features exist, unless `allow_partial=True` is explicitly passed;
- generated TOML can be retrieved for debugging;
- service lifecycle uses Rust handle from Phase 14.

## Acceptance criteria

- Python users can replace common `pproxy` subprocess startup with an in-process Eggress service.

---

# Workstream 3: Python sync and async lifecycle ergonomics

## Goal

Make library usage natural for both synchronous scripts and async applications.

## Sync API

```python
with EggressService.from_toml(toml).start() as handle:
    ...
```

## Async API

If Phase 14 did not add native async wrappers, add a conservative async facade that delegates blocking work to threads:

```python
async with EggressService.from_toml(toml).astart() as handle:
    metrics = await handle.metrics_text_async()
```

Do not integrate Rust runtime into Python event loop directly unless the design is audited.

## Requirements

- no deadlocks with `asyncio`;
- no leaked runtime thread on exceptions;
- shutdown is deterministic;
- errors propagate as Python exceptions.

## Tests

- sync context manager shutdown;
- async context manager shutdown;
- exception inside context still shuts down;
- repeated metrics/status calls while traffic active.

## Acceptance criteria

- Sync and async examples work.

---

# Workstream 4: Python tests mirroring common pproxy examples

## Goal

Prove common pproxy migration flows work from Python.

## Test file

```text
python/tests/test_pproxy_compat.py
```

## Required scenarios

1. Local SOCKS5 direct.
2. Local HTTP CONNECT direct.
3. Local SOCKS4 direct if Python test client exists.
4. Local SOCKS5 through HTTP upstream.
5. Local SOCKS5 through SOCKS5 upstream.
6. Multiple `-r` remotes default to round-robin in generated TOML.
7. Auth success.
8. Auth failure.
9. Unsupported SSH returns structured unsupported feature.
10. Shadowsocks TCP warning/downgrade is visible if translated.
11. UDP direct through SOCKS5 if Python helper exists.

## Rule

Tests should use local echo fixtures only. No public internet.

## Acceptance criteria

- Python pproxy compatibility tests run without pproxy installed for translation/runtime cases.
- Differential tests against pproxy remain optional/gated.

---

# Workstream 5: Optional gated Python differential tests against pproxy

## Goal

Compare the Python package’s pproxy helper behavior with real pproxy where feasible.

## Environment gate

```bash
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1
```

## Python version guidance

Use a version known to run pproxy, likely Python 3.11 or 3.12. Do not rely on Python 3.14.

## Scenarios

- pproxy local SOCKS5 direct vs Eggress Python helper local SOCKS5 direct;
- pproxy local HTTP direct vs Eggress Python helper local HTTP direct;
- multiple remote scheduler behavior if practical;
- auth failure class.

## Acceptance criteria

- Gated differential tests exist or docs explain why Rust-side differentials are sufficient.
- Unrun gated tests are not counted as compatibility proof.

---

# Workstream 6: Python API docs and examples

## Required docs

Update:

```text
docs/PYTHON_BINDINGS.md
python/README.md
README.md
```

Create examples:

```text
python/examples/start_socks5.py
python/examples/pproxy_translate.py
python/examples/pproxy_run.py
python/examples/reload_config.py
python/examples/async_service.py
```

## Required examples

- start service from TOML;
- start service from pproxy args;
- print generated TOML;
- inspect warnings/unsupported features;
- context-manager lifecycle;
- metrics/status;
- reload;
- async usage.

## Acceptance criteria

- Docs distinguish supported, partial, experimental, and intentional non-parity features.

---

# Workstream 7: Python security and redaction tests

## Goal

Ensure Python ergonomics do not leak credentials.

## Tests

- `repr(TranslationResult)` redacts credentials;
- `repr(EggressConfig)` redacts credentials;
- exception messages redact credentials;
- generated TOML may contain credentials only when explicitly requested/expected;
- warnings mention plaintext TOML credentials without printing the secret;
- metrics/status do not expose credentials.

## Acceptance criteria

- Python layer preserves Rust redaction guarantees.

---

# Workstream 8: Concurrency and multi-service behavior

## Goal

Clarify and test how Python users can run services.

## Tests

- start two services on port 0 if supported;
- independent bound addresses;
- one service shutdown does not kill the other;
- concurrent `metrics_text()` calls;
- concurrent reload/status calls if safe;
- thread-start/shutdown smoke test.

If multiple services are not supported, document the limitation and enforce a clear error.

## Acceptance criteria

- Multi-service behavior is explicit and tested.

---

# Workstream 9: Documentation of known non-parity

## Goal

Do not let Python package marketing overstate pproxy parity.

## Required statements

- Shadowsocks TCP is experimental/non-standard;
- Shadowsocks UDP support exists but external interop may require gated verification;
- no inbound Shadowsocks listener unless later implemented;
- no legacy stream ciphers;
- no plugin transports;
- no transparent proxy/redir;
- no pproxy daemon mode;
- repeated remotes default to pproxy-like round-robin group semantics;
- direct fallback requires explicit config.

## Acceptance criteria

- Python docs match Rust parity matrix.

---

# Recommended commit sequence

1. Bind pproxy translation result types to Python.
2. Add `translate_pproxy_args` / `translate_pproxy_uri` Python APIs.
3. Add `EggressService.from_pproxy_args` and optional `start_pproxy` helper.
4. Add sync/async lifecycle wrappers.
5. Add Python pproxy-compat tests.
6. Add security/redaction/concurrency tests.
7. Add examples/docs/completion record.

---

# Required verification

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p eggress-pproxy-compat
cargo test -p eggress-python
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
maturin develop
python -m pytest python/tests
python -m mypy python/eggress
python -m ruff check python
```

Optional/gated:

```bash
EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 python -m pytest python/tests/test_pproxy_differential.py
```

---

# Definition of done

Phase 16 is complete only when:

1. Python exposes pproxy translation helpers.
2. Python can start Eggress from pproxy-style args.
3. Translation result includes structured warnings and unsupported features.
4. Common pproxy migration examples work from Python.
5. Sync and async lifecycle examples work or limitations are documented.
6. Python tests cover common pproxy-style direct/upstream/auth scenarios.
7. Security/redaction tests pass.
8. Multi-service/concurrency behavior is tested or explicitly unsupported.
9. Docs clearly state non-parity and experimental features.
10. Rust and Python checks pass locally.

## Completion record

Add:

```text
docs/PHASE_16_PYTHON_PPROXY_LIBRARY_PARITY_COMPLETION.md
```

Include APIs added, examples, tests, non-parity caveats, and blockers for Phase 17 release-candidate audit.
