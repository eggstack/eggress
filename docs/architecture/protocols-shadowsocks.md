# eggress-protocol-shadowsocks

`crates/eggress-protocol-shadowsocks/`

Shadowsocks proxy protocol with AEAD cipher support.

The strict pproxy 2.7.9 oracle uses these modern AEAD method-specific salt/IV
sizes: `aes-128-gcm` 16 bytes, `aes-192-gcm` 24 bytes, `aes-256-gcm` 32 bytes,
and `chacha20-ietf-poly1305` 32 bytes. These values come from the frozen
`pproxy/cipher.py` classes and are tracked in the phase-0 manifest; they are
not a claim that legacy stream ciphers or OTA are supported.

## Supported Ciphers

| Method | Key Size | Salt/IV Size | Nonce Size | Tag Size |
|---|---:|---:|---:|---:|
| AES-128-GCM | 16 bytes | 16 bytes | 12 bytes | 16 bytes |
| AES-192-GCM | 24 bytes | 24 bytes | 12 bytes | 16 bytes |
| AES-256-GCM | 32 bytes | 32 bytes | 12 bytes | 16 bytes |
| ChaCha20-IETF-Poly1305 | 32 bytes | 32 bytes | 12 bytes | 16 bytes |

## Key Functions

| Function | Description |
|---|---|
| `shadowsocks_accept()` | Server-side TCP session handling |
| `shadowsocks_connect()` | Client-side TCP CONNECT with encrypted header |

## Key Types

| Type | Description |
|---|---|
| `CipherMethod` | AEAD cipher method enum |
| `ShadowsocksError` | Protocol error type with diagnostic codes |
| `ShadowsocksMetrics` | Protocol-specific metrics |

## Wire Format

- Key derivation uses EVP_BytesToKey-compatible MD5 expansion followed by
  HKDF-SHA1 with `info = "ss-subkey"`; output length is the method key size.
- Address encoded in encrypted payload (IPv4, IPv6, domain)
- TCP and UDP salts use the method-specific sizes above. TCP uses a fresh salt
  per direction; UDP uses a fresh salt per datagram.
- Nonces are 12-byte little-endian counters for TCP and zero for each UDP
  packet.

## Legacy Detection

- `is_legacy_method()` detects unsupported legacy ciphers → `LegacyMethodUnsupported` error
- SSR detection → `SsrUnsupported` error

## Dependencies

- `eggress-core` — `BoxStream`, `ProtocolId`

See [overview.md](overview.md) for context.
