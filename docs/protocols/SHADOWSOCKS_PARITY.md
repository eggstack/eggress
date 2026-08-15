# Shadowsocks TCP Parity Specification

This is the current wire-format reference for Eggress Shadowsocks TCP. The
strict compatibility target is pproxy 2.7.9, with maintained Shadowsocks
implementations used for the standard-method interop checks.

## Methods and sizes

| Method | Key size | Salt/IV size | Nonce size | Tag size |
|---|---:|---:|---:|---:|
| `aes-128-gcm` | 16 | 16 | 12 | 16 |
| `aes-192-gcm` | 24 | 24 | 12 | 16 |
| `aes-256-gcm` | 32 | 32 | 12 | 16 |
| `chacha20-ietf-poly1305` | 32 | 32 | 12 | 16 |

Legacy stream ciphers, OTA, plugins, and SSR remain intentionally rejected.

## Key derivation

For each direction, pproxy derives a master key with the EVP_BytesToKey MD5
sequence, then derives a method-sized subkey as follows:

1. `D_i = MD5(D_(i-1) || password)`, with `D_0` empty.
2. Concatenate digest blocks and take the first method key-size bytes.
3. HKDF-Extract with SHA-1, using the method-sized salt as HKDF salt.
4. HKDF-Expand with `info = "ss-subkey"` to the method key size.

The implementation is in
`crates/eggress-protocol-shadowsocks/src/method.rs`. Deterministic vectors
for all four methods are tested in `test_pproxy_known_answer_vectors`.

## Direction and handshake state

Each direction has an independent salt, subkey, and nonce counter. The client
direction begins with a method-sized salt and an encrypted address chunk. The
first chunk may also contain application data (standard clients commonly
coalesce it):

```text
method-sized salt
AEAD(2-byte first-chunk length, nonce=0)
AEAD(address [+ first payload], nonce=1)
AEAD(next data length, nonce=2)
AEAD(next data payload, nonce=3)
...
```

When the first chunk contains application data, the next two lines describe
the following chunk; otherwise they describe the first application-data
chunk.

The server response direction starts with its own method-sized salt and then
uses nonce 0 for its first data length block; it has no address header. All
nonces are 12-byte little-endian counters, and each data chunk consumes two
successive nonces.

## Data chunks

Each data chunk is two independent AEAD operations:

```text
AEAD(length_u16_be, nonce=N) || AEAD(payload, nonce=N+1)
```

The encrypted length block is always 18 bytes (2-byte length plus the
16-byte tag). The pproxy `PACKET_LIMIT` is 16,383 bytes, so the largest
encrypted payload block is 16,399 bytes. Length and payload blocks are read
with bounded, partial-read-safe state machines; authentication failure or a
length over the limit terminates the stream.

## UDP

Maintained standard Shadowsocks UDP uses one AEAD operation per datagram:

```text
method-sized salt || AEAD(address || payload, nonce=0)
```

The salt is fresh for every packet. `encode_udp_packet` and
`decode_udp_packet` implement this path for standard Shadowsocks upstream and
external interop.

The pproxy-compatible standalone inbound path uses the distinct PacketCipher
format:

```text
method-sized salt || AEAD(length_u16_be, nonce=0) || AEAD(address || payload, nonce=1)
```

`encode_pproxy_udp_packet` and `decode_pproxy_udp_packet` implement that path;
the standalone listener uses it for both inbound packets and responses. No
TCP nonce or subkey state is shared with UDP.

## Evidence

Normal crate tests cover method sizes, KDF/cipher known-answer vectors, TCP
partial framing, nonce separation, UDP method-specific salts, and legacy
rejection. Gated interop tests cover:

- Eggress ↔ pproxy 2.7.9 for all four methods;
- Eggress ↔ maintained `ssserver`/`sslocal` for AES-128-GCM,
  AES-256-GCM, and ChaCha20-IETF-Poly1305;
- TCP and UDP in both directions where the external implementation supports
  the mode.

Run the gated suites with the commands in `AGENTS.md`. Do not promote a
parity claim from an Eggress-to-Eggress roundtrip alone.
