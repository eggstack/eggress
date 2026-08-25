# eggress-protocol-shadowsocks

Shadowsocks TCP and UDP implementation. Native path is AEAD-only (SIP003); pproxy's legacy stream ciphers and SSR framing live behind explicit feature gates and are classified as legacy/compat.

## Module map

| File | Role |
|---|---|
| `src/method.rs` | `CipherMethod` enum (4 AEAD variants), `EVP_BytesToKey` MD5 expansion, HKDF-SHA1 subkey derivation, `is_legacy_method` recognition (33 names) |
| `src/aead.rs` | Frame and chunk encrypt/decrypt, `AeadCipher` dispatch, nonce increment, pproxy multi-chunk helpers |
| `src/tcp.rs` | `shadowsocks_connect` (client), `shadowsocks_accept` (server): salt + encrypted address header framing |
| `src/tcp_stream.rs` | `ShadowsocksAeadStream`: bidirectional AsyncRead/Write adapter, per-direction subkey derivation, `Zeroizing` key material |
| `src/address.rs` | SOCKS5-style ATYP address encode/decode (IPv4 0x01, domain 0x03, IPv6 0x04) |
| `src/udp.rs` | Standard SS UDP (`encode_udp_packet`) and pproxy chunked UDP (`encode_pproxy_udp_packet`), legacy UDP feature-gated wrappers |
| `src/nonce.rs` | `NonceCounter`: checked little-endian u64 counter in 12-byte nonce, overflow-safe |
| `src/legacy.rs` | Feature `legacy-crypto`: ~20 stream ciphers incl. Table, RC4, AES-CFB/CTR/OFB, Blowfish, Camellia, ChaCha20/Salsa20; OTA HMAC-SHA1 truncated to 10 bytes |
| `src/compat/mod.rs` | Feature `pproxy-legacy`: SSR framing and plugin codec surface |
| `src/compat/plugin.rs` | Six pproxy 2.7.9 plugins: plain, origin, http_simple, tls1.2_ticket_auth, verify_simple, verify_deflate |
| `src/compat/ssr.rs` | `ssr_connect`/`ssr_accept`: SSR handshake with ordered plugin framing, `PproxyStream` adapter |
| `src/error.rs` | `ShadowsocksError` enum with `LegacyMethodUnsupported` vs `UnsupportedMethod` distinction |
| `src/metrics.rs` | `ShadowsocksMetrics`: atomic counters for TCP/UDP sessions, flows, decrypt failures, method rejects |
| `src/server.rs` | Test-helper server (`run_shadowsocks_server`) |

## Wire format (byte-level)

### AEAD key derivation chain

```
password --[EVP_BytesToKey(MD5, no salt)]--> 48-byte IKM
  truncated to key_size bytes --+
                                 |
salt (random, method.salt_size())|
                                 v
  HKDF-SHA1(ikm, salt, info="ss-subkey") --> subkey (key_size bytes)
```

`EVP_BytesToKey` (`method.rs:150`): `d1 = MD5(password)`, `d2 = MD5(d1 || password)`, ... up to 48 bytes. Truncated to `key_size` (16/24/32) before HKDF. This matches OpenSSL/shadowsocks-rust behavior.

### TCP connect handshake (client to server)

```
Offset  Field                        Encrypted
0..N    salt (method.salt_size())    plaintext
N..N+18 AEAD(len_u16_be, nonce=0)   encrypted (length block)
N+18..  AEAD(address, nonce=1)      encrypted (address block)
```

After the handshake, data chunks use the standard SIP003 framing:

```
AEAD(len_u16_be, nonce=N)     -- 18 bytes (2 plaintext + 16 tag)
AEAD(payload, nonce=N+1)      -- payload_len + 16 bytes
```

Chunk payload max: `0x3FFF` (16383, `MAX_CHUNK_PAYLOAD` in `aead.rs:15`). Zero-length payload signals end-of-stream (`tcp_stream.rs:360`).

Client write nonces start at 2 (address header consumed 0,1). Server read nonces mirror this offset (`tcp_stream.rs:117-120`).

### Server accept handshake

```
Offset  Field                        Encrypted
0..N    salt (method.salt_size())    plaintext
N..N+18 AEAD(len_u16_be, nonce=0)   encrypted
N+18..  AEAD(address, nonce=1)      encrypted
```

Server derives read subkey from client's salt. Server's first response includes its own salt, from which the client derives the read subkey (`tcp_stream.rs:271-301`).

### Standard UDP packet

```
Offset  Field
0..N    salt (method.salt_size())
N..     AEAD(address || payload, nonce=0)
```

Single AEAD encryption over concatenated address + payload with all-zero nonce.

### pproxy chunked UDP packet

```
Offset  Field
0..N    salt (method.salt_size())
N..     [AEAD(len_block, nonce) + AEAD(payload_block, nonce+1)]*
```

Reuses the TCP chunk format over datagrams (`udp.rs:163-185`).

### Legacy stream-cipher handshake

```
Offset  Field
0..N    IV (cleartext, method.iv_len())
N..     stream_cipher(address_header || data)
```

OTA adds `HMAC-SHA1(iv||key, chunk_data)` truncated to 10 bytes per chunk with sequence numbers (`legacy.rs:660-674`).

## Crypto notes

### Cipher inventory

| Method | Key | Salt | Nonce | Tag |
|---|---|---|---|---|
| AES-128-GCM | 16 | 16 | 12 | 16 |
| AES-192-GCM | 24 | 24 | 12 | 16 |
| AES-256-GCM | 32 | 32 | 12 | 16 |
| ChaCha20-IETF-Poly1305 | 32 | 32 | 12 | 16 |

