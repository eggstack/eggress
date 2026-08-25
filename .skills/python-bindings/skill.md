# Python Bindings and Packaging

## When to use
Use when modifying the PyO3 extension (`crates/eggress-python`), the canonical
`python/eggress` package, the opt-in `python-pproxy-compat` distribution, or
any Python-facing packaging/release concern.

## Distribution layout (authoritative)

| Path | What it is |
|---|---|
| `crates/eggress-python` | PyO3 crate; builds the `_eggress` native extension via maturin (`module-name = "eggress._eggress"`, abi3-py39) |
| `python/eggress/` | Canonical pure-Python package shipped in the `eggress` wheel |
| `python/pproxy/` | Source for the top-level `pproxy` namespace |
| `python-pproxy-compat/` | Separate setuptools distribution owning the top-level `pproxy` import; depends on `eggress==<version>` |

Namespace rules — do not regress these:
- The `eggress` wheel contains ONLY the `eggress` package plus `_eggress`.
  It must never install or alias a top-level `pproxy` package.
- Top-level `import pproxy` comes only from installing
  `pip install ./python-pproxy-compat` (or the published
  `eggress-pproxy-compat` distribution).
- Upstream `pproxy` and `eggress-pproxy-compat` must not be installed together.
- `python/eggress/pproxy.py` is the migration-oriented translation/service
  helper module inside `eggress`; it is not the top-level namespace.

## Public API surface (`eggress` package)

- `EggressConfig.from_toml(toml)` / `from_file(path)` — parse and validate config
- `EggressService.from_toml(toml)` / `from_file(path)` / `.start()` — blocking start → `EggressHandle`
- `handle.bound_addresses`, `handle.status()`, `handle.metrics_text()`,
  `handle.reload_toml(toml)`, `handle.shutdown()` (idempotent), `with handle:`
- `OutboundConnector` — native `OutboundStream`/`AsyncOutboundStream` wrappers;
  `ProxyConnection` uses this path directly (never start a temporary local listener)
- Capability metadata: `eggress.__version__`, `eggress.version()`, `eggress.capabilities()`

### pproxy migration helpers (`eggress.pproxy`)

- `PPProxyService.from_args(args)` / `from_uri(local, remotes)` / `from_toml(toml)` / `from_file(path)` — pproxy-shaped builder, not a strict drop-in contract
- `check_pproxy_args(args)` → `CompatibilityReport` (tier, ok, warnings, unsupported, diagnostics, features, toml, parsed_uris, raw_args); `FeatureInfo` dataclass
- `start_pproxy(args=, local=, remote=, config=, config_path=)` — multi-mode convenience entry point
- `PPProxyHandle` — alias for `EggressHandle`

### Connection / Server / protocol objects

- `eggress.Connection` delegates to PyO3 `PyConnection`; state machine is
  `Arc<AtomicU8>`; `close()` releases the GIL via `py.detach()`; `__del__`
  does best-effort cleanup with `ResourceWarning`. Rust owns networking,
  Python owns the coroutine contract.
- `eggress.protocol` — pproxy-compatible protocol objects (`Socks5`, `HTTP`,
  `SS`, …) with `MAPPINGS` and `get_protos()`.
- `eggress.cipher` — AEAD cipher objects delegating to Rust; keep the
  supported AEAD surface deterministic behind the `cipher-api` extra.
- `eggress.plugin` — bounded callback bridge (`PluginBridge`) between Rust
  async tasks and Python callbacks.

## PyO3 binding pattern

Each class wraps an inner `eggress-embed` type; blocking calls release the GIL:

```rust
#[pyclass]
struct PyEggressHandle {
    inner: Option<eggress_embed::EggressHandle>,
}

fn shutdown(&mut self, py: Python<'_>) -> PyResult<()> {
    if let Some(handle) = self.inner.take() {
        py.detach(|| handle.shutdown_blocking())
            .map_err(|e| map_error(py, e))?;
    }
    Ok(())
}
```

Error mapping: `Config→ConfigError`, `Startup→StartupError`,
`Reload→ReloadError`, `Shutdown→ShutdownError`,
`UnsupportedFeature→UnsupportedFeatureError`,
`Runtime/Internal→InternalError`; all inherit from `EggressError`.

## Building and testing

```bash
# Local development loop (repo-root venv; pytest.ini forces --import-mode=importlib
# so the source tree cannot shadow the installed wheel's compiled _eggress)
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q

# Wheel + sdist
cd crates/eggress-python
maturin build --release --out ../../dist
maturin sdist --out ../../dist
```

Wheel validation must happen in a clean environment against the installed
artifact (`scripts/test_wheel.sh`, `scripts/release_artifact_smoke.py`);
never by mutating `sys.modules` in the test process.

Key metadata: `py.typed` PEP 561 marker included; version pinned in lockstep
with the workspace (see the release skill); classifiers list Python 3.9–3.13;
`cipher-api` optional extra gates the cryptography dependency.

## Verification checklist

- [ ] Namespace rules above hold (inspect built wheel contents if unsure)
- [ ] Credentials redacted in reprs, diagnostics, and errors
- [ ] `.pyi` stubs updated for new public symbols
- [ ] `.venv/bin/python -m pytest python/tests tests/compat -q` passes
- [ ] Clean-environment wheel smoke passes before release claims

## References

- `docs/PYTHON_BINDINGS.md` — full bindings reference
- `docs/python/IMPORT_STRATEGY.md`, `docs/python/PACKAGING.md`, `docs/python/INSTALLATION.md`
- `docs/adr/ADR_python_import_and_distribution_strategy.md`
- `docs/PYPI_RELEASE.md` — tag-triggered publish procedure
- `architecture/python-bindings.md`, `architecture/pproxy-compat.md`
