# pproxy Oracle Maintenance Guide

This document describes how to maintain, update, and verify the pproxy
oracle used for differential compatibility testing.

## Oracle Overview

The oracle is the frozen reference implementation (`pproxy==2.7.9`) against
which eggress's pproxy compatibility is measured. Every differential test
runs the same scenario against both the oracle and the candidate and
compares structured observations.

## Files

| File | Purpose |
|------|---------|
| `compat/pproxy-2.7.9/provenance.toml` | Package pinning, source, license, interpreter versions |
| `compat/pproxy-2.7.9/hashes.toml` | SHA256 hashes for wheel/sdist verification |
| `compat/pproxy-2.7.9/requirements-oracle.txt` | Oracle environment requirements |
| `compat/pproxy-2.7.9/requirements-optional.txt` | Optional dependencies for protocol coverage |
| `compat/pproxy-2.7.9/known-defects.toml` | Registry of reproducible upstream defects |
| `compat/pproxy-2.7.9/namespace-baseline.json` | Python namespace inventory baseline |
| `compat/pproxy-2.7.9/cli-baseline.json` | CLI flags baseline |

## Bootstrap

```bash
python3.11 -m venv .venv-oracle
.venv-oracle/bin/pip install -r compat/pproxy-2.7.9/requirements-oracle.txt
.venv-oracle/bin/pip install -r compat/pproxy-2.7.9/requirements-optional.txt
.venv-oracle/bin/python -c "import pproxy; print(pproxy.__version__)"
```

## Verification

The oracle runner verifies before every execution:

1. **Installed version** matches `2.7.9`
2. **Package hash** matches `hashes.toml` (when `EGRESS_ORACLE_REQUIRE_HASH=1`)
3. **Python version** is in the tested set (`3.9`–`3.13`)
4. **Optional dependencies** are installed for protocol coverage

On mismatch, execution halts with a hard error. No silent fallback to a
system-installed pproxy is permitted.

## Updating the Oracle

### When to update

- pproxy releases a new version with bug fixes that affect compatibility
- A new Python version changes pproxy behavior
- A security vulnerability in pproxy requires tracking

### Steps

1. **Download the new package:**
   ```bash
   pip download pproxy==NEW_VERSION --no-deps --dest /tmp/pproxy-new
   ```

2. **Compute hashes:**
   ```bash
   sha256sum /tmp/pproxy-new/pproxy-NEW_VERSION-*.whl
   ```

3. **Update `hashes.toml`** with the new wheel hash.

4. **Update `provenance.toml`** with the new version, retrieval date,
   and any changed metadata.

5. **Update `requirements-oracle.txt`** with the new version pin.

6. **Re-extract namespace baseline:**
   ```bash
   python3.11 -m venv /tmp/pproxy-ns
   /tmp/pproxy-ns/bin/pip install /tmp/pproxy-new/pproxy-NEW_VERSION-*.whl
   # Run namespace extraction script
   ```

7. **Re-extract CLI baseline:**
   ```bash
   /tmp/pproxy-ns/bin/python -m pproxy --help
   ```

8. **Run the full test suite** to identify any behavioral changes:
   ```bash
   cargo test --workspace
   EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
   ```

9. **Update `known-defects.toml`** — add new defects, remove fixed ones,
   update affected scenarios.

10. **Regenerate reports:**
    ```bash
    # Generate strict report
    cargo test -p eggress-testkit --lib strict_manifest -- --nocapture
    ```

11. **Commit all changes** with a message like:
    ```
    chore(compat): update pproxy oracle to X.Y.Z
    ```

### Version compatibility matrix

| pproxy version | Python versions | Status |
|----------------|-----------------|--------|
| 2.7.9 | 3.9–3.13 | Current oracle |
| 2.7.8 | 3.9–3.13 | Previous |
| 2.7.0+ | 3.9–3.13 | Supported |

## Hash Verification

The wheel hash is the primary verification mechanism. The sdist hash
is `null` because pproxy 2.7.9 is published as wheel-only on PyPI.

Hash format: SHA256, lowercase hex, 64 characters.

Example:
```
sha256_wheel = "a073d02616a47c43e1d20a547918c307dbda598c6d53869b165025f3cfe58e80"
```

## Known Defects

Defects are registered in `known-defects.toml`. Each defect must have:

- A reproducible oracle-only failure
- Platforms and Python versions affected
- Expected candidate policy
- Approval rationale
- Regression test reference

See `docs/parity/PPROXY_COMPATIBILITY_POLICY.md` Rule 6 for requirements.

## Troubleshooting

### "Version mismatch" error

The installed pproxy version doesn't match the expected version.
Reinstall from the pinned requirements:
```bash
.venv-oracle/bin/pip install -r compat/pproxy-2.7.9/requirements-oracle.txt
```

### "Hash mismatch" error

The installed package hash doesn't match `hashes.toml`. This could mean:
- The package was corrupted during download
- A different package was installed
- `hashes.toml` needs updating (see "Updating the Oracle" above)

### "Oracle not found" error

The oracle venv doesn't exist or isn't on PATH. Bootstrap it:
```bash
python3.11 -m venv .venv-oracle
.venv-oracle/bin/pip install -r compat/pproxy-2.7.9/requirements-oracle.txt
```

### Differential tests fail with import errors

Missing optional dependencies. Install them:
```bash
.venv-oracle/bin/pip install -r compat/pproxy-2.7.9/requirements-optional.txt
```
