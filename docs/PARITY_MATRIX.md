# Parity Matrix

This document tracks feature-by-feature comparison between Eggress and pproxy,
with links to differential and runtime tests that prove behavioral equivalence
or document supported-but-unverified functionality.

For the canonical per-feature evidence table with test commands, see
[COMPATIBILITY_EVIDENCE.md](COMPATIBILITY_EVIDENCE.md).

For the machine-validated pproxy capability manifest (106 capabilities, 5
tiers: `drop_in`, `compatible_with_warning`, `native_equivalent`,
`intentional_non_parity`, `unsupported`), see
[`docs/parity/pproxy_capability_manifest.toml`](parity/pproxy_capability_manifest.toml)
and the auto-generated summary
[`docs/parity/PPROXY_PARITY_REPORT.md`](parity/PPROXY_PARITY_REPORT.md). The
report can be regenerated with
`python3 scripts/validate_pproxy_parity_manifest.py --write-report
docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml`
and verified with `--check-report`.

## Compatibility Tiers

| Tier | Meaning |
|------|---------|
| **Compatible** | Eggress behavior matches pproxy for tested scenarios (has runtime or differential test reference) |
| **Supported** | Eggress supports the feature, but pproxy equivalence is not claimed |
| **Partial** | Usable subset exists but not enough for compatibility |
| **Experimental** | Code exists but no compatibility/stability promise |
| **Intentional non-parity** | Deliberately not replicated with rationale |
| **Unsupported** | Not implemented |

## Feature Matrix

### Inbound TCP Protocols

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| HTTP CONNECT | server + client | server + client | Compatible | integration tests | `differential_http_connect_tcp_echo` | Byte-exact payload match |
| HTTP forward proxy | ordinary HTTP request handling | persistent session forward | Compatible | integration tests | `differential_http_forward_get` | Persistent session model implemented (19.1). Differential tests added (19.3). |
| SOCKS4/4a | server + client | server + client | Compatible | integration tests | `differential_socks4_connect_tcp_echo`, `differential_socks4a_connect_domain` | Differential tests with pproxy 2.7.9 added (19.4). |
| SOCKS5 CONNECT | server + client | server + client | Compatible | integration tests | `differential_socks5_connect_tcp_echo`, `differential_socks5_connect_ipv6`, `differential_socks5_connect_domain`, `differential_socks5_refused_target` | Expanded differential test coverage including auth, IPv6, domain, refused targets (19.5). |
| SOCKS5 UDP ASSOCIATE | client only (relay uses own protocol) | server + client + standalone mode | Supported | `udp.rs` integration | `differential_socks5_udp_associate` | eggress uses SOCKS5 UDP ASSOCIATE framing; pproxy uses its own custom framing. Both relay UDP successfully. |
| Shadowsocks TCP | full AEAD + stream | server + client (explicit protocol mode) | Supported | integration tests | none | Standard SIP003 AEAD framing; interoperable with standard Shadowsocks (ssserver/sslocal). Not pproxy-differential tested. |
| Transparent TCP proxy (`redir://`) | Linux only | Linux only | Supported | `transparent.rs` tests | none | Requires `SO_ORIGINAL_DST`; iptables/nftables REDIRECT rule needed |
| Unix domain socket (`unix://`) | Unix only | Unix only | Supported | `unix_listener.rs` tests | none | Listen on filesystem socket path; Windows not supported |
| Reverse/backward proxy (`+in`/`bind://`/`listen://`) | TCP-only raw relay, optional plaintext auth, one session per control channel | Reverse acceptor + control client (`crates/eggress-protocol-reverse`) | Supported | `integration.rs`, `reverse_runtime.rs` (10 supervisor-wiring tests), `reverse_interop.rs` | `reverse_payload_byte_equality_eggress_loopback` (self-interop payload byte-equality) | TCP only; UDP not supported by pproxy either; no built-in TLS; defense-in-depth validation refuses non-loopback external_bind without auth+allow_bind (Phase 25-28 H11) |
| macOS PF transparent proxy | supported | not implemented | Intentional non-parity | none | none | Use pfctl with standard listener instead |
| Trojan | server + client | server + client | Supported | unit tests | none | Inbound listener with TLS + SHA224; upstream client |

