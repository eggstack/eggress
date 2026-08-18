# eggress-protocol-shadowsocks

`crates/eggress-protocol-shadowsocks/`

Shadowsocks proxy protocol with AEAD cipher support and an explicit,
feature-gated pproxy legacy compatibility path.

The strict pproxy 2.7.9 oracle uses these modern AEAD method-specific salt/IV
sizes: `aes-128-gcm` 16 bytes, `aes-192-gcm` 24 bytes, `aes-256-gcm` 32 bytes,
and `chacha20-ietf-poly1305` 32 bytes. These values come from the frozen
`pproxy/cipher.py` classes and are tracked in the phase-0 manifest.

## Supported Ciphers

| Method | Key Size | Salt/IV Size | Nonce Size | Tag Size |
|---|---:|---:|---:|---:|
| AES-128-GCM | 16 bytes | 16 bytes | 12 bytes | 16 bytes |
| AES-192-GCM | 24 bytes | 24 bytes | 12 bytes | 16 bytes |
| AES-256-GCM | 32 bytes | 32 bytes | 12 bytes | 16 bytes |
| ChaCha20-IETF-Poly1305 | 32 bytes | 32 bytes | 12 bytes | 16 bytes |

## Compatibility evidence

The four modern methods have pproxy 2.7.9 bidirectional coverage in the
all-method TCP tests and pproxy PacketCipher UDP tests. Maintained Shadowsocks
interoperability covers the standard AEAD framing; the gated suites are the
claim evidence, while local Eggress-to-Eggress tests remain regression checks.
Legacy stream ciphers, OTA, and PacketCipher compatibility are isolated behind
`legacy-crypto`, warning-bearing, and limited to the maintained subset. The
`cast5-cfb`, `idea-cfb`, `rc2-cfb`, and `seed-cfb` names fail explicitly.

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

- `is_legacy_method()` recognizes the exact pproxy `cipher.py`/`cipherpy.py`
  legacy inventory, including the `!` OTA suffix and `-py` aliases.
- With `legacy-crypto`, `legacy_connect()`/`legacy_accept()` implement the
  maintained RustCrypto subset using EVP_BytesToKey and stateful stream
  ciphers. The path emits an insecure-compatibility warning and is never
  enabled by default.
- TCP OTA uses the pproxy address tag, truncated HMAC-SHA1, incremental
  two-byte length/HMAC/data chunks, and monotonic per-direction sequence
  numbers. Wrong or truncated HMACs fail closed.
- `udp::encode_legacy_udp_packet()` and `decode_legacy_udp_packet()` implement
  packet-local pproxy `PacketCipher` framing. OTA is stream-only, matching
  pproxy.
- `cast5-cfb`, `idea-cfb`, `rc2-cfb`, and `seed-cfb` remain recognized but
  unsupported because no maintained safe Rust primitive is included. Without
  `legacy-crypto`, all legacy methods fail with `LegacyMethodUnsupported`.
- Feature-gated pproxy SSR compatibility is isolated under `compat/` and
  implements only pproxy 2.7.9 address framing plus the six built-in plugin
  names. It is not native Shadowsocks AEAD and `tls1.2_ticket_auth` is not
  real TLS.

## SSR compatibility

`ssr://` uses the SOCKS-style IPv4/domain/IPv6 address tags, big-endian port,
and an optional configured user prefix. Plugin metadata is retained in source
order and validated against `plain`, `origin`, `http_simple`,
`tls1.2_ticket_auth`, `verify_simple`, and `verify_deflate`. The code is behind
the `pproxy-legacy` feature; UDP SSR and external SIP003 plugins are not
implemented.

## Dependencies

- `eggress-core` — `BoxStream`, `ProtocolId`

See [overview.md](overview.md) for context.
