# Shadowsocks Protocol

## Overview

Shadowsocks proxy protocol with AEAD cipher support. TCP upstream is
**supported** with full bidirectional AEAD stream encryption.

### TCP Status: Supported

Full bidirectional AEAD stream encryption is implemented via
`ShadowsocksAeadStream`, a stream adapter that maintains independent
read/write nonce sequences. The server helper `shadowsocks_accept`
performs the initial handshake and returns the encrypted stream.

The TCP client sends an encrypted address header and all subsequent
bidirectional data is encrypted with per-direction AEAD nonces.

### UDP Status: Experimental

The UDP packet format uses `nonce + encrypted(address + payload)` instead
of the standard Shadowsocks AEAD UDP format (`salt + encrypted(address + payload)`).
This format is non-interoperable with standard Shadowsocks servers (e.g.,
`shadowsocks-rust`, `ssserver`).

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
+--------+---------------------------+
|  Salt  |  Encrypted Address Header |
+--------+---------------------------+
 variable         variable
```

- **Salt**: Random bytes (`salt_size` for the method)
- **Address Header**: AEAD-encrypted target address (nonce = zero bytes of `nonce_size`)

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
+--------+--------------------------------+
| Nonce  |  Encrypted(address + payload)  |
+--------+--------------------------------+
 12 bytes           variable
```

- **Nonce**: Random 12 bytes per packet (unique per packet)
- **Plaintext**: Target address (Shadowsocks format) concatenated with payload
- **AEAD**: Encrypts the entire plaintext with the nonce

Each UDP packet is self-contained. Tampered packets or wrong keys cause
decryption failure.

### Key Differences from TCP

- Each packet has its own random nonce (no stream state)
- Address + payload encrypted together per packet
- No salt in the UDP format (key is pre-derived)

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

Test count: 53+ tests across `eggress-protocol-shadowsocks`, including
stream adapter tests for `ShadowsocksAeadStream` and 5 runtime
integration tests in `shadowsocks_tcp.rs`.

## Limitations

- **UDP format non-interoperable** — uses `nonce + ciphertext` instead of standard `salt + ciphertext`
- No legacy stream ciphers (RC4, etc.) -- only AEAD methods
- No plugin transport modes (simple-obfs, v2ray-plugin, etc.)
- No multi-hop UDP (single Shadowsocks hop only)
- No server-side implementation (client/upstream only)
- Maximum frame size: 65,535 bytes
- Marked as experimental in capability classifier — not selected by default in routing