### Inbound UDP Protocols

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| SOCKS5 UDP ASSOCIATE relay | own UDP framing protocol | SOCKS5 UDP ASSOCIATE | Partial | `udp.rs` | `differential_socks5_udp_associate` | Framing differs; relay success matches |
| Standalone UDP relay | `-ul` mode (no TCP control) | `mode = "standalone_pproxy_udp"` | Compatible | `udp.rs` | `differential_standalone_udp_direct_echo, differential_standalone_udp_domain_target` | pproxy-compatible standalone UDP mode; differential tests verify behavioral parity with pproxy -ul |
| Shadowsocks UDP | supported | standard AEAD format | Supported | `shadowsocks_udp.rs` | none | Standard AEAD format; interoperable with standard Shadowsocks. Not pproxy-differential tested. |
| Direct UDP forwarding | via `-ul` flag | via SOCKS5 UDP ASSOCIATE or standalone mode | Supported | `udp.rs` | none | Both entry points supported |

### Upstream TCP Protocols

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| HTTP CONNECT upstream | supported | supported | Compatible | integration tests | `differential_socks5_through_http_upstream` | Chain payloads match |
| SOCKS4/SOCKS4a upstream | supported | supported | Supported | integration tests | none | Unit tested |
| SOCKS5 upstream | supported | supported | Compatible | integration tests | `differential_socks5_through_socks5_upstream` | Chain payloads match |
| Shadowsocks upstream | supported | supported | Supported | `shadowsocks_tcp.rs` | none | Standard AEAD framing; interoperable with standard Shadowsocks |
| Trojan upstream | supported | supported | Supported | unit tests | none | Client with rustls TLS |
| Direct upstream | supported | supported | Compatible | integration tests | implicit in echo tests | Both connect directly |

### Upstream UDP Protocols

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| SOCKS5 UDP upstream relay | supported | one-hop only | Partial | `udp_upstream.rs` | none | Eggress: one-hop; pproxy: multi-hop capable |
| HTTP UDP upstream | N/A | unsupported | Unsupported | none | none | Not implemented |
| Shadowsocks UDP upstream | supported | standard AEAD one-hop | Supported | `shadowsocks_udp.rs` | none | Single-hop only; pproxy: multi-hop capable |

### Chain Behavior

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| Single-hop TCP chain | supported | supported | Compatible | integration tests | `differential_socks5_through_*` | Tested through pproxy upstream |
| Multi-hop TCP chain | supported | basic support | Partial | integration tests | none | 3+ hops exist but compatibility untested |
| UDP chain | supported (SOCKS5 relay) | one-hop SOCKS5 only | Partial | `udp_upstream.rs` | none | No multi-hop UDP chains |
| Chain capability validation | implicit | explicit validation | Supported | integration tests | none | Rejects invalid combos |

### Scheduler Behavior

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| Round-robin | default for multiple remotes (`-s rr`) | default for groups | Supported | `scheduler_runtime.rs` | none | Uses global atomic cursor; pproxy resets on reload. No pproxy differential test. |
| First-available | via `-s fa` | `FirstAvailable` scheduler | Supported | `scheduler_runtime.rs` | none | Both return first eligible upstream |
| Random | not default | `Random` scheduler | Supported | `scheduler_runtime.rs` | none | Eggress-specific; deterministic variant for testing |
| Least-connections | not available | `LeastConnections` scheduler | Supported | `scheduler_runtime.rs` | none | Uses active + in_flight count |
| Health-aware skip | implicit via alive check | explicit health state machine | Supported | `scheduler_runtime.rs` | none | Eggress: hysteresis state machine |
| Fallback on all fail | `-F` flag (direct only) | `GroupFallback`: reject/direct/use-unhealthy | Partial | `scheduler_runtime.rs` | none | Eggress offers more granular control |
| Retry within group | not documented | not implemented | Supported | none | none | Single attempt per request; pproxy behavior undocumented |
| Active lease tracking | not documented | `PendingLease`/`ActiveLease` two-phase | Supported | `scheduler_runtime.rs` | none | Precise connection accounting |
| Scheduler state persistence | resets on reload | persists across reloads | Intentional non-parity | none | none | Eggress preserves cursor for unchanged groups |

