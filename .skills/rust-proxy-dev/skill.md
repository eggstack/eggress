# Rust Proxy Development

## When to use
Use when implementing new proxy protocols, transport wrappers, or modifying core relay/chain behavior.

## Key conventions
- Edition 2021, MSRV 1.85, `unsafe_code = "deny"` everywhere
- Async runtime: Tokio. Errors: `thiserror`. CLI: `clap` derive.
- Streams are boxed at protocol/transport boundaries (`BoxStream`) — never propagate generic stream types
- No C deps, no OpenSSL, no `build.rs` files

## SSR/legacy Shadowsocks handling

Legacy stream ciphers are an explicit compatibility-only path. The
feature-gated `legacy-crypto` implementation uses maintained RustCrypto
primitives for the supported pproxy 2.7.9 inventory subset, EVP_BytesToKey,
stateful TCP framing, OTA HMAC framing, and PacketCipher-style UDP packets.
It is separate from native Shadowsocks AEAD and rustls TLS:

- `LegacyMethodUnsupported` error variant — produced when the optional path is
  off, or when an inventory member without a maintained primitive (`cast5-cfb`,
  `idea-cfb`, `rc2-cfb`, `seed-cfb`) is requested. Modern AEAD coverage is
  `aes-128-gcm`, `aes-192-gcm`, `aes-256-gcm`, and `chacha20-ietf-poly1305`.
- `PproxyPlugin` — closed enum for `plain`, `origin`, `http_simple`, `tls1.2_ticket_auth`, `verify_simple`, and `verify_deflate`.
- `ssr_connect()` / `ssr_accept()` — SOCKS-address framing with optional prefix and ordered plugin adapters.
- `is_legacy_method()` in `eggress-protocol-shadowsocks::method` — detects known legacy methods.

## SSH upstream transport

SSH is an optional, compatibility-only upstream transport behind the `ssh`
feature. It uses `eggress-transport-ssh` and `russh` with no C/OpenSSL
dependency; the workspace MSRV is therefore 1.85. Default and `common` builds
must remain SSH-free, and SSH remains upstream-only (listener forms fail with a
structured diagnostic).

The transport implements pproxy 2.7.9's password and `:private-key-path`
credentials, direct TCP and Unix channels, chained SSH hops, cached sessions,
keepalive, and explicit remote TCP forwarding. It accepts all server host keys
to match pproxy's `known_hosts=None`; keep this behavior isolated, warning
visible, and never describe it as a native security feature. Do not add remote
commands, SFTP, agent forwarding, or unbounded forwarding. Redact passwords in
errors and diagnostics. Verify against the OpenSSH fixture with:
`cargo test -p eggress-transport-ssh --test openssh`.

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
- [ ] Active capability manifest status/evidence and the practical matrix are
      updated together when a compatibility claim changes

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

### pproxy compatibility API

- `PPProxyService.from_args(args)` / `from_uri(local, remotes)` / `from_toml(toml)` / `from_file(path)` — pproxy-shaped migration service builder, not a strict drop-in contract
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

#### Final Python package surface

The wheel includes all ten Phase 0 `pproxy` modules: the package metadata and
entry-point modules, protocol/cipher/plugin modules, `server`, `cipherpy`,
`sysproxy`, and `verbose`. `python -m pproxy` and the installed `pproxy`
console script call the same Python/native compatibility entry point. Modern
`cipherpy` AEAD classes reuse `eggress.cipher`; legacy pure-Python names are
import-compatible but fail explicitly when constructed. `sysproxy` uses the
native system-proxy bridge and reports unsupported host platforms clearly.

The contract and minimal listener smoke tests live in
`python/tests/test_pproxy_phase4_contract.py`.

### pproxy-style binary

- `pproxy` binary target in `eggress-cli` — pproxy-style translator and runtime wrapper; the frozen executable surface is strictly gated before startup
- Source: `crates/eggress-cli/src/pproxy_main.rs` — raw arg parsing (not clap), delegates to `PproxyArgs::parse()` → `translate_pproxy_args()`
- Strict executable flags: `-l`, `-r`, `-ul`, `-ur`, `-b`, `-a`, `-s`, `-d`, `-v`, `--ssl`, `--pac <path>`, `--test <target>`, `--sys`, `--daemon`, `--reuse`, `--get <path,file>`, `--auth <seconds>`, `--version`, `-h/--help`. `-d` and `-v` are repeatable count actions, including clustered forms.
- Positional URIs, `--listen`/`--remote` aliases, `--log`, and `--rulefile` are not pproxy 2.7.9 executable options and must fail before startup. Migration-only translation helpers may retain separate extension handling.
- `--help` prints comprehensive flag reference; `--version` prints `eggress-pproxy-compat {VERSION}`
- The compatibility URI AST preserves combined protocol tokens, modifiers,
  fragment auth, local binding, fixed targets, plugins, raw rules, and the
  original URI. Translation must diagnose fields that are parsed but not
  runtime-supported, and must redact credentials in all diagnostics.
