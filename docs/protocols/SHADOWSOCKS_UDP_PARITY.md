# Shadowsocks UDP Parity Specification

This document specifies the standard Shadowsocks AEAD UDP packet format
as implemented in eggress Phase 10.

## Packet Layout

```
+--------+-----------------------------------+
|  Salt  |  AEAD(address + payload, nonce=0) |
+--------+-----------------------------------+
 variable              variable
```

- **Salt**: Random bytes (`salt_size` for the method, always 16 bytes)
- **Ciphertext**: AEAD-encrypted concatenation of target address and payload
- **Nonce**: 12 zero bytes (all methods)

## Packet Fields

### Salt

- Length: 16 bytes (same for all AEAD methods)
- Purpose: Allows receiver to derive the per-packet subkey
- Generation: Cryptographically random per packet
- Each packet is independently encrypted with its own salt

### AEAD Encryption

- Nonce: 12 zero bytes (all methods)
- Key: Derived from password + salt via HKDF-SHA256
- Plaintext: `address_bytes + payload_bytes`
- Ciphertext includes a 16-byte authentication tag

### Address Encoding

Same as TCP Shadowsocks address format:

| ATYP | Value | Address Length |
|------|-------|----------------|
| 0x01 | IPv4  | 4 bytes        |
| 0x03 | Domain | 1 byte length + domain bytes |
| 0x04 | IPv6  | 16 bytes       |

Followed by 2-byte big-endian port number.

Source: `crates/eggress-protocol-shadowsocks/src/address.rs`

## Key Derivation

Same as TCP Shadowsocks:

1. Compute `IKM = SHA256(password)`
2. Expand with `HKDF-SHA256(salt, IKM, info="ss-subkey")` to `key_size` bytes

Each packet has a unique salt, producing a unique subkey.

Source: `crates/eggress-protocol-shadowsocks/src/method.rs`

## Supported Methods

| Method                  | Key Size | Salt Size | Nonce Size | Tag Size |
|-------------------------|----------|-----------|------------|----------|
| `aes-128-gcm`           | 16 bytes | 16 bytes  | 12 bytes   | 16 bytes |
| `aes-256-gcm`           | 32 bytes | 16 bytes  | 12 bytes   | 16 bytes |
| `chacha20-ietf-poly1305` | 32 bytes | 16 bytes  | 12 bytes   | 16 bytes |

## API

### Encode

```rust
pub fn encode_udp_packet(
    method: CipherMethod,
    password: &[u8],
    target: &TargetAddr,
    payload: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, ShadowsocksError>;
```

The caller provides a random salt (16 bytes). The function derives the
subkey, encrypts address + payload, and returns `salt + ciphertext`.

### Decode

```rust
pub fn decode_udp_packet(
    method: CipherMethod,
    password: &[u8],
    packet: &[u8],
) -> Result<(TargetAddr, Vec<u8>), ShadowsocksError>;
```

The function extracts the salt from the packet prefix, derives the subkey,
decrypts, and returns `(target, payload)`.

## Maximum Datagram Size

Standard Shadowsocks UDP packets should not exceed 65535 bytes. Packets
larger than this limit are rejected.

## Differences from TCP

- Each UDP packet is self-contained (no stream state)
- Random salt per packet (TCP uses a single salt per connection)
- Nonce is always zero (TCP uses incrementing nonces starting at 1)
- No chunk framing (TCP uses 2-byte length prefix per chunk)

## Interoperability

This format is interoperable with standard Shadowsocks implementations:

- `shadowsocks-rust` (ssserver/sslocal)
- `shadowsocks-libev` (ss-server/ss-local)
- Other AEAD-capable Shadowsocks implementations

The previous non-standard format (`nonce + ciphertext` with no salt) has
been replaced by this standard format as of Phase 10.
