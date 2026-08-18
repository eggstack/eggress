# Shadowsocks Legacy Stream Ciphers (pproxy Behavior)

This document captures pproxy 2.7.9's behavior with legacy (non-AEAD) Shadowsocks
stream ciphers. All observations are from the pinned pproxy oracle, not from
third-party SSR or Shadowsocks community documentation.

## Supported Legacy Stream Ciphers

pproxy exposes the following exact legacy inventory across `cipher.py` and
`cipherpy.py`. Eggress implements the maintained subset behind `legacy-crypto`:

| Cipher | Key Size | IV Size | Mode |
|--------|----------|---------|------|
| `aes-128-ctr` | 16 bytes | 16 bytes | CTR |
| `aes-192-ctr` | 24 bytes | 16 bytes | CTR |
| `aes-256-ctr` | 32 bytes | 16 bytes | CTR |
| `aes-128-cfb` | 16 bytes | 16 bytes | CFB |
| `aes-192-cfb` | 24 bytes | 16 bytes | CFB |
| `aes-256-cfb` | 32 bytes | 16 bytes | CFB |
| `rc4-md5` | 16 bytes | 16 bytes | RC4-MD5 |
| `chacha20-ietf`, `chacha20`, `xchacha20` | 32 bytes | 8/12/24 bytes | Stream |
| `salsa20` | 32 bytes | 8 bytes | Stream |
| `aes-*-cfb1`, `aes-*-cfb8`, `aes-*-ofb` | method-specific | 16 bytes | CFB/OFB |
| `bf-cfb`, `des-cfb`, `camellia-*-cfb` | method-specific | 8/16 bytes | CFB |

`cast5-cfb`, `idea-cfb`, `rc2-cfb`, and `seed-cfb` are recognized inventory
members but remain unsupported because no maintained safe Rust primitive is
included. The exact `-py` aliases and `!` OTA suffix are recognized.

Method names are case-insensitive in pproxy.

## URI Forms

pproxy accepts legacy stream cipher URIs using the same Shadowsocks URI format:

```
ss://method:password@host:port
ss://base64(method:password)@host:port
```

Examples:

```
ss://aes-256-ctr:mypassword@10.0.0.1:8388
ss://rc4-md5:mypassword@10.0.0.1:8388
ss://aes-128-cfb:mypassword@10.0.0.1:8388
```

The method name appears in the userinfo section before the colon separator.

## OTA (One-Time Authentication) Behavior

pproxy 2.7.9 implements OTA when the method has a `!` suffix. The destination
header carries the OTA flag and a truncated HMAC-SHA1 keyed by `IV + derived
key`; each later chunk carries a two-byte length, a truncated HMAC-SHA1 keyed
by `IV + big-endian sequence`, and the data. Eggress verifies this framing
incrementally and hard-fails malformed or incorrectly authenticated chunks.

## TCP Behavior

Legacy stream ciphers differ from AEAD in the following ways:

### Stream Cipher TCP Framing

```
+--------+----------------------------------------------+
|  IV    |  Encrypted( address_header + payload )       |
+--------+----------------------------------------------+
  16 bytes            variable (continuous stream)
```

- **IV**: 16 random bytes (or 12 for chacha20-ietf), sent in the clear
- **Payload**: Address header + data encrypted as a continuous stream
- No length framing (stream ciphers operate on bytes, not chunks)
- No authentication tag

### Key Derivation

pproxy uses `EVP_BytesToKey` (OpenSSL legacy KDF) for key derivation from the
password, similar to AEAD methods. The derived key is used directly for the
stream cipher.

### Differences from AEAD

| Aspect | Stream Ciphers | AEAD |
|--------|---------------|------|
| Authentication | None | 16-byte tag per chunk |
| Length framing | None (continuous stream) | 2-byte encrypted length per chunk |
| Tamper detection | None | Yes (AEAD tag) |
| Replay protection | None | Yes (nonce-based) |
| Bit-flipping attacks | Vulnerable | Protected by AEAD |

## Security Concerns

Legacy stream ciphers have significant security weaknesses:

1. **No authentication**: Stream ciphers provide confidentiality only. An attacker
   can modify ciphertext without detection (bit-flipping attacks).

2. **Vulnerable to bit-flipping**: Because there is no authentication tag, an
   attacker can flip bits in the ciphertext and the corresponding plaintext bits
   will be flipped. This allows manipulation of address headers and payload.

3. **No replay protection**: There is no mechanism to detect replayed ciphertexts.
   An attacker can capture and replay a session.

4. **Known cipher weaknesses**: `rc4-md5` uses the RC4 stream cipher which has
   known statistical biases. `chacha20-ietf` (without poly1305) lacks authentication.

5. **No RFC standard**: Stream cipher Shadowsocks was never formally standardized
   in an RFC. The AEAD ciphers are the standard defined in SIP003.

## Eggress Status

**Optional compatibility**: Legacy stream ciphers are implemented only with the
explicit `legacy-crypto` feature and always emit an insecure compatibility
warning. Default/common builds recognize the method and reject it with a clear
diagnostic:

```
unsupported feature: legacy stream cipher requires the optional legacy-crypto feature
```

See `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md` for the decision record.

## Source

All behavior captured from pproxy 2.7.9 (Python package) during the Phase 9
parity audit. This is not derived from third-party SSR documentation or
community wikis.
