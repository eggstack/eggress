# Phase 8 Completion: pproxy-Compatible CLI and URI Translation

## Summary

Phase 8 adds a compatibility layer for common Python pproxy invocation and URI shapes. Users can now translate or run common pproxy commands through Eggress without manually writing full Eggress TOML.

## What Was Implemented

### New Crate: `eggress-pproxy-compat`

- URI parser for pproxy-style URIs (`http://`, `socks4://`, `socks5://`, `trojan://`)
- TOML translator generating canonical Eggress config
- Structured error types with credential redaction
- Warning and unsupported feature reporting

### CLI Subcommands

- `eggress pproxy translate` -- Convert pproxy args to Eggress TOML
- `eggress pproxy check` -- Report parity tier and warnings
- `eggress pproxy run` -- Translate and run in one step

### URI Support

| Scheme | Local | Upstream | Notes |
|--------|-------|----------|-------|
| `http://` | Yes | Yes | Full support |
| `socks4://` | Yes | Yes | Full support |
| `socks5://` | Yes | Yes | Full support |
| `trojan://` | No | Yes | Upstream-only |
| `shadowsocks://` | No | Experimental | Unsupported in Phase 8 |

### Generated TOML Features

- Stable naming: `pproxy-local-N`, `pproxy-upstream-N`, `pproxy-chain`, `pproxy-default`
- Explicit `version = 1`
- First-available scheduler for upstream groups
- Config round-trips through `eggress-config` validation

## Test Coverage

### Unit Tests (52 total)

- URI parsing: 12 tests covering all schemes, auth, IPv6, TLS, rules
- Translation: 17 tests covering all supported combinations, scheduler mapping, unsupported flag handling
- Args parsing: 16 tests covering flags, positional arguments, unknown flag detection
- Integration: 7 tests covering TOML generation, credential redaction, stable naming

### Differential Tests

Existing differential tests in `crates/eggress-cli/tests/differential_pproxy.rs` cover:
1. SOCKS5 CONNECT TCP echo
2. HTTP CONNECT TCP echo
3. SOCKS5 UDP ASSOCIATE
4. SOCKS5 through HTTP upstream
5. SOCKS5 through SOCKS5 upstream
6. SOCKS5 auth failure
7. HTTP auth failure

Plus 4 probe tests for undocumented pproxy behaviors.

## What Was NOT Implemented (Non-Goals)

- No new proxy protocols
- No Shadowsocks support
- No Python bindings
- No PyPI packaging
- No multi-hop UDP
- No new runtime architecture

## Verification

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test -p eggress-pproxy-compat
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Corrective Audit Notice

Differential tests in `crates/eggress-cli/tests/differential_pproxy.rs` are
gated (`EGRESS_REQUIRE_EXTERNAL_INTEROP=1`) and have not yet been run
end-to-end. The pproxy environment requires Python 3.11/3.12 (not compatible
with Python 3.14). See `docs/DIFFERENTIAL_TESTING.md` for details.

## Remaining Items for Phase 9

- Probe undocumented pproxy behaviors (SOCKS5 BIND, UDP ASSOCIATE as server, etc.)
- Shadowsocks AEAD key derivation parity
- Trojan password hashing variant investigation
- HTTP forward proxy (non-CONNECT) support
- Multi-hop chain error handling refinement
