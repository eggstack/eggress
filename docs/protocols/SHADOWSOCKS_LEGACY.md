# Shadowsocks Legacy Stream Ciphers (pproxy Behavior)

This document captures pproxy 2.7.9's behavior with legacy (non-AEAD) Shadowsocks
stream ciphers. All observations are from the pinned pproxy oracle, not from
third-party SSR or Shadowsocks community documentation.

## Supported Legacy Stream Ciphers

pproxy supports the following stream ciphers for Shadowsocks:

| Cipher | Key Size | IV Size | Mode |
|--------|----------|---------|------|
| `aes-128-ctr` | 16 bytes | 16 bytes | CTR |
| `aes-192-ctr` | 24 bytes | 16 bytes | CTR |
| `aes-256-ctr` | 32 bytes | 16 bytes | CTR |
| `aes-128-cfb` | 16 bytes | 16 bytes | CFB |
| `aes-192-cfb` | 24 bytes | 16 bytes | CFB |
| `aes-256-cfb` | 32 bytes | 16 bytes | CFB |
| `rc4-md5` | 16 bytes | 16 bytes | RC4-MD5 |
| `chacha20-ietf` | 32 bytes | 12 bytes | Stream |

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

pproxy 2.7.9 does not appear to implement Shadowsocks OTA (One-Time Authentication)
for stream ciphers. OTA was an older authentication mechanism used with stream ciphers
that was deprecated in favor of AEAD. pproxy's stream cipher connections use the
plaintext method identifier and password for key derivation without additional
authentication.

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

**Rejected**: Legacy stream ciphers are intentionally not implemented in eggress.

When a legacy stream cipher method is used in a pproxy URI, eggress recognizes the
method name and rejects it with a clear diagnostic:

```
unsupported feature: Legacy stream ciphers are not supported; use AEAD methods
```

See `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md` for the decision record.

## Source

All behavior captured from pproxy 2.7.9 (Python package) during Phase 7 parity
audit and subsequent phases. This is not derived from third-party SSR documentation
or community wikis.
