# Differential and Interoperability Testing

This document describes the gated differential and interoperability test
environment for eggress. These tests compare eggress behavior against external
proxy implementations and real-world tools.

## Prerequisites

### Python Version

Use **Python 3.11** or **3.12**. Python 3.14 is **not compatible** with the
required pproxy version.

```bash
python3 --version  # Should show 3.11.x or 3.12.x
```

### pproxy Installation

```bash
pip install "pproxy==2.7.9"
```

### Shadowsocks Tools (Optional)

For Shadowsocks interoperability tests, you need `ssserver` and `sslocal` from
shadowsocks-rust or shadowsocks-libev:

```bash
# shadowsocks-rust (recommended)
cargo install shadowsocks-rust
# or from package manager
```

## Environment Variables

### Required Gate Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `EGRESS_REQUIRE_EXTERNAL_INTEROP` | `1` | Enable pproxy differential tests |
| `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP` | `1` | Enable Shadowsocks interop tests |

### Optional Tool Path Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `EGRESS_SSSERVER_BIN` | `ssserver` | Path to ssserver binary |
| `EGRESS_SSLOCAL_BIN` | `sslocal` | Path to sslocal binary |

## Running Gated Tests

### pproxy Differential Tests

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

### Shadowsocks Interoperability Tests

```bash
# All Shadowsocks interop tests (TCP tests will fail due to non-standard framing)
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored

# Only UDP tests (more likely to pass)
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1 udp
```

### Combined Run

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 \
  cargo test -p eggress-cli --test differential_pproxy --test interoperability_shadowsocks -- --ignored
```

## Test Coverage

### pproxy Differential Tests (`differential_pproxy.rs`)

| Test | Description |
|------|-------------|
| `differential_socks5_connect_tcp_echo` | SOCKS5 CONNECT inbound to local TCP echo |
| `differential_http_connect_tcp_echo` | HTTP CONNECT inbound to local TCP echo |
| `differential_socks5_udp_associate` | SOCKS5 UDP ASSOCIATE direct local UDP echo |
| `differential_socks5_through_http_upstream` | SOCKS5 inbound through HTTP CONNECT upstream |
| `differential_socks5_through_socks5_upstream` | SOCKS5 inbound through SOCKS5 upstream |
| `differential_socks5_auth_failure` | SOCKS5 auth failure behavior |
| `differential_http_auth_failure` | HTTP auth failure behavior |
| `differential_refused_target_failure_class` | Refused target failure class equivalence |
| `differential_auth_failure_class` | Auth failure class equivalence |
| `differential_unsupported_route_behavior` | Unsupported route behavior |
| Various `probe_*` tests | Black-box exploration of pproxy behavior |

### Shadowsocks Interoperability Tests (`interoperability_shadowsocks.rs`)

| Test | Description | Expected Result |
|------|-------------|-----------------|
| `interop_shadowsocks_tcp_eggress_to_external_server` | eggress SOCKS5 → external ssserver → echo | **FAIL** (non-standard framing) |
| `interop_shadowsocks_tcp_external_client_to_eggress` | sslocal → eggress SS server → echo | **FAIL** (non-standard framing) |
| `interop_shadowsocks_tcp_eggress_self_consistent` | eggress-to-eggress SS TCP | **PASS** |
| `interop_shadowsocks_udp_eggress_to_external_server` | eggress SS UDP → external ssserver → echo | May pass (standard UDP format) |
| `interop_shadowsocks_udp_eggress_self_consistent` | eggress-to-eggress SS UDP | **PASS** |
| `interop_shadowsocks_tcp_wrong_password_fails` | Wrong password failure | **PASS** (should fail) |
| `interop_shadowsocks_udp_wrong_password_fails` | Wrong password UDP failure | **PASS** (should fail) |
| `interop_shadowsocks_tcp_aes_128_gcm` | aes-128-gcm method coverage | **FAIL** (non-standard framing) |
| `interop_shadowsocks_tcp_chacha20_ietf_poly1305` | chacha20-ietf-poly1305 method coverage | **FAIL** (non-standard framing) |
| `interop_shadowsocks_udp_via_toml_config` | UDP via full TOML-configured stack | May pass |

## Known Failures and Skips

### Shadowsocks TCP: Non-Standard Framing

The eggress Shadowsocks TCP implementation uses **non-standard AEAD framing**:

- **Standard Shadowsocks**: Two separate AEAD operations per chunk (encrypted
  length + encrypted payload), nonces increment by 2.
- **Eggress**: Single AEAD operation with cleartext 2-byte length prefix,
  nonces increment by 1.

This means TCP interop tests against standard Shadowsocks servers (`ssserver`,
`shadowsocks-rust`, `shadowsocks-libev`) **will fail** with AEAD decryption
errors after the initial handshake.

See [`docs/protocols/SHADOWSOCKS_TCP_AUDIT.md`](protocols/SHADOWSOCKS_TCP_AUDIT.md)
for the full corrective audit.

**Status**: The TCP Shadowsocks client is classified as **Experimental** (not
wire-compatible with standard implementations).

### Shadowsocks UDP: Standard-Compliant

The eggress Shadowsocks UDP implementation uses standard AEAD format (salt
prefix, standard nonce derivation). UDP interop tests have a better chance of
passing against standard implementations.

### Python Version Compatibility

pproxy 2.7.9 is not compatible with Python 3.14. Use Python 3.11 or 3.12.

### External Tool Availability

All gated tests skip gracefully if the required external tools are not
installed. The tests document their expected behavior regardless of whether the
external tools are present.

## Running Without External Tools

The gated tests are designed to be skipped in normal CI. They are useful for:

- Manual validation against real proxy implementations
- Verifying protocol compatibility after changes
- Debugging interop issues reported by users
- Validating the non-standard framing status

To run the full test suite without external dependencies:

```bash
# Standard test suite (no external tools needed)
cargo test --workspace

# Property tests
cargo test -p eggress-protocol-socks --test codec_properties
cargo test -p eggress-protocol-http --test connect_properties
cargo test -p eggress-protocol-trojan --test request_properties
cargo test -p eggress-routing --test properties

# Fuzz smoke tests
cargo test -p eggress-protocol-socks --test fuzz_smoke
```
