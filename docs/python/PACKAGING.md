# Packaging

## Wheel build matrix

Wheels are built for five targets using maturin:

| Target | Platform | Architecture |
|--------|----------|-------------|
| `x86_64-unknown-linux-gnu` | Linux | x86_64 |
| `aarch64-unknown-linux-gnu` | Linux | aarch64 |
| `x86_64-apple-darwin` | macOS | x86_64 |
| `aarch64-apple-darwin` | macOS | arm64 |
| `x86_64-pc-windows-msvc` | Windows | x86_64 |

Each target produces a platform-specific abi3 (`cp39-abi3`) wheel resolved by
pip for the host platform.

## Source distribution

`sdist` builds via `maturin sdist` produce a source archive that requires the
Rust toolchain to compile. The sdist includes:

- `crates/` — full Rust workspace source
- `python/` — pure Python package source
- `crates/eggress-python/pyproject.toml` — maturin build configuration

No pre-compiled artifacts are included in the sdist.

## `py.typed` marker

The `eggress/py.typed` marker file is included in all wheel builds, declaring
the package as PEP 561 compliant. Static type checkers (mypy, pyright) will
recognize inline types.

## No secrets in package data

The build and packaging pipeline does not include:

- Environment variables or API tokens
- TLS certificates or private keys
- Configuration files with real credentials
- `.env` files or secret snapshots

Generated test fixtures and config files in the repository use placeholder
credentials (`user:password`, `example.com`). These are never included in
published wheels.

## maturin as build backend

`pyproject.toml` declares `maturin` as the build backend:

```toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"
```

The `[tool.maturin]` section configures:

- `features = ["pyo3/extension-module"]` — PyO3 extension module support
- `python-source = "../../python"` — pure Python source directory
- `module-name = "eggress._eggress"` — native module path
- `include = ["eggress/**/*.py", "eggress/py.typed"]` — package data

## Module structure

```
eggress/
├── __init__.py          # Re-exports all public symbols from _eggress + Python wrappers
├── _eggress.*.so        # Native extension (PyO3, platform-specific)
├── config.py            # EggressConfig wrapper
├── service.py           # EggressService, EggressHandle, AsyncEggressHandle
├── connection.py        # Connection (pproxy-style) wrapper
├── async_connection.py  # Async connection lifecycle helpers
├── pproxy.py            # pproxy migration/translation helpers
├── pproxy_connection.py # PPProxyService/PPProxyHandle wiring
├── protocol.py          # pproxy-compatible protocol objects
├── cipher.py            # AEAD cipher objects delegating to Rust
├── plugin.py            # bounded plugin callback bridge
├── outbound.py          # native sync/async outbound stream wrappers
├── _asyncio.py          # asyncio semantic bridge
├── _asyncio_adapter.py  # loop/coroutine adapters
├── wrapper.py           # shared wrapper utilities
├── exceptions.py        # Exception hierarchy
└── py.typed             # PEP 561 marker
```

- `eggress._eggress` — native extension compiled by maturin from
  `crates/eggress-python/src/lib.rs`. All blocking Rust calls release the GIL.
- Pure Python wrappers provide the public API, error hierarchy, and context
  manager support.

The top-level `pproxy` namespace is NOT part of this wheel; it belongs to the
separate `eggress-pproxy-compat` distribution (see IMPORT_STRATEGY.md).

## See also

- [INSTALLATION.md](INSTALLATION.md) — user-facing installation instructions
- [IMPORT_STRATEGY.md](IMPORT_STRATEGY.md) — canonical import paths
- [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) — release procedure
