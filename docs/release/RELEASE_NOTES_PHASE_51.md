# Release Notes: Phase 51 — Final Parity Certification

> Historical release notes. Track B/C closure supersedes the 145-capability
> snapshot; see `FINAL_PPROXY_PARITY_CERTIFICATION_TRACK_BC.md` and the
> generated `docs/parity/PPROXY_PARITY_REPORT.md` for current claims.

**Date:** 2026-07-07
**Phase:** 51 (Final pproxy Parity Certification)
**Status:** Certified

## 1. What Changed

Phase 51 is a documentation and certification pass — no code changes.
The following artifacts were produced:

- **Manifest frozen** at 145 capabilities across 5 categories (CLI=21,
  URI=22, Protocol=44, Routing=10, Python=12).
- **Certification document** created:
  `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md`
- **Release notes** created:
  `docs/release/RELEASE_NOTES_PHASE_51.md` (this document)
- **Go/no-go checklist** created:
  `docs/release/GO_NO_GO_CHECKLIST.md`

## 2. Verification

All verification checks pass:

| Check | Result |
|---|---|
| Strict manifest validation | ✅ PASS |
| Report consistency | ✅ PASS |
| Manifest tests (32/32) | ✅ PASS |
| Workspace unit/lib tests (~1578) | ✅ PASS |
| Property tests (61) | ✅ PASS |
| Format check | ✅ PASS |
| Clippy | ✅ PASS |
| CLI binary builds | ✅ PASS |
| CLI runs correctly | ✅ PASS |
| pproxy compat tests (216) | ✅ PASS |

## 3. Upgrade Notes

No upgrade action required. This phase contains no API changes, no
configuration schema changes, and no behavioral changes. It is purely
a documentation and certification milestone.

## 4. Known Limitations

1. **Integration tests require port binding** — integration tests may
   hang in environments with port binding conflicts. This is
   environment-specific; hosted CI status is not verified; local
   verification is the source of truth.

2. **Python wheel requires matching architecture** — the arm64 wheel
   does not install on x86_64 Python. Wheels are built per-architecture
   in CI and validated on matching targets.

3. **Differential tests require Python 3.11** — pproxy 2.7.9 uses
   `asyncio.get_event_loop()` which is removed in Python 3.14. Gated
   differential tests require Python 3.11/3.12.

4. **No hosted CI visibility** — local verification is the source of
   truth; see `docs/CI_STATUS.md`.

5. **Certification is conditional** — certification is conditional on
   hosted CI/release workflow validation if not yet executed.

## 5. Links

- [FINAL_PPROXY_PARITY_CERTIFICATION.md](FINAL_PPROXY_PARITY_CERTIFICATION.md)
- [GO_NO_GO_CHECKLIST.md](GO_NO_GO_CHECKLIST.md)
- [FINAL_PPROXY_PARITY_REPORT.md](FINAL_PPROXY_PARITY_REPORT.md)
- [PARITY_RELEASE_GO_NO_GO.md](PARITY_RELEASE_GO_NO_GO.md)
- [RELEASE_NOTES_PARITY_RC.md](RELEASE_NOTES_PARITY_RC.md)
