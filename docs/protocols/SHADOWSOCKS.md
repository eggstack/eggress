# Shadowsocks Protocol

## Overview

Shadowsocks proxy protocol with AEAD cipher support for both TCP and UDP.
Uses HKDF-SHA256 for key derivation from a password.

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

### Limitation

The current implementation sends the encrypted address header but does NOT
encrypt subsequent bidirectional data. Full stream encryption requires a
wrapping stream adapter (planned for future work).

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

Test count: 38 tests across `eggress-protocol-shadowsocks`.

## Limitations

- No legacy stream ciphers (RC4, etc.) -- only AEAD methods
- No plugin transport modes (simple-obfs, v2ray-plugin, etc.)
- No multi-hop UDP (single Shadowsocks hop only)
- TCP bidirectional encryption not yet implemented (address header only)
- No server-side implementation (client/upstream only)
- Maximum frame size: 65,535 bytes
