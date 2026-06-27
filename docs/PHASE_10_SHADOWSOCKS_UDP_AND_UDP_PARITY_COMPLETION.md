# Phase 10: Shadowsocks UDP & UDP Parity — Completion Record

## Supported Methods

- `aes-128-gcm` (16-byte key)
- `aes-256-gcm` (32-byte key)
- `chacha20-ietf-poly1305` (32-byte key)

Legacy stream ciphers are not supported.

## UDP Wire Format

Standard AEAD Shadowsocks UDP format:

```
salt (16 bytes) + encrypted(address + payload) + AEAD tag (16 bytes)
```

- Per-datagram random salt (16 bytes) prepended to each UDP packet.
- Per-connection subkey derived via HKDF-SHA256 from shared secret + salt.
- Address is encoded inside the encrypted envelope (not in the clear).
- AEAD tag authenticates the full plaintext (address + payload).

This matches the standard Shadowsocks UDP format used by shadowsocks-rust, shadowsocks-libev, and other compliant implementations.

## What Was Implemented

1. `ShadowsocksUdpCodec` — encode/decode for standard AEAD UDP datagrams (`udp.rs`)
2. UDP relay integration through the existing SOCKS5 UDP association path (`eggress-udp`)
3. Single-hop Shadowsocks UDP upstream relay support

## Test List

### Unit tests (9 in `eggress-protocol-shadowsocks`)

| Area | Count | Details |
|------|-------|---------|
| UDP encode/decode roundtrips | 9 | All three methods, various payload sizes, IPv4/IPv6/domain addresses |

### Runtime integration tests (in `eggress-runtime/tests/shadowsocks_udp.rs`)

| Test | Description |
|------|-------------|
| `shadowsocks_udp_upstream_routes_udp_echo` | Full SOCKS5 UDP ASSOCIATE → SS UDP → echo roundtrip |
| `shadowsocks_udp_wrong_password_drops` | Wrong password causes decode failure |
| `shadowsocks_udp_unsupported_method_rejected` | Unsupported method drops packets |
| `shadowsocks_udp_metrics_increment` | Verifies UDP upstream metrics after relay |
| `shadowsocks_udp_target_flow_idle_cleanup` | Target flow idle timeout evicts flow, gauges return to zero |

## Differential Evidence

pproxy differential tests for Shadowsocks UDP are **gated** (not yet run). The pproxy environment has Python 3.14 compatibility issues that prevent running the differential test suite. Unit and runtime integration tests provide the primary correctness evidence.

## UDP Parity Matrix

| Feature | Status | Notes |
|---------|--------|-------|
| Single-hop Shadowsocks UDP upstream | Supported | Standard AEAD format; interoperable with standard Shadowsocks |
| Multi-hop Shadowsocks UDP chains | Not implemented | Intentional scope limitation |
| Inbound Shadowsocks UDP listener | Not implemented | Client/upstream only |

## Verification Commands

All verification commands pass:

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-runtime shadowsocks_udp
```

## Intentional Non-parity

- No inbound Shadowsocks listener (client/upstream only)
- No legacy stream ciphers (security policy)
- No plugin transports (simple-obfs, v2ray-plugin)
- No multi-hop UDP chains
