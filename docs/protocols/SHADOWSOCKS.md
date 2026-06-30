# Shadowsocks Protocol

## Overview

Shadowsocks proxy protocol with AEAD cipher support. Both TCP and UDP use
standard, wire-compatible AEAD framing per SIP003.

### TCP Status: Standard (SIP003 AEAD Framing)

Bidirectional AEAD stream encryption is implemented via
`ShadowsocksAeadStream` using standard SIP003-compatible framing:
each data chunk consists of two separate AEAD operations (encrypted length
chunk + encrypted payload chunk). The TCP path is wire-compatible with standard
Shadowsocks implementations (shadowsocks-rust, shadowsocks-libev, pproxy).

See [TCP Audit](SHADOWSOCKS_TCP_AUDIT.md) for the history of the framing
correction (Phase 21) and [TCP Parity](SHADOWSOCKS_PARITY.md) for the full
wire format specification.

### UDP Status: Standard

The UDP packet format uses the standard Shadowsocks AEAD UDP format:
`salt + encrypted(address + payload)`. This is interoperable with standard
Shadowsocks implementations (e.g., `shadowsocks-rust`, `ssserver`).

See [UDP Parity](SHADOWSOCKS_UDP_PARITY.md) for the full wire format
specification.

### Inbound Listener: Supported

Eggress can act as a Shadowsocks server (inbound listener) accepting
standard Shadowsocks AEAD TCP connections. The inbound listener is configured
via the `protocol = "shadowsocks"` listener directive with a `method` and
`password` in the credentials section. Shadowsocks is **not** auto-detected
in mixed-protocol listeners because the wire format is encrypted and carries
no detectable signature.

Source: `crates/eggress-protocol-shadowsocks/src/`

## Supported Cipher Methods

| Method                  | Key Size | Salt Size | Nonce Size | Tag Size |
|-------------------------|----------|-----------|------------|----------|
| `aes-128-gcm`           | 16 bytes | 16 bytes  | 12 bytes   | 16 bytes |
| `aes-256-gcm`           | 32 bytes | 16 bytes  | 12 bytes   | 16 bytes |
| `chacha20-ietf-poly1305` | 32 bytes | 16 bytes  | 12 bytes   | 16 bytes |

Method names are case-insensitive. Unsupported methods return `UnsupportedMethod`.

## Key Derivation

Subkeys are derived using HKDF-SHA256:

1. Compute `IKM = SHA256(password)`
2. Expand with `HKDF-SHA256(salt, IKM, info="ss-subkey")` to `key_size` bytes

Source: `crates/eggress-protocol-shadowsocks/src/method.rs:50`

## TCP Wire Format

### Initial Payload

```
+--------+----------------------------------------------+
|  Salt  |  AEAD( address_header, nonce=0x000...000 )  |
+--------+----------------------------------------------+
 16 bytes              variable
```

- **Salt**: 16 random bytes, sent in the clear
- **Address Header**: AEAD-encrypted target address (nonce = 12 zero bytes)

### Data Chunks (repeated until close)

Each data chunk consists of two separate AEAD operations:

```
+-----------------------------------------------+------------------------------------+
|  AEAD( len=2 bytes, nonce=write_N )          |  AEAD( payload, nonce=write_N+1 ) |
+-----------------------------------------------+------------------------------------+
  2 bytes plaintext + 16 bytes tag = 18 bytes      variable + 16 bytes tag
```

1. **Length chunk**: 2-byte big-endian payload length, AEAD-encrypted (18 bytes on wire)
2. **Payload chunk**: The actual data, AEAD-encrypted with the next nonce

Nonces increment by **2** per chunk (one for length, one for payload).

Maximum payload per chunk: **65,535 bytes** (2^16 - 1).

### Address Format

```
+------+----------+------+
| ATYP | Address  | Port |
+------+----------+------+
  1     variable    2
```

| ATYP | Value | Address Length |
|------|-------|----------------|
| 0x01 | IPv4  | 4 bytes        |
| 0x03 | Domain | 1 byte length + domain bytes |
| 0x04 | IPv6  | 16 bytes       |

Source: `crates/eggress-protocol-shadowsocks/src/address.rs`

## UDP Wire Format

### Packet Structure

```
+--------+-----------------------------------+
|  Salt  |  AEAD(address + payload, nonce=0) |
+--------+-----------------------------------+
 variable              variable
```

- **Salt**: Random bytes (`salt_size` for the method), unique per packet
- **Plaintext**: Target address (Shadowsocks format) concatenated with payload
- **AEAD**: Encrypts the entire plaintext with nonce = zero bytes

Each UDP packet is self-contained. Tampered packets or wrong keys cause
decryption failure.

### Key Differences from TCP

- Each packet has its own random salt (no stream state)
- Salt is per-packet (random), not session-wide like TCP
- Address + payload encrypted together per packet

## URI Format

```
ss://method:password@host:port
```

Example: `ss://aes-256-gcm:mypassword@192.168.1.1:8388`

## Test Coverage

- Encrypt/decrypt roundtrips for IPv4, IPv6, and domain targets
- All three cipher methods tested
- UDP packet encode/decode roundtrips
- Tampered packet detection
- Wrong key detection
- Empty and large payload handling
- Packet too short detection
- Nonce uniqueness verification
- Address encoding/decoding edge cases (truncated, unknown ATYP)
- TCP connect sends correct payload structure
- UDP standard AEAD format (salt + encrypted payload)
- UDP interoperability with standard Shadowsocks format
- Standard SIP003 AEAD TCP framing (two AEAD ops per chunk)
- Inbound listener protocol handling

Test count: 53+ tests across `eggress-protocol-shadowsocks`, including
stream adapter tests for `ShadowsocksAeadStream` and runtime
integration tests in `shadowsocks_tcp.rs`.

## Inbound Listener Configuration

Shadowsocks inbound listeners accept standard AEAD TCP connections from
Shadowsocks clients. Because the wire format is encrypted with no
detectable signature, Shadowsocks **must** be explicitly declared as the
protocol in the listener configuration — it is not auto-detected in
mixed-protocol listeners.

### Example TOML Configuration

```toml
[[listener]]
protocol = "shadowsocks"
bind = "0.0.0.0:8388"

[listener.credentials]
method = "aes-256-gcm"
password = "your-secret-password"

[[upstream]]
bind = "direct"
```

### Supported Client Software

Standard Shadowsocks clients can connect to the inbound listener:

- `shadowsocks-rust` (sslocal)
- `shadowsocks-libev` (ss-local)
- Other AEAD-capable Shadowsocks implementations

## Limitations

- No legacy stream ciphers (RC4, etc.) -- only AEAD methods. Legacy stream cipher methods (aes-*-ctr, aes-*-cfb, rc4, rc4-md5, chacha20-ietf, etc.) are detected at parse time via `is_legacy_method()` and produce a `LegacyMethodUnsupported` error with a helpful message suggesting AEAD methods.
- No plugin transport modes (simple-obfs, v2ray-plugin, etc.)
- No multi-hop UDP (single Shadowsocks hop only)
- Shadowsocks is not auto-detected in mixed-protocol listeners (encrypted
  wire format has no detectable signature; must be declared explicitly)
- ShadowsocksR (SSR) is intentionally unsupported. SSR URIs (`ssr://`) are parsed and rejected with a clear `SsrUnsupported` error. See ADR at `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`.
