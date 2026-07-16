# Track B/C Release Closure Certification

**Status:** Certified subset / release-candidate closure
**Reference target:** `pproxy==2.7.9`
**Canonical contract:** `docs/parity/pproxy_capability_manifest.toml`

This document supersedes the Phase 51 snapshot for current release-closure
claims. The manifest is machine-validated and currently contains 148
capabilities:

| Tier | Count |
|---|---:|
| `drop_in` | 103 |
| `compatible_with_warning` | 16 |
| `native_equivalent` | 15 |
| `intentional_non_parity` | 9 |
| `unsupported` | 5 |
| **Total** | **148** |

Track B/C closes the following release surfaces:

- Python outbound TCP uses a native Rust stream with sync and asyncio wrappers;
  `ProxyConnection` no longer starts a temporary local listener.
- `eggress-pproxy-compat` is a separately built pure-Python distribution that
  installs `import pproxy` only when explicitly requested, pins `eggress`, and
  declares the supported cipher dependency.
- Cipher behavior has an explicit `eggress[cipher-api]` extra and a matching
  dependency in the compatibility distribution; legacy ciphers remain
  intentionally unsupported.
- `scripts/release_evidence.py` produces redacted metadata, scenario results,
  wheel hashes, retained inputs, and `SHA256SUMS` for release evidence.

## Local verification record

The release evidence bundle is generated under `target/release-evidence/` and
records the exact commit, platform, Python/Rust versions, commands, hashes, and
skipped external scenarios. A clean compatibility-wheel smoke test verifies
that `eggress` and `pproxy` import from their intended distributions without
namespace aliasing.

Required checks for a release tag are:

```bash
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
python3 scripts/release_evidence.py --output target/release-evidence --reference pproxy==2.7.9
```

The installed-wheel smoke test passes on the local supported artifact path.
The pinned external pproxy differential suite was also attempted with
`pproxy==2.7.9` under Python 3.11, but representative SOCKS5 and HTTP echo
cases returned an empty reference-side payload while Eggress returned the
payload. This remains a publication blocker and is retained as a failed
reference-oracle scenario, not reported as a pass. The full target-platform
wheel matrix remains CI-gated evidence and must run on matching platforms
before publishing.

This is a certified subset claim, not strict full pproxy parity. Unsupported
and warning-tier capabilities remain documented in the canonical manifest and
generated report.
