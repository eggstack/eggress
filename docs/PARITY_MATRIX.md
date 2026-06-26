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
| HTTP forward proxy | ordinary HTTP request handling | single-exchange forward | Partial | integration tests | none | Eggress: one request per connection; pproxy: persistent |
| SOCKS4/4a | server + client | server + client | Supported | integration tests | none | Separate unit tests; no differential |
| SOCKS5 CONNECT | server + client | server + client | Compatible | integration tests | `differential_socks5_connect_tcp_echo` | Byte-exact payload match |
| SOCKS5 UDP ASSOCIATE | client only (relay uses own protocol) | server + client | Supported | `udp.rs` integration | `differential_socks5_udp_associate` | pproxy uses custom UDP framing, not SOCKS5 UDP ASSOCIATE as server |
| Shadowsocks TCP | full AEAD + stream | client/upstream only | Partial | none | none | No inbound listener; upstream has full AEAD stream encryption |
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
| Shadowsocks upstream | supported | supported | Compatible | `shadowsocks_tcp.rs` | none | Full AEAD stream encryption: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305 |
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
| Round-robin | default for multiple remotes | supported | Supported | integration tests | none | Different default policies |
| First-available | via rulefile | supported | Supported | integration tests | none | |
| Random | N/A | supported | Supported | integration tests | none | Eggress-specific |
| Least-connections | N/A | supported | Supported | integration tests | none | Eggress-specific |
| Fallback | `-F` flag | direct fallback or reject | Partial | integration tests | none | Different fallback model |

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
| `-ul` UDP listen | supported | unsupported | Unsupported | none | none | Eggress uses SOCKS5 UDP ASSOCIATE instead |
| `-ur` UDP remote | supported | unsupported | Unsupported | none | none | Eggress uses SOCKS5 UDP ASSOCIATE instead |
| `--config` TOML | supported | supported | Supported | integration tests | none | Different schema |
| `--rulefile` | supported | translated to TOML | Compatible | cli_tests | none | pproxy compat layer translates rule file to `[[rules]]` |
| `--daemon` | supported | not yet | Unsupported | none | none | |
| `-b` bind | supported | supported via `-l` | Partial | cli_tests | none | Different flag syntax |
| pproxy compat CLI | `pproxy translate/check/run` | `eggress pproxy translate/check/run` | Compatible | cli_tests | none | Translates pproxy CLI args to TOML config |
| pproxy URI translation | N/A | `eggress pproxy translate` | Compatible | cli_tests | none | Converts pproxy listen/remote URIs to TOML |
| `--reuse` | supported | N/A | Intentional non-parity | none | none | Connection pooling not implemented |

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
| Python library | `pproxy.Server()` API | not started | Unsupported | none | none | Phase 13-14 target |
| PyPI package | `pip install pproxy` | not started | Unsupported | none | none | Phase 15 target |

## Coverage Summary

- **TCP proxying (Compatible)**: Full parity for SOCKS5 CONNECT, HTTP CONNECT, and direct upstream — differential tests produce byte-exact echo payloads.
- **TCP proxying (Supported)**: SOCKS4/4a and SOCKS5 UDP ASSOCIATE inbound are fully implemented; unit tested but not differentially verified against pproxy.
- **UDP relay (Partial)**: Both relay UDP datagrams successfully; pproxy uses its own UDP framing vs. SOCKS5 UDP ASSOCIATE headers. Framing differs but relay behavior matches.
- **Chaining (Compatible / Partial)**: Single-hop TCP chains through pproxy upstream are byte-exact. Multi-hop chains exist but compatibility with pproxy multi-hop is untested.
- **Auth (Compatible)**: Both reject unauthenticated SOCKS5 and HTTP connections.
- **CLI (Compatible / Partial)**: `-l` and `-r` flags share syntax. `-ul` and `-ur` are unsupported (Eggress uses SOCKS5 UDP ASSOCIATE). `--daemon` is not yet implemented.
- **Shadowsocks / Trojan (Partial / Supported)**: Shadowsocks TCP upstream has full AEAD stream encryption (aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305). Shadowsocks UDP uses standard AEAD format. Trojan is client-only. Neither has differential coverage.
- **Python bindings (Unsupported)**: Not started; planned for later phases.

## Limitations

1. **UDP protocol framing**: pproxy's UDP relay uses a custom framing protocol, not SOCKS5 UDP ASSOCIATE headers. The differential test verifies relay success (payload reaches echo and returns), not wire-level equivalence.
2. **SOCKS5 UDP ASSOCIATE**: pproxy does not implement SOCKS5 UDP ASSOCIATE as a server — it uses its own `-ul` UDP relay. The test verifies eggress's SOCKS5 UDP ASSOCIATE independently, with pproxy as a comparison point for UDP relay capability.
3. **Auth credential exchange**: pproxy embeds credentials in the listen URI fragment; eggress uses config-level auth. The differential tests verify rejection behavior, not credential exchange wire format.
4. **Shadowsocks/Trojan**: Not covered in differential tests — pproxy supports Shadowsocks but eggress's Shadowsocks and Trojan are tested via their own unit/integration test suites.
5. **TLS transport**: Not covered in differential tests — pproxy TLS requires certificate files; tested separately in `eggress-transport-tls` tests.
6. **Multi-hop chains beyond 2**: Only single-hop chains through pproxy are tested. Multi-hop chains within eggress are tested in `integration.rs`.
7. **HTTP forward proxy**: pproxy supports persistent HTTP forward proxy (multiple requests per connection); eggress implements single-exchange forward only.
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
