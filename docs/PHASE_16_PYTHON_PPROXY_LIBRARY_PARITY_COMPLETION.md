# Phase 16 Completion: Python pproxy Library Parity

## Summary

Phase 16 makes the Python `eggress` package a practical replacement for users who
currently run or embed `pproxy`. The Rust `eggress-pproxy-compat` translation
layer is now accessible directly from Python, with sync and async lifecycle
patterns, comprehensive tests, and migration examples.

## APIs Added

### Translation helpers (Rust-backed)

| Function | Description |
|----------|-------------|
| `translate_pproxy_args(args)` | Translate pproxy CLI args to eggress TOML |
| `translate_pproxy_uri(local, remotes)` | Translate pproxy URI strings to eggress TOML |
| `check_pproxy_args(args)` | Alias for `translate_pproxy_args` |

### Translation result types

| Type | Fields |
|------|--------|
| `TranslationResult` | `.toml`, `.warnings`, `.unsupported`, `.ok`, `.config()` |
| `TranslationWarning` | `.category`, `.message` |
| `UnsupportedFeature` | `.feature`, `.message` |

### Convenience APIs

| API | Description |
|-----|-------------|
| `EggressService.from_pproxy_args(args, allow_partial=False)` | Create service from pproxy args |
| `start_pproxy(args, allow_partial=False)` | Start service from pproxy args (convenience) |

### Async lifecycle

| API | Description |
|-----|-------------|
| `EggressService.astart()` | Async start, returns `AsyncEggressHandle` |
| `AsyncEggressHandle.bound_addresses` | Async bound address discovery |
| `AsyncEggressHandle.status()` | Async status query |
| `AsyncEggressHandle.metrics_text()` | Async metrics |
| `AsyncEggressHandle.reload_toml()` | Async reload |
| `AsyncEggressHandle.shutdown()` | Async shutdown |
| `async with handle:` | Async context manager |

## Examples Created

| File | Description |
|------|-------------|
| `python/examples/start_socks5.py` | Start from TOML |
| `python/examples/pproxy_translate.py` | Translate pproxy args, print TOML |
| `python/examples/pproxy_run.py` | Start from pproxy args |
| `python/examples/reload_config.py` | Reload config |
| `python/examples/async_service.py` | Async usage |

## Tests Added

| File | Tests | Description |
|------|-------|-------------|
| `python/tests/test_pproxy_compat.py` | 11 | Migration scenarios (SOCKS5/HTTP/SOCKS4 direct, upstreams, round-robin, auth, unsupported, Shadowsocks, from_pproxy_args) |
| `python/tests/test_pproxy_differential.py` | 3 (gated) | Differential against real pproxy |
| `python/tests/test_pproxy_redaction.py` | 8 | Security/redaction (repr, TOML, warnings, exceptions, metrics) |
| `python/tests/test_pproxy_concurrency.py` | 6 | Multi-service, concurrent access, thread safety |

**Total: 45 passing tests (1 skipped — gated differential)**

## Documentation Updated

- `docs/PYTHON_BINDINGS.md` — async API, pproxy compat section, translation types, limitations
- `python/README.md` — async usage, pproxy migration, non-parity section
- `README.md` — Phase 16 status, pproxy compat checklist, doc links

## Non-parity Caveats

- Shadowsocks TCP is experimental (non-standard AEAD framing)
- No inbound Shadowsocks or Trojan listeners (upstream-only)
- No legacy stream ciphers (aes-ctr, aes-cfb, rc4-md5, etc.)
- No SSH, Unix socket, or transparent proxy (redir) transport
- No pproxy daemon mode (`--daemon`)
- No `-ul`/`-ur` standalone UDP relay (uses SOCKS5 UDP ASSOCIATE)
- Multiple remotes default to round-robin (matches pproxy behavior)
- Direct fallback requires explicit config

## Rust Changes

- `crates/eggress-python/Cargo.toml` — added `eggress-pproxy-compat` dependency
- `crates/eggress-python/src/lib.rs` — added `PyTranslationWarning`, `PyUnsupportedFeature`,
  `PyTranslationResult` classes; `translate_pproxy_args`, `translate_pproxy_uri`,
  `check_pproxy_args` functions

## Python Changes

- `python/eggress/pproxy.py` — new module with Python wrappers for translation types
- `python/eggress/service.py` — added `from_pproxy_args()`, `astart()`, `AsyncEggressHandle`
- `python/eggress/__init__.py` — exported new types and functions

## Blockers for Phase 17

- No blockers identified. Phase 16 is complete.

## Verification

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-pproxy-compat
maturin develop --bindings pyo3 --target x86_64-apple-darwin --manifest-path crates/eggress-python/Cargo.toml
python -m pytest python/tests
```