### Nonce discipline

- Initial nonce is all zeros.
- Each standard chunk consumes two nonces: one for the length block, one for the payload block (`aead.rs:244`).
- `NonceCounter` (`nonce.rs:4`) stores a `u64` counter, serialized as little-endian in the first 8 bytes of the 12-byte nonce. Overflow of the 64-bit counter returns `NonceCounterOverflow`.

### Zeroize coverage

- `ShadowsocksAeadStream` wraps subkeys in `Zeroizing<Vec<u8>>` (`tcp_stream.rs:48,52,55`).
- `password_ikm` (cached 48-byte `EVP_BytesToKey` output) is also `Zeroizing` (`tcp_stream.rs:55`).
- `LegacyStream` key, read_iv, write_iv are all `Zeroizing` (`legacy.rs:806-810`).

### Tamper detection

AEAD tag verification is inherent in AES-GCM and Poly1305. Decrypting a tampered ciphertext returns `DecryptionFailed` at the AEAD layer before any length/payload parsing (`aead.rs:78-87`). Tests explicitly verify this for length-block and payload-block tampering (`aead.rs:680-703`).

## Feature gates

| Feature | Modules enabled | Purpose |
|---|---|---|
| `legacy-crypto` | `legacy` | Stream ciphers: AES-CFB/CTR/OFB, RC4, RC4-MD5, Blowfish, Camellia, ChaCha20, Salsa20, etc. `warn!()` emitted on every use (`legacy.rs:796-801`) |
| `pproxy-legacy` | `compat::plugin`, `compat::ssr` | SSR framing, six pproxy plugins (plain, origin, http_simple, tls1.2_ticket_auth, verify_simple, verify_deflate) |

Neither feature is enabled by default (`Cargo.toml:14`). The `is_legacy_method` function in `method.rs:93-133` recognizes 33 legacy cipher names regardless of feature flags, returning `LegacyMethodUnsupported` (not `UnsupportedMethod`) for stable diagnostics.

## Security notes

- `EVP_BytesToKey` is a legacy KDF; its MD5 expansion is intentional for pproxy/shadowsocks-rust compatibility, not a security choice.
- Legacy stream ciphers provide no authentication. OTA (HMAC-SHA1, 10-byte truncation) is optional and legacy-only.
- `MAX_DECOMPRESSED_FRAME` (256 KiB, `compat/plugin.rs:15`) bounds decompression in the `verify_deflate` plugin to prevent decompression bombs.
- UDP packets use nonce=0 for all datagrams; each packet carries its own random salt, ensuring distinct subkeys.
- Salt is generated randomly per TCP session via `rand::thread_rng()` (`tcp.rs:32-33`).

## Test coverage map

| Category | Location | What it covers |
|---|---|---|
| KATs (AEAD subkey + ciphertext) | `aead.rs:575-622` | 4 methods, pproxy-captured vectors |
| KATs (pproxy UDP) | `udp.rs:683-730` | 4 methods, pproxy-captured datagram vectors |
| KATs (legacy ciphers) | `legacy.rs:1150-1183` | 19 pproxy 2.7.9 known-answer vectors |
| Roundtrip (TCP connect/accept) | `tcp.rs:160-400` | All 4 AEAD methods, echo-back verification |
| Stream adapter | `tcp_stream.rs:561-936` | Small/large data, bidirectional, EOF, tamper detection, partial reads |
| Address encode/decode | `address.rs:106-178` | IPv4, IPv6, domain, edge cases |
| UDP encode/decode | `udp.rs:270-680` | Roundtrips, layout verification, tamper, oversized, all methods |
| Legacy stream roundtrip | `legacy.rs:1125-1147` | Fragmentation invariance for 6 ciphers |
| Plugin codec | `compat/plugin.rs:341-418` | VerifySimple/Deflate roundtrips, CRC rejection, decompression bomb |
| SSR roundtrip | `compat/ssr.rs:287-311` | Full SSR connect/accept with auth prefix + VerifySimple |
| Fuzz smoke | `tests/fuzz_smoke.rs` | 10 hand-crafted address parser inputs |
| Fuzz target | `fuzz/fuzz_targets/shadowsocks_frame.rs` | Arbitrary bytes to `decode_address` |

## Reviewer gotchas

1. **Nonce offset asymmetry**: client write nonces start at 2; server read nonces start at 2; peer nonces start at 0. The address header consumes nonces 0 (length) and 1 (payload).
2. **`EVP_BytesToKey` truncation**: produces 48 bytes but only `key_size` bytes are passed to HKDF (`method.rs:80`). Passing the full 48 bytes would produce wrong subkeys.
3. **Legacy cipher count**: `is_legacy_method` recognizes 33 names; `LegacyMethod::parse` implements ~20 (the rest return `LegacyMethodUnsupported` from `parse` but `is_legacy_method` still returns true).
4. **UDP nonce=0**: every UDP datagram uses a fresh random salt but always nonce=0. This is correct because each salt derives a unique subkey.
5. **Zero-length payload = EOF**: a standard chunk with `len=0` is the stream terminator (`tcp_stream.rs:359-362`).
6. **pproxy-legacy vs legacy-crypto**: `pproxy-legacy` enables SSR framing/plugins; `legacy-crypto` enables stream ciphers. They are independent.

## See also

- [protocols-trojan.md](protocols-trojan.md)
- [overview.md](overview.md)