- `--pac`, `--test`, and `--get` consume exactly one value. Their values remain
  owned by the option. PAC and valid `PATH,FILE` GET values use the admin
  server; TEST passes its exact URL-shaped target to the native upstream test
  from both compatibility execution entry points and never starts listeners.
- PAC and `-v/-vv/-vvv` are supported with compatibility warnings: PAC maps to
  the admin route, while verbosity selects Rust tracing defaults (`debug` for
  one or two occurrences, `trace` for three or more) unless `RUST_LOG` is set.
- `-d` selects a debug-level default tracing filter via the shared
  `PproxyArgs::default_log_level` helper and promotes compatibility session
  failures to visible error diagnostics. It is independent of `-v` and
  `--daemon`; Python traceback bytes are not reproduced. Explicit `RUST_LOG`
  remains authoritative.
- `--sys` is supported in pproxy compatibility mode through the existing
  system-proxy backend. It applies after listener bind, prefers a local
  SOCKS5 listener over HTTP, and restores captured settings on shutdown or
  failed startup. Native `eggress system-proxy` commands retain their own
  explicit semantics.
- `--daemon` is fatal unless the optional `pproxy-daemon` feature is enabled;
  that feature uses a Linux safe re-exec after validation, with the child
  owning runtime signals and system-proxy rollback. Do not add unsafe daemon
  forks or a second lifecycle manager.
- `--auth <seconds>` enables bounded, process-local source-IP authentication
  reuse when listener credentials are configured. Native mode never enables
  this cache implicitly.
- `-v/-vv/-vvv` maps to RUST_LOG defaults: 0→info, 1-2→debug, 3+→trace, and
  compatibility session reports add connection events at `-v` and byte totals
  at `-vv` without a duplicate metrics store.
- Both the standalone `pproxy` binary and `eggress pproxy run` apply the
  same fail-closed policy through the shared gate. Unknown, unsupported,
  and non-equivalent options cannot start a partial service from either
  entry point.
- `python -m pproxy` and the installed console script use the same native
  parser/action contract and pass `--auth`, `--sys`, `-d`, and `-v` to the
  compatibility supervisor. Do not reimplement those semantics in Python.
- Startup banner prints version, listeners, remotes, UDP, TLS, PAC to stderr
- Tests: `cargo test -p eggress-cli --test pproxy_binary` and
  `cargo test -p eggress-cli --test pproxy_run_process`

Compatibility runtime notes:
- `httponly` is an upstream HTTP request adapter, not a listener protocol.
- `echo` is an explicit TCP/UDP listener mode and is not enabled by unrelated
  native listener defaults.
- Brace-delimited raw/tunnel fixed targets are bounded listener/upstream forms;
  they do not imply general multi-hop UDP support.
- Unix upstreams are TCP-only and compile to a stable unsupported-platform error
  on Windows. Local source binds are per-connection socket options.

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

The canonical PyPI package is `eggress`. Its wheel also installs a bounded
top-level `pproxy` namespace with public factory, protocol, and cipher adapters.
`eggress.pproxy` remains the migration-oriented translation/service helper
module. The upstream `pproxy` distribution must be uninstalled before
installing Eggress because both distributions provide the same namespace.

`OutboundConnector` is exposed through `eggress.OutboundConnector` and returns
native `OutboundStream`/`AsyncOutboundStream` wrappers. `ProxyConnection` uses
that path directly; do not implement client connections by starting a
temporary local listener. The `eggress[cipher-api]` extra keeps the supported
AEAD API deterministic.

Key metadata:
- `py.typed` PEP 561 marker included
- Version sourced from native module's `CARGO_PKG_VERSION`
- Capability metadata via `eggress.__version__`, `eggress.version()`, `eggress.capabilities()`

### Smoke tests

```bash
python -m pytest python/tests/test_wheel_import_smoke.py -v
```

Build the authoritative `eggress` wheel and import-test it in a clean
environment. It contains both the `eggress` namespace and the bounded top-level
`pproxy` package; there is no separate compatibility wheel. Never validate it
by mutating `sys.modules` in the test process.

Use `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` for current claims,
`docs/parity/pproxy_capability_manifest.toml` for detailed evidence, and
`docs/parity/PPROXY_CLOSURE_SCENARIOS.md` for the compact final oracle surface.
The older strict manifest is historical provenance. Do not report aggregate
parity percentages or call skipped external tests passes.

See `docs/adr/ADR_python_import_and_distribution_strategy.md` for the ADR.
See `docs/python/PACKAGING.md` and `docs/python/INSTALLATION.md` for packaging and installation details.
See `docs/PYPI_RELEASE.md` for the full release procedure.
