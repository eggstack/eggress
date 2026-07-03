# Phase 29-32: Python Hardening and Evidence Closure — Completion Record

## Summary

P1-P15 audit pass across Python bindings, documentation, evidence classification, and security posture. Sixteen work items completed; five code fixes applied; one clippy warning resolved. All Rust and Python checks pass.

## Audit Results

### P1: Tier Language Normalization
- `docs/PHASE_29_PYTHON_API_PARITY_COMPLETION.md`: Relabeled Eggress-only features from "Tier A" to "Eggress-native (not in pproxy)"
- `docs/python/PYTHON_LIFECYCLE_PARITY.md`: Changed 4 rows from `**A** (Eggress-native feature)` to `**Eggress-native** — no pproxy equivalent`; added to parity legend
- `docs/python/README.md`: Added Eggress-native tier row (count 4) with explanatory note

### P2: Evidence Reclassification
- `tests/compat/pproxy_manifest.toml`: Changed 4 Eggress-only features (`python_api_config_reload`, `python_api_error_hierarchy`, `python_api_context_manager`, `python_api_gil_release`) from `egress_status = "supported"` to `intentional_non_parity`
- `docs/COMPATIBILITY_EVIDENCE.md`: Downgraded all 20+ CLI Compatible+Synthetic rows to Supported; added header note explaining CLI features verify eggress behavior, not pproxy parity
- `docs/PARITY_MATRIX.md`: Changed legend from `Eggress-only` to `Eggress-native = not in pproxy (not a parity claim)`

### P3: GIL Release and Thread-Safety Audit
- **3 GIL-holding methods fixed** in `crates/eggress-python/src/lib.rs`:
  - `parse_toml_config` (L430): TOML parsing wrapped in `py.detach()`
  - `route_explain` (L715): TOML+compile+route wrapped in `py.detach()` with custom error type
  - `test_upstream_connect` (L772): DNS+TCP connect wrapped in `py.detach()`
- All 11 other blocking methods already used `py.detach()`
- `EggressHandle` is auto `Send + Sync` via Arc-wrapped fields; no unsafe impls

### P4: Server Lifecycle Edge-Case Audit
- All lifecycle transitions verified OK: start-after-close, double-start, double-close, async/sync mixing, exception during start, context manager body throws, object deletion
- Readiness: Rust polls at 10ms intervals up to 30s, so `start()` returns only when ready

### P5: Server.run() Signal Handling
- Added main-thread guard: `RuntimeError` if called from non-main thread
- Wrapped SIGTERM signal installation in try/except to restore SIGINT handler on partial failure
- Signal handlers always restored in finally block

### P6: Exception and Diagnostic Consistency
- Exception hierarchy: PASS — all 7 Rust variants map correctly
- `exceptions.py` re-exports: PASS
- `AlreadyStartedError` consistency: PASS
- **Fixed**: Added 5 missing functions to `__all__` in `__init__.py`: `explain_config_toml`, `explain_pproxy_args`, `explain_pproxy_uri`, `route_explain`, `check_upstream`

### P7: Utility Helper Correctness
- All 9 utility functions verified correct with tests
- Missing coverage noted: `describe_reverse_pproxy_uri`, `compatibility_version()`

### P8: Oracle Test Gating
- **Fixed**: Removed dead `ORACLE_REQUIRED` variable from `test_pproxy_oracle.py`
- Docstring corrected to describe actual auto-detection gating mechanism

### P9: Wheel Smoke Tests
- **Fixed**: Added `test_py_typed_marker_exists` to smoke suite
- Import collision coverage verified

### P10: Packaging Metadata
- Both `pyproject.toml` files consistent (version, classifiers, license)
- Note: version `0.1.0` hardcoded in both files — drift risk (documented, not fixed)

### P11: Test Reliability
- All ports use `:0` (OS-assigned)
- Good cleanup via context managers and try/finally
- No shared mutable state between tests

### P12: Documentation Accuracy
- Example files verified present (5 files)
- `EGRESS_PYTHON_API_CURRENT_STATE.md`: **Fixed** `__all__` count from 25 to 36

### P13: Manifest Validator
- 12 Python entries with empty test arrays documented as known gaps
- `validate_manifest()` accepts empty tests for `intentional_non_parity` entries

### P14: API Coherence
- `start_pproxy()` and `Server` APIs verified coherent
- `config` property documented but not implemented on `Server` — noted as documentation bug

### P15: Security Review
- Python-specific attack surfaces documented (GIL release, concurrent handles, resource exhaustion, panic propagation, FFI boundary)
- No critical issues found; all reviewed surfaces acceptably hardened

## Code Changes

| File | Change |
|------|--------|
| `crates/eggress-python/src/lib.rs` | Wrap `parse_toml_config`, `route_explain`, `test_upstream_connect` in `py.detach()` for GIL release |
| `crates/eggress-python/src/lib.rs` | Fix clippy redundant closure warning |
| `python/eggress/__init__.py` | Add 5 missing functions to `__all__` |
| `python/tests/test_pproxy_oracle.py` | Remove dead `ORACLE_REQUIRED` variable, fix docstring |
| `python/tests/test_wheel_import_smoke.py` | Add `test_py_typed_marker_exists` |
| `docs/python/EGRESS_PYTHON_API_CURRENT_STATE.md` | Update `__all__` count 25→36 |

## Verification

```bash
cargo check --workspace                          # passes
cargo fmt --all -- --check                       # clean
cargo clippy --workspace --all-targets -- -D warnings  # no issues
rtk pytest python/tests/ -v                      # 548 passed
```

## Known Gaps (Deferred)

- `PYTHON_BINDINGS.md` omits 7 public API functions (documentation gap, not code)
- `Server.config` property documented but not implemented
- 12 manifest Python entries have empty test arrays
- `python_api_cases.toml` missing fixture cases for Phase 30-31 APIs
- Security review lacks Python-specific analysis in `SECURITY_REVIEW.md`
