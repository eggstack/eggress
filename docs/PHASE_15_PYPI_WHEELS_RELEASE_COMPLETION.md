# Phase 15 Completion: PyPI Packaging, Wheels, and Release Pipeline

## Summary

Phase 15 converted the Phase 14 Python bindings into a distributable Python package
with full PyPI metadata, wheel build configuration, GitHub Actions workflows, and
release documentation.

## Workstreams Completed

### Workstream 1: Python Package Metadata
- Updated `python/pyproject.toml` and `crates/eggress-python/pyproject.toml` with
  explicit version (0.1.0), authors, classifiers, project URLs, dev dependencies
- Added `__version__ = "0.1.0"` to `python/eggress/__init__.py`
- Created `LICENSE` file with dual MIT + Apache-2.0 text
- Fixed TOML key quoting for "Bug Tracker" URL

### Workstream 2: Wheel Build Configuration
- Configured maturin `python-source`, `module-name`, and `include` patterns
- Discovered that maturin requires explicit `include` patterns for Python source
  files when `python-source` points outside the crate directory
- Verified no prohibited dependencies in wheel (no OpenSSL, no native-tls)

### Workstream 3: Local Wheel Install Tests
- Created `scripts/test_wheel.sh` for automated wheel build and test
- Verified wheel installs in clean venv without Rust toolchain
- All 20 Python tests pass from installed wheel

### Workstream 4: GitHub Actions Release Workflows
- Created `.github/workflows/python-test.yml` (matrix: ubuntu/macos × Python 3.9/3.12/3.13)
- Created `.github/workflows/python-wheels.yml` (5 platform targets + sdist)
- Created `.github/workflows/publish-pypi.yml` (TestPyPI/PyPI via trusted publishing)

### Workstream 5: TestPyPI Release Dry Run
- Documented TestPyPI upload and install verification commands in PYPI_RELEASE.md
- Process documented but not executed (requires PyPI credentials)

### Workstream 6: Production PyPI Release Procedure
- Created `docs/PYPI_RELEASE.md` with complete release workflow
- Covers: local build, wheel test, TestPyPI dry run, production upload, rollback

### Workstream 7: Platform-Specific Validation
- Added platform support table to `docs/RELEASE_READINESS.md`
- Supported: Linux x86_64/aarch64, macOS arm64/x86_64, Windows x86_64
- Not built: musllinux, Windows arm64

### Workstream 8: Supply-Chain and Artifact Audit
- Created `docs/WHEEL_AUDIT.md` with audit checklist and commands
- `cargo deny check` passes (advisories ok, bans ok, licenses ok, sources ok)
- `cargo audit` passes (1 allowed warning: `rustls-pemfile` unmaintained RUSTSEC-2025-0134)
- `check-wheel-contents dist/*.whl` passes (OK)
- Wheel contains expected files only; no secrets, no `.env`, no unexpected libraries

### Workstream 9: Documentation Updates
- Updated `README.md`: PyPI capability, packaging items, documentation links
- Updated `AGENTS.md`: wheel build/test commands, Python bindings architecture fact
- Updated `docs/CI_STATUS.md`: Python workflow documentation
- Updated `docs/PYTHON_BINDINGS.md`: PyPI and wheel install instructions
- Updated `.skills/rust-proxy-dev/skill.md`: PyPI packaging section

## Verification Results

| Check | Status |
|-------|--------|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo deny check` | PASS |
| `cargo audit` | PASS (1 allowed warning: rustls-pemfile unmaintained) |
| `check-wheel-contents dist/*.whl` | PASS |
| `python -m pytest python/tests` | PASS (20/20) |
| Wheel build (`maturin build --release`) | PASS |
| Wheel install in clean venv | PASS |
| `python -c "import eggress; print(eggress.__version__)"` | PASS (0.1.0) |

## Wheel Artifact

```
dist/eggress-0.1.0-cp314-cp314-macosx_10_12_x86_64.whl
├── eggress/__init__.py          (522 bytes)
├── eggress/_eggress.cpython-314-darwin.so  (8.5 MB)
├── eggress/config.py            (950 bytes)
├── eggress/exceptions.py        (336 bytes)
├── eggress/py.typed             (0 bytes)
├── eggress/service.py           (2267 bytes)
└── eggress-0.1.0.dist-info/    (metadata, WHEEL, RECORD, SBOM)
```

## Known Limitations

- Hosted GitHub Actions CI remains non-functional (billing issue)
- Local verification is the source of truth
- Wheel built for macOS x86_64 only (local platform); CI would build all targets
- TestPyPI/production PyPI upload not executed (requires credentials)
- Cross-platform validation (Linux, Windows, macOS arm64) requires CI or other machines
- maturin `python-source` requires explicit `include` patterns for cross-directory builds

## Blockers for Phase 16

- None identified. Phase 15 deliverables are complete.
