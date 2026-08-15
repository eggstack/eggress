# Shadowsocks UDP Parity Specification

Eggress maintains two deliberately distinct UDP wire formats: the standard
Shadowsocks AEAD format for maintained Shadowsocks interoperability, and the
pproxy 2.7.9 `PacketCipher` format for the standalone inbound compatibility
path. They share method-specific KDF inputs but are not interchangeable.

## Standard Packet Layout

```
+--------+-----------------------------------+
|  Salt  |  AEAD(address + payload, nonce=0) |
+--------+-----------------------------------+
 variable              variable
```

- **Salt**: Random bytes (`salt_size` for the method: 16, 24, 32, or 32 bytes)
- **Ciphertext**: AEAD-encrypted concatenation of target address and payload
- **Nonce**: 12 zero bytes (all methods)

## Standard Packet Fields

### Salt

- Length: the method-specific salt/IV length
- Purpose: Allows receiver to derive the per-packet subkey
- Generation: Cryptographically random per packet
- Each packet is independently encrypted with its own salt

### AEAD Encryption

- Nonce: 12 zero bytes (all methods)
- Key: Derived with EVP-MD5 expansion followed by HKDF-SHA1 and
  `info = "ss-subkey"`
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

Same as TCP Shadowsocks: EVP_BytesToKey-compatible chained MD5 expansion,
followed by HKDF-SHA1 with the method-sized packet salt and
`info = "ss-subkey"`, expanded to `key_size` bytes.

Each packet has a unique salt, producing a unique subkey.

Source: `crates/eggress-protocol-shadowsocks/src/method.rs`

## Supported Methods

| Method                  | Key Size | Salt Size | Nonce Size | Tag Size |
|-------------------------|----------|-----------|------------|----------|
| `aes-128-gcm`           | 16 bytes | 16 bytes  | 12 bytes   | 16 bytes |
| `aes-192-gcm`           | 24 bytes | 24 bytes  | 12 bytes   | 16 bytes |
| `aes-256-gcm`           | 32 bytes | 32 bytes  | 12 bytes   | 16 bytes |
| `chacha20-ietf-poly1305` | 32 bytes | 32 bytes  | 12 bytes   | 16 bytes |

## Standard API

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

The caller provides a random method-sized salt. The function derives the
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

## pproxy PacketCipher API

`encode_pproxy_udp_packet` and `decode_pproxy_udp_packet` implement pproxy's
UDP `PacketCipher` exactly: method-sized salt, followed by the chunk sequence
`AEAD(length_u16_be, nonce=0) || AEAD(payload, nonce=1)`. The standalone
Shadowsocks inbound listener uses this path so pproxy 2.7.9 UDP clients can
connect directly. Responses on that listener use the same format.

## Maximum Datagram Size

Standard Shadowsocks UDP packets should not exceed 65535 bytes. Packets
larger than this limit are rejected.

## Differences from TCP

- Each UDP packet is self-contained (no stream state)
- Random salt per packet (TCP uses a single salt per connection)
- Nonce is always zero (TCP uses incrementing nonces starting at 1)
- Standard UDP has no chunk framing; the pproxy-compatible path uses one
  length/payload chunk sequence with a zero-based nonce pair.

## Interoperability

This format is interoperable with standard Shadowsocks implementations:

- `shadowsocks-rust` (ssserver/sslocal)
- `shadowsocks-libev` (ss-server/ss-local)
- Other AEAD-capable Shadowsocks implementations

The previous non-standard format (`nonce + ciphertext` with no salt) has
been replaced by this standard format as of Phase 10.