### Authentication Behavior

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| SOCKS5 auth rejection | rejects unauthenticated | rejects unauthenticated | Compatible | integration tests | `differential_socks5_auth_failure` | Both reject |
| HTTP auth rejection | rejects unauthenticated | rejects unauthenticated | Compatible | integration tests | `differential_http_auth_failure` | Both reject |
| SOCKS5 username/password | URI-embedded | URI-embedded | Supported | integration tests | none | |
| HTTP Basic auth | URI-embedded | URI-embedded | Supported | integration tests | none | |
| Shadowsocks password | URI-embedded | URI-embedded | Supported | `shadowsocks_tcp.rs` | none | |

### CLI Compatibility

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| `-l` listen flag | supported | supported | Compatible | cli_tests | none | Same syntax |
| `-r` remote flag | supported | supported | Compatible | cli_tests | none | Same syntax |
| `-ul` UDP listen | supported | supported | Compatible | cli_tests, pproxy_cli_tests | none | Generates standalone UDP listener config (`mode = "standalone_pproxy_udp"`) |
| `-ur` UDP remote | supported | supported | Compatible | cli_tests, pproxy_cli_tests | none | Generates UDP upstream config with transport-matching rule |
| `--config` TOML | supported | supported (different schema) | Partial | integration tests | none | Different schema |
| `--rulefile` | supported | supported (generates TOML) | Compatible | cli_tests | none | Phase 38: translates pproxy rulefiles to `[[rules]]` with diagnostics for untranslatable patterns |
| `--daemon` | supported | rejected | Intentional non-parity | none | none | Use systemd or a process manager |
| `-b` bind | supported | supported (generates TOML) | Compatible | cli_tests | none | Phase 38: generates `[[rules]] reject` entries |
| `--ssl` TLS listener | supported | supported (generates TOML) | Native equivalent | cli_tests, pproxy_compat_manifest, pproxy_compat_report | none | Phase 38: generates TLS listener TOML config; Phase 42: TLS now applied to all listeners (matches pproxy, which loads cert chain into every ssl context) |
| `-a` alive/health | supported | supported (generates TOML) | Compatible | cli_tests | none | Phase 38: generates `[health] interval = "Ns"` |
| `--pac` PAC serving | supported | supported (generates TOML) | Compatible | cli_tests | none | Phase 38: generates `[admin.pac] enabled = true` |
| `--test` test-and-exit | supported | supported | Compatible | cli_tests | none | Phase 38: translates config and runs `eggress upstream test` |
| `--sys` system proxy | supported | supported | Compatible | cli_tests | none | Phase 38: auto-invokes `eggress system-proxy inspect` before starting |
| `--log` logging | supported | diagnostic only | Intentional non-parity | cli_tests | none | Phase 38: emits structured diagnostic |
| `--get` connection reuse | supported | diagnostic only | Intentional non-parity | cli_tests | none | Phase 38: emits structured diagnostic |
| `--reuse` | supported | diagnostic only | Intentional non-parity | cli_tests | none | Phase 38: emits structured diagnostic |
| pproxy compat CLI | `pproxy translate/check/run` | `eggress pproxy translate/check/run` | Compatible | cli_tests | none | Translates pproxy CLI args to TOML config |
| pproxy URI translation | N/A | `eggress pproxy translate` | Compatible | cli_tests | none | Converts pproxy listen/remote URIs to TOML |
| Exit codes | generic (1 for all errors) | granular (0–7, 130, 143) | Intentional non-parity | `cli_exit_codes.rs` | none | pproxy uses 1 for all failures; eggress provides differentiated codes per error class |
| JSON output (`--json`) | N/A | `pproxy check --json`, `route explain --json`, `upstream test --json` | Intentional non-parity | `cli_exit_codes.rs` | none | Machine-readable JSON output with tier, diagnostics, features, parsed URIs |
| Structured diagnostics | N/A | `StructuredDiagnostic` with stable `DiagnosticCode` | Intentional non-parity | `diagnostics.rs` | none | 13 stable diagnostic codes; serializable to JSON; includes tier and suggestion fields |
| CLI inventory completeness | all flags mapped | 14 of 14 pproxy flags mapped | Supported | `cli_tests` | none | Phase 38: all flags now have equivalent behavior (TOML generation, diagnostic, or rejection) |

## Remaining Protocol Audit

