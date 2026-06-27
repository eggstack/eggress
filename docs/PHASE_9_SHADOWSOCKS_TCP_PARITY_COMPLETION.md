# Phase 9: Shadowsocks TCP Parity — Completion Record

> **CORRECTIVE AUDIT NOTICE**: A corrective audit of the TCP AEAD framing
> found that the implementation uses non-standard chunk framing (single AEAD
> operation per chunk with cleartext length prefix instead of two separate AEAD
> operations). The TCP framing is **not wire-compatible** with standard
> Shadowsocks implementations (shadowsocks-rust, shadowsocks-libev). This
> completion record should be read with that caveat. The TCP Shadowsocks client
> has been downgraded from "Supported" to "Experimental (Non-Standard Framing)"
> in documentation. UDP remains standard-compliant. See
> `docs/protocols/SHADOWSOCKS_TCP_AUDIT.md` for the full audit.

## Supported Methods

- `aes-128-gcm` (16-byte key)
- `aes-256-gcm` (32-byte key)
- `chacha20-ietf-poly1305` (32-byte key)

## What Was Implemented

1. `NonceCounter` — sequential nonce management with overflow detection (`nonce.rs`)
2. `ShadowsocksAeadStream<S>` — bidirectional AEAD stream adapter implementing `AsyncRead`/`AsyncWrite` (`tcp_stream.rs`)
3. Updated `shadowsocks_connect` to wrap streams with the AEAD adapter (`tcp.rs`)
4. Added `shadowsocks_accept` server-side counterpart (`tcp.rs`)
5. Synthetic Shadowsocks TCP server for testing (`server.rs`)
6. Public `aead_encrypt_raw`/`aead_decrypt_raw` primitives (`aead.rs`)

## Test List

### Unit tests (56 in `eggress-protocol-shadowsocks`)

| Area | Count | Details |
|------|-------|---------|
| Method parsing, key derivation, sizes | 9 | Method enum, key length, salt/tag sizes |
| AEAD encrypt/decrypt roundtrips | 9 | All three methods, various plaintext sizes |
| Address encode/decode | 8 | IPv4, IPv6, domain, edge cases |
| Nonce counter | 4 | Increment, overflow detection, wrap |
| Stream adapter roundtrips | 8 | Small/large data, bidirectional, EOF, flush, multi-chunk |
| Stream adapter tamper/wrong-key | 3 | Tampered ciphertext, tampered length header, wrong key |
| TCP connect/accept with AEAD wrapping | 5 | Full handshake, data relay, error paths |
| UDP encode/decode | 9 | Unchanged from Phase 6 |

### Runtime integration tests (7 in `eggress-runtime/tests/shadowsocks_tcp.rs`)

| Test | Description |
|------|-------------|
| `shadowsocks_upstream_routes_tcp_echo` | Full SOCKS5 → SS → echo roundtrip |
| `http_connect_inbound_routes_tcp_echo` | HTTP CONNECT inbound → SS → echo roundtrip |
| `shadowsocks_upstream_wrong_password_fails` | Wrong password causes connection failure |
| `shadowsocks_upstream_all_methods` | Tests all 3 AEAD methods end-to-end |
| `shadowsocks_upstream_unsupported_method_rejected` | Unsupported method is rejected |
| `shadowsocks_upstream_direct_route_bypasses_ss` | Direct routes bypass SS transport via `direct = true` rule |
| `shadowsocks_upstream_metrics_increment` | Verifies `upstream_open_total{protocol="shadowsocks",outcome="success"}` after relay |

## Interop Evidence

- Synthetic server tests prove client/server interoperability using independently implemented code
- Client (`shadowsocks_connect`) and server (`shadowsocks_accept`/`server.rs`) use separate decryption paths
- pproxy differential tests deferred (pproxy incompatible with Python 3.14 on this system)

## Remaining Issues

- TCP framing is non-standard (see corrective audit notice above)
- UDP wire format uses standard AEAD format (`salt + AEAD(address + payload, nonce=0)`) — interoperable with standard implementations

## Intentional Non-parity

- No inbound Shadowsocks listener (client/upstream only)
- No legacy stream ciphers (security policy)
- No plugin transports (simple-obfs, v2ray-plugin)
- No multi-hop UDP
