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
| `EGRESS_RUN_PPROXY_DIFFERENTIAL` | `1` | Enable Phase 41 differential parity harness |
| `EGRESS_REQUIRE_REVERSE_INTEROP` | `1` | Enable reverse proxy pproxy interop tests |
| `EGRESS_REQUIRE_SOAK` | `1` | Enable reverse proxy soak/performance tests |

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
# All Shadowsocks interop tests (gated; require ssserver/sslocal on PATH)
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored

# Only UDP tests
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1 udp
```

### Phase 41 Differential Parity Harness

```bash
# All Phase 41 scenario + CLI tests (gated; requires pproxy==2.7.9)
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored

# Specific scenario
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential differential_socks5_connect -- --ignored --nocapture
```

### Scenario-Driven Oracle Harness

Gate: `EGRESS_ORACLE=1`

```bash
# All oracle scenarios (31 scenarios across 5 categories)
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle -- --ignored

# Specific scenario
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle oracle_tcp_socks5_connect -- --ignored

# Generate JSON report
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle oracle_generate_report -- --ignored
```

### Oracle Harness (Phase A3)

The oracle harness has been expanded with:

#### Declarative Scenario Schema
- TOML-based scenario files under `crates/eggress-testkit/tests/oracle/scenarios/`
- Schema version 1 with validation (73 total scenarios: 31 hardcoded + 42 TOML)
- Maps to A2 composition IDs from `docs/parity/composition_matrix.toml`
- Client actions: Socks5TcpConnect, HttpConnect, HttpForwardGet/Post, Socks4Connect, Socks4aConnect, UdpEchoRoundtrip, etc.

#### Semantic Observations
- `ProxyObservation` model captures: bound addresses, exit codes, connection results, protocol replies, bytes transferred, auth results, timing, cleanup status
- `compare_observations()` produces structured comparison results
- No byte-for-byte text equality requirements for unstable messages

#### Reusable Protocol Probes
- `probes.rs` provides: socks5_tcp_connect, socks5_tcp_connect_auth, socks5_connect_refused, socks5_auth_failure, http_connect, http_connect_refused, http_forward_get, http_forward_post, socks4_connect, socks4a_connect
- Each returns `ProbeResult` with success, bytes_sent, bytes_received, response, error, reply_code

#### Process Supervisor
- `supervisor.rs` provides `SupervisedProcess` with:
  - Process-group ownership (Unix: `process_group(0)`, kill group on cleanup)
  - Bounded stdout/stderr capture (configurable max lines and line bytes)
  - Artifact retention (stdout/stderr logs saved on drop)
  - `ReadinessProbe` enum: TcpPort, StdoutPattern, FixedDelay, FileExists
  - Structured `ProcessExit` with exit code, signal, lifetime

#### CI Tiers
- **FastStructural**: Schema validation, startup, port binding (gate: `EGRESS_ORACLE=1`)
- **CoreDifferential**: HTTP, SOCKS, CLI with pinned pproxy (gate: `EGRESS_ORACLE=1`)
- **ExtendedDifferential**: UDP, TLS, Shadowsocks, Trojan, routing (gate: `EGRESS_ORACLE_EXTENDED=1`)
- **PlatformDifferential**: macOS/Windows/Linux-specific (gate: `EGRESS_ORACLE_PLATFORM=1`)
- **PrivilegedExternal**: Transparent proxy, packet capture (gate: `EGRESS_ORACLE_PRIVILEGED=1`)

#### Report Generation
- JSON reports with observation data, timing tolerances, divergence tracking
- Markdown reports grouped by category with status icons
- Manifest consistency checks (validates capability IDs)
- CI tier filtering

#### Running Oracle Tests

```bash
# Run all oracle scenarios (requires pproxy==2.7.9)
EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle -- --ignored

# Run schema validation tests (no pproxy needed)
cargo test -p eggress-testkit --test oracle_scenario_files

# Run oracle unit tests (no pproxy needed)
cargo test -p eggress-testkit --lib oracle
```

### Reverse Proxy Interoperability Tests

Gate: `EGRESS_REQUIRE_REVERSE_INTEROP=1` (requires pproxy on PATH)

```bash
# All reverse interop tests
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored

# Self-interop tests only (no pproxy needed)
cargo test -p eggress-runtime --test reverse_interop reverse_eggress_self_interop_loopback
cargo test -p eggress-runtime --test reverse_interop reverse_payload_byte_equality_eggress_loopback
```

### Reverse Proxy Soak Tests

Gate: `EGRESS_REQUIRE_SOAK=1` (run with `--test-threads=1`)

```bash
EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1
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
| `differential_http_connect_auth_success` | HTTP CONNECT with valid auth credentials |
| `differential_http_connect_auth_missing` | HTTP CONNECT with missing/invalid auth |
| `differential_http_connect_ipv4_target` | HTTP CONNECT to IPv4 target |
| `differential_http_connect_domain_target` | HTTP CONNECT to domain target |
| `differential_http_connect_ipv6_target` | HTTP CONNECT to IPv6 target |
| `differential_http_connect_refused_target` | HTTP CONNECT to refused target |
| `differential_http_connect_timeout` | HTTP CONNECT upstream timeout (connection refused) |
| `differential_http_connect_client_half_close` | HTTP CONNECT client half-close after tunnel |
| `differential_http_connect_server_half_close` | HTTP CONNECT server half-close after tunnel |
| `differential_http_connect_fragmented_client_payload` | HTTP CONNECT fragmented client payload relay |
| `differential_http_connect_fragmented_upstream_payload` | HTTP CONNECT fragmented upstream payload relay |
| `differential_http_forward_get` | HTTP forward GET request |
| `differential_http_forward_post_with_body` | HTTP forward POST with body |
| `differential_http_forward_head` | HTTP forward HEAD request |
| `differential_http_forward_connection_close` | HTTP forward Connection: close |
| `differential_http_forward_persistent_connection` | HTTP forward persistent connection (two requests) |
| `differential_http_forward_chunked_body` | HTTP forward chunked transfer encoding |
| `differential_http_forward_upstream_connection_close` | HTTP forward upstream Connection: close |
| `differential_http_forward_malformed_request` | HTTP forward malformed request |
| `differential_http_forward_unsupported_transfer_coding` | HTTP forward unsupported Transfer-Encoding |
| `differential_http_forward_auth_success` | HTTP forward auth success |
| `differential_socks4_connect_tcp_echo` | SOCKS4 CONNECT TCP echo |
| `differential_socks4a_connect_domain` | SOCKS4a domain resolution |
| `differential_socks4_user_id_propagation` | SOCKS4 user ID propagation |
| `differential_socks4_domain_fails` | SOCKS4 domain resolution (expected failure) |
| `differential_socks4_refused_target` | SOCKS4 refused target |
| `differential_socks4_malformed_version` | SOCKS4 malformed version byte |
| `differential_socks4_truncated_request` | SOCKS4 truncated request |
| `differential_socks5_connect_ipv6` | SOCKS5 IPv6 target |
| `differential_socks5_connect_domain` | SOCKS5 domain target |
| `differential_socks5_refused_target` | SOCKS5 refused target |
| `differential_socks5_malformed_address_type` | SOCKS5 malformed address type |
| `differential_socks5_unsupported_udp_command` | SOCKS5 unsupported UDP ASSOCIATE command |
| `differential_socks5_early_close_greeting` | SOCKS5 early client close during greeting |
| `differential_socks5_early_close_request` | SOCKS5 early client close during request |
| `differential_socks5_server_half_close` | SOCKS5 server half-close during tunnel |
| `differential_standalone_udp_direct_echo` | Standalone UDP direct echo |
| `differential_standalone_udp_domain_target` | Standalone UDP domain target |
| `differential_standalone_udp_malformed_short_datagram` | Standalone UDP malformed short datagram |
| `differential_standalone_udp_nonzero_frag` | Standalone UDP nonzero FRAG |
| `differential_standalone_udp_two_clients` | Standalone UDP two clients on same listener |
| `differential_standalone_udp_oversized_datagram` | Standalone UDP oversized datagram |
| `differential_standalone_udp_two_targets_from_same_client` | Standalone UDP two targets from same client |
| Various `probe_*` tests | Black-box exploration of pproxy behavior |