This section classifies every remaining pproxy protocol/scheme for Phase 11.

### Inbound Listener Protocols

| Scheme | pproxy role | TCP/UDP | Auth/Encryption | Eggress status | Decision | Rationale |
|--------|-------------|---------|-----------------|----------------|----------|-----------|
| `http://` | inbound | TCP | Basic auth, optional TLS | Supported | **Compatible** | Full parity with differential tests |
| `https://` | inbound | TCP | TLS + Basic auth | Supported | **Compatible** | Maps to `http+tls` in eggress |
| `socks4://` | inbound | TCP | User ID | Supported | **Compatible** | Differential tests with pproxy 2.7.9 |
| `socks4a://` | inbound | TCP | User ID | Supported | **Compatible** | Differential tests with pproxy 2.7.9 |
| `socks5://` | inbound | TCP+UDP | Username/password | Supported | **Compatible** | Full parity with differential tests |
| `ss://` / `shadowsocks://` | inbound | TCP | AEAD password | Supported | **Supported** | Explicit protocol mode only; no mixed-listener auto-detection |
| `trojan://` | inbound | TCP | Password (SHA224) | Supported | **Supported** | Inbound listener with TLS + SHA224 password verification |
| `redir://` | inbound | TCP | None | Supported | **Supported** | Linux only; requires `SO_ORIGINAL_DST` via iptables REDIRECT/nftables |
| `unix://` | inbound | TCP | None | Supported | **Supported** | Unix only; listen on Unix domain socket path |
| `ssh://` | inbound | TCP | SSH auth | Rejected | **Intentional non-parity** | SSH is not a proxy protocol |
| `bind://` / `listen://` | inbound (reverse) | TCP | Optional plaintext auth | Supported | **Supported** | Reverse acceptor; raw-relay control channel |
| `backward://` / `rebind://` | upstream (reverse) | TCP | Optional plaintext auth | Supported | **Supported** | Reverse control client; raw-relay control channel |
| `+in` modifier | upstream (reverse) | TCP | Optional plaintext auth | Supported | **Supported** | Activates reverse/backward mode on any protocol scheme |

### Upstream Protocols

| Scheme | pproxy role | TCP/UDP | Auth/Encryption | Eggress status | Decision | Rationale |
|--------|-------------|---------|-----------------|----------------|----------|-----------|
| `http://` | upstream | TCP | Basic auth | Supported | **Compatible** | Full parity with differential tests |
| `https://` | upstream | TCP | TLS + Basic auth | Supported | **Compatible** | Maps to `http+tls` in eggress |
| `socks4://` | upstream | TCP | User ID | Supported | **Supported** | Unit tested |
| `socks4a://` | upstream | TCP | User ID | Supported | **Supported** | Alias for `socks4` |
| `socks5://` | upstream | TCP+UDP | Username/password | Supported | **Compatible** | Full parity with differential tests |
| `ss://` / `shadowsocks://` | upstream | TCP+UDP | AEAD password | Supported | **Supported** | Standard AEAD framing; interoperable with standard Shadowsocks |
| `trojan://` | upstream | TCP | Password (SHA224) | Supported | **Supported** | Client with rustls TLS |
| `ssh://` | upstream | TCP | SSH auth | Rejected | **Intentional non-parity** | SSH transport is out-of-scope for a proxy |
| `direct://` | upstream | TCP+UDP | None | Supported | **Compatible** | Direct connection, no proxy |

### Transport/Wrapping

