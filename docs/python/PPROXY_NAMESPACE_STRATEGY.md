# ADR: pproxy Namespace and Packaging Strategy

> Current decision: the canonical `eggress` wheel does not install the
> top-level `pproxy` namespace. The opt-in `eggress-pproxy-compat` distribution
> provides the bounded compatibility package.

| Field | Value |
|-------|-------|
| Status | Accepted |
| Date | Phase C1, Workstream 5 |
| Decision makers | Eggress maintainers |
| Related | `docs/adr/ADR_python_import_and_distribution_strategy.md`, `docs/python/IMPORT_STRATEGY.md`, `crates/eggress-cli/src/pproxy_main.rs` |

## Context

Eggress provides pproxy compatibility at two layers:

1. **Python bindings** (`eggress` PyPI package) — pproxy URI translation, `Server`
   wrapper, and compatibility helpers live under `eggress.pproxy`.
2. **CLI binary** (`pproxy` — a standalone Rust binary in `eggress-cli`) —
   translates pproxy-style CLI args to eggress TOML and starts the service.

Both layers must coexist with the upstream `pproxy` package (PyPI: `pproxy`,
version 2.7.9) and the upstream `pproxy` CLI binary. The key question is how
to handle namespace ownership, import conflicts, CLI entry point collisions,
and version metadata across these overlapping surfaces.

Key constraints:

- The upstream `pproxy` PyPI package occupies the `pproxy` top-level name.
- The upstream `pproxy` CLI binary occupies the `pproxy` executable name on `$PATH`.
- eggress publishes under the `eggress` PyPI name.
- Users may have both packages installed simultaneously (migration period, mixed
  environments, CI matrices).
- Editable installs (`pip install -e .`) and source-tree `PYTHONPATH` injection
  are common during development.

## Decision

### 1. Bounded `import pproxy` package

`import pproxy` resolves to Eggress's bounded compatibility package when the
opt-in `eggress-pproxy-compat` distribution is installed. It re-exports the public `proto`, `server`, and
`cipher` adapters and factory aliases without cloning pproxy private internals.

**Rationale**: The requested compatibility surface requires the import
namespace, but the canonical wheel must remain namespace-safe. Installing the
compatibility distribution alongside upstream pproxy is explicitly unsupported.

### 2. Separate compatibility distribution

The `eggress-pproxy-compat` PyPI package contains the Python `pproxy` modules,
depends on the matching `eggress` release, and is installed only when the
top-level namespace is wanted. The canonical `eggress` wheel contains only
the `eggress` namespace.

**Trade-offs**:

The split keeps the canonical package namespace-safe and makes the compatibility
surface an explicit opt-in, at the cost of publishing two versioned packages.

### 3. CLI entry point ownership

**Eggress owns the `pproxy` binary name.** The `eggress-cli` crate produces
two binaries: `eggress` and `pproxy` (`crates/eggress-cli/src/pproxy_main.rs`).

**Conflict resolution when both are installed**:

| Scenario | Resolution |
|----------|-----------|
| Both `pip install pproxy` and `pip install eggress` | Unsupported because both wheels provide the `pproxy` import namespace. Uninstall upstream pproxy before installing Eggress. |
| Both `pproxy` Rust binary and upstream `pproxy` Python CLI on `$PATH` | The last-installed or first-on-`$PATH` wins. The eggress `pproxy` binary prints `eggress-pproxy-compat <version>` on `--version`, making identification unambiguous. |
| Virtual environment with both | The `pproxy` binary in `bin/` will be eggress's (installed by cargo). The upstream pproxy Python package is importable but has no CLI entry point. |

The upstream `pproxy` Python package (2.7.9) does **not** install a console
script or CLI entry point. It is a pure Python library. Therefore there is
no pip-level `console_scripts` collision.

### 4. Coexistence behavior

**When upstream pproxy is installed alongside eggress:**

- `import pproxy` → Eggress's bounded compatibility package after installing the Eggress wheel.
- `import eggress` → eggress bindings (always).
- `from eggress import pproxy` → eggress pproxy compat layer (always).
- `pproxy` CLI on `$PATH` → eggress binary (if installed via cargo/release tarball), or absent.

**Import order sensitivity**: None. The two packages occupy disjoint namespace
trees (`pproxy.*` vs `eggress.*`). Importing one does not affect the other.

**Editable install behavior**: Both `pip install -e .` (from `crates/eggress-python`)
and `pip install -e .` (from upstream pproxy source) can coexist. The source
tree's `python/` directory is on `sys.path` for `eggress`, and pproxy's
source is on `sys.path` for `pproxy`. No collision occurs because the top-level
package names differ (`eggress` vs `pproxy`).

**Development with both**: Common in CI matrices that run differential tests.
The test suite (`test_wheel_import_smoke.py::test_eggress_pproxy_coexists`)
explicitly verifies that both can be imported without conflict.

### 5. Version metadata

Eggress identifies itself as a compatibility implementation through:

