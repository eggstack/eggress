# Parity Matrix

This document tracks feature-by-feature comparison between Eggress and pproxy,
with links to differential and runtime tests that prove behavioral equivalence
or document supported-but-unverified functionality.

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
| SOCKS5 UDP ASSOCIATE | client only (relay uses own protocol) | server + client | Supported | `udp.rs` integration | `differential_socks5_udp_associate` | pproxy uses custom UDP framing, not SOCKS5 UDP ASSOCIATE as server |
| Shadowsocks TCP | full AEAD + stream | client/upstream only (non-standard framing) | Experimental | none | none | No inbound listener; upstream has non-standard AEAD framing (not wire-compatible with standard Shadowsocks); see TCP audit |
| Trojan | server + client | client only | Partial | unit tests | none | No Trojan server; no differential |

### Inbound UDP Protocols

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| SOCKS5 UDP ASSOCIATE relay | own UDP framing protocol | SOCKS5 UDP ASSOCIATE | Partial | `udp.rs` | `differential_socks5_udp_associate` | Framing differs; relay success matches |
| Shadowsocks UDP | supported | standard AEAD format | Supported | `shadowsocks_udp.rs` | none | Interoperable with standard Shadowsocks |
| Direct UDP forwarding | via `-ul` flag | via SOCKS5 UDP ASSOCIATE | Supported | `udp.rs` | none | Different entry points |

### Upstream TCP Protocols

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| HTTP CONNECT upstream | supported | supported | Compatible | integration tests | `differential_socks5_through_http_upstream` | Chain payloads match |
| SOCKS4/SOCKS4a upstream | supported | supported | Supported | integration tests | none | Unit tested |
| SOCKS5 upstream | supported | supported | Compatible | integration tests | `differential_socks5_through_socks5_upstream` | Chain payloads match |
| Shadowsocks upstream | supported | supported | Experimental | `shadowsocks_tcp.rs` | none | TCP: non-standard AEAD framing (not wire-compatible); UDP: standard AEAD format |
| Trojan upstream | supported | supported (client only) | Partial | unit tests | none | No server side |
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
| Round-robin | default for multiple remotes (`-s rr`) | default for groups | Compatible | `scheduler_runtime.rs` | none | Eggress uses global atomic cursor; pproxy resets on reload |
| First-available | via `-s fa` | `FirstAvailable` scheduler | Compatible | `scheduler_runtime.rs` | none | Both return first eligible upstream |
| Random | not default | `Random` scheduler | Supported | `scheduler_runtime.rs` | none | Eggress-specific; deterministic variant for testing |
| Least-connections | not available | `LeastConnections` scheduler | Supported | `scheduler_runtime.rs` | none | Uses active + in_flight count |
| Health-aware skip | implicit via alive check | explicit health state machine | Compatible | `scheduler_runtime.rs` | none | Eggress: hysteresis state machine |
| Fallback on all fail | `-F` flag (direct only) | `GroupFallback`: reject/direct/use-unhealthy | Partial | `scheduler_runtime.rs` | none | Eggress offers more granular control |
| Retry within group | not documented | not implemented | Compatible | none | none | Single attempt per request |
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
| `-ul` UDP listen | supported | rejected | Intentional non-parity | none | none | Eggress uses SOCKS5 UDP ASSOCIATE instead |
| `-ur` UDP remote | supported | rejected | Intentional non-parity | none | none | Eggress uses SOCKS5 UDP ASSOCIATE instead |
| `--config` TOML | supported | supported | Supported | integration tests | none | Different schema |
| `--rulefile` | supported | rejected | Intentional non-parity | cli_tests | none | Use eggress TOML routing rules |
| `--daemon` | supported | rejected | Intentional non-parity | none | none | Use systemd or a process manager |
| `-b` bind | supported | supported via `-l` | Partial | cli_tests | none | Different flag syntax |
| pproxy compat CLI | `pproxy translate/check/run` | `eggress pproxy translate/check/run` | Compatible | cli_tests | none | Translates pproxy CLI args to TOML config |
| pproxy URI translation | N/A | `eggress pproxy translate` | Compatible | cli_tests | none | Converts pproxy listen/remote URIs to TOML |
| `--reuse` | supported | N/A | Intentional non-parity | none | none | Connection pooling not implemented |

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
| `ss://` / `shadowsocks://` | inbound | TCP | AEAD password | Rejected | **Intentional non-parity** | No inbound listener; upstream-only |
| `trojan://` | inbound | TCP | Password (SHA224) | Rejected | **Intentional non-parity** | No inbound listener; upstream-only |
| `redir://` | inbound | TCP | None | Rejected | **Intentional non-parity** | Requires root, kernel hooks (`SO_ORIGINAL_DST`) |
| `unix://` | inbound | TCP | None | Rejected | **Intentional non-parity** | Not in scope |
| `ssh://` | inbound | TCP | SSH auth | Rejected | **Intentional non-parity** | SSH is not a proxy protocol |

