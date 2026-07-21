# pproxy 2.7.9 Oracle

This directory contains the frozen upstream oracle definition for
pproxy 2.7.9, used as the behavioral reference for all differential
compatibility testing.

## Purpose

The oracle provides a reproducible reference implementation against which
eggress's pproxy compatibility is measured. Every differential test runs
the same scenario against both the oracle (pproxy 2.7.9) and the candidate
(eggress + eggress-pproxy-compat) and compares structured observations.

## Bootstrap

```bash
python3.11 -m venv .venv-oracle
.venv-oracle/bin/pip install -r compat/pproxy-2.7.9/requirements-oracle.txt
.venv-oracle/bin/pip install -r compat/pproxy-2.7.9/requirements-optional.txt
.venv-oracle/bin/python -c "import pproxy; print(pproxy.__version__)"
```

## Verification

The oracle runner verifies:
- Installed package version matches 2.7.9
- Package hash matches hashes.toml (when hashes are populated)
- Required Python version is available
- Optional dependencies are installed for protocol coverage

## Governing Rules

1. The oracle environment is isolated from the candidate environment.
2. Version/hash mismatch causes hard failure before probes execute.
3. Evidence bundles include resolved environment metadata.
4. No test silently falls back to a system-installed pproxy.
5. See `docs/parity/PPROXY_COMPATIBILITY_POLICY.md` for the full policy.
