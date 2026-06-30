# pproxy Shadowsocks Behavior

## Supported Methods

pproxy supports the following Shadowsocks AEAD cipher methods:

- `aes-128-gcm`
- `aes-256-gcm`
- `chacha20-ietf-poly1305`

Stream ciphers (aes-128-ctr, aes-192-ctr, aes-256-ctr, etc.) are supported by
pproxy but are intentionally excluded from eggress due to lack of authentication.

## URI Forms

pproxy accepts Shadowsocks URIs in SIP002/SIP003 format:

```
ss://method:password@host:port#tag
ss://base64(method:password)@host:port#tag
```

eggress uses the same URI format for upstream definitions.

## TCP Handling

pproxy uses standard SIP003 AEAD framing for TCP:

- Each chunk: 2-byte encrypted length (AEAD) + encrypted payload (AEAD)
- AEAD key derived from HKDF or EVP_BytesKDF depending on the method
- Salt is prepended to the stream on initial connection

eggress now uses identical SIP003 AEAD framing, making it wire-compatible
with pproxy and all standard Shadowsocks implementations.

## UDP Handling

Both pproxy and eggress use standard Shadowsocks UDP AEAD format:

- Each datagram: salt + encrypted(target_addr + payload) + tag
- No length framing (datagram boundaries preserved by UDP itself)

## Known Differences

| Aspect | pproxy | eggress |
|--------|--------|---------|
| Chain depth | Multi-hop | Single-hop only |
| Stream ciphers | Supported | Intentionally excluded |
| TCP framing | SIP003 AEAD | SIP003 AEAD (compatible) |
| UDP framing | Standard AEAD | Standard AEAD (compatible) |
| Password source | URI-embedded | URI-embedded or TOML config |

## Interoperability Evidence

TCP and UDP interop tests against external `ssserver`/`sslocal` (shadowsocks-rust)
confirm wire-level compatibility. See `interoperability_shadowsocks.rs` for the
full test suite covering:

- eggress SOCKS5 inbound → Shadowsocks upstream → external ssserver → TCP echo
- External sslocal → eggress Shadowsocks inbound server → TCP echo
- aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305 method coverage
- UDP echo through all AEAD methods
- Inbound Shadowsocks UDP server with echo and wrong-password rejection
