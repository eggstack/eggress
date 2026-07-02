# Python API Parity Documentation

This directory contains the Phase 29 Python API discovery and parity specification
between eggress and pproxy 2.7.9.

**Phase 29 is a specification phase — it does not claim full Python API compatibility.**

## Documents

| Document | Purpose |
|----------|---------|
| [PPROXY_API_INVENTORY.md](PPROXY_API_INVENTORY.md) | 114-entry inventory of pproxy's public Python API with tier classification |
| [PPROXY_EMBEDDED_USAGE_PATTERNS.md](PPROXY_EMBEDDED_USAGE_PATTERNS.md) | 10 common pproxy embedded usage patterns with eggress equivalents |
| [PYTHON_LIFECYCLE_PARITY.md](PYTHON_LIFECYCLE_PARITY.md) | Lifecycle model comparison: asyncio vs Tokio, blocking vs async |
| [EGGRESS_PYTHON_API_CURRENT_STATE.md](EGRESS_PYTHON_API_CURRENT_STATE.md) | Current eggress Python package audit and gap analysis |

## Tier Classification

| Tier | Meaning | Count |
|------|---------|-------|
| **A** — Exact match | API shape and semantics match pproxy | 20 |
| **B** — Functional equivalent | Different API shape but same capability | 34 |
| **C** — Partial | Usable subset exists | 1 |
| **D** — Deferred | Not yet implemented, no final decision | 5 |
| **N/A** — Not applicable | pproxy feature out of scope | 54 |

## Oracle Testing

The oracle test harness lives at `python/tests/test_pproxy_oracle.py`. Run with:

```bash
EGRESS_REQUIRE_PPROXY_ORACLE=1 python -m pytest python/tests/test_pproxy_oracle.py -v
```

The pproxy API snapshot is frozen at `tests/compat/fixtures/pproxy_api_snapshot.json`.
Re-generate with `scripts/snapshot_pproxy_api.py`.

## Manifest

Phase 29 added 12 entries to `tests/compat/pproxy_manifest.toml` covering Python API
surfaces (exports, translation, lifecycle, reload, errors, context managers, GIL,
protocols, ciphers, scheduling).

## Related

- `docs/PYTHON_BINDINGS.md` — User-facing Python bindings documentation
- `python/README.md` — Python package README
- `tests/compat/pproxy_manifest.toml` — Canonical compatibility manifest
