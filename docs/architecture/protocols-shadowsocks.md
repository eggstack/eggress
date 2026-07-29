# eggress-protocol-shadowsocks

`crates/eggress-protocol-shadowsocks/`

Shadowsocks proxy protocol with AEAD cipher support.

## Supported Ciphers

| Method | Key Size | Nonce Size | Tag Size |
|---|---|---|---|
| AES-128-GCM | 16 bytes | 12 bytes | 16 bytes |
| AES-256-GCM | 32 bytes | 12 bytes | 16 bytes |
| ChaCha20-IETF-Poly1305 | 32 bytes | 12 bytes | 16 bytes |

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

- Key derivation via HKDF-SHA256
- Address encoded in encrypted payload (IPv4, IPv6, domain)
- Random nonce per message
- UDP packets use same AEAD with random nonce

## Legacy Detection

- `is_legacy_method()` detects unsupported legacy ciphers → `LegacyMethodUnsupported` error
- SSR detection → `SsrUnsupported` error

## Dependencies

- `eggress-core` — `BoxStream`, `ProtocolId`

See [overview.md](overview.md) for context.