| Feature | pproxy support | Eggress status | Decision | Rationale |
|---------|---------------|----------------|----------|-----------|
| `+tls` suffix | `socks5+tls://` etc. | Supported | **Supported** | Maps to TLS wrapper via `eggress-transport-tls` |
| Shadowsocks AEAD ciphers | `aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305` | Supported | **Compatible** | All three AEAD methods supported; standard TCP framing |
| Shadowsocks stream ciphers | `aes-*-ctr`, `aes-*-cfb`, `rc4-md5`, etc. | Rejected | **Intentional non-parity** | Rejected with `LegacyMethodUnsupported` error; recognized legacy methods include aes-*-ctr, aes-*-cfb, rc4, rc4-md5, chacha20-ietf |
| ShadowsocksR (SSR) | Supported in some forks | Rejected | **Intentional non-parity** | Rejected with `SsrUnsupported` error; SSR URIs (`ssr://`) parsed and rejected in pproxy compat layer |
| HTTP/2 CONNECT | Supported | Supported (protocol-crate only) | **Supported** | Phase 26, synthetic; refused as listener/upstream through CLI/config compiler (Phase 25-28 H5/H6/H7) |
| WebSocket tunnels | Supported | Supported (protocol-crate only) | **Supported** | Phase 26, synthetic; refused as listener/upstream through CLI/config compiler (Phase 25-28 H5/H6/H7) |
| Raw fixed-target tunnels | Supported | Supported (protocol-crate only) | **Supported** | Phase 26, synthetic; refused as listener/upstream through CLI/config compiler (Phase 25-28 H5/H6/H7) |
| TLS ALPN negotiation | Supported | Supported | **Supported** | Phase 26, synthetic |
| QUIC transport | Deferred | Deferred | **Intentional non-parity** | ADR: docs/adr/ADR_quic_h3_pproxy_parity.md |
| HTTP/3 | Deferred | Deferred | **Intentional non-parity** | ADR: docs/adr/ADR_quic_h3_pproxy_parity.md |

### CLI/Config Features

| Feature | pproxy support | Eggress status | Decision | Rationale |
|---------|---------------|----------------|----------|-----------|
| `-l` listen flag | Supported | Supported | **Compatible** | Same syntax |
| `-r` remote flag | Supported | Supported | **Compatible** | Same syntax |
| `-ul` UDP listen | Supported | Supported | **Compatible** | Generates standalone UDP listener config (`mode = "standalone_pproxy_udp"`) |
| `-ur` UDP remote | Supported | Supported | **Compatible** | Generates UDP upstream config with transport-matching rule |
| `--daemon` | Supported | Rejected | **Intentional non-parity** | Use systemd or process manager |
| `--ssl` TLS listener | Supported | Supported | **Native equivalent** | Phase 38: generates TLS listener TOML config; Phase 42: TLS now applied to all listeners (matches pproxy, which loads cert chain into every ssl context) |
| `-b` block rules | Supported | Supported | **Compatible** | Phase 38: generates `[[rules]] reject` entries |
| `--reuse` | Supported | Rejected | **Intentional non-parity** | Connection pooling not implemented |
| `--log` | Supported | Supported | **Compatible** | Phase 38: emits structured diagnostic |
| `--sys` | Supported | Supported | **Compatible** | Phase 38: auto-invokes `eggress system-proxy inspect` before starting |
| `--rulefile` | Supported | Supported | **Compatible** | Phase 38: translates pproxy rulefiles to `[[rules]]` with diagnostics |
| Multi-hop UDP chains | Supported | Rejected | **Intentional non-parity** | One-hop only |
| Persistent HTTP forwarding | Supported | Supported | **Compatible** | Persistent session model with HTTP/1.1 keep-alive |
| Python library | `pproxy.Server()` API | `eggress` package via PyO3 | **Supported** | Python bindings wrap `eggress-embed` API | Not a 1:1 API match; see Python Bindings doc |

### Diagnostics for Unsupported Features

When an unsupported protocol or feature is encountered in pproxy compat mode, eggress produces structured diagnostics:

| Input | Diagnostic type | Error message |
|-------|----------------|---------------|
| `ssh://...` as upstream | `UnsupportedFeature` | "SSH transport is not supported" |
| `redir://...` as upstream listener | N/A | Now supported as transparent TCP proxy (Linux only) |
| `unix://...` as upstream | `UnsupportedFeature` | "Unix domain sockets are not supported as upstream" |
| `unix://...` as listener | N/A | Now supported as Unix domain socket listener (Unix only) |
| `ss://...` as listener | N/A | Now supported as explicit protocol mode |
| `trojan://...` as listener | N/A | Now supported as inbound Trojan listener with TLS + SHA224 password auth |
| Legacy stream cipher URI | `UnsupportedFeature` | "Legacy stream ciphers are not supported; use AEAD methods" |
| `--daemon` flag | `UnsupportedFeature` | "--daemon mode is not supported; use systemd or process manager" |
| `--rulefile` flag | `UnsupportedFeature` | "--rulefile is not supported; use eggress TOML routing rules" |
| Unknown URI scheme | `CompatError` | "unsupported protocol: {scheme}" |

