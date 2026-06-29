# Shadowsocks TCP AEAD Framing — Corrective Audit

## Standardization Complete (Phase 21)

The non-standard TCP AEAD framing identified in this audit has been corrected
as of Phase 21. Eggress now implements standard SIP003-compatible AEAD TCP
stream framing:

- Length chunks are AEAD-encrypted (18 bytes on wire)
- Payload chunks are separately AEAD-encrypted
- Nonce increments by 2 per chunk
- MAX_CHUNK_PAYLOAD = 65535 bytes

The TCP path is now wire-compatible with standard Shadowsocks implementations.

---

This document records a corrective audit of the Eggress Shadowsocks TCP AEAD
framing implementation, performed against the standard Shadowsocks AEAD TCP
format as defined in [SIP003](https://shadowsocks.org/en/spec/SIP003-AEAD-Ciphers.html)
and [SIP022](https://shadowsocks.org/en/spec/SIP022-AEAD-Padding.html).

## 1. Expected Standard Format (Shadowsocks AEAD TCP)

The standard Shadowsocks AEAD TCP stream format has three phases:

### Phase 1: Initial Handshake

```
+--------+----------------------------------------------+
|  Salt  |  AEAD( address_header, nonce=0x000...000 )  |
+--------+----------------------------------------------+
 16 bytes              variable
```

- **Salt**: 16 random bytes, sent in the clear
- **Address header**: AEAD-encrypted target address (ATYP + addr + port)
- **First nonce**: 12 zero bytes (`0x000000000000000000000000`)

### Phase 2: Data Chunks (repeated until close)

Each data chunk consists of **two** separate AEAD operations:

```
+-----------------------------------------------+------------------------------------+
|  AEAD( len=2 bytes, nonce=write_N )          |  AEAD( payload, nonce=write_N+1 ) |
+-----------------------------------------------+------------------------------------+
  2 bytes plaintext + 16 bytes tag = 18 bytes      variable + 16 bytes tag
```

1. **Length chunk**: 2-byte big-endian payload length, AEAD-encrypted → 18 bytes on wire
2. **Payload chunk**: The actual data, AEAD-encrypted with the next nonce

### Nonce Sequencing (Standard)

Nonces increment by **2** per chunk (one for length, one for payload):

```
Nonce[0] = 0x000...000  (address header)
Nonce[1] = 0x000...001  (first length chunk)
Nonce[2] = 0x000...002  (first payload chunk)
Nonce[3] = 0x000...003  (second length chunk)
Nonce[4] = 0x000...004  (second payload chunk)
...
```

### Max Payload (Standard)

- Maximum payload per chunk: **65,535 bytes** (2^16 - 1)
- Maximum encrypted length chunk: 18 bytes (2 + 16 tag)
- Maximum encrypted payload chunk: 65,551 bytes (65,535 + 16 tag)

## 2. Current Eggress Format

### Phase 1: Initial Handshake

Eggress matches the standard for the initial handshake:

```
+--------+----------------------------------------------+
|  Salt  |  AEAD( address_header, nonce=0x000...000 )  |
+--------+----------------------------------------------+
 16 bytes              variable
```

### Phase 2: Data Chunks (non-standard)

Eggress combines length and payload into a **single** AEAD operation with a
cleartext 2-byte length prefix:

```
+------------------------------------------------------+
|  len (2 bytes, cleartext) |  AEAD( len + payload )  |
+------------------------------------------------------+
  2 bytes cleartext              variable + 16 bytes tag
```

### Nonce Sequencing (Eggress)

Nonces increment by **1** per chunk (single combined AEAD):

```
Nonce[0] = 0x000...000  (address header)
Nonce[1] = 0x000...001  (first chunk: len_u16 + payload)
Nonce[2] = 0x000...002  (second chunk: len_u16 + payload)
...
```

### Max Payload (Eggress)

- Maximum payload per chunk: **65,517 bytes** (65,535 - 2 cleartext len bytes - 16 tag bytes)
- The 2-byte length prefix is in the clear, not AEAD-encrypted

## 3. Differences Table

| Aspect | Standard Shadowsocks | Eggress | Impact |
|--------|---------------------|---------|--------|
| Length encoding | AEAD-encrypted (18 bytes on wire) | Cleartext 2 bytes before AEAD chunk | Length is visible to observers; not integrity-protected |
| AEAD operations per chunk | 2 (length + payload) | 1 (combined len_u16 + payload) | Different ciphertext; not wire-compatible |
| Nonce increment per chunk | 2 (one per AEAD op) | 1 (single AEAD op) | Nonce sequences diverge after first chunk |
| Max payload | 65,535 bytes | 65,517 bytes (65,535 - 18) | Reduced; standard implementations may send larger chunks |
| Initial handshake | Identical | Identical | Compatible |
| Key derivation (HKDF-SHA256) | Identical | Identical | Compatible |
| Salt format | Identical | Identical | Compatible |

## 4. Compatibility Impact

### Self-Consistency

The Eggress implementation is **self-consistent**: an Eggress client can
communicate with an Eggress server (or its own synthetic server) without issues.
The combined AEAD operation still provides confidentiality and integrity for
the payload; only the framing differs.

### Cross-Implementation Compatibility

The implementation is **not wire-compatible** with standard Shadowsocks
implementations:

- **shadowsocks-rust** (`ssserver`/`sslocal`): Will fail to decrypt Eggress chunks
  because it expects two separate AEAD operations per chunk and encrypted length headers.
- **shadowsocks-libev** (`ssserver`/`sslocal`): Same incompatibility.
- **pproxy**: Same incompatibility (pproxy uses standard AEAD framing).

An Eggress client connecting to a standard Shadowsocks server (or vice versa)
will fail with AEAD decryption errors after the initial handshake.

### Why This Happened

The stream adapter (`ShadowsocksAeadStream`) was implemented with a simplified
framing model that combines the length prefix into the AEAD operation for
efficiency, rather than performing two separate AEAD operations per chunk.
The initial handshake was implemented correctly to match the standard.

## 5. Correction Decision

### Recommendation: Decision C — Downgrade to Experimental/Non-Standard

Fixing the TCP framing would require significant rework of the stream adapter
(`ShadowsocksAeadStream`) and all chunk encrypt/decrypt paths. The correction
is **out of scope** for this corrective audit per the audit plan criteria:

- The stream adapter is a complex component with independent read/write nonce
  counters, multiple callers, and integration test dependencies.
- The self-consistent behavior is sufficient for Eggress-only deployments.
- The standard UDP format is unaffected and remains interoperable.

**Action taken**: Documentation has been updated across the project to
accurately reflect the non-standard TCP framing status. The TCP Shadowsocks
client is classified as **Experimental** (not wire-compatible with standard
implementations). UDP remains **Supported** (standard-compliant).

### Future Work

If wire-compatibility with standard Shadowsocks is required in the future:
1. Refactor `ShadowsocksAeadStream` to use two AEAD operations per chunk
2. Encrypt the 2-byte length prefix with AEAD (producing 18 bytes on wire)
3. Increment nonces by 2 per chunk instead of 1
4. Update `MAX_FRAME_SIZE` to 65,535 (standard)
5. Add differential tests against `shadowsocks-rust` or `ssserver`
