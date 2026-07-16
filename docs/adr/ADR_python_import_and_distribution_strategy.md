# ADR: Python Import and Distribution Strategy

| Field | Value |
|-------|-------|
| Status | Accepted; amended by Track B/C release closure |
| Date | Phase 32 |
| Decision makers | Eggress maintainers |
| Related | `python/eggress/`, `crates/eggress-python/pyproject.toml`, `docs/PYTHON_BINDINGS.md` |

## Context

eggress provides Python bindings via PyO3/maturin. The package is distributed
on PyPI as `eggress`. A subset of pproxy compatibility APIs is exposed under
`eggress.pproxy` (not as a separate top-level `pproxy` package). The project
must decide on a clear import and distribution strategy that avoids ambiguity,
package name collisions, and false compatibility claims.

Key considerations:

1. **Import clarity**: Users must be able to `import eggress` and discover
   pproxy compatibility helpers without confusion about where they live.
2. **Package name collisions**: Installing eggress must not shadow or conflict
   with the upstream `pproxy` package on PyPI.
3. **Separation of concerns**: The pproxy compatibility layer is a subset of
   eggress functionality, not a standalone product.
4. **Honest compatibility claims**: eggress implements a significant subset of
   pproxy's interface, but is not a drop-in replacement. The packaging and
   import paths must communicate this clearly.

## Decision

**Canonical import is `import eggress`. pproxy compatibility helpers live at
`eggress.pproxy`. No separate `pproxy` shim package or `eggress-pproxy` package
is provided.**

Specifically:

1. **Canonical import**: `import eggress` is the primary entry point. All public
   APIs (config, service, handle, errors) are available at the top level.

2. **Compatibility helpers at `eggress.pproxy`**: pproxy translation, URI
   inspection, diagnostics, and `Server` wrapper live under `eggress.pproxy`.
   Users access them as `eggress.pproxy.translate_pproxy_args(...)`,
   `eggress.pproxy.Server(...)`, etc.

3. **No top-level `pproxy` shim**: A separate `pproxy` package that re-exports
   eggress APIs is deferred until real API parity evidence demonstrates that
   users actually need it. Premature shimming risks confusing the ecosystem.

4. **No separate `eggress-pproxy` package**: The pproxy compatibility layer is
   part of the main `eggress` package. Splitting it into a separate package
   would add distribution complexity without clear benefit.

5. **No shadowing of upstream `pproxy`**: Installing `eggress` does NOT install
   or shadow the upstream `pproxy` package. The two packages use different names
   and different import paths (`eggress` vs `pproxy`). Users who need both can
   install both without conflict.

6. **Version and capability metadata**:
   - `eggress.__version__` exposes the eggress version string.
   - `eggress.version()` is a callable that returns the version string.
   - `eggress.pproxy.compatibility_version()` returns `"2.7.9"` (the target
     pproxy version whose APIs are partially reimplemented).
   - `eggress.pproxy.supported_features()` returns the list of supported pproxy
     protocol features.
   - `eggress.capabilities()` returns a dict with version, python version,
     pproxy compatibility version, supported protocols, and supported schedulers.

7. **Partial compatibility communicated honestly**: Documentation, docstrings,
   and metadata describe eggress as providing "pproxy compatibility helpers"
   or "pproxy URI translation" — never as a "drop-in replacement." The
   `supported_features()` function provides runtime introspection of what is
   and is not covered.

## Rationale

### Import Clarity

The `eggress.pproxy` submodule path makes the relationship explicit: pproxy
compatibility is a feature of eggress, not a separate entity. Users who search
for "pproxy" in the eggress documentation find `eggress.pproxy` naturally.

### Package Name Collision Avoidance

The upstream `pproxy` package on PyPI is a separate project. Installing eggress
must never shadow it. Using a different top-level package name (`eggress`) and
a nested submodule (`eggress.pproxy`) ensures no collision occurs.

### No Premature Shimming

