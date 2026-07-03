# Phase 29-32 Python Corrective Closure Plan

## Purpose

The first Python hardening pass improved claim discipline, fixed several GIL-release gaps, guarded `Server.run()` signal handling, and downgraded synthetic CLI evidence. It also left a concise set of known gaps.

This corrective closure pass is intentionally narrower than the previous hardening plan. It should finish the explicit remaining Python/PyPI cleanup items before new parity features resume.

## Current known gaps

The latest completion record lists these remaining gaps:

- `docs/PYTHON_BINDINGS.md` omits seven public API functions.
- `Server.config` is documented but not implemented.
- Twelve Python manifest entries have empty test arrays.
- `tests/compat/fixtures/python_api_cases.toml` is missing cases for Phase 30-31 APIs.
- `docs/SECURITY_REVIEW.md` lacks Python-specific analysis.
- Oracle gate naming remains partially inconsistent: docs still reference `EGRESS_REQUIRE_PPROXY_ORACLE`, while plans preferred `EGRESS_REQUIRE_PPROXY_PYTHON_API`.
- Wheel smoke tests check imports and `py.typed`, but stronger proof is needed that the smoke command tests an installed wheel outside the source tree.

## Non-goals

Do not add top-level `import pproxy` compatibility.

Do not implement pproxy protocol class/cipher/plugin APIs.

Do not publish to PyPI.

Do not add major networking features.

Do not promote Python API surfaces to pproxy behavioral parity without oracle tests.

## Work items

### C1. Fix `docs/PYTHON_BINDINGS.md` public API drift

Audit the public Python package exports and update `docs/PYTHON_BINDINGS.md`.

Tasks:

- Compare `python/eggress/__init__.py.__all__` against `docs/PYTHON_BINDINGS.md`.
- Compare `python/eggress/pproxy.py` public helpers against docs.
- Add missing documentation for:
  - `explain_config_toml`;
  - `explain_pproxy_args`;
  - `explain_pproxy_uri`;
  - `route_explain`;
  - `check_upstream`;
  - `compatibility_version`;
  - `supported_features` or any other exported helper missing from docs.
- Add return shape examples for explanation helpers.
- Mark network-performing helpers clearly, especially `check_upstream`.
- Add a short “API stability” note distinguishing stable public wrappers from lower-level native exports.

Acceptance:

- `docs/PYTHON_BINDINGS.md` lists every public API exported through `eggress.__all__` and `eggress.pproxy`.
- No public helper is documented only in code docstrings.

### C2. Resolve `Server.config` documentation drift

`Server.config` is documented but not implemented. Pick one path and make docs/code agree.

Preferred implementation:

- Store the translated or supplied `EggressConfig` on `Server` as `self._config`.
- Add:

```python
@property
def config(self):
    """The EggressConfig used by this Server."""
    return self._config
```

- Ensure returned config is safe to inspect and does not expose secrets except through existing config APIs.
- Add tests:
  - `Server(config=cfg).config is cfg` or documented equivalent;
  - `Server(listen=[...]).config.redacted_toml()` works;
  - config remains available after close.

Alternative:

- Remove all docs references to `Server.config` if not implemented.

Acceptance:

- No docs mention a missing `Server.config` property.
- Tests cover whichever behavior is chosen.

### C3. Add test arrays or rationale for Python manifest entries

Twelve Python manifest entries currently have empty test arrays. Empty tests are acceptable for intentional non-parity only when the rationale is explicit, but concrete implemented Python surfaces should have tests.

Tasks:

- Audit Python entries in `tests/compat/pproxy_manifest.toml`.
- For implemented/supported Python entries, add relevant test names.
- For intentional non-parity entries, add `divergence` rationale and either:
  - a diagnostic/rejection test; or
  - a docs-only rationale marker accepted by manifest validation.
- Candidate test mappings:
  - import/version → `test_import_eggress`, `test_version_metadata`;
  - URI translation → `test_pproxy_utility_fixtures` cases;
  - diagnostics → `test_pproxy_diagnostics`;
  - server lifecycle → `test_server_lifecycle` cases;
  - wheel/import → `test_wheel_import_smoke` cases;
  - import non-shadowing → `test_no_pproxy_shadow`, `test_eggress_pproxy_coexists`.
- If a manifest entry is purely future/deferred, classify it as unsupported/deferred instead of supported.

Acceptance:

