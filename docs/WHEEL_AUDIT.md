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

## Audit Checklist

- [ ] `cargo deny check` passes
- [ ] `cargo audit` passes (or known advisories documented)
- [ ] Wheel contains `eggress/_eggress.*.so` (or platform equivalent)
- [ ] Wheel contains `eggress/__init__.py`
- [ ] Wheel contains `eggress/py.typed`
- [ ] Wheel contains `eggress/config.py` and `eggress/service.py`
- [ ] Wheel does NOT contain `.env` files
- [ ] Wheel does NOT contain private keys (`.pem`, `.key`)
- [ ] Wheel does NOT contain test-only configuration files
- [ ] Wheel does NOT contain unexpected shared libraries
- [ ] Native extension loads without errors
- [ ] `check-wheel-contents` reports no issues
- [ ] License file is included
- [ ] README is included

## Known Acceptable Dependencies

The following are expected dynamic dependencies for the native extension:
- `libSystem.B.dylib` (macOS) / `libc.so.6` (Linux) / `KERNEL32.DLL` (Windows)
- Standard C runtime

No OpenSSL, no libssl, no libcrypto, no native-tls.