### Phase 41 Differential Parity Harness (`pproxy_differential.rs`)

Structured scenario tests comparing eggress with pproxy using the reusable
harness from `eggress_testkit::differential`. Gated on
`EGGRESS_RUN_PPROXY_DIFFERENTIAL=1`.

| Test | Description |
|------|-------------|
| `differential_http_connect` | HTTP CONNECT relay (TCP echo through proxy) |
| `differential_http_forward` | HTTP forward proxy (GET to origin) |
| `differential_socks4_connect` | SOCKS4 CONNECT relay |
| `differential_socks5_connect` | SOCKS5 CONNECT relay |
| `differential_socks5_auth` | SOCKS5 username/password auth |
| `differential_socks5_udp_associate` | SOCKS5 UDP ASSOCIATE (eggress-only on macOS; pproxy UDP broken) |
| `differential_standalone_udp` | Standalone UDP relay (eggress-only on macOS; pproxy broken) |
| `differential_scheduler_round_robin` | Round-robin scheduler with 2 upstreams |
| `differential_block_behavior` | Rule-based connection rejection |
| `differential_tls_listener` | TLS listener (SOCKS5 over TLS) |
| `differential_cli_help_output` | CLI --help output comparison |
| `differential_cli_version_output` | CLI --version output comparison |
| `differential_cli_pproxy_translate` | `eggress pproxy translate` output check |
| `differential_cli_pproxy_check` | `eggress pproxy check` parity check |
| `differential_cli_invalid_uri_diagnostic` | Invalid URI diagnostic message |
| `differential_cli_unsupported_uri_diagnostic` | Unsupported URI diagnostic message |

### Standalone UDP Differential Tests

Standalone UDP tests compare pproxy's `-ul` UDP listener with eggress's
standalone UDP relay (`mode = "standalone_pproxy_udp"`). Both use SOCKS5 UDP
datagram framing (RSV + FRAG + ATYP + ADDR + PORT + PAYLOAD).

| Test | Description |
|------|-------------|
| `differential_standalone_udp_direct_echo` | Direct IPv4 target echo through both relays |
| `differential_standalone_udp_domain_target` | Domain (`localhost`) target echo through both relays |
| `differential_standalone_udp_malformed_short_datagram` | Both silently drop datagrams shorter than 4 bytes |
| `differential_standalone_udp_nonzero_frag` | Both silently drop datagrams with FRAG=1 |
| `differential_standalone_udp_two_clients` | Two different clients on the same UDP listener |
| `differential_standalone_udp_oversized_datagram` | Oversized (70KB) datagram handling |
| `differential_standalone_udp_two_targets_from_same_client` | Same client targeting two different destinations |

### Shadowsocks Interoperability Tests (`interoperability_shadowsocks.rs`)

| Test | Description | Expected Result |
|------|-------------|-----------------|
| `interop_shadowsocks_tcp_eggress_to_external_server` | eggress SOCKS5 → external ssserver → echo | **PASS** (standard SIP003 AEAD framing) |
| `interop_shadowsocks_tcp_external_client_to_eggress` | sslocal → eggress SS server → echo | **PASS** (standard SIP003 AEAD framing) |
| `interop_shadowsocks_tcp_eggress_self_consistent` | eggress-to-eggress SS TCP | **PASS** |
| `interop_shadowsocks_udp_eggress_to_external_server` | eggress SS UDP → external ssserver → echo | **PASS** (standard AEAD format) |
| `interop_shadowsocks_udp_eggress_self_consistent` | eggress-to-eggress SS UDP | **PASS** |
| `interop_shadowsocks_tcp_wrong_password_fails` | Wrong password failure | **PASS** (should fail) |
| `interop_shadowsocks_udp_wrong_password_fails` | Wrong password UDP failure | **PASS** (should fail) |
| `interop_shadowsocks_tcp_aes_128_gcm` | aes-128-gcm method coverage | **PASS** |
| `interop_shadowsocks_tcp_chacha20_ietf_poly1305` | chacha20-ietf-poly1305 method coverage | **PASS** |
| `interop_shadowsocks_tcp_large_payload` | Multi-chunk AEAD payload (>65535-byte test) | **PASS** |
| `interop_shadowsocks_tcp_domain_target` | SOCKS5 domain-name target through SS upstream | **PASS** |
| `interop_shadowsocks_tcp_half_close` | Half-close behavior on Shadowsocks path | **PASS** |
| `interop_shadowsocks_udp_inbound_large_packet` | UDP listener large-packet decoding | **PASS** |
| `interop_shadowsocks_udp_via_toml_config` | UDP via full TOML-configured stack | **PASS** |