- No supported Python manifest entry has an empty test array.
- Intentional non-parity Python entries have explicit divergence rationale and at least docs/test evidence.

### C4. Expand `python_api_cases.toml` for Phase 30-31 APIs

The fixture file should cover the implemented Python server lifecycle and utility helpers.

Add cases for:

- `eggress.pproxy.Server` constructor from listen URI.
- `Server.start()` / `Server.close()`.
- `Server.astart()` / `Server.aclose()`.
- sync context manager.
- async context manager.
- `Server.run()` main-thread restriction.
- `Server.config` if implemented.
- `Server.addresses`, `is_ready`, `listener_info`, `metrics_text`.
- `explain_config_toml`.
- `explain_pproxy_args`.
- `explain_pproxy_uri`.
- `route_explain`.
- `check_upstream`.
- `compatibility_version`.
- `supported_features`.
- `diagnostics_for_uri`.
- `redact_pproxy_uri`.

Each case should include:

- feature id;
- expected tier;
- test command;
- whether network is required;
- whether pproxy oracle is required;
- whether behavior is pproxy-compatible, functional equivalent, or Eggress-native.

Acceptance:

- Phase 30 and 31 public APIs have fixture cases.
- Fixture cases map to manifest feature IDs.

### C5. Extend fixture validation for `python_api_cases.toml`

Make `python_api_cases.toml` mechanically enforced.

Tasks:

- Extend `crates/eggress-testkit/src/corpus.rs` or add a Python API fixture validator.
- Validate required fields for every Python API case:
  - id;
  - feature_id;
  - category;
  - expected_tier;
  - evidence_level;
  - test_command or deferred rationale;
  - pproxy_oracle_required boolean;
  - network_required boolean.
- Reject duplicate ids.
- Validate `expected_tier` against allowed values.
- Validate every `feature_id` exists in `tests/compat/pproxy_manifest.toml`.
- Validate every Python manifest feature has at least one fixture or explicit rationale.
- Add tests for validator success and failure.

Acceptance:

- `cargo test -p eggress-testkit python_api_cases` or equivalent validates the fixture file.
- Python API fixture drift can no longer pass silently.

### C6. Normalize Python oracle gate naming

Pick a single canonical environment variable and support a compatibility alias if useful.

Recommended:

- Canonical: `EGRESS_REQUIRE_PPROXY_PYTHON_API=1`.
- Legacy alias: `EGRESS_REQUIRE_PPROXY_ORACLE=1` accepted for now.

Tasks:

- Update `python/tests/test_pproxy_oracle.py` to accept both env vars if currently gated.
- Update skip messages to mention the canonical name first.
- Update docs:
  - `docs/COMPATIBILITY_EVIDENCE.md`;
  - `docs/python/README.md`;
  - `docs/python/RELEASE_CHECKLIST.md`;
  - `docs/python/MIGRATION_FROM_PPROXY.md`;
  - README Python section if applicable.
- Add a test for gate detection helper if practical.

Acceptance:

- User-facing docs consistently show `EGRESS_REQUIRE_PPROXY_PYTHON_API=1`.
- Old `EGRESS_REQUIRE_PPROXY_ORACLE=1` continues to work or is explicitly removed everywhere.

### C7. Add Python-specific security review section

Update `docs/SECURITY_REVIEW.md` with Python embedding risks.

Cover:

- untrusted pproxy URI input;
- open-proxy risk when binding non-loopback addresses;
- `Server.run()` signal handling and main-thread restriction;
- lifecycle cleanup and background service handles;
- GIL release and concurrent handle access;
- FFI/panic boundary assumptions;
- credential redaction in exceptions, repr, diagnostics, generated TOML, metrics, and logs;
- reverse proxy plaintext auth risks;
- wheel/supply-chain risks;
- upstream pproxy coexistence and import-shadowing avoidance.

Add explicit safe defaults:

- docs examples should prefer `127.0.0.1` or Unix sockets;
- non-loopback binds should be labeled deliberate;
- reverse proxy examples should include auth/allowlist notes.

Acceptance:

- Python embedding has a dedicated security review subsection.
- The section references relevant tests or docs where possible.

### C8. Strengthen wheel smoke to prove installed artifact behavior

The smoke tests are useful, but the script must prove imports come from the installed wheel, not the repo checkout.

Tasks:

- Update `scripts/test_wheel.sh` to:
  - build a wheel;
  - create a fresh temp virtualenv;
  - install the wheel;
  - run tests from a temp directory outside the repository;
  - unset or sanitize `PYTHONPATH`;
  - print `eggress.__file__` and assert it points inside the venv/site-packages;
  - assert `Path.cwd()` is not the repo root;
  - run `python/tests/test_wheel_import_smoke.py` by path or copy it into temp dir.
- Extend `test_wheel_import_smoke.py` to assert source-tree imports are not used when an env var such as `EGRESS_EXPECT_INSTALLED_WHEEL=1` is set.
- Verify `py.typed` marker from installed package resources.

Acceptance:

- Wheel smoke fails if it accidentally imports from the source tree.
- Smoke validates installed artifact contents and namespace behavior.

### C9. Add docs-example tests for Python quickstarts

Move the most important docs examples into executable tests or ensure equivalent tests exist.

Examples to cover:

- README Python quickstart.
- `docs/python/IMPORT_STRATEGY.md` import coexistence example.
- `docs/python/MIGRATION_FROM_PPROXY.md` translation example.
- `docs/python/SERVER_LIFECYCLE_COMPATIBILITY.md` sync context manager example.
- `docs/python/INSTALLATION.md` import smoke.

Implementation options:

- doctest where examples are simple;
- pytest examples file for networking cases;
- static check that examples use loopback binds unless explicitly marked.

Acceptance:

- Core Python docs examples are backed by tests or tested equivalents.

### C10. Update Python release checklist with corrective checks

Update `docs/python/RELEASE_CHECKLIST.md` to include this corrective closure.

Add required checks:

- `cargo test -p eggress-testkit python_api_cases` or equivalent;
- `scripts/test_wheel.sh` installed-artifact smoke;
- `python -m pytest python/tests/test_server_lifecycle.py`;
- `python -m pytest python/tests/test_pproxy_utility_fixtures.py`;
- `EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/test_pproxy_oracle.py` when oracle is available;
- docs/evidence taxonomy audit.

Acceptance:

- Release checklist reflects actual current validation, not aspirational CI.

### C11. Final completion record

Create:

```text
docs/PHASE_29_32_PYTHON_CORRECTIVE_CLOSURE_COMPLETION.md
```

Record:

- every gap closed;
- any gap deliberately deferred;
- test commands run;
- wheel smoke result;
- manifest/fixture validator result;
- remaining Python/PyPI gaps.

Acceptance:

- Completion record is explicit about what is still not pproxy parity.

## Validation commands

Rust:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-python
cargo test -p eggress-testkit manifest
cargo test -p eggress-testkit corpus
cargo test -p eggress-testkit python_api_cases
```

Python:

```bash
maturin develop
python -m pytest python/tests/test_pproxy_compat.py -q
python -m pytest python/tests/test_pproxy_diagnostics.py -q
python -m pytest python/tests/test_pproxy_utility_fixtures.py -q
python -m pytest python/tests/test_server_lifecycle.py -q
python -m pytest python/tests/test_config_explain.py -q
python -m pytest python/tests/test_wheel_import_smoke.py -q
python -m compileall python/eggress
```

Wheel:

```bash
scripts/test_wheel.sh
```

Oracle:

```bash
python -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/test_pproxy_oracle.py -q
```

## Acceptance criteria for corrective closure

This corrective pass is complete when:

- `docs/PYTHON_BINDINGS.md` matches public exports.
- `Server.config` documentation drift is resolved by code or docs.
- Supported Python manifest entries have tests.
- `python_api_cases.toml` covers Phase 30-31 APIs.
- Python API cases are mechanically validated and mapped to manifest entries.
- Oracle gate naming is consistent.
- Python-specific security review exists.
- Wheel smoke proves installed artifact behavior outside the source tree.
- Main Python docs examples have test coverage or tested equivalents.
- Release checklist includes corrective checks.
- Completion record documents remaining gaps honestly.

## Expected remaining gaps after this pass

- Top-level `import pproxy` remains deliberately absent.
- Protocol/cipher/plugin class APIs remain deferred.
- Full pproxy Python behavioral parity remains partial and oracle-gated.
- Production PyPI publishing remains manual/deferred.

## Handoff notes

This is a closure pass, not a new feature phase. Prefer deleting or downgrading overclaims over adding new surface area. The goal is for future readers to trust every Python/PyPI compatibility claim without having to inspect the implementation manually.