### Upstream Protocols

| Scheme | pproxy role | TCP/UDP | Auth/Encryption | Eggress status | Decision | Rationale |
|--------|-------------|---------|-----------------|----------------|----------|-----------|
| `http://` | upstream | TCP | Basic auth | Supported | **Compatible** | Full parity with differential tests |
| `https://` | upstream | TCP | TLS + Basic auth | Supported | **Compatible** | Maps to `http+tls` in eggress |
| `socks4://` | upstream | TCP | User ID | Supported | **Supported** | Unit tested |
| `socks4a://` | upstream | TCP | User ID | Supported | **Supported** | Alias for `socks4` |
| `socks5://` | upstream | TCP+UDP | Username/password | Supported | **Compatible** | Full parity with differential tests |
| `ss://` / `shadowsocks://` | upstream | TCP+UDP | AEAD password | Supported | **Experimental** | TCP: non-standard AEAD framing (not wire-compatible); UDP: standard AEAD format |
| `trojan://` | upstream | TCP | Password (SHA224) | Supported | **Supported** | Client-only; no server side |
| `ssh://` | upstream | TCP | SSH auth | Rejected | **Intentional non-parity** | SSH transport is out-of-scope for a proxy |
| `direct://` | upstream | TCP+UDP | None | Supported | **Compatible** | Direct connection, no proxy |

### Transport/Wrapping

| Feature | pproxy support | Eggress status | Decision | Rationale |
|---------|---------------|----------------|----------|-----------|
| `+tls` suffix | `socks5+tls://` etc. | Supported | **Compatible** | Maps to TLS wrapper via `eggress-transport-tls` |
| Shadowsocks AEAD ciphers | `aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305` | Supported | **Compatible** | All three AEAD methods supported |
| Shadowsocks stream ciphers | `aes-*-ctr`, `aes-*-cfb`, `rc4-md5`, etc. | Rejected | **Intentional non-parity** | No authentication; vulnerable to bit-flipping |
| ShadowsocksR (SSR) | Supported in some forks | Rejected | **Intentional non-parity** | Non-standard extension; no RFC |
| QUIC transport | Not in pproxy | Rejected | **Intentional non-parity** | Out of scope |
| HTTP/3 | Not in pproxy | Rejected | **Intentional non-parity** | Out of scope |
| WebSocket tunnels | Not in pproxy | Rejected | **Intentional non-parity** | Transport wrapper; not a proxy protocol |

### CLI/Config Features