### Reverse Proxy Interoperability Tests (`reverse_interop.rs`)

Tests verify eggress's reverse protocol (raw-relay control channel) interoperates
with pproxy and with itself. Two flavours: un-gated self-interop (no external
tools) and gated pproxy interop.

#### Self-Interop Tests (no gate)

| Test | Description |
|------|-------------|
| `reverse_eggress_self_interop_loopback` | eggress server + eggress client roundtrip against loopback echo |
| `reverse_payload_byte_equality_eggress_loopback` | Byte-for-byte payload verification (0..=255 × 4 cycles, 1024 bytes) through external → reverse_server → control → reverse_client → echo |
| `reverse_redacts_credentials_in_logs` | Smoke test for `redact_auth` helper (no leaked passwords) |

#### Gated Interop Tests (`EGRESS_REQUIRE_REVERSE_INTEROP=1`)

| Test | Description |
|------|-------------|
| `gated_pproxy_client_to_eggress_server` | pproxy reverse client (backward mode `socks5+in://`) connects to eggress reverse server; verifies control connection established |
| `gated_eggress_client_to_pproxy_server` | eggress reverse client connects to pproxy reverse server (`bind://` mode); verifies handshake metrics |

### Reverse Proxy Soak Tests (`reverse_soak.rs`)

Performance and resilience tests under sustained load. Gated behind
`EGRESS_REQUIRE_SOAK=1` and run with `--test-threads=1`.

| Test | Description |
|------|-------------|
| `performance_reverse_soak` | 30-second sustained echo roundtrips through reverse pair with auth; asserts at least one successful connection |
| `performance_reverse_reconnect_churn` | 20 sequential echo roundtrips with 10ms inter-iteration delay; asserts 100% success rate |
| `performance_reverse_auth_failure_churn` | 10 sequential wrong-password auth attempts; asserts all fail cleanly with no resource leaks |

> **Phase 21 note**: Shadowsocks TCP now uses standard SIP003 AEAD framing
> (two-AEAD-per-chunk with encrypted length block). All TCP interop tests
> against `ssserver`/`sslocal` from `shadowsocks-rust` are expected to pass.
> See [`docs/protocols/SHADOWSOCKS.md`](protocols/SHADOWSOCKS.md) for details.

## Known Failures and Skips

### Shadowsocks TCP: Standard SIP003 Framing (Phase 21)

As of Phase 21, eggress Shadowsocks TCP uses **standard SIP003 AEAD framing**:

- Two AEAD operations per chunk (encrypted length + encrypted payload).
- Nonces increment by 2 (separate read/write counters, both starting at 2 after
  the address header consumed nonces 0 and 1).
- Wire-compatible with `shadowsocks-rust` / `shadowsocks-libev` / `go-shadowsocks2`.

Previous experimental/non-standard framing (single-AEAD-per-chunk with cleartext
length prefix) was replaced. See
[`docs/protocols/SHADOWSOCKS_TCP_AUDIT.md`](protocols/SHADOWSOCKS_TCP_AUDIT.md)
for the corrective audit history.

**Status**: TCP Shadowsocks is classified as **Supported** (standard wire format,
single-hop upstream only).

### Shadowsocks UDP: Standard-Compliant

The eggress Shadowsocks UDP implementation uses standard AEAD format (salt
prefix, standard nonce derivation). UDP interop tests pass against standard
implementations.

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
- Validating the standard SIP003 AEAD framing status

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

# Manifest validation (no external tools needed)
cargo test -p eggress-testkit validate_real_manifest
```
