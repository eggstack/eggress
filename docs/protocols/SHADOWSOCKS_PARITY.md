# Shadowsocks TCP Parity Specification

This document specifies the Shadowsocks TCP wire format as implemented in
Eggress, aligned with pproxy's AEAD behavior from Phase 7. It serves as an
authoritative reference for the cipher suite, key derivation, chunk framing,
and nonce sequencing that Eggress uses for Shadowsocks upstream connections.

## 1. pproxy Shadowsocks Behavior (Phase 7)

pproxy 2.7.9 supports Shadowsocks AEAD ciphers with a standard wire format:

- Standard `ss://` URI: `ss://method:password@host:port`
- AEAD encryption of the full TCP stream (salt + encrypted address header +
  encrypted length chunks + encrypted payload chunks)
- The same AEAD cipher is used for both client→server and server→client
  directions
- Password-based key derivation via HKDF

Eggress matches this behavior for AEAD methods. The initial address header
encryption and bidirectional stream encryption are implemented via
`ShadowsocksAeadStream` using standard SIP003 AEAD framing (two separate AEAD
operations per chunk: encrypted length + encrypted payload). The TCP path is
wire-compatible with standard Shadowsocks implementations. UDP uses the same
standard AEAD format and is interoperable.

Source: `crates/eggress-protocol-shadowsocks/src/`

## 2. Supported Shadowsocks Specifications

| SIP | Title | Status | Notes |
|-----|-------|--------|-------|
| SIP002 | URI and QR Code Format | **Supported** | `ss://method:password@host:port` URI parsing |
| SIP003 | AEAD Ciphers and Key Derivation | **Supported** | HKDF-SHA256 with IKM=SHA256(password) |
| SIP022 | AEAD Padding | **Supported** | Standard AEAD chunk framing with length prefix |
| SIP004 | Stream Ciphers | **Rejected** | Insecure, no authentication |
| SIP008 | Online Configuration | **Rejected** | Not in scope |
| SIP023 | AES-GCM-SIV | **Rejected** | Not in standard AEAD set |

Eggress intentionally rejects all legacy stream cipher specifications (SIP004).
Only AEAD methods are supported.

## 3. Supported AEAD Methods

| Method                  | Key Size | Salt Size | Nonce Size | Tag Size |
|-------------------------|----------|-----------|------------|----------|
| `aes-128-gcm`           | 16 bytes | 16 bytes  | 12 bytes   | 16 bytes |
| `aes-256-gcm`           | 32 bytes | 16 bytes  | 12 bytes   | 16 bytes |
| `chacha20-ietf-poly1305` | 32 bytes | 16 bytes  | 12 bytes   | 16 bytes |

Method names are case-insensitive. The `CipherMethod::parse_method` function
in `method.rs:16` accepts all case variants (e.g., `AES-256-GCM`,
`AES-256-Gcm`).

Unsupported methods return `ShadowsocksError::UnsupportedMethod`.

Source: `crates/eggress-protocol-shadowsocks/src/method.rs`

## 4. Key Derivation

Subkeys are derived per-connection using HKDF-SHA256, following SIP003:

### Steps

1. Compute the initial keying material: `IKM = SHA256(password)`
2. Derive the subkey: `subkey = HKDF-SHA256(salt=connection_salt, IKM, info="ss-subkey")`
3. Output length equals `key_size` for the selected method (16 or 32 bytes)

### Implementation

```rust
pub fn derive_key(&self, password: &[u8], salt: &[u8]) -> Vec<u8> {
    let ikm = Sha256::digest(password);
    let hk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut key = vec![0u8; self.key_size()];
    hk.expand(b"ss-subkey", &mut key)
        .expect("HKDF expand failed");
    key
}
```

### Properties

- **Deterministic**: Same password + salt always produces the same subkey
- **Per-connection**: Each connection uses a unique random salt, producing a
  unique subkey
- **Non-reversible**: SHA256 preimage resistance prevents password recovery
- **Salted**: Different salts produce different subkeys from the same password,
  preventing rainbow table attacks

Source: `crates/eggress-protocol-shadowsocks/src/method.rs:50`

## 5. Salt / Subkey Lifecycle

Each TCP connection generates exactly one salt:

```
Client                                       Server
  |                                            |
  |-- salt (16 bytes, random) --------------->|
  |                                            |
  |  Both sides derive subkey from             |
  |  password + salt using HKDF-SHA256         |
  |                                            |
  |-- encrypted data (using subkey) --------->|
  |<- encrypted data (using subkey) ----------|
```