A top-level `pproxy` shim package would:
- Risk confusing users about which package they are actually using.
- Require maintaining a separate package on PyPI.
- Create a dependency between `pproxy` and `eggress` on PyPI that could cause
  version resolution conflicts.
- Be premature without evidence that users need it (the pproxy Python library
  has a small user base, and most users interact via CLI, not Python API).

If real API parity is achieved and user demand materializes, a shim can be
added later without breaking changes.

### Honest Compatibility

eggress provides pproxy compatibility for common use cases (URI translation,
protocol selection, configuration generation). It does not reimplement every
pproxy feature. The `supported_features()` function and compatibility version
metadata let users and tools programmatically determine coverage.

## Consequences

### Positive

- **Clear mental model**: Users know to `import eggress` and look in
  `eggress.pproxy` for compatibility helpers.
- **No package conflicts**: Installing eggress never shadows or conflicts with
  the upstream `pproxy` package.
- **Runtime introspection**: `supported_features()` and `compatibility_version()`
  provide machine-readable compatibility information.
- **Honest messaging**: Documentation and metadata never overstate compatibility.
- **Future flexibility**: A top-level `pproxy` shim or separate package can be
  added later if justified by evidence.

### Negative

- **Longer import path**: `eggress.pproxy.translate_pproxy_args(...)` is longer
  than `pproxy.trans(...)`. This is acceptable because the longer path is
  explicit about what it provides.
- **No backward-compat shim yet**: Users who want `import pproxy` semantics must
  wait for a shim package. This is intentionally deferred.

### Neutral

- **Compatibility version is pinned to 2.7.9**: This is the pproxy version
  against which eggress compatibility is tested. It is a static string, not
  dynamically derived from pproxy's actual version.

## Alternatives Considered

### 1. Top-level `pproxy` Shim Package

Create a separate `pproxy` package on PyPI that re-exports `eggress.pproxy`.

**Rejected because**: Premature. The upstream `pproxy` package already occupies
the `pproxy` name on PyPI. A shim would either shadow it (causing confusion) or
require a different name (defeating the purpose). This can be reconsidered when
real API parity evidence exists.

### 2. Separate `eggress-pproxy` Package

Split pproxy compatibility into a separate `eggress-pproxy` PyPI package with
its own versioning.

**Rejected because**: Adds distribution complexity (two packages to release,
version coupling, dependency resolution) without clear benefit. The pproxy
compatibility code is tightly coupled to eggress internals.

### 3. Direct `import pproxy` with Install-Time Detection

Attempt to detect whether the upstream `pproxy` is installed and provide
compatibility shims at import time.

**Rejected because**: Fragile and confusing. Users would not know which
`pproxy` they are importing. This violates the principle of least surprise.

## References

- `python/eggress/__init__.py` — Top-level exports
- `python/eggress/pproxy.py` — pproxy compatibility submodule
- `crates/eggress-python/pyproject.toml` — Package metadata
- `docs/PYTHON_BINDINGS.md` — Python bindings documentation
- `docs/PPROXY_PARITY_SPEC.md` — pproxy compatibility specification

## Amendment: Track B/C release closure (2026-07-16)

The original decision remains authoritative for the canonical `eggress` wheel:
it owns the `eggress` namespace and never mutates `sys.modules` or shadows an
installed upstream package. The release-closure plan adds an explicit,
separately built distribution for users who intentionally want the top-level
compatibility import:

- `python-pproxy-compat/` publishes as `eggress-pproxy-compat`.
- It installs `pproxy` only when explicitly installed, pins the matching
  `eggress` release, and declares `cryptography` for the supported AEAD API.
- It is validated in a clean environment and is documented as compatibility
  for the certified subset, not strict full pproxy parity.

This amendment supersedes the “no separate package” alternative as a release
packaging decision while preserving the namespace-safety decision for the
canonical wheel. Pip does not provide a portable package-conflict mechanism;
the compatibility package therefore must not be installed alongside upstream
`pproxy`.
