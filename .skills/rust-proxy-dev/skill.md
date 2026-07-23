# Rust Proxy Development

## When to use
Use when implementing new proxy protocols, transport wrappers, or modifying core relay/chain behavior.

## Key conventions
- Edition 2021, MSRV 1.75, `unsafe_code = "forbid"` everywhere
- Async runtime: Tokio. Errors: `thiserror`. CLI: `clap` derive.
- Streams are boxed at protocol/transport boundaries (`BoxStream`) — never propagate generic stream types
- No C deps, no OpenSSL, no `build.rs` files

## SSR/legacy Shadowsocks handling

SSR and legacy stream ciphers are intentionally unsupported. The codebase provides clear diagnostic errors:

- `LegacyMethodUnsupported` error variant — produced when a legacy stream cipher method (e.g., `aes-*-ctr`, `aes-*-cfb`, `rc4`, `rc4-md5`, `chacha20-ietf`) is detected at parse time.
- `SsrUnsupported` error variant — produced when an SSR URI (`ssr://`) is encountered.
- `is_legacy_method()` in `eggress-protocol-shadowsocks::method` — detects known legacy methods.

## SSH upstream parity

SSH upstream transport is intentional non-parity (Phase 47 ADR). SSH URIs are recognized at parse time for clean diagnostics but rejected with an actionable recommendation to use OpenSSH dynamic forwarding (`ssh -D`). See `docs/adr/ADR_ssh_upstream_parity.md`.

## Adding a new protocol

### 1. Protocol detection
Add a `ProtocolDetector` implementation in `eggress-core/src/detect.rs`. Detectors run in order — the first match wins. Mixed-protocol listeners are the norm.

### 2. Server handler
Create the protocol module under `crates/eggress-protocol-<name>/`:
- `src/lib.rs` — module re-exports
- `src/detect.rs` — protocol detection
- `src/server.rs` — server-side handshake (accept inbound connection, produce `AcceptedSession`)
- `src/client.rs` — client-side handshake (connect to upstream, produce `BoxStream`)
- `src/error.rs` — error types

Follow the pattern in `eggress-protocol-socks/` or `eggress-protocol-http/`.

### 3. Chain integration
The chain executor in `eggress-core/src/chain.rs` folds over hops with protocol-specific handlers. You must:
- Validate chain capabilities (`UdpRelayCapability` for UDP, similar for other protocols)
- Implement the hop handler that takes a stream to the hop and produces a stream to the next target

### 4. Registration
- Add the protocol variant to `ProtocolId` enum in `eggress-core/src/detect.rs`
- Register the detector in the appropriate listener setup
- Add URI scheme handling in `eggress-uri/`

### 5. Advanced transport considerations
For H2, WebSocket, or raw tunnel transports, see `.skills/advanced-transports/skill.md` for specialized guidance. All intermediate-hop handlers (WS, Raw, H2) are stream-consuming — they perform handshake over the prior-hop stream provided by the chain executor. Chain entries (socks5→ws, http→ws, socks5→raw, http→raw, socks5→h2, http→h2) are classified as `drop_in`.

## Listener types

### Standard TCP listener
Binds to a TCP socket. Configured via `[[listeners]]` with `bind = "host:port"`.

### Transparent TCP listener (Linux)
Intercepts connections redirected by iptables/nftables. Extracts original destination via `SO_ORIGINAL_DST`.
- Config: `[listeners.transparent]` with `enabled = true`, `protocol = "redir"`
- Platform: Linux only, requires `CAP_NET_ADMIN` or root
- Source: `crates/eggress-server/src/listener/transparent.rs`
- Platform capability model: `crates/eggress-runtime/src/platform.rs`

### Unix domain socket listener
Listens on a filesystem socket path for local-only deployments.
- Config: `[listeners.unix]` with `path`, `unlink_existing`, `mode`
- Platform: Unix only (Linux, macOS, BSDs)
- Source: `crates/eggress-server/src/listener/unix.rs`

## Testing
- Unit tests in the protocol crate
- Integration tests in `crates/eggress-runtime/tests/`
- Interoperability tests in `crates/eggress-cli/tests/`
- Oracle scenario schema: TOML files under `crates/eggress-testkit/tests/oracle/scenarios/` define declarative test scenarios with `client_actions` (e.g., Socks5TcpConnect, HttpConnect), `expected_observations`, and `composition_id` mapping to A2 composition matrix entries. Schema version 1, validated by `cargo test -p eggress-testkit --test oracle_scenario_files`
- Always run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`

## Exit codes and diagnostics
- Use exit code constants from `eggress-pproxy-compat::exit_codes` — never ad-hoc `process::exit` or raw numbers
- Use `DiagnosticCode` enum for structured error/warning codes; wrap in `StructuredDiagnostic` for JSON output
- `PproxyCheckOutput` struct drives `pproxy check --json` output

## Verification checklist
- [ ] `cargo check --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean
- [ ] No new `unsafe` code
- [ ] Credentials never logged (use redacted Display)
- [ ] Bounded parsers/handshake timeouts
- [ ] Capability classifier reflects actual wire compatibility (not just internal code existence)
- [ ] Strict manifest `certification_scope` field set for any new capability records

## Embed API (eggress-embed)

For embedding eggress in another Rust process, use the `eggress-embed` crate:

- `EggressConfig::from_toml_str()` / `from_toml_file()` — parse and validate config
- `EggressService::new(config).start_blocking()` — blocking start, returns `EggressHandle`
- `EggressService::new(config).start().await` — async start within a Tokio runtime
- `handle.bound_addresses()` — discover listener ports (supports port-0)
- `handle.status()` — generation, readiness, uptime, active connections
- `handle.metrics_text()` — Prometheus metrics without HTTP
- `handle.reload_toml_str()` — hot-reload routing/upstreams
- `handle.shutdown()` / `shutdown_blocking()` — graceful shutdown

See `docs/EMBED_API.md` for full reference.

## Python bindings (eggress-python)

For Python embedding, use the `eggress-python` crate and `python/eggress` package:

- `EggressConfig.from_toml(toml)` / `from_file(path)` — parse and validate config
- `EggressService.from_toml(toml)` / `from_file(path)` — create a service
- `service.start()` — blocking start, returns `EggressHandle`
- `handle.bound_addresses` — listener name to address mapping
- `handle.status()` — generation, readiness, uptime, connections
- `handle.metrics_text()` — Prometheus metrics text
- `handle.reload_toml(toml)` — hot-reload routing/upstreams
- `handle.shutdown()` — graceful shutdown (idempotent)
- `with handle:` — context manager shuts down on exit

### pproxy drop-in API (Phase 40)

- `PPProxyService.from_args(args)` / `from_uri(local, remotes)` / `from_toml(toml)` / `from_file(path)` — pproxy-compatible service builder
- `service.start()` / `with service:` — start and manage lifecycle
- `check_pproxy_args(args)` → `CompatibilityReport` — tier classification, diagnostics, TOML output
- `start_pproxy(args=, local=, remote=, config=, config_path=)` — multi-mode convenience function
- `PPProxyHandle` — alias for `EggressHandle`
- `CompatibilityReport` — dataclass with tier, ok, warnings, unsupported, diagnostics, features, toml, parsed_uris, raw_args
- `FeatureInfo` — dataclass with feature_id, tier, supported
- `.pyi` type stubs for all public modules

#### Connection class (Phase C2)

`eggress.Connection` wraps a Rust-owned proxy service. The Python class delegates to `PyConnection` (PyO3) which manages the `EggressHandle` lifecycle. Key design:

- Constructor translates pproxy URIs → TOML → `EggressService::start_blocking()`
- State machine stored as `Arc<AtomicU8>` for thread-safe transitions
- `close()` calls `handle.shutdown_blocking()` (GIL released via `py.detach()`)
- `__del__` does best-effort cleanup with `ResourceWarning`

When adding features to Connection, follow the pattern: Rust handles networking, Python handles the coroutine contract.

#### Protocol/cipher/plugin objects (Phase C4)

`eggress.protocol` provides pproxy-compatible protocol objects (`Socks5`, `HTTP`, `SS`, etc.) with `MAPPINGS` dict and `get_protos()` parser. `eggress.cipher` provides AEAD cipher objects (`AES_256_GCM_Cipher`, etc.) that delegate to Rust. `eggress.plugin` provides a bounded callback bridge (`PluginBridge`) between Rust async tasks and Python callbacks. Tests: `python/tests/test_protocol_cipher.py`.

### pproxy drop-in binary

