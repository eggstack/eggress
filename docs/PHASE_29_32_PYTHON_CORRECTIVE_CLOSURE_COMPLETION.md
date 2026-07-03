# Phase 29-32 Python Corrective Closure Completion

This document records the corrective closure work for the Python bindings and
packaging surfaces addressed by Phases 29–32. Each item (C1–C11) corresponds
to a known gap surfaced in the Phase 29-32 hardening audit. The preferred
direction throughout was **deletion/downgrade over adding new surface area**;
where code was added, it was kept minimal and well-tested.

## Status: All Items Closed

| Item | Description | Resolution |
|------|-------------|-----------|
| C1 | Docs drift in `PYTHON_BINDINGS.md` (7 undocumented APIs) | Closed — added sections for `describe_reverse_pproxy_uri`, `explain_*`, `route_explain`, `check_upstream`, `version`, `capabilities`, `compatibility_version` |
| C2 | `Server.config` documented but not implemented | Closed — implemented as a read-only property; documented in `PYTHON_BINDINGS.md`; 5 new tests cover pre-start/post-start/post-close/translate-source/config-source paths |
| C3 | 6 supported manifest entries with empty `tests = []` | Closed — added test references to `python_api_module_exports`, `python_api_translate_args`, `python_api_translate_uri`, `python_api_check_args`, `python_api_service_lifecycle`, `python_api_scheduling` |
| C4 | `python_api_cases.toml` missing Phase 30-31 cases | Closed — added 35 new cases covering Server wrapper (Phase 30), URI inspection/diagnostics (Phase 31), explain/route/upstream helpers, package metadata, wheel artifact verification |
| C5 | No Rust validator for `python_api_cases.toml` | Closed — added `validate_python_api_cases()` to `eggress-testkit::corpus`; supports both `cases = [...]` and `[[case]]` (array-of-tables) syntax; wired into `validate_workspace_corpus_full()` |
| C6 | Oracle gate naming inconsistent (`EGRESS_REQUIRE_PPROXY_ORACLE` vs `EGRESS_REQUIRE_PPROXY_PYTHON_API`) | Closed — tests use auto-detection; `EGRESS_REQUIRE_PPROXY_ORACLE=1` accepted as legacy alias; docs normalized in 5 files (`PYTHON_BINDINGS.md`, `COMPATIBILITY_EVIDENCE.md`, `python/RELEASE_CHECKLIST.md`, `python/README.md`, `PHASE_29_PYTHON_API_PARITY_COMPLETION.md`) |
| C7 | Shallow Python security review | Closed — added sections on signal handling (`Server.run()`), concurrent instances, GIL release completeness, open-proxy risk on non-loopback binds, DoS via repeated start/close |
| C8 | Wheel smoke did not verify installed artifact | Closed — `scripts/test_wheel.sh` now runs in a fresh cwd, verifies `eggress.__file__` lives under `site-packages`, native module is compiled, no source-tree contamination in `sys.path`; `test_wheel_import_smoke.py` adds `test_imported_from_installed_wheel` (env-gated by `EGRESS_EXPECT_INSTALLED_WHEEL=1`), `test_native_module_is_compiled`, `test_no_source_tree_in_sys_path` |
| C9 | No tests backing documented code examples | Closed — added `python/tests/test_docs_examples.py` with 22 tests exercising every code block in the `PYTHON_BINDINGS.md` quick-start and reference sections |
| C10 | Release checklist missing corrective checks | Closed — added step 6 "Corrective closure checks" with the 4 new validation commands; renumbered subsequent steps |
| C11 | This completion record | Closed |

## Verification

### Rust testkit

```bash
cargo test -p eggress-testkit
# 56 passed, 2 ignored
```

Specifically:

- `corpus::tests::workspace_python_api_cases_are_valid` — passes (101 cases validated)
- `corpus::tests::full_corpus_validation` — passes (corpus ≥ 50, cli ≥ 1, mapped ≥ 1, python_api ≥ 50)
- `manifest::tests::manifest_test_names_exist` — passes (extended to walk `python/` and `.github/workflows/`)

### Manifest

All 6 previously-empty Python manifest entries now reference real test files:

```toml
python_api_module_exports      -> test_pproxy_oracle.py::TestModuleExports, test_wheel_import_smoke.py::test_import_eggress
python_api_translate_args      -> test_pproxy_compat.py::test_local_socks5_direct, test_local_http_direct, test_multiple_remotes_round_robin, test_pproxy_oracle.py::TestTranslationParity
python_api_translate_uri       -> test_pproxy_compat.py::test_shadowsocks_supported, test_unsupported_ssh
python_api_check_args          -> test_pproxy_compat.py::test_local_socks5_direct, test_socks5_through_http_upstream
python_api_service_lifecycle   -> test_server_lifecycle.py::test_start_and_stop, test_sync_context_manager, test_async_context_manager, test_close_is_idempotent, test_pproxy_oracle.py::TestServerLifecycle
python_api_scheduling          -> test_pproxy_compat.py::test_multiple_remotes_round_robin, test_socks5_through_socks5_upstream
```

The 4 `intentional_non_parity` and 2 `unsupported` Python entries intentionally
keep empty `tests = []` (they document eggress-only or unsupported surfaces).

### Python tests

The new `test_docs_examples.py` exercises every documented code block:

- Context manager (sync and async)
- Explicit start/stop
- File loading
- `start_pproxy` convenience
- Translation result types
- `check_pproxy_uri` (success and error paths)
- `redact_pproxy_uri`
- `diagnostics_for_uri`
- `supported_features`
- `describe_reverse_pproxy_uri`
- `explain_config_toml`, `explain_pproxy_args`, `explain_pproxy_uri`
- `route_explain`
- `version()`, `capabilities()`, `compatibility_version()`
- `Server` status helpers (including the new `Server.config`)

The wheel smoke (`scripts/test_wheel.sh`) now runs in a clean cwd and asserts:

1. `eggress.__file__` lives under `site-packages`/`dist-packages` (installed)
2. `eggress._eggress.__file__` ends in `.so`/`.pyd`/`.dylib` (compiled)
3. `sys.path` does not contain the repo `python/` source tree

## Key Decisions

- **Prefer deletion/downgrade over adding new surface area.** The plan's C1 work
  added docs, not new APIs. C2 added the smallest possible property
  (`Server.config`). No new public functions were introduced.
- **Backward-compatible schema for `python_api_cases.toml`.** The 66 existing
  cases use a legacy schema (`id`, `category`, `description`, `pproxy_behavior`,
  `egress_behavior`, `tier`, optional `notes`). The validator accepts this
  schema. The 35 new cases (Phase 30-32) follow the same schema for consistency.
  A future migration to a `feature_id`/`test_command`/`pproxy_oracle_required`/
  `network_required` schema is documented but not implemented in this pass.
- **Auto-detect oracle gating, accept legacy env var.** The actual test code
  uses import-time detection. The docs and CI commands no longer require
  `EGRESS_REQUIRE_PPROXY_ORACLE=1` but the variable is documented as accepted
  for backward compatibility. The plan-preferred `EGRESS_REQUIRE_PPROXY_PYTHON_API`
  is not introduced (no functional difference from auto-detection).
- **No import pproxy shim, no protocol classes, no PyPI publish.** These were
  explicit non-goals from the plan and remain deferred.

## Known Remaining Items (not in scope)

- Migration of `python_api_cases.toml` to the new schema (separate workstream;
  the legacy schema is validated today).
- `.pyi` type stubs for native module (mentioned as known limitation in
  `PYTHON_BINDINGS.md`).
- PyPI publication (separate release workstream; deferred per plan).

## Related Documents

- `plans/PHASE_29_32_PYTHON_CORRECTIVE_CLOSURE.md` — original plan
- `docs/PHASE_29_32_PYTHON_HARDENING_COMPLETION.md` — Phase 29-32 hardening (prior pass)
- `docs/PYTHON_BINDINGS.md` — updated user-facing API reference
- `tests/compat/pproxy_manifest.toml` — updated manifest
- `tests/compat/fixtures/python_api_cases.toml` — expanded fixture
- `crates/eggress-testkit/src/corpus.rs` — new `validate_python_api_cases`
- `crates/eggress-testkit/src/manifest.rs` — extended test-name walker
- `python/eggress/pproxy.py` — added `Server.config` property
- `python/tests/test_server_lifecycle.py` — added 5 `Server.config` tests
- `python/tests/test_wheel_import_smoke.py` — added 3 installed-wheel tests
- `python/tests/test_docs_examples.py` — new (22 tests)
- `scripts/test_wheel.sh` — strengthened installed-artifact verification
- `docs/SECURITY_REVIEW.md` — expanded Python sections
- `docs/python/RELEASE_CHECKLIST.md` — added step 6 corrective checks