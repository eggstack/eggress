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
- `OutboundConnector::from_pproxy_uri()` — one pproxy remote expression,
  including `__` multi-hop chains (order preserved, no listener, fail-closed
  on unsupported hops, redacted errors; `pproxy-compat` feature)
- `OutboundConnector::connect_tcp()` / `connect_tcp_timeout()` — execute the
  compiled chain in-process via `ChainExecutor`

See `docs/EMBED_API.md` for full reference.

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

## Python bindings (eggress-python)

For Python embedding (PyO3 bindings, the `python/eggress` package, the
`eggress.pproxy` migration helpers, packaging, and namespace rules), see
`.skills/python-bindings/skill.md`. The `eggress` wheel ships only the
`eggress` package; top-level `import pproxy` belongs to the separate
`eggress-pproxy-compat` distribution.