### Rules

- **Salt**: Generated randomly per connection using `rand::thread_rng()`
- **Salt size**: Always 16 bytes regardless of cipher method
- **Salt transmission**: Sent as the first 16 bytes of the TCP stream
- **Subkey lifetime**: Derived once, used for the entire connection
- **Nonce reset**: Nonces start at 0 for each direction (read/write)
- **No salt reuse**: Random generation with a 128-bit space makes collision
  negligible; no explicit deduplication is needed

## 6. TCP Stream Framing

The Shadowsocks TCP stream consists of an initial handshake followed by
encrypted data chunks:

### Complete Wire Format

```
+--------+-----------------------------------------------+------------------------------------+------------------------------------+-----+
|  Salt  |  AEAD( address_header, nonce=0 )             |  AEAD( len, nonce=N )             |  AEAD( payload, nonce=N+1 )       | ... |
+--------+-----------------------------------------------+------------------------------------+------------------------------------+-----+
 16 bytes           variable                          18 bytes                           variable + 16 bytes
```

### Phase 1: Initial Handshake (first bytes on the wire)

```
+--------+----------------------------------------------+
|  Salt  |  AEAD( address_header, nonce=0x000...000 )  |
+--------+----------------------------------------------+
 16 bytes              variable
```

- **Salt**: 16 random bytes, sent in the clear
- **Address header**: AEAD-encrypted target address (ATYP + address + port)
- **First nonce**: 12 zero bytes (`0x000000000000000000000000`)

### Phase 2: Data Chunks (repeated until close)

Each data chunk consists of **two** separate AEAD operations:

```
+-----------------------------------------------+------------------------------------+
|  AEAD( len=2 bytes, nonce=write_N )          |  AEAD( payload, nonce=write_N+1 ) |
+-----------------------------------------------+------------------------------------+
  2 bytes plaintext + 16 bytes tag = 18 bytes      variable + 16 bytes tag
```

1. **Length chunk**: 2-byte big-endian payload length, AEAD-encrypted (18 bytes on wire)
2. **Payload chunk**: The actual data, AEAD-encrypted with the next nonce

Nonces increment by **2** per chunk (one for length, one for payload).

## 7. Length Chunk Encryption

Each data chunk begins with an AEAD-encrypted length field:

### Wire Format

```
+-----------------------------------------------+
|  AEAD( LEN_H + LEN_L, nonce=write_counter )  |
+-----------------------------------------------+
              2 bytes plaintext           16 bytes tag
              = 18 bytes total on wire
```

### Encoding

```rust
let len = plaintext.len() as u16;
// AEAD-encrypt the 2-byte length with the current write nonce
let encrypted_len = aead_encrypt(key, write_nonce, &len.to_be_bytes())?;
// write_nonce increments by 1 after this operation
```

### Constraints

- **Maximum payload length**: 65,535 bytes (2^16 - 1)
- **Minimum**: 0 bytes (empty chunk for keepalive/probe)
- **Big-endian**: Network byte order (MSB first)

Source: `crates/eggress-protocol-shadowsocks/src/aead.rs`

## 8. Payload Chunk Encryption

After the encrypted length chunk, the payload itself is AEAD-encrypted
with the **next** nonce:

### Wire Format

```
+-------------------------------------+
|  AEAD( payload, nonce=write_N+1 )  |
+-------------------------------------+
   variable plaintext     16 bytes tag
```

### Relationship to Length Chunk

The length chunk and payload chunk are encrypted independently with
**consecutive nonces**:

```
Length chunk:  AEAD( len, nonce = N )
Payload chunk: AEAD( payload, nonce = N + 1 )
```

Where N is the current write nonce counter (starting at 0 for the first
data chunk, after the address header consumed nonce 0).

Source: `crates/eggress-protocol-shadowsocks/src/aead.rs`

## 9. Nonce Sequencing

AEAD nonces are 12 bytes (96 bits) and are managed independently for each
direction of a connection.

### Read Nonce (server→client)

```
Nonce[0]  = 0x000000000000000000000000  (address header)
Nonce[1]  = 0x000000000000000000000001  (first length chunk)
Nonce[2]  = 0x000000000000000000000002  (first payload chunk)
Nonce[3]  = 0x000000000000000000000003  (second length chunk)
Nonce[4]  = 0x000000000000000000000004  (second payload chunk)
...
```

### Write Nonce (client→server)

```
Nonce[0]  = 0x000000000000000000000000  (address header)
Nonce[1]  = 0x000000000000000000000001  (first length chunk)
Nonce[2]  = 0x000000000000000000000002  (first payload chunk)
...
```

### Rules

- **Independent counters**: Read and write nonce counters are completely
  independent. Each direction maintains its own counter starting at 0.
- **Starting value**: Both counters begin at 0 (address header)
- **Increment by 2 per chunk**: Each data chunk consumes two nonces — one
  for the length AEAD operation, one for the payload AEAD operation
- **Ordering**: Nonces must be used in strictly increasing order within each
  direction. Skipping a nonce is a protocol error.
- **No wrap**: With 12-byte (96-bit) nonces and 16-byte tags, the birthday
  bound allows 2^48 packets before nonce reuse risk — effectively unlimited
  for a single connection

### Nonce Counter State Machine

```
          encrypt/decrypt
     ┌─────────────────────────┐
     │                         │
     ▼                         │
  [nonce=0] ──────────────► [nonce=1] ──────────────► [nonce=2] ──► ...

  Each AEAD operation increments the counter by 1.
  Each data chunk consumes two nonces (length + payload).
  Read and write directions have independent counters.
```

## 10. Max Chunk Size

| Parameter | Value | Notes |
|-----------|-------|-------|
| Maximum payload per chunk | 65,535 bytes | 2^16 - 1, fits in a 2-byte length field |
| Maximum encrypted length chunk | 18 bytes | 2 bytes plaintext + 16 bytes AEAD tag |
| Maximum encrypted payload chunk | 65,551 bytes | 65,535 bytes plaintext + 16 bytes AEAD tag |

The `MAX_CHUNK_PAYLOAD` constant is:

```rust
pub const MAX_CHUNK_PAYLOAD: usize = 65535;
```

Implementations may send chunks smaller than 65,535 bytes. The receiver must
accept any chunk size up to this maximum. Chunks exceeding this limit are a
protocol error.

## 11. Unsupported Legacy Ciphers

The following cipher methods are **not supported** by Eggress and will return
`ShadowsocksError::UnsupportedMethod`:

| Cipher | Status | Reason |
|--------|--------|--------|
| `aes-128-ctr` | **Rejected** | No authentication; vulnerable to bit-flipping |
| `aes-192-ctr` | **Rejected** | No authentication |
| `aes-256-ctr` | **Rejected** | No authentication |
| `aes-128-cfb` | **Rejected** | No authentication |
| `aes-192-cfb` | **Rejected** | No authentication |
| `aes-256-cfb` | **Rejected** | No authentication |
| `aes-128-cfb1` | **Rejected** | No authentication; deprecated |
| `aes-128-cfb8` | **Rejected** | No authentication; deprecated |
| `aes-128-cfb11` | **Rejected** | No authentication; non-standard |
| `rc4-md5` | **Rejected** | Known biases and weaknesses |
| `rc4-md5-6` | **Rejected** | Known biases |
| `chacha20-ietf` | **Rejected** | No authentication (use poly1305 variant) |
| `xchacha20-ietf-poly1305` | **Rejected** | Not in standard AEAD set; non-interoperable |
| `salsa20-ietf-poly1305` | **Rejected** | Not in standard AEAD set |
| `bf-cfb` | **Rejected** | Blowfish deprecated |
| `camellia-128-cfb` | **Rejected** | No authentication |
| `camellia-256-cfb` | **Rejected** | No authentication |
| `idea-cfb` | **Rejected** | Weak cipher |
| `seed-cfb` | **Rejected** | No authentication |
| `table` | **Rejected** | XOR with fixed table; trivially breakable |

This is a deliberate security policy, not a feature gap. Stream ciphers
without authentication provide no integrity protection and are vulnerable to
chosen-plaintext and bit-flipping attacks.

## 12. Address Format

Target addresses are encoded in Shadowsocks format (SIP002):

### Wire Format

```
+------+----------+------+
| ATYP | Address  | Port |
+------+----------+------+
  1     variable    2
```

### ATYP Values

| ATYP | Value | Address Length |
|------|-------|----------------|
| `0x01` | IPv4 | 4 bytes |
| `0x03` | Domain | 1 byte length + domain bytes |
| `0x04` | IPv6 | 16 bytes |

### Address Lengths

| ATYP | Total address header size |
|------|--------------------------|
| `0x01` (IPv4) | 1 + 4 + 2 = 7 bytes |
| `0x03` (Domain) | 1 + 1 + len(domain) + 2 bytes |
| `0x04` (IPv6) | 1 + 16 + 2 = 19 bytes |

### Examples

**IPv4 target (192.168.1.1:8080):**
```
01 C0 A8 01 01 1F 90
│  └─────────┘  └──┘
ATYP=IPv4   addr  port=8080
```

**Domain target (example.com:443):**
```
03 0B 65 78 61 6D 70 6C 65 2E 63 6F 6D 01 BB
│  │  └────────────────────────────┘  └──┘
│  len=11  "example.com"             port=443
ATYP
```

**IPv6 target ([::1]:443):**
```
04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 01 01 BB
│  └──────────────────────────────────────────────┘  └──┘
ATYP              ::1 (16 bytes)                    port=443
```

Source: `crates/eggress-protocol-shadowsocks/src/address.rs`

## 13. Connection Lifecycle

### Client-Side (Eggress as Upstream)

```
1. TCP connect to Shadowsocks server
2. Generate random 16-byte salt
3. Derive subkey = HKDF-SHA256(password, salt, "ss-subkey")
4. Encode target address in SS format (ATYP + addr + port)
5. AEAD-encrypt address header with nonce=0x000...0
6. Send: [salt][encrypted_address_header]
7. Begin chunked data transfer:
   a. For each data chunk:
      - Split data into ≤ 65535-byte segments
      - AEAD-encrypt length (2 bytes big-endian) with write_nonce → 18 bytes
      - Increment write_nonce by 1
      - AEAD-encrypt payload with write_nonce
      - Increment write_nonce by 1
   b. For each received chunk:
      - AEAD-decrypt length chunk with read_nonce → get payload length
      - Increment read_nonce by 1
      - AEAD-decrypt payload chunk with read_nonce
      - Increment read_nonce by 1
8. On EOF: send final chunks, close write side
9. On read EOF: drain remaining chunks, close connection
```

### Server-Side (Standard Shadowsocks)

```
1. Accept TCP connection
2. Read 16-byte salt from client
3. Derive subkey = HKDF-SHA256(password, salt, "ss-subkey")
4. AEAD-decrypt address header with nonce=0x000...0
5. Parse target address from decrypted data
6. TCP connect to target
7. Begin bidirectional encrypted relay (same chunk framing)
```

## 14. Wire Format Diagram

### Full TCP Connection (Annotated)

```
    Client                                          Server
      │                                               │
      │──── TCP SYN ─────────────────────────────────>│
      │<─── TCP SYN-ACK ─────────────────────────────│
      │──── TCP ACK ─────────────────────────────────>│
      │                                               │
      │──── [salt: 16 bytes] ────────────────────────>│
      │     [encrypted addr header] ─────────────────>│
      │                                               │
      │     ┌─────────────────────────────────────┐   │
      │     │ Encrypted addr header (AEAD):       │   │
      │     │   plaintext = ATYP + addr + port    │   │
      │     │   nonce = 0x000000000000000000000000 │   │
      │     │   output = ciphertext + 16-byte tag  │   │
      │     └─────────────────────────────────────┘   │
      │                                               │
      │     ═══════ Data Transfer Phase ═══════        │
      │                                               │
      │──── [encrypted len chunk] ──────────────────>│
      │──── [encrypted payload chunk] ──────────────>│
      │<─── [encrypted len chunk] ──────────────────│
      │<─── [encrypted payload chunk] ──────────────│
      │     ...                                       │
      │                                               │
      │──── [encrypted len chunk, len=0] ───────────>│  (optional keepalive)
      │                                               │
      │──── FIN ────────────────────────────────────>│
      │<─── FIN ─────────────────────────────────────│
```

### Chunk Sequence Detail

```
Wire bytes (hex, conceptual):

[salt: 16 bytes random]
[encrypted_addr: AEAD(addr, nonce=0) → ciphertext + tag]

[encrypted_len_chunk_0: AEAD(len0, nonce=1) → ciphertext + tag]      (18 bytes)
[encrypted_payload_chunk_0: AEAD(data0, nonce=2) → ciphertext + tag]

[encrypted_len_chunk_1: AEAD(len1, nonce=3) → ciphertext + tag]      (18 bytes)
[encrypted_payload_chunk_1: AEAD(data1, nonce=4) → ciphertext + tag]

...

[encrypted_len_chunk_N: AEAD(0x0000, nonce=2N+1) → ciphertext + tag]  (empty = close)
[encrypted_payload_chunk_N: AEAD(empty, nonce=2N+2) → ciphertext + tag]
```

## 15. Error Handling

### Error Types

| Error | When | Behavior |
|-------|------|----------|
| `UnsupportedMethod` | Unknown cipher method in URI or config | Connection rejected; method name logged |
| `DecryptionFailed` | AEAD tag mismatch, truncated ciphertext, wrong key | Connection reset immediately |
| `InvalidAddress` | Malformed ATYP, truncated address, unknown ATYP byte | Connection reset; no partial processing |
| `FrameTooLarge` | Chunk exceeds `MAX_CHUNK_PAYLOAD` (65535 bytes) | Connection reset |
| `InvalidKeyLength` | Derived key does not match method's expected size | Connection rejected at setup |
| `PasswordTooLong` | Password exceeds internal buffer limits | Connection rejected at setup |
| `Io` | TCP read/write failure | Connection closed; error propagated |

### Failure Modes

| Scenario | Client behavior | Server behavior |
|----------|----------------|-----------------|
| Wrong password | AEAD tag mismatch on address header; connection reset | N/A (client-side only) |
| Truncated salt | Read returns fewer than 16 bytes; connection closed | Connection reset before decryption |
| Truncated address | AEAD tag mismatch or address parse error; connection reset | Connection reset |
| Corrupted chunk | AEAD tag mismatch; connection reset (no partial data) | Connection reset |
| Oversized chunk | `FrameTooLarge` returned; connection reset | Connection reset |
| Unknown ATYP | `InvalidAddress` returned; connection reset | Connection reset |

### Key Security Properties

- **No partial decryption**: If any AEAD operation fails, the connection is
  immediately reset. No partial plaintext is ever delivered.
- **No retry**: Failed connections are not retried with different parameters.
- **No fallback**: There is no fallback from AEAD to stream ciphers.
- **Constant-time comparison**: AEAD tag verification is performed by the
  underlying `aes-gcm` / `chacha20poly1305` crates, which use constant-time
  tag comparison.

## 16. Security Considerations

### Nonce Reuse

Nonce reuse with the same key catastrophically breaks AEAD confidentiality.
Eggress prevents this by:

1. **Per-connection keys**: Each connection derives a unique subkey from a
   random salt, limiting nonce scope to a single connection
2. **Sequential nonces**: Nonces are simple counters starting at 0, never
   reused within a connection
3. **128-bit salt space**: Random 16-byte salts make salt collision negligible
   (~2^-64 probability after 2^32 connections)

### Replay Attacks

- Shadowsocks AEAD provides **integrity** but not **replay protection** at
  the protocol level
- Replayed packets will decrypt successfully but will be treated as new data
  by the application layer
- Application-layer protocols (TLS, HTTP) provide their own replay protection

### Traffic Analysis

- Encrypted chunks have a fixed overhead (2 bytes length + 16 bytes tag = 18
  bytes per chunk)
- Chunk sizes are visible to observers, leaking information about payload
  sizes
- No explicit padding to fixed-size chunks is implemented (SIP022 padding
  chunks are optional)

### Password Security

- Passwords are hashed with SHA-256 before use as IKM (not stored as plaintext)
- The password never appears in logs (Eggress URI display uses redacted format
  `****:****@`)
- No password stretching (PBKDF2, Argon2) — relies on the password entropy
  itself; Shadowsocks standard behavior

### Key Isolation

- Read and write nonces are independent counters
- Compromise of one direction's nonce state does not affect the other
- Each connection has a unique subkey, isolating connections from each other

## 17. Interoperability Testing

### Synthetic Server Tests

Unit and integration tests in `eggress-protocol-shadowsocks`:

| Test | Description |
|------|-------------|
| `test_shadowsocks_connect_sends_payload` | Verifies initial payload structure (salt + encrypted addr) |
| `test_shadowsocks_connect_all_methods` | Tests all three AEAD methods produce valid payloads |
| `test_encrypt_decrypt_roundtrip_aes128` | AEAD encrypt/decrypt roundtrip with AES-128-GCM |
| `test_encrypt_decrypt_roundtrip_aes256` | AEAD encrypt/decrypt roundtrip with AES-256-GCM |
| `test_encrypt_decrypt_roundtrip_chacha20` | AEAD encrypt/decrypt roundtrip with ChaCha20-Poly1305 |
| `test_encrypt_decrypt_chunk_roundtrip` | Chunk encrypt/decrypt roundtrip |
| `test_tampered_ciphertext_fails` | Tampered ciphertext is rejected |
| `test_wrong_key_fails` | Wrong key produces decryption failure |
| `test_empty_plaintext` | Empty payload handled correctly |
| `test_large_plaintext` | Large (65536-byte) payload handled correctly |
| `test_different_nonces_produce_different_ciphertext` | Nonce uniqueness verified |

### Differential pproxy Tests

Differential tests against pproxy 2.7.9 in `crates/eggress-cli/tests/differential_pproxy.rs`:

| Test | Description | Result |
|------|-------------|--------|
| `differential_socks5_connect_tcp_echo` | SOCKS5 → Shadowsocks upstream chain | Byte-exact payload match |
| `differential_socks5_through_socks5_upstream` | SOCKS5 through pproxy SOCKS5 | Payload matches direct |
| `differential_http_connect_tcp_echo` | HTTP → Shadowsocks upstream chain | Byte-exact payload match |
| `differential_socks5_auth_failure` | Auth failure produces connection reset | Both reject identically |
| `differential_http_auth_failure` | HTTP auth failure | Both reject identically |

### Test Commands

```bash
# Run all Shadowsocks tests
cargo test -p eggress-protocol-shadowsocks

# Run differential pproxy tests (gated)
cargo test -p eggress-cli --test differential_pproxy

# Run pproxy compatibility tests
cargo test -p eggress-pproxy-compat

# Run specific test by name
cargo test -p eggress-protocol-shadowsocks test_encrypt_decrypt_roundtrip_aes256
```

### Test Coverage Summary

- 38+ tests across `eggress-protocol-shadowsocks` crate
- All three AEAD methods tested
- IPv4, IPv6, and domain address formats tested
- Edge cases: empty payload, large payload, truncated input, unknown ATYP
- Tamper detection and wrong-key detection verified
- Nonce uniqueness verified across multiple encryptions

## 18. Implementation Notes

### Current Status

| Component | Status | Notes |
|-----------|--------|-------|
| Address header encryption | **Implemented** | Single-shot AEAD encrypt in `shadowsocks_connect` |
| Chunk encryption/decryption | **Implemented** | `encrypt_chunk` / `decrypt_chunk` in `aead.rs` |
| Full bidirectional stream encryption (TCP) | **Standard** | `ShadowsocksAeadStream` wraps stream with read/write nonce counters; standard SIP003 AEAD framing (two AEAD operations per chunk); wire-compatible with standard Shadowsocks implementations |
| UDP (standard AEAD format) | **Standard** | Standard AEAD format: salt + AEAD(address + payload, nonce=0); interoperable with standard Shadowsocks implementations |
| Inbound listener (TCP) | **Supported** | Explicit `protocol = "shadowsocks"` declaration; not auto-detected in mixed-protocol listeners |

### Source Files

| File | Purpose |
|------|---------|
| `src/method.rs` | Cipher method enum, key derivation, sizes |
| `src/aead.rs` | Frame and chunk AEAD encrypt/decrypt |
| `src/tcp.rs` | TCP connect handshake, initial payload construction |
| `src/address.rs` | Shadowsocks address encoding/decoding |
| `src/udp.rs` | UDP packet encrypt/decrypt (standard AEAD format) |
| `src/error.rs` | Error types |

## 19. References

- [Shadowsocks AEAD Ciphers](https://shadowsocks.org/en/spec/AEAD-Ciphers.html)
- [SIP002 — URI and QR Code](https://shadowsocks.org/en/spec/SIP002-URI.html)
- [SIP003 — AEAD Ciphers](https://shadowsocks.org/en/spec/SIP003-AEAD-Ciphers.html)
- [SIP022 — AEAD Padding](https://shadowsocks.org/en/spec/SIP022-AEAD-Padding.html)
- [RFC 5869 — HKDF](https://datatracker.ietf.org/doc/html/rfc5869)
- [pproxy GitHub repository](https://github.com/nimlang/pproxy)
- [Eggress Parity Matrix](../PARITY_MATRIX.md)
- [Eggress Shadowsocks Protocol](./SHADOWSOCKS.md)
- [Eggress pproxy Parity Spec](../PPROXY_PARITY_SPEC.md)
- [Eggress Differential Tests](../../crates/eggress-cli/tests/differential_pproxy.rs)