| Symbol | Location | Value | Purpose |
|--------|----------|-------|---------|
| `eggress.__version__` | `eggress/__init__.py` | `"0.1.0"` (from native module) | Eggress version |
| `eggress.version()` | `eggress/__init__.py` | Callable, returns version string | Programmatic access |
| `eggress.capabilities()` | `eggress/__init__.py` | Dict with version, python_version, pproxy_compatibility_version, supported_protocols, supported_schedulers | Full capability snapshot |
| `eggress.pproxy.compatibility_version()` | `eggress/pproxy.py` | `"2.7.9"` | Target pproxy version |
| `eggress.pproxy.supported_features()` | `eggress/pproxy.py` | List of supported protocol feature IDs | Runtime feature introspection |
| `pproxy --version` | CLI binary | `"eggress-pproxy-compat <version>"` | CLI identification |
| `eggress pproxy check -- <args>` | CLI subcommand | JSON report with tier, features, diagnostics | Machine-readable compat check |

The `compatibility_version()` is a static string (`"2.7.9"`), not dynamically
derived from the upstream pproxy package. This is intentional — it represents
the version against which eggress compatibility is tested, not a dependency.

### 6. Summary of the recommended approach

| Question | Answer |
|----------|--------|
| Does `import pproxy` work from Eggress wheel? | **Yes, bounded public subset.** |
| Separate `pproxy` shim package? | **No.** Deferred until evidence of need. |
| Separate `eggress-pproxy-compat` distribution? | **No.** Bundled in main `eggress` package. |
| CLI `pproxy` binary ownership? | **Eggress owns it.** Upstream pproxy has no CLI entry point. |
| Coexistence with upstream pproxy? | **Unsupported.** Uninstall upstream first. |
| Import order sensitivity? | **None.** Different top-level names. |
| Version metadata? | **Static compatibility version + runtime feature introspection.** |

## Consequences

### Positive

- **Zero namespace conflicts**: The `pproxy` and `eggress` namespace trees are
  completely disjoint. No shadowing, no surprises.
- **Honest origin**: `eggress.pproxy` clearly communicates "eggress's
  compatibility layer for pproxy", not "this is pproxy."
- **Future-proof**: A `pproxy` shim or separate compat package can be added
  later without breaking existing imports.
- **Simple release**: One package to build, publish, and version.
- **Testable coexistence**: Differential tests verify both packages work
  side-by-side.

### Negative

- **Longer import path**: `eggress.pproxy.translate_pproxy_args(...)` is longer
  than `pproxy.trans(...)`. This is an acceptable trade-off for namespace safety.
- **No automatic migration path**: Users who have `import pproxy` throughout
  their codebase must change to `from eggress import pproxy` or
  `import eggress.pproxy`. This is documented in `MIGRATION_FROM_PPROXY.md`.
- **CLI name collision risk**: If a future upstream pproxy release adds a
  console script named `pproxy`, the eggress binary would shadow it on `$PATH`.
  Mitigated by the `--version` output clearly identifying the binary as
  `eggress-pproxy-compat`.

### Neutral

- **Compatibility version is static**: `"2.7.9"` is a test target, not a
  runtime dependency. This decouples eggress releases from upstream pproxy
  releases.
- **Binary is Rust, not Python**: The `pproxy` CLI entry point is a compiled
  Rust binary, not a Python console_script. This means pip cannot conflict
  with it at the Python packaging level.

## Testing requirements

1. **Compatibility-package smoke test** (`test_wheel_import_smoke.py`):
   - the canonical wheel must not contain top-level `pproxy` files;
   - installing `eggress-pproxy-compat` must make `import pproxy` work;
   - upstream pproxy and the compatibility distribution must not be mixed.

2. **Differential tests** (`test_pproxy_oracle.py`): auto-skip if pproxy is
   not installed; verify eggress and pproxy produce equivalent results for
   shared protocol paths.

3. **CLI binary identification**: `pproxy --version` prints
   `eggress-pproxy-compat <version>`, not a version string that could be
   confused with upstream pproxy.

4. **Editable install tests**: verify that `pip install -e .` from both
   `crates/eggress-python` and upstream pproxy source can coexist in the same
   environment without namespace pollution.

## See also

- `docs/adr/ADR_python_import_and_distribution_strategy.md` — broader import
  and distribution strategy (supersedes this for general import questions)
- `docs/python/IMPORT_STRATEGY.md` — user-facing import paths
- `docs/python/MIGRATION_FROM_PPROXY.md` — migration guide from pproxy
- `docs/python/PACKAGING.md` — wheel build matrix and module structure
- `docs/python/INSTALLATION.md` — installation methods
- `crates/eggress-cli/src/pproxy_main.rs` — CLI pproxy compatibility binary

## Compatibility distribution implementation

The canonical wheel keeps the namespace-safety rule: `import eggress` is the
only top-level import it owns, and it never uses `sys.modules` or `sys.path`
mutation to impersonate upstream `pproxy`. The Rust compatibility crate remains
internal; `from eggress import pproxy` is the migration-helper path, while the
top-level package is provided only by `eggress-pproxy-compat`.
