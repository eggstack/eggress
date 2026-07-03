# Phase 29: Python API Discovery and Parity Spec — Completion Record

## Summary

Phase 29 established the Python API compatibility specification between eggress and pproxy 2.7.9. The phase produced:
- Frozen pproxy API snapshot for oracle testing
- Comprehensive API inventory with tier classification
- Embedded usage pattern documentation
- Lifecycle parity analysis
- 66 compatibility fixture test cases
- Oracle test harness skeleton
- 12 new manifest entries for Python API surfaces

## Deliverables

### 29.1 pproxy Oracle Fixture
- `tests/compat/requirements-pproxy.txt` — pinned pproxy==2.7.9
- `scripts/snapshot_pproxy_api.py` — API introspection script
- `tests/compat/fixtures/pproxy_api_snapshot.json` — frozen API snapshot (9.8KB)

### 29.2-29.3 API Inventory + Tier Classification
- `docs/python/PPROXY_API_INVENTORY.md` — 424 lines, 114 entries across 8 sections
- 20 exact matches (A), 34 functional equivalents (B), 1 partial (C), 5 deferred (D), 54 N/A

### 29.4 Embedded Usage Patterns
- `docs/python/PPROXY_EMBEDDED_USAGE_PATTERNS.md` — 516 lines
- 10 pproxy patterns with eggress equivalents
- 4 supported, 3 partial, 3 not supported

### 29.5 Lifecycle Parity
- `docs/python/PYTHON_LIFECYCLE_PARITY.md` — 318 lines
- Phase-by-phase lifecycle comparison
- Thread model comparison (asyncio vs Tokio)

### 29.6 Compatibility Fixture Cases
- `tests/compat/fixtures/python_api_cases.toml` — 671 lines, 66 test cases
- Covers exports, protocols, ciphers, scheduling, config, translation, lifecycle, errors, reload, metrics, threads, async, reverse, diagnostics

### 29.7 Oracle Test Harness
- `python/tests/test_pproxy_oracle.py` — 190 lines
- Auto-skips if pproxy is not installed (legacy `EGRESS_REQUIRE_PPROXY_ORACLE=1` env var accepted but no longer required)
- Module exports, protocol classes, translation parity, snapshot consistency

### 29.8 Manifest Updates
- 12 new entries in `tests/compat/pproxy_manifest.toml`
- Python API surfaces: exports, translation, lifecycle, reload, errors, context managers, GIL, protocols, ciphers, scheduling

### 29.9 Eggress Python API Audit
- `docs/python/EGRESS_PYTHON_API_CURRENT_STATE.md` — 234 lines
- Complete inventory of current native module + Python wrappers

## Key Findings

### pproxy API Surface
- 4 module exports, 18 protocol classes, 43 cipher classes, 4 scheduling algorithms
- URI-first design: everything configured via URI strings
- No context managers, no structured errors, no config reload

### Eggress API Surface
- 6 native classes, 4 functions, 7 exceptions
- TOML-first design with URI translation helpers
- Context managers (sync/async), structured errors, hot-reload

### Compatibility Assessment
- **Eggress-native** (not in pproxy): Context managers, error hierarchy, config reload, GIL release, status/metrics
- **Tier A** (exact match): Blocking service start/stop lifecycle
- **Tier B** (functional equivalent): URI config, CLI args translation, scheduling, health checks
- **Tier D** (deferred): Protocol class access, cipher access, plugin system

## Verification

```bash
# Run oracle tests (auto-skips if pproxy not installed)
python -m pytest python/tests/test_pproxy_oracle.py -v

# Validate manifest
cargo test -p eggress-testkit validate_real_manifest

# Run all Python tests
python -m pytest python/tests/ -v
```

## Next Steps

- Phase 30: Implement deferred API surfaces based on user demand
- Consider exposing protocol class references as documentation constants
- Evaluate cipher configuration exposure for advanced use cases
