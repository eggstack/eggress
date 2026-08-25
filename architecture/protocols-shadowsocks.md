# eggress-protocol-shadowsocks — AEAD Ciphers, Legacy Ciphers, SSR Path

Shadowsocks TCP+UDP implementation. Native path is AEAD-only (SIP003);
pproxy's legacy stream ciphers and ShadowsocksR live behind explicit feature
gates and are loudly classified as legacy/compat rather than silently enabled.

## Module map

| File | Role |
|---|---|
| `src/method.rs` | `CipherMethod`: AES-{128,192,256}-GCM, ChaCha20-IETF-Poly1305. Key derivation = `EVP_BytesToKey` (MD5 expansion) then HKDF-SHA1 with info `"ss-subkey"`. `is_legacy_method()` recognizes ~30 legacy names without enabling them |
| `src/aead.rs` | Frame encrypt/decrypt; nonce counter discipline |
| `src/tcp.rs` | `shadowsocks_connect` (client) / `shadowsocks_accept` (server): salt + encrypted [len block][payload block] framing |
| `src/tcp_stream.rs` | `ShadowsocksAeadStream`: bidirectional AsyncRead/Write adapter; key material in `Zeroizing` |
| `src/address.rs` | SOCKS5-style ATYP address encode/decode |
| `src/udp.rs` | Standard SS UDP packets AND pproxy's chunked variant (`encode_pproxy_udp_packet`) |
| `src/nonce.rs` | Checked little-endian nonce counter (overflow-safe) |
| `src/legacy.rs` | Feature `legacy-crypto`: RC4/AES-CFB/CTR/OFB/Blowfish/Camellia/ChaCha20/Salsa20 etc., incl. OTA HMAC-SHA1 modes; warns on every use |
| `src/compat/{plugin,ssr}.rs` | Bounded plugin codec surface; feature-gated pproxy 2.7.9 SSR compatibility stub |

## Wire-format essentials for review

- Chunk payload max 0x3FFF (16 KiB − 1); tags are 16 bytes; tamper detection
  is authenticated-failure, tested explicitly.
- UDP standard: salt + AEAD(address‖payload, nonce=0). pproxy variant reuses
  the TCP chunk format over datagrams.
- Legacy: cleartext IV, stream cipher over address header + data; OTA adds
  truncated HMAC-SHA1 per chunk with sequence numbers.

## Review entry points

- Known-answer tests for pproxy vectors + 19 legacy cipher KATs.
- Verify: `cargo test -p eggress-protocol-shadowsocks`; fuzz target
  `shadowsocks_frame`.