- `pproxy` binary target in `eggress-cli` — direct drop-in replacement for the original pproxy command
- Source: `crates/eggress-cli/src/pproxy_main.rs` — raw arg parsing (not clap), delegates to `PproxyArgs::parse()` → `translate_pproxy_args()`
- Flags: `-l`, `-r`, `-ul`, `-ur`, `-b`, `-a`, `-s`, `-v/-vv/-vvv`, `--ssl`, `--pac`, `--test`, `--sys`, `--daemon/-d`, `--reuse`, `--get`, `--log`, `--rulefile`, `--version`, `-h/--help`
- `--help` prints comprehensive flag reference; `--version` prints `eggress-pproxy-compat {VERSION}`
- `--test` spawns `eggress upstream test -c <config>` and exits with its status
- `--sys` calls `inspect_system_proxy()` and prints results before starting
- `-v/-vv/-vvv` maps to RUST_LOG levels: 0→info, 1-2→debug, 3+→trace
- Startup banner prints version, listeners, remotes, UDP, TLS, PAC to stderr
- Tests: `cargo test -p eggress-cli --test pproxy_binary`

### Building

```bash
cd crates/eggress-python
maturin build --release --target x86_64-apple-darwin
pip install --force-reinstall target/wheels/eggress-*.whl
```

### PyO3 binding pattern

Each Python class wraps a Rust inner type from `eggress-embed`:

```rust
#[pyclass]
struct PyEggressHandle {
    inner: Option<eggress_embed::EggressHandle>,
}
```

Methods use `py.detach(|| ...)` to release the GIL during blocking Rust calls:

```rust
fn shutdown(&mut self, py: Python<'_>) -> PyResult<()> {
    if let Some(handle) = self.inner.take() {
        py.detach(|| handle.shutdown_blocking())
            .map_err(|e| map_error(py, e))?;
    }
    Ok(())
}
```

### Error mapping

`eggress_embed::EggressError` variants map to Python exception subclasses:

| Rust variant | Python exception |
|---|---|
| `Config(_)` | `ConfigError` |
| `Startup(_)` | `StartupError` |
| `Reload(_)` | `ReloadError` |
| `Shutdown(_)` | `ShutdownError` |
| `UnsupportedFeature { .. }` | `UnsupportedFeatureError` |
| `Runtime(_)`, `Internal(_)` | `InternalError` |

All inherit from `EggressError` → `Exception`.

### Testing

```bash
python -m pytest python/tests
```

See `docs/PYTHON_BINDINGS.md` for full reference.

### PyPI packaging

To build a distributable wheel:

```bash
cd crates/eggress-python
maturin build --release --out ../../dist
pip install --force-reinstall ../../dist/eggress-*.whl
```

To build an sdist:

```bash
cd crates/eggress-python && maturin sdist --out ../../dist
```

To validate wheel/sdist metadata:

```bash
python -m twine check dist/*
```

To test the wheel in a clean environment:

```bash
./scripts/test_wheel.sh
```

### Import strategy and distribution

The canonical PyPI package is `eggress`. The import path is `eggress`, and the
canonical wheel never aliases the top-level `pproxy` namespace. `eggress.pproxy`
provides bundled translation/service helpers. The separate
`python-pproxy-compat/` project publishes `eggress-pproxy-compat` for the
certified subset and installs `import pproxy` only when explicitly requested.

`OutboundConnector` is exposed through `eggress.OutboundConnector` and returns
native `OutboundStream`/`AsyncOutboundStream` wrappers. `ProxyConnection` uses
that path directly; do not implement client connections by starting a
temporary local listener. The `eggress[cipher-api]` extra and compatibility
wheel dependency keep the supported AEAD API deterministic.

Key metadata:
- `py.typed` PEP 561 marker included
- Version sourced from native module's `CARGO_PKG_VERSION`
- Capability metadata via `eggress.__version__`, `eggress.version()`, `eggress.capabilities()`

### Smoke tests

```bash
python -m pytest python/tests/test_wheel_import_smoke.py -v
```

For the compatibility wheel, build it separately and import-test it in a clean
environment with both the matching `eggress` wheel and the compat wheel
installed. Never validate it by mutating `sys.modules` in the test process.

See `docs/adr/ADR_python_import_and_distribution_strategy.md` for the ADR.
See `docs/python/PACKAGING.md` and `docs/python/INSTALLATION.md` for packaging and installation details.
See `docs/PYPI_RELEASE.md` for the full release procedure.
