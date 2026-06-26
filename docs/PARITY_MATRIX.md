# Differential Interoperability Parity Matrix

This document tracks feature-by-feature comparison between Eggress and pproxy,
with links to the differential tests that prove behavioral equivalence.

## Test File

All differential tests are in `crates/eggress-cli/tests/differential_pproxy.rs`.

Tests are gated:
- `#[ignore]` — not run by default
- `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` — required env var
- Python 3 + pproxy — required runtime

Run with:
```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

## Feature Matrix

| Feature | Eggress | pproxy | Differential Test | Notes |
|---------|---------|--------|-------------------|-------|
| SOCKS5 CONNECT (TCP) | supported | supported | `differential_socks5_connect_tcp_echo` | Byte-exact payload match |
| HTTP CONNECT (TCP) | supported | supported | `differential_http_connect_tcp_echo` | Byte-exact payload match |
| SOCKS5 UDP ASSOCIATE | supported | supported (own relay protocol) | `differential_socks5_udp_associate` | Both relay UDP to echo; protocol framing differs |
| SOCKS5 → HTTP chain | supported | N/A (eggress chains through pproxy) | `differential_socks5_through_http_upstream` | pproxy as upstream; payload matches direct |
| SOCKS5 → SOCKS5 chain | supported | N/A (eggress chains through pproxy) | `differential_socks5_through_socks5_upstream` | pproxy as upstream; payload matches direct |
| SOCKS5 auth rejection | supported | supported | `differential_socks5_auth_failure` | Both reject unauthenticated connections |
| HTTP auth rejection | supported | supported | `differential_http_auth_failure` | Both reject unauthenticated connections |

## Coverage Summary

- **TCP proxying**: Full parity — both SOCKS5 and HTTP CONNECT produce identical echo payloads.
- **UDP relay**: Both relay UDP datagrams; pproxy uses its own UDP framing vs. SOCKS5 UDP ASSOCIATE header. Coarse behavior (relay success) matches.
- **Chaining**: Eggress correctly chains through pproxy as upstream for both HTTP and SOCKS5. Chain payloads match direct-through-proxy payloads.
- **Auth**: Both implementations reject unauthenticated connections for SOCKS5 and HTTP.

## Limitations

1. **UDP protocol framing**: pproxy's UDP relay uses a custom framing protocol, not SOCKS5 UDP ASSOCIATE headers. The differential test verifies relay success (payload reaches echo and returns), not wire-level equivalence.
2. **SOCKS5 UDP ASSOCIATE**: pproxy does not implement SOCKS5 UDP ASSOCIATE as a server — it uses its own `-ul` UDP relay. The test verifies eggress's SOCKS5 UDP ASSOCIATE independently, with pproxy as a comparison point for UDP relay capability.
3. **Auth credential exchange**: pproxy embeds credentials in the listen URI fragment; eggress uses config-level auth. The differential tests verify rejection behavior, not credential exchange wire format.
4. **Shadowsocks/Trojan**: Not covered in differential tests — pproxy supports Shadowsocks but eggress's Shadowsocks and Trojan are tested via their own unit/integration test suites.
5. **TLS transport**: Not covered in differential tests — pproxy TLS requires certificate files; tested separately in `eggress-transport-tls` tests.
6. **Multi-hop chains beyond 2**: Only single-hop chains through pproxy are tested. Multi-hop chains within eggress are tested in `integration.rs`.

## Non-Tested Features (by design)

These features exist in one implementation but not the other, making differential comparison inapplicable:

| Feature | Eggress | pproxy | Why not tested |
|---------|---------|--------|----------------|
| SOCKS4/4a | supported | supported | Covered in `integration.rs` and `interoperability_curl.rs` |
| HTTP forward proxy | supported | supported | Covered in `integration.rs` |
| TOML config reload | supported | N/A | eggress-specific feature |
| Health checks | supported | N/A | eggress-specific feature |
| Route rules | supported | regex rules | Different rule engines; covered in `routing-rules.md` |
| Metrics | supported | N/A | eggress-specific feature |
