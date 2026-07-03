# Phase 29-32 Python Hardening Plan: API Truthfulness, Runtime Safety, and Packaging Evidence

## Purpose

Phases 29-32 established the first serious Python/PyPI compatibility block: a pproxy API inventory, frozen pproxy snapshots, Python compatibility fixtures, a pproxy-shaped `Server` wrapper, Python utility APIs, diagnostics, packaging/import docs, wheel smoke tests, and import-collision policy.

The implementation is now useful, but the current shape needs a hardening pass before it should be treated as release-grade. This pass focuses on verifying PyO3 safety claims, tightening lifecycle semantics, ensuring packaging evidence is real, cleaning up compatibility-tier language, and preventing `eggress.pproxy` from becoming a divergent or overclaimed replacement for upstream `pproxy`.

## Current observed state

Recent Python work added or heavily changed:

- `crates/eggress-python/src/lib.rs` with substantial PyO3 exports.
- `python/eggress/pproxy.py` with translation helpers, diagnostics, `Server`, config explanation, route explanation, and upstream checks.
- `python/eggress/__init__.py` with public exports and `start_pproxy()` convenience behavior.
- `python/tests/test_server_lifecycle.py`.
- `python/tests/test_pproxy_diagnostics.py`.
- `python/tests/test_pproxy_utility_fixtures.py`.
- `python/tests/test_pproxy_oracle.py`.
- `python/tests/test_wheel_import_smoke.py`.
- `python/tests/test_config_explain.py`.
- `tests/compat/fixtures/pproxy_api_snapshot.json`.
- `tests/compat/fixtures/python_api_cases.toml`.
- `tests/compat/requirements-pproxy.txt`.
- `docs/python/*` API, lifecycle, packaging, import, migration, and release docs.
- `docs/adr/ADR_python_import_and_distribution_strategy.md`.
- expanded Python entries in `tests/compat/pproxy_manifest.toml` and evidence docs.

The broad shape is good. The immediate hardening need is precision: verify strong claims, demote overclaims, and ensure tests exercise the actual wheel/import/runtime behavior users will rely on.

## Non-goals

Do not add a top-level `pproxy` shim package.

Do not publish to production PyPI in this pass.

Do not implement deferred pproxy internals such as protocol class access, cipher class access, or plugin APIs.

Do not expand protocol functionality. This is a Python surface hardening pass, not a networking feature phase.

Do not claim pproxy Python behavioral parity unless a gated oracle test demonstrates comparable behavior against `pproxy==2.7.9`.

## Work items

### P1. Normalize Python compatibility tier language

There is current wording drift: the Phase 29 completion doc labels some Eggress-native features as Tier A / exact match even while noting that upstream pproxy lacks those features.

Tasks:

- Audit:
  - `docs/PHASE_29_PYTHON_API_PARITY_COMPLETION.md`;
  - `docs/python/PPROXY_API_INVENTORY.md`;
  - `docs/python/EGRESS_PYTHON_API_CURRENT_STATE.md`;
  - `docs/python/PYTHON_LIFECYCLE_PARITY.md`;
  - `docs/COMPATIBILITY_EVIDENCE.md`;
  - `docs/PARITY_MATRIX.md`;
  - `tests/compat/pproxy_manifest.toml`.
- Replace misleading terms such as `exact match` where the feature is actually Eggress-native or a functional equivalent.
- Use Python-specific tiers consistently:
  - `drop_in_target` for pproxy-shaped APIs intended to mimic pproxy usage;
  - `functional_equivalent` for Eggress APIs that solve the same task differently;
  - `eggress_native` for features pproxy lacks;
  - `partial` for incomplete compatibility;
  - `deferred` for deliberately postponed surfaces;
  - `intentional_non_parity` for rejected surfaces.
- Add a short tier legend to `docs/python/README.md`.
- Update the completion doc to distinguish discovery findings from implemented compatibility.

Acceptance:

- No doc claims context managers, hot reload, structured errors, or granular diagnostics are exact pproxy matches.
- Python docs clearly separate pproxy compatibility from Eggress-native enhancements.

### P2. Reclassify synthetic/Eggress-native evidence in compatibility docs

Some compatibility-evidence rows still use `Compatible` for synthetic or Eggress-native behavior. This weakens the taxonomy.

Tasks:

- Audit `docs/COMPATIBILITY_EVIDENCE.md` for Python and CLI rows using `Compatible` with only synthetic evidence.
- For features that are useful but not pproxy behavioral parity, reclassify as `Supported`, `Eggress-native`, or `Intentional non-parity` as appropriate.
- In `tests/compat/pproxy_manifest.toml`, ensure `compatible` status is reserved for behavior with pproxy differential or equivalent oracle evidence.
- Add or strengthen manifest validator rules if needed:
  - `egress_status = "compatible"` should not pair with `evidence_level = "implemented_synthetic"` unless explicitly allowed for non-pproxy surfaces under a different category.
  - Python API entries should include a `category = "python_api"` or equivalent so their evidence semantics are clear.