All diagnostic messages redact credentials.

### URI Compatibility

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| `http://` scheme | supported | supported | Supported | cli_tests | none | |
| `socks5://` scheme | supported | supported | Supported | cli_tests | none | |
| `socks4://` scheme | supported | supported | Supported | cli_tests | none | |
| `ss://` scheme | supported | supported | Supported | cli_tests | none | |
| `trojan://` scheme | supported | supported | Supported | unit tests | none | |
| `__` chain separator | supported | supported | Supported | integration tests | none | |
| `user:pass@` auth | supported | supported | Supported | integration tests | none | |

### Config/Reload Behavior

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| Hot reload | SIGHUP | SIGHUP (routing only) | Partial | `reload.rs` | none | Eggress: routing/upstreams only; pproxy: full config |
| TOML config | supported | supported (different schema) | Partial | integration tests | none | Different schema |
| Runtime state preservation | varies | atomic swap via ArcSwap | Supported | `reload.rs` | none | |

### Python Library/Bindings

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| Python library | `pproxy.Server()` API | `eggress` package (PyO3) | Supported | `test_pproxy_compat.py`, `test_pproxy_redaction.py`, `test_pproxy_concurrency.py`, `test_server_lifecycle.py`, `test_pproxy_oracle.py` | none | `EggressService`, `EggressHandle`, `Server`, `start_pproxy`, translation helpers |
| pproxy drop-in API | `pproxy.Server(listen, remote)` | `PPProxyService.from_args()`, `from_uri()`, `from_toml()`, `from_file()` | Supported | `test_pproxy_dropin.py` | none | Phase 40: `PPProxyService`, `CompatibilityReport`, `FeatureInfo`, `check_pproxy_args`, `.pyi` stubs |
| PyPI package | `pip install pproxy` | `pip install eggress` | Supported | wheel tests | none | Wheels for Linux/macOS/Windows; `py.typed` included |

#### Phase 29 Python API Inventory (114 entries)

| API Surface | pproxy | Eggress | Tier | Notes |
|---|---|---|---|---|
| Module exports | `Connection`, `Server`, `Rule`, `DIRECT` | `Connection`, `Server`, `Rule`, `DIRECT` | A (exact) | Snapshot exports match |
| Protocol classes | 18 classes (`Http`, `Socks5`, `Shadow`, etc.) | Not exposed | D (deferred) | Documented in inventory |
| Cipher classes | 43 classes (`AEAD`, `BaseCipher`, etc.) | Not exposed | D (deferred) | Documented in inventory |
| Scheduling | `rr`, `fa`, `ra`, `lc` | `RoundRobin`, `FirstAvailable`, `Random`, `LeastConnections` | B (functional) | Different names, same algorithms |
| Server constructor | `Server([listen], [remote], ...)` | `eggress.Server(listen=[...], remote=[...])` | B (functional) | pproxy `rserver`/`server` mapped to `listen`/`remote`; pre-built config via `config=` kwarg |
| Async lifecycle | `asyncio.ensure_future(server_forever())` | `EggressService.start_background()` | B (functional) | Tokio vs asyncio |
| Blocking lifecycle | `asyncio.run(server_forever())` | `EggressService.start()` | A (exact) | Both block until shutdown |
| Context managers | Not available | `async with EggressService(config)` | Eggress-only | Eggress advantage |
| Config reload | Not available | `handle.reload_config(path)` | Eggress-only | Hot-reload support |
| Error types | Generic `Exception` | 7 typed exceptions | Eggress-only | Structured errors |
| URI translation | URI strings only | `pproxy_to_toml()` | Eggress-only | Translation helpers |
| GIL handling | N/A | Released on all blocking calls | Eggress-only | `py.detach()` |

**Tier key:** A = exact match, B = functional equivalent, D = deferred, Eggress-native = not in pproxy (not a parity claim)

Full inventory: `docs/python/PPROXY_API_INVENTORY.md`

## Coverage Summary

