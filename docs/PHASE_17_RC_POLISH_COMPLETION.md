# Phase 17 RC Polish Completion Record

## Summary

Phase 17 RC polish addressed narrow correctness-boundary issues identified in the release-candidate audit. This pass did not add new proxy protocols, rewrite Shadowsocks TCP, expand parity claims, or begin GA hardening. It made the existing RC boundary precise, predictable, and easier to maintain.

## Status: Complete

## Commits

All changes in this pass are documentation corrections, code comments, and Python test additions. No functional proxy code changes.

### Files Modified

- `crates/eggress-embed/src/lib.rs` — Added thread ownership documentation to `EggressHandle` struct doc and `Drop` impl
- `docs/EMBED_API.md` — Expanded lifecycle section with thread ownership model and shutdown behavior
- `python/eggress/__init__.py` — Version now sourced from native module (`_eggress.__version__`) with fallback
- `python/eggress/service.py` — No code changes (lifecycle already correct)
- `python/README.md` — Added lifecycle management guidance and shutdown idempotency note
- `python/tests/test_service.py` — Added version-match, double-shutdown, and exception-context-manager tests
- `docs/PYTHON_BINDINGS.md` — Added lifecycle management section, mypy typing debt note
- `docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md` — Fixed evidence taxonomy wording; split RC/GA blockers
- `docs/PHASE_17_TRUE_PPROXY_PARITY_RELEASE_CANDIDATE_COMPLETION.md` — Split RC/GA blockers; updated go/no-go
- `docs/PYPI_RELEASE.md` — Clarified authoritative build path; added TestPyPI status
- `python/pyproject.toml` — Added comment marking as local-dev convenience only
- `AGENTS.md` — Updated embed lifecycle and Python binding architecture facts; added new doc link
- `README.md` — Added Phase 17 RC polish doc link

### Files Created

- `docs/PHASE_17_RC_POLISH_COMPLETION.md` — This file

## Workstream Decisions

### 1. Embed Runtime Ownership

**Decision:** Document the existing thread model rather than simplify.

**Rationale:** The two-path model (async vs blocking) is correct and well-tested. Simplifying would risk regressions. Thread ownership is now documented in code (`EggressHandle` struct doc, `Drop` impl doc) and in `docs/EMBED_API.md`.

**Thread model:**
- Async path: Tokio blocking-pool thread + `"eggress-embed-rt"` OS thread owns `ServiceSupervisor::run()`
- Blocking path: outer `"eggress-embed-rt"` thread (startup only, terminates) + inner `"eggress-embed-run"` thread owns `run()`
- `Drop` performs best-effort join (5-second timeout on async path)
- No extra orchestration thread remains after startup in either path

### 2. Python Lifecycle Semantics

**Decision:** Reinforce explicit lifecycle management in docs and tests. No code changes needed — `shutdown()` is already idempotent, `__exit__` calls shutdown and returns `False`, `__aexit__` awaits shutdown and returns `False`.

**Added tests:**
- `test_version_matches_native_module` — version derived from native module
- `test_shutdown_is_idempotent` — double shutdown is safe
- `test_context_manager_on_exception` — context manager shuts down on exception

### 3. Packaging Entrypoint

**Decision:** `crates/eggress-python/pyproject.toml` is the authoritative build entrypoint. `python/pyproject.toml` is a local-dev convenience only.

**Rationale:** All workflows, scripts, and release docs already use `crates/eggress-python` consistently. Added clarifying comment to `python/pyproject.toml` and updated `docs/PYPI_RELEASE.md`.

### 4. Version Source of Truth

**Decision:** Python `__version__` imports from the native module's `CARGO_PKG_VERSION` export, with a fallback to `"0.1.0"` if the native module is not available.

**Rationale:** Prevents version drift between Rust and Python surfaces. Static metadata versions in `pyproject.toml` files still require manual alignment at release time; the release checklist should verify this.

### 5. RC Evidence Taxonomy

**Decision:** Corrected wording to use explicit evidence labels. No unrun gated tests are treated as proof.

**Changes:**
- "verified by differential or runtime tests" → "Evidence labels indicate verification method"
- Feature table evidence column already had per-row labels; wording now matches

### 6. RC vs GA Blocker Wording

**Decision:** Split "Release blockers: None" into pre-release RC blockers (none) and GA blockers (6 items).

**GA blocker list:**
- Hosted CI must run successfully or have a documented release fallback
- TestPyPI install must be verified
- At least core pproxy differential tests should run or be explicitly scoped out
- Shadowsocks UDP interop should hard-fail on failure if claimed standard-compatible
- Formal wheel install tests on supported platforms
- Security residuals (mTLS, protocol detection timeout, global connection limit) triaged for GA

### 7. Workflow and Wheel-Script Paths

**Status:** Already consistent. All workflows (`python-test.yml`, `python-wheels.yml`, `publish-pypi.yml`) and `scripts/test_wheel.sh` use `crates/eggress-python` as the working directory for maturin builds. No changes needed.

### 8. Mypy/Typing Debt

**Decision:** Accept as known pre-release typing debt.

**Rationale:** PyO3 native types (`_inner` attribute) are invisible to mypy, producing ~20 expected false-positive errors. This is documented in `docs/PYTHON_BINDINGS.md` limitations section. A future release will add `.pyi` stubs or type wrappers.

### 9. TestPyPI/Package-Name Status

**Decision:** Documented as pending in `docs/PYPI_RELEASE.md`. The package name `eggress` must be reserved on TestPyPI before the first pre-release upload.

## Verification Commands Run

| Command | Status |
|---------|--------|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |

## Python Verification

| Command | Status |
|---------|--------|
| `maturin build --release` | PASS |
| `pip install dist/eggress-*.whl` | PASS |
| `python -m pytest python/tests` | PASS (including new tests) |

## Remaining Pre-release RC Blockers

None identified.

## Remaining GA Blockers

- Hosted CI must run successfully or have a documented release fallback
- TestPyPI install must be verified
- At least core pproxy differential tests should run or be explicitly scoped out
- Shadowsocks UDP interop should hard-fail on failure if claimed standard-compatible
- Formal wheel install tests on supported platforms
- Security residuals triaged for GA (mTLS, protocol detection timeout, global connection limit, regex DoS, rate limiting, credential rotation)
- Mypy/typing: add `.pyi` stubs or type wrappers for PyO3 native types