| Feature | pproxy support | Eggress status | Decision | Rationale |
|---------|---------------|----------------|----------|-----------|
| `-l` listen flag | Supported | Supported | **Compatible** | Same syntax |
| `-r` remote flag | Supported | Supported | **Compatible** | Same syntax |
| `-ul` UDP listen | Supported | Rejected | **Intentional non-parity** | Uses SOCKS5 UDP ASSOCIATE instead |
| `-ur` UDP remote | Supported | Rejected | **Intentional non-parity** | Uses SOCKS5 upstream instead |
| `--daemon` | Supported | Rejected | **Intentional non-parity** | Use systemd or process manager |
| `--ssl` TLS listener | Supported | Rejected | **Intentional non-parity** | Configure TLS in eggress TOML |
| `-b` block rules | Supported | Rejected | **Intentional non-parity** | Use eggress TOML routing rules |
| `--reuse` | Supported | Rejected | **Intentional non-parity** | Connection pooling not implemented |
| `--log` | Supported | Rejected | **Intentional non-parity** | Use tracing-subscriber |
| `--sys` | Supported | Rejected | **Intentional non-parity** | System proxy config not supported |
| `--rulefile` | Supported | Rejected | **Intentional non-parity** | Use eggress TOML routing rules |
| Multi-hop UDP chains | Supported | Rejected | **Intentional non-parity** | One-hop only |
| Persistent HTTP forwarding | Supported | Supported | **Compatible** | Persistent session model with HTTP/1.1 keep-alive |
| Python library | `pproxy.Server()` API | `eggress` package via PyO3 | **Supported** | Python bindings wrap `eggress-embed` API | Not a 1:1 API match; see Python Bindings doc |

### Diagnostics for Unsupported Features

When an unsupported protocol or feature is encountered in pproxy compat mode, eggress produces structured diagnostics:

| Input | Diagnostic type | Error message |
|-------|----------------|---------------|
| `ssh://...` as upstream | `UnsupportedFeature` | "SSH transport is not supported" |
| `unix://...` as upstream | `UnsupportedFeature` | "Unix domain sockets are not supported" |
| `redir://...` as upstream | `UnsupportedFeature` | "Transparent/redir proxy is not supported" |
| `ss://...` as listener | `UnsupportedFeature` | "Shadowsocks listener: not supported as local protocol" |
| `trojan://...` as listener | `UnsupportedFeature` | "Trojan listener: Trojan is upstream-only, not a local listener" |
| Legacy stream cipher URI | `UnsupportedFeature` | "Legacy stream ciphers are not supported; use AEAD methods" |
| `--daemon` flag | `UnsupportedFeature` | "--daemon mode is not supported; use systemd or process manager" |
| `-ul`/`-ur` flags | `UnsupportedFeature` | "-ul/-ur UDP relay uses SOCKS5 UDP ASSOCIATE instead" |
| `--rulefile` flag | `UnsupportedFeature` | "--rulefile is not supported; use eggress TOML routing rules" |
| Unknown URI scheme | `CompatError` | "unsupported protocol: {scheme}" |

All diagnostic messages redact credentials.

### URI Compatibility

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| `http://` scheme | supported | supported | Compatible | cli_tests | none | |
| `socks5://` scheme | supported | supported | Compatible | cli_tests | none | |
| `socks4://` scheme | supported | supported | Compatible | cli_tests | none | |
| `ss://` scheme | supported | supported | Supported | cli_tests | none | |
| `trojan://` scheme | supported | supported | Supported | unit tests | none | |
| `__` chain separator | supported | supported | Compatible | integration tests | none | |
| `user:pass@` auth | supported | supported | Compatible | integration tests | none | |

### Config/Reload Behavior

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| Hot reload | SIGHUP | SIGHUP (routing only) | Partial | `reload.rs` | none | Eggress: routing/upstreams only; pproxy: full config |
| TOML config | supported | supported (different schema) | Partial | integration tests | none | Different schema |
| Runtime state preservation | varies | atomic swap via ArcSwap | Supported | `reload.rs` | none | |

### Python Library/Bindings

| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
|---|---|---|---|---|---|---|
| Python library | `pproxy.Server()` API | `eggress` package (PyO3) | Supported | `test_pproxy_compat.py`, `test_pproxy_redaction.py`, `test_pproxy_concurrency.py` | none | `EggressService`, `EggressHandle`, `start_pproxy`, translation helpers |
| PyPI package | `pip install pproxy` | `pip install eggress` | Supported | wheel tests | none | Wheels for Linux/macOS/Windows; `py.typed` included |