- **TCP proxying (Compatible)**: Full parity for SOCKS5 CONNECT, HTTP CONNECT, SOCKS4/4a, HTTP forward proxy, and direct upstream — differential tests produce byte-exact echo payloads.
- **TCP proxying (Supported)**: SOCKS5 UDP ASSOCIATE inbound is fully implemented; unit tested but not differentially verified against pproxy.
- **UDP relay (Supported)**: Both relay UDP datagrams successfully. Standalone UDP mode (`mode = "standalone_pproxy_udp"`) provides pproxy-compatible behavior without TCP control connection. SOCKS5 UDP ASSOCIATE is also supported for TCP-controlled UDP relay.
- **Chaining (Compatible / Partial)**: Single-hop TCP chains through pproxy upstream are byte-exact. Multi-hop chains exist but compatibility with pproxy multi-hop is untested.
- **Auth (Compatible)**: Both reject unauthenticated SOCKS5 and HTTP connections.
- **CLI (Compatible)**: `-l`, `-r`, `-ul`, and `-ur` flags share syntax and are properly classified as compatible. Phase 38 closed remaining gaps: `--ssl`, `-b`, `--rulefile`, `-a`, `--pac`, `--test`, `--sys` now generate equivalent TOML config or auto-invoke eggress subcommands. All 14 pproxy flags are now mapped.
- **Shadowsocks (Supported / Partial / Supported)**: Shadowsocks inbound listener and upstream both use standard AEAD framing and are interoperable with standard Shadowsocks. Trojan inbound listener and upstream both supported. Shadowsocks inbound is explicit protocol mode only (no mixed-listener auto-detection).
- **Transparent proxy (Supported)**: Linux-only transparent TCP proxy via `SO_ORIGINAL_DST`. Requires iptables/nftables REDIRECT rules. macOS PF transparent proxy is intentional non-parity (use pfctl with standard listener).
- **Unix domain sockets (Supported)**: Unix-only listener on filesystem socket paths. Not available on Windows. Socket file management and permissions are operator-managed.
- **Python bindings (Supported)**: `eggress` package via PyO3 wraps `eggress-embed` with `EggressConfig`, `EggressService`, `EggressHandle`, exception hierarchy, context manager, pproxy translation helpers, and async lifecycle. Phase 29 inventoried pproxy's 114-entry Python API surface and classified compatibility tiers. Phase 40 added `PPProxyService` drop-in API with `CompatibilityReport`, `FeatureInfo`, `check_pproxy_args`, `.pyi` stubs, and multi-mode `start_pproxy`. See `docs/PYTHON_BINDINGS.md` and `docs/python/`.

## Limitations

1. **UDP protocol framing**: pproxy's UDP relay uses a custom framing protocol, not SOCKS5 UDP ASSOCIATE headers. The differential test verifies relay success (payload reaches echo and returns), not wire-level equivalence.
2. **SOCKS5 UDP ASSOCIATE**: pproxy does not implement SOCKS5 UDP ASSOCIATE as a server — it uses its own `-ul` UDP relay. The test verifies eggress's SOCKS5 UDP ASSOCIATE independently, with pproxy as a comparison point for UDP relay capability. Standalone mode now provides compatible behavior.
3. **Auth credential exchange**: pproxy embeds credentials in the listen URI fragment; eggress uses config-level auth. The differential tests verify rejection behavior, not credential exchange wire format.
4. **Trojan**: Not covered in differential tests — Trojan inbound and upstream tested via unit/integration test suite.
5. **TLS transport**: Not covered in differential tests — pproxy TLS requires certificate files; tested separately in `eggress-transport-tls` tests.
6. **Multi-hop chains beyond 2**: Only single-hop chains through pproxy are tested. Multi-hop chains within eggress are tested in `integration.rs`.
7. **HTTP forward proxy**: Eggress now supports persistent HTTP forward proxy (multiple requests per connection) matching pproxy behavior (Phase 19).
8. **Hot reload scope**: Eggress reloads routing, upstreams, groups, and health config atomically via `ArcSwap`. Listener topology changes require restart. pproxy reloads its full config on SIGHUP.
9. **Fallback model**: pproxy uses `-F` flag for fallback; eggress falls back to direct connection or rejects based on route rules. Different semantics.
10. **Multi-hop UDP chains**: Not implemented. pproxy supports multi-hop UDP chains; eggress supports single-hop only.
11. **Transparent proxy**: Linux only, requires `SO_ORIGINAL_DST` and iptables/nftables REDIRECT rules. macOS PF transparent proxy is not implemented (use pfctl with standard listener). Not available on Windows.
12. **Unix domain socket listeners**: Unix only. Socket file permissions and cleanup are operator-managed. Not available on Windows.