- Add a docs note that `eggress.pproxy` helper compatibility may be pproxy-shaped without being drop-in parity.

Acceptance:

- Compatibility docs no longer conflate Eggress-native behavior with pproxy parity.
- Manifest and docs agree on Python API status.

### P3. PyO3 GIL release and thread-safety audit

The current docs state that all blocking Rust calls use `py.detach()` and that handles are safe for concurrent access. Verify or correct this.

Tasks:

- Audit `crates/eggress-python/src/lib.rs` for every method that can block:
  - config loading from file;
  - TOML parsing/compilation;
  - service start;
  - shutdown;
  - reload;
  - status snapshot;
  - metrics rendering;
  - route explanation;
  - upstream connectivity test;
  - pproxy translation if it can perform filesystem/network work.
- Confirm each blocking path uses `py.detach()` or equivalent GIL release.
- Add comments or helper wrappers documenting why each path is safe.
- Verify Rust types behind `EggressHandle` are `Send + Sync` if docs say methods are safe from multiple Python threads.
- Add Rust compile-time assertions where useful.
- If any method is not thread-safe, document it and protect it with Python/Rust synchronization.
- Add Python tests that call status/metrics from multiple threads while the service runs.
- Add tests that call shutdown concurrently and verify idempotence/no panics.

Acceptance:

- GIL/thread-safety docs match implementation.
- Blocking PyO3 methods are either detached or documented as non-blocking.
- Concurrent shutdown/status behavior is tested.

### P4. Server lifecycle edge-case hardening

The `eggress.pproxy.Server` wrapper is useful but must have precise lifecycle behavior.

Tasks:

- Audit `python/eggress/pproxy.py::Server` for:
  - start after close;
  - double start;
  - double close;
  - async start after sync start;
  - sync close after async start;
  - exception during start;
  - exception inside context manager body;
  - object deletion without close;
  - readiness timing;
  - address availability before and after stop.
- Add tests in `python/tests/test_server_lifecycle.py` for each case.
- Decide and document whether a `Server` can be restarted after `close()`.
- Ensure `Server.run()` is documented as main-thread-only because Python signal registration must occur in the main thread.
- Add a test or guard that `Server.run()` raises a clear error when invoked from a non-main thread.
- Ensure `SIGTERM` registration is platform-gated for Windows if needed.
- Confirm signal handlers are always restored even if `start()` fails after partial initialization.

Acceptance:

- Lifecycle behavior is deterministic and documented.
- Signal handling is main-thread-safe and platform-aware.
- Repeated close is idempotent; repeated start behavior is explicit.

### P5. `Server.run()` and signal behavior review

The current `Server.run()` directly installs SIGINT/SIGTERM handlers. This should be hardened separately.

Tasks:

- Add explicit main-thread check using `threading.current_thread() is threading.main_thread()`.
- On platforms without SIGTERM, handle gracefully.
- Ensure the previous signal handlers are restored if start succeeds, if start fails, and if signal handling setup fails.
- Decide whether `KeyboardInterrupt` should propagate or be swallowed.
- Document exact behavior in `docs/python/SERVER_LIFECYCLE_COMPATIBILITY.md`.
- Add tests using monkeypatch where practical rather than sending process signals in normal pytest.

Acceptance:

- No hidden Python signal API footguns.
- `run()` is safe as a convenience function and not misrepresented as a general async lifecycle primitive.

### P6. Python exception and diagnostic consistency

There are native exceptions, Python wrapper exceptions, diagnostic dataclasses, and unsupported-feature errors. Make the model coherent.

Tasks:

- Audit all exception classes in:
  - `crates/eggress-python/src/lib.rs`;
  - `python/eggress/exceptions.py`;
  - `python/eggress/pproxy.py`;
  - `python/eggress/__init__.py`.
- Ensure `UnsupportedFeatureError` raised by `Server` is the same exported class users are expected to catch.
- Add `.code`, `.feature_id`, `.tier`, and `.suggestion` fields where practical for compatibility errors.
- Ensure `str(exc)` and `repr(exc)` redact credentials.
- Add tests for secrets in:
  - HTTP userinfo;
  - SOCKS5 userinfo;
  - Shadowsocks passwords;
  - Trojan passwords;
  - reverse auth URIs;
  - query strings or plugin fragments if parsed.
- Ensure diagnostic JSON/dict serialization never leaks secrets.

Acceptance:

- A single documented exception hierarchy exists.
- Python diagnostics are redaction-safe and test-covered.

### P7. Python utility helper correctness and drift prevention

The Python utilities should remain a presentation layer over Rust, not a divergent parser.

Tasks:

- Confirm these helpers call Rust/native implementation rather than Python reimplementations:
  - `translate_pproxy_args`;
  - `translate_pproxy_uri`;
  - `check_pproxy_args`;
  - `check_pproxy_uri`;
  - `redact_pproxy_uri`;
  - `diagnostics_for_uri`;
  - `explain_pproxy_args`;
  - `explain_pproxy_uri`;
  - `route_explain`;
  - `check_upstream`.
- Add tests that compare Python output against CLI/fixture corpus for at least all URI corpus cases and CLI golden cases.
- Ensure `check_pproxy_uri` never raises for invalid input if its docstring promises that.
- Ensure `redact_pproxy_uri` behavior is documented if it raises on invalid input.
- Add type annotations for all public helpers and return dataclasses.

Acceptance:

- Python utility behavior is fixture-backed and source-of-truth remains Rust.

### P8. Oracle test naming and gating consistency

Current planning used `EGRESS_REQUIRE_PPROXY_PYTHON_API`; implementation appears to use `EGRESS_REQUIRE_PPROXY_ORACLE`. Pick one convention or document aliases.

Tasks:

- Standardize env var names across:
  - `python/tests/test_pproxy_oracle.py`;
  - docs/python files;
  - compatibility evidence docs;
  - README;
  - workflow files;
  - plan/completion docs.
- Recommended canonical name:
  - `EGRESS_REQUIRE_PPROXY_PYTHON_API=1` for Python API oracle tests;
  - optionally accept `EGRESS_REQUIRE_PPROXY_ORACLE=1` as legacy alias.
- Ensure skipped tests report clear skip reasons.
- Ensure real upstream pproxy import is not shadowed by `eggress.pproxy`.
- Add an import coexistence test:
  - install/import upstream `pproxy` if available;
  - import `eggress.pproxy` as `eggress_pproxy`;
  - assert module identities differ.

Acceptance:

- Gated Python oracle behavior is predictable and consistently documented.

### P9. Wheel smoke and import-collision hardening

Phase 32 added docs and smoke tests; now make the smoke evidence concrete.

Tasks:

- Verify wheel build output includes:
  - `eggress/__init__.py`;
  - `eggress/pproxy.py`;
  - `eggress/exceptions.py`;
  - `eggress/config.py`;
  - `eggress/service.py`;
  - native `eggress/_eggress.*` extension;
  - `py.typed` if advertised.
- Add test that installs built wheel into a fresh virtualenv and runs `python/tests/test_wheel_import_smoke.py` against the installed package, not the source tree.
- Ensure test removes repo root from `PYTHONPATH` or runs outside repo to avoid source-tree imports.
- Add import collision check:
  - `import eggress` works;
  - `import eggress.pproxy` works;
  - `import pproxy` fails if upstream pproxy is absent and succeeds only if upstream is installed;
  - Eggress never supplies top-level `pproxy`.
- Update `scripts/test_wheel.sh` to enforce these checks.

Acceptance:

- Wheel smoke verifies installed artifact behavior, not local source imports.
- Import strategy is mechanically tested.

### P10. Packaging metadata and release checklist audit

The packaging docs are now broad. Confirm metadata matches.

Tasks:

- Audit `python/pyproject.toml` and `crates/eggress-python/pyproject.toml` for consistency:
  - package name;
  - version source;
  - Python version requirement;
  - classifiers;
  - license;
  - README path;
  - package data;
  - maturin config;
  - abi3 policy if any.
- Ensure docs do not advertise wheel targets that workflows do not build.
- Ensure `docs/python/RELEASE_CHECKLIST.md` includes local verification alternatives because hosted GitHub Actions may be unavailable.
- Run or document `twine check` for wheel/sdist metadata.
- Verify source distribution includes enough files to build but excludes generated/build artifacts and secrets.

Acceptance:

- Packaging docs and metadata agree.
- Release checklist does not imply CI success where only local checks are available.

### P11. Python test reliability and isolation

New Python tests include networking and service lifecycle behavior. Make them deterministic.

Tasks:

- Audit new tests for fixed ports; use port 0 wherever possible.
- Ensure every test tears down services even on failure.
- Add pytest fixtures for local echo servers and service lifecycle cleanup.
- Avoid sleep-based readiness waits where possible; poll readiness with timeout.
- Mark slow/network tests explicitly if needed.
- Ensure tests run in parallel only if safe; otherwise document serial requirement.
- Ensure Windows/macOS platform differences are skipped with explicit reasons.

Acceptance:

- Python tests are isolated, deterministic, and do not leave background services or bound ports.

### P12. Python docs example verification

Docs now contain many code examples. Keep them from drifting.

Tasks:

- Extract or duplicate major docs examples into tests:
  - `docs/python/INSTALLATION.md` import smoke;
  - `docs/python/MIGRATION_FROM_PPROXY.md` translation example;
  - `docs/python/SERVER_LIFECYCLE_COMPATIBILITY.md` context-manager example;
  - `docs/python/IMPORT_STRATEGY.md` import coexistence example;
  - README Python quickstart.
- Use doctest if practical, or normal pytest examples.
- Ensure examples avoid real external network calls.

Acceptance:

- Main docs examples are executable or tested equivalents exist.

### P13. Manifest validator expansion for Python API cases

The repo now has `tests/compat/fixtures/python_api_cases.toml`. Validate it mechanically.

Tasks:

- Extend `eggress-testkit` fixture validation to load `python_api_cases.toml`.
- Validate required fields:
  - id;
  - category;
  - pproxy behavior/source;
  - Eggress behavior/status;
  - expected tier;
  - manifest feature id;
  - test command or deferred rationale.
- Reject duplicate IDs.
- Ensure every Python API fixture maps to a manifest entry.
- Ensure every Python manifest entry maps back to at least one fixture or explicit docs-only rationale.

Acceptance:

- Python API cases cannot drift as passive docs.

### P14. `start_pproxy()` and `Server` API coherence

The repo exposes both `start_pproxy()` and `eggress.pproxy.Server`. Ensure they behave consistently.

Tasks:

- Audit `python/eggress/__init__.py::start_pproxy` and `python/eggress/pproxy.py::Server`.
- Ensure both use the same translation path.
- Ensure both honor `allow_partial` consistently.
- Ensure unsupported features raise the same exception class and redacted message.
- Ensure both expose comparable handle/address/status behavior.
- Add tests comparing `start_pproxy([...])` and `Server(listen=..., remote=...).start()` for simple cases.

Acceptance:

- Users do not get different behavior depending on which Python convenience entry point they use.

### P15. Security review for Python embedding

Embedding a network service from Python has security-specific docs and defaults.

Tasks:

- Update `docs/SECURITY_REVIEW.md` with a Python embedding subsection covering:
  - untrusted URI input;
  - credential redaction;
  - reverse proxy plaintext risks;
  - binding to non-loopback addresses;
  - signal handling;
  - lifecycle cleanup;
  - supply-chain/wheel install risks;
  - upstream pproxy coexistence.
- Ensure `Server` examples default to loopback binds.
- Ensure docs warn when examples bind `:1080` or `0.0.0.0`.
- Add tests or static checks that examples use loopback unless explicitly labeled.

Acceptance:

- Python embedding docs do not encourage accidental open proxy exposure.

### P16. Final validation and completion record

Run focused validation and document results.

Baseline Rust:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-python
cargo test -p eggress-testkit manifest
cargo test -p eggress-testkit corpus
```

Python local development:

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

Wheel smoke:

```bash
scripts/test_wheel.sh
```

Gated oracle:

```bash
python -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/test_pproxy_oracle.py -q
```

Create completion record:

```text
docs/PHASE_29_32_PYTHON_HARDENING_COMPLETION.md
```

Completion record must include:

- what was downgraded;
- what was verified;
- what remains synthetic/specification-only;
- wheel smoke result;
- oracle gate result;
- thread/GIL audit result;
- remaining Python API gaps.

## Acceptance criteria for this hardening pass

The pass is complete when:

- Python compatibility tier language is internally consistent.
- Synthetic/Eggress-native evidence is not labeled as pproxy behavioral parity.
- PyO3 GIL/thread-safety claims are verified or corrected.
- `Server` lifecycle edge cases are tested and documented.
- `Server.run()` signal behavior is main-thread-safe and documented.
- Exception/diagnostic classes are coherent and redaction-safe.
- Python utility helpers are fixture-backed and reuse Rust parser/classifier code.
- Python oracle gate naming is consistent.
- Wheel smoke tests verify installed artifacts and import collision behavior.
- Packaging metadata/docs/release checklist agree.
- Python tests are deterministic and isolated.
- `python_api_cases.toml` is mechanically validated and mapped to manifest entries.
- `start_pproxy()` and `Server` have consistent semantics.
- Python embedding security notes are updated.

## Expected remaining gaps after hardening

- Full top-level `import pproxy` replacement remains deferred.
- pproxy protocol class/cipher/plugin Python APIs remain deferred unless separately prioritized.
- True pproxy Python API behavioral differential coverage remains partial.
- Production PyPI publishing remains manual/deferred until release credentials and final policy are confirmed.

## Handoff notes

The guiding rule is simple: `eggress.pproxy` may be a compatibility layer, but it is not upstream `pproxy`. Keep imports explicit, evidence conservative, tests artifact-based, and docs honest about what is drop-in, what is equivalent, and what is Eggress-native.