## Coverage Summary

- **TCP proxying (Compatible)**: Full parity for SOCKS5 CONNECT, HTTP CONNECT, SOCKS4/4a, HTTP forward proxy, and direct upstream — differential tests produce byte-exact echo payloads.
- **TCP proxying (Supported)**: SOCKS5 UDP ASSOCIATE inbound is fully implemented; unit tested but not differentially verified against pproxy.
- **UDP relay (Partial)**: Both relay UDP datagrams successfully; pproxy uses its own UDP framing vs. SOCKS5 UDP ASSOCIATE headers. Framing differs but relay behavior matches.
- **Chaining (Compatible / Partial)**: Single-hop TCP chains through pproxy upstream are byte-exact. Multi-hop chains exist but compatibility with pproxy multi-hop is untested.
- **Auth (Compatible)**: Both reject unauthenticated SOCKS5 and HTTP connections.
- **CLI (Compatible / Partial)**: `-l` and `-r` flags share syntax. `-ul` and `-ur` are unsupported (Eggress uses SOCKS5 UDP ASSOCIATE). `--daemon` is not yet implemented.
- **Shadowsocks / Trojan (Experimental / Partial / Supported)**: Shadowsocks TCP upstream has non-standard AEAD framing (not wire-compatible with standard implementations; see TCP audit). Shadowsocks UDP uses standard AEAD format and is interoperable. Trojan is client-only. Neither has differential coverage.
- **Python bindings (Supported)**: `eggress` package via PyO3 wraps `eggress-embed` with `EggressConfig`, `EggressService`, `EggressHandle`, exception hierarchy, context manager, pproxy translation helpers, and async lifecycle. See `docs/PYTHON_BINDINGS.md`.

## Limitations

1. **UDP protocol framing**: pproxy's UDP relay uses a custom framing protocol, not SOCKS5 UDP ASSOCIATE headers. The differential test verifies relay success (payload reaches echo and returns), not wire-level equivalence.
2. **SOCKS5 UDP ASSOCIATE**: pproxy does not implement SOCKS5 UDP ASSOCIATE as a server — it uses its own `-ul` UDP relay. The test verifies eggress's SOCKS5 UDP ASSOCIATE independently, with pproxy as a comparison point for UDP relay capability.
3. **Auth credential exchange**: pproxy embeds credentials in the listen URI fragment; eggress uses config-level auth. The differential tests verify rejection behavior, not credential exchange wire format.
4. **Shadowsocks/Trojan**: Not covered in differential tests — pproxy supports Shadowsocks but eggress's Shadowsocks and Trojan are tested via their own unit/integration test suites.
5. **TLS transport**: Not covered in differential tests — pproxy TLS requires certificate files; tested separately in `eggress-transport-tls` tests.
6. **Multi-hop chains beyond 2**: Only single-hop chains through pproxy are tested. Multi-hop chains within eggress are tested in `integration.rs`.
7. **HTTP forward proxy**: Eggress now supports persistent HTTP forward proxy (multiple requests per connection) matching pproxy behavior (Phase 19).
8. **Hot reload scope**: Eggress reloads routing, upstreams, groups, and health config atomically via `ArcSwap`. Listener topology changes require restart. pproxy reloads its full config on SIGHUP.
9. **Fallback model**: pproxy uses `-F` flag for fallback; eggress falls back to direct connection or rejects based on route rules. Different semantics.
10. **Multi-hop UDP chains**: Not implemented. pproxy supports multi-hop UDP chains; eggress supports single-hop only.

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
`tests/compat/pproxy_manifest.toml`.

### Evidence Levels

| Level | Meaning |
|-------|---------|
| `unimplemented` | Not implemented in eggress |
| `implemented_synthetic` | Implemented but only tested without real pproxy |
| `implemented_differential` | Tested against real pproxy differential behavior |
| `implemented_interop` | Tested via external protocol interop |
| `compatible` | Real pproxy differential or interop evidence |
| `intentional_non_parity` | Deliberately not replicated with rationale |

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