## Test Infrastructure

### Differential Tests

All differential tests are in `crates/eggress-cli/tests/differential_pproxy.rs`.

Tests are gated:
- `#[ignore]` — not run by default
- `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` — required env var
- Python 3 + pproxy — required runtime

Run with:
```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

### Interoperability Tests

Interop tests live in `crates/eggress-cli/tests/interoperability_pproxy.rs`. They are **not gated** — they skip gracefully when pproxy is unavailable. These tests exercise cross-implementation sanity checks against a running pproxy instance.

### Runtime / Integration Tests

UDP integration tests are in `crates/eggress-runtime/tests/udp.rs` and `udp_upstream.rs`.
TCP integration tests are in `crates/eggress-runtime/tests/integration.rs`.
CLI tests are in `crates/eggress-cli/tests/cli_tests.rs`.
Config reload tests are in `crates/eggress-runtime/tests/reload.rs`.

### Property and Fuzz Tests

Per-crate property tests validate codec round-trips, parser round-trips, and route match consistency. Fuzz smoke tests exercise seed inputs for `cargo fuzz` targets. See `docs/TESTING.md` for the full testing guidance.

## Compatibility Evidence Discipline

Phase 18 establishes machine-verified compatibility evidence. Phase 19 expands coverage
with persistent HTTP forwarding and differential tests for HTTP CONNECT, SOCKS4/4a, and
SOCKS5. All compatibility claims must be backed by a manifest entry in
`tests/compat/pproxy_manifest.toml`. The human-readable evidence table is at
[COMPATIBILITY_EVIDENCE.md](COMPATIBILITY_EVIDENCE.md).

### Evidence Levels

| Level | Meaning |
|-------|---------|
| `unimplemented` | Not implemented in eggress |
| `implemented_synthetic` | Implemented but only tested without real pproxy |
| `implemented_differential` | Tested against real pproxy differential behavior |
| `implemented_interop` | Tested via external protocol interop |
| `compatible` | Real pproxy differential or interop evidence |
| `intentional_non_parity` | Deliberately not replicated with rationale |

> **Note:** The `Compatible` tier in the Feature Matrix now requires a matching manifest
> entry with `evidence_level = "compatible"` backed by pproxy differential tests.
> Features with `implemented_interop` evidence (e.g., Shadowsocks) are classified as
> `Supported` instead.

### Rules

- A feature may only be marked `compatible` in this matrix if it has a matching
  manifest entry with `compatible` or `implemented_interop` evidence level.
- `implemented_synthetic` is not sufficient for compatibility claims.
- `intentional_non_parity` requires a rationale and user-visible diagnostic.
- The parity report at `target/compat/pproxy-parity-report.json` is the
  machine-readable source of truth for evidence levels.

### Parity Report

After running differential tests, a parity report is generated:

- JSON: `target/compat/pproxy-parity-report.json`
- Markdown: `target/compat/pproxy-parity-report.md`

The report includes: eggress commit, pproxy version, OS, Rust/Python versions,
per-feature evidence levels, test results, and suggested evidence updates.

## Performance and Regression Gates

Phase 34 establishes performance baselines and regression gates. Performance tests are
classified into 4 tiers:

| Tier | Purpose | Gating | Tests |
|------|---------|--------|-------|
| 0 | Microbenchmarks | Informational | `cargo bench --workspace` (Criterion) |
| 1 | Performance smoke | Automated | `performance_smoke.rs`, Python perf tests |
| 2 | Soak / load | Gated (EGRESS_REQUIRE_SOAK) | `reverse_soak.rs`, `load.rs` |
| 3 | pproxy comparison | Gated (EGRESS_REQUIRE_PPROXY_PERF) | `scripts/perf/run_pproxy_comparison.sh` |

Tier 0-1 tests run as part of the standard test suite. Tier 2-3 tests require explicit
opt-in via environment variables. See `docs/performance/REGRESSION_GATE_POLICY.md` for
the full policy and `docs/performance/BENCHMARK_INVENTORY.md` for the benchmark catalog.
