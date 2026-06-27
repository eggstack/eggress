# Wheel Artifact Audit

This document describes the supply-chain and artifact audit checks for the `eggress` Python package.

## Purpose

Ensure release wheels do not undermine the project's dependency or security policy:
- No OpenSSL/native-tls dependencies
- No production `aws-lc-sys` (ring/rustls-only policy)
- No unexpected dynamic library dependencies
- No bundled secrets, certificates, or test-only configs
- No platform-specific native build tools required at install time

## Pre-release Audit Commands

### Rust-side checks

```bash
cargo deny check
cargo audit
```

### Wheel content inspection

```bash
# List wheel contents
python -m zipfile -l dist/*.whl

# Check for unexpected files
python -m zipfile -l dist/*.whl | grep -E '\.(env|pem|key|crt|toml)$' && echo "WARNING: unexpected files in wheel" || echo "OK"
```

### Verify expected contents

A correctly built wheel should contain:

```
eggress/__init__.py
eggress/_eggress.*.so  (or .dylib on macOS, .pyd on Windows)
eggress/config.py
eggress/service.py
eggress/exceptions.py
eggress/py.typed
eggress-<version>.dist-info/METADATA
eggress-<version>.dist-info/RECORD
eggress-<version>.dist-info/WHEEL
LICENSE  (if included by maturin)
README.md  (if included by maturin)
```

### Wheel quality checks

```bash
pip install check-wheel-contents
check-wheel-contents dist/*.whl
```

### Verify native extension

```bash
# Check that the native extension loads
python3 -m venv .venv-audit
source .venv-audit/bin/activate
pip install dist/eggress-*.whl
python -c "import eggress._eggress; print('Native extension loaded OK')"
deactivate
rm -rf .venv-audit
```

### Verify no dynamic dependencies (Linux)

```bash
# Inspect dynamic dependencies of the native extension
unzip -p dist/eggress-*.whl 'eggress/_eggress*.so' > /tmp/_eggress.so
ldd /tmp/_eggress.so
rm /tmp/_eggress.so
```

Expected: only libc, libm, libdl, libpthread, and standard system libraries. No OpenSSL, no libssl, no libcrypto.

### Verify py.typed is included

```bash
python -m zipfile -l dist/*.whl | grep py.typed
```

### Verify license is included

```bash
python -m zipfile -l dist/*.whl | grep -i license
```

## Audit Results (v0.1.0)

| Check | Result | Notes |
|-------|--------|-------|
| `cargo deny check` | ✅ PASS | advisories ok, bans ok, licenses ok, sources ok |
| `cargo audit` | ✅ PASS | 1 allowed warning: `rustls-pemfile` unmaintained (RUSTSEC-2025-0134) |
| Wheel contains `eggress/_eggress.*.so` | ✅ | `eggress/_eggress.cpython-314-darwin.so` (8.5 MB) |
| Wheel contains `eggress/__init__.py` | ✅ | 522 bytes |
| Wheel contains `eggress/py.typed` | ✅ | 0 bytes (marker file) |
| Wheel contains `eggress/config.py` and `eggress/service.py` | ✅ | 950 and 2267 bytes |
| Wheel does NOT contain `.env` files | ✅ | None found |
| Wheel does NOT contain private keys | ✅ | None found |
| Wheel does NOT contain test-only config | ✅ | None found |
| Wheel does NOT contain unexpected shared libraries | ✅ | Only native extension present |
| Native extension loads without errors | ✅ | `import eggress._eggress` succeeds |
| `check-wheel-contents` reports no issues | ✅ | `OK` |
| License file included | ✅ | MIT AND Apache-2.0 |
| README included | ✅ | Via maturin metadata |

## Audit Checklist

- [x] `cargo deny check` passes
- [x] `cargo audit` passes (or known advisories documented)
- [x] Wheel contains `eggress/_eggress.*.so` (or platform equivalent)
- [x] Wheel contains `eggress/__init__.py`
- [x] Wheel contains `eggress/py.typed`
- [x] Wheel contains `eggress/config.py` and `eggress/service.py`
- [x] Wheel does NOT contain `.env` files
- [x] Wheel does NOT contain private keys (`.pem`, `.key`)
- [x] Wheel does NOT contain test-only configuration files
- [x] Wheel does NOT contain unexpected shared libraries
- [x] Native extension loads without errors
- [x] `check-wheel-contents` reports no issues
- [x] License file is included
- [x] README is included

## Known Acceptable Dependencies

The following are expected dynamic dependencies for the native extension:
- `libSystem.B.dylib` (macOS) / `libc.so.6` (Linux) / `KERNEL32.DLL` (Windows)
- Standard C runtime

No OpenSSL, no libssl, no libcrypto, no native-tls.

## Known Acceptable Advisories

- `rustls-pemfile` 2.2.0 (RUSTSEC-2025-0134): unmaintained. This is a dev/read-only dependency
  used only for certificate parsing in tests. Not a runtime dependency of the built wheel.
  Acceptable until an actively maintained alternative is adopted upstream.
