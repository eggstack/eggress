# eggress-protocol-trojan

Trojan-over-TLS protocol implementation. Small, focused crate: SHA224 password-hash authentication and the Trojan request wire format. Server-side accept operates on an already-TLS stream; the client connector performs TLS via the shared transport crate.

## Module map

| File | Role |
|---|---|
| `src/hash.rs` | `password_hash` (SHA224 to 56 hex chars), `trojan_check_password` (constant-time 56-byte compare via `subtle::ConstantTimeEq`) |
| `src/tcp.rs` | `encode_trojan_request` (all targets as ATYP 0x03), `trojan_accept` (server parser), `trojan_connect` (TLS + request write), OnceLock-memoized default TLS config |
| `src/error.rs` | `TrojanError` enum + stable `TrojanDiagnosticCode` classification (6 codes) |
| `src/lib.rs` | Re-exports: `TrojanError`, `TrojanDiagnosticCode`, `trojan_check_password`, `trojan_accept`, `trojan_connect`, `TrojanAcceptResult` |

## Wire format (byte-level)

### Password hash

```
password --[SHA224]--> 28-byte digest --[hex lowercase]--> 56 ASCII chars
```

SHA224 output is 28 bytes; each byte encodes as two lowercase hex digits (`hash.rs:9-14`). Total: 56 characters. No salt, no iterations, no key stretching. This matches the Trojan protocol specification.

### Client request

```
Offset  Field                  Size
0..56   SHA224(password) hex   56 bytes (ASCII)
56..58  CRLF                   2 bytes (\r\n)
58      CMD                    1 byte (0x01 = CONNECT)
59      ATYP                   1 byte (always 0x03)
60      domain_len             1 byte (1..255)
61..61+N  domain string        N bytes
61+N..63+N  port               2 bytes (big-endian)
63+N..65+N  CRLF               2 bytes (\r\n)
```

**Total for IPv4 target** (e.g. `"93.184.216.34"`): 56 + 2 + 1 + 1 + 1 + 13 + 2 + 2 = 78 bytes.
**Total for domain target** (e.g. `"example.com"`): 56 + 2 + 1 + 1 + 1 + 11 + 2 + 2 = 76 bytes.

### All targets encoded as ATYP 0x03

`encode_trojan_request` (`tcp.rs:25-57`) encodes all targets -- including literal IPv4 and IPv6 -- as ATYP 0x03 (domain). IPs are formatted to their standard string representation (`ip.to_string()`). This matches pproxy 2.7.9 client behavior and avoids ATYP-related interop issues with standard Trojan servers.

### Server accept parsing

`trojan_accept` (`tcp.rs:78-208`) reads:

1. **58 bytes**: 56-byte hash + CRLF.
2. **Hash comparison**: constant-time via `subtle::ConstantTimeEq` (`tcp.rs:97-100`).
3. **CRLF check**: if hash matched but CRLF is wrong, returns `AuthFailed` (not a distinguishable error) (`tcp.rs:105-109`).
4. **CMD byte**: must be 0x01 (CONNECT); others rejected with `Protocol` error (`tcp.rs:118-123`).
5. **ATYP-dependent address parsing**:
   - 0x01 (IPv4): 4 bytes IP + 2 bytes port (`tcp.rs:134-146`)
   - 0x03 (domain): 1 byte len + domain + 2 bytes port (`tcp.rs:148-171`)
   - 0x04 (IPv6): 16 bytes IP + 2 bytes port (`tcp.rs:173-187`)
6. **Domain normalization**: numeric strings in ATYP 0x03 are parsed back to `TargetHost::Ip` via `domain.parse::<IpAddr>()` (`tcp.rs:167-169`).
7. **Trailing CRLF**: must be `\r\n`; otherwise `Protocol` error (`tcp.rs:203-205`).

## Crypto notes

### Password hashing

- Algorithm: SHA224 (not SHA256 or SHA512), per Trojan spec.
- Output: 56 lowercase hex characters. No salt, no iteration count.
- The hash is the authentication credential; the actual password is never sent on the wire.

### Constant-time comparison

`trojan_check_password` (`hash.rs:21-24`) uses `subtle::ConstantTimeEq` on the full 56-byte hash. `trojan_accept` performs the same comparison inline (`tcp.rs:97-100`). Neither function short-circuits on first-byte mismatch.

### Oracle resistance

A correct hash followed by a malformed CRLF produces `AuthFailed` -- identical to a wrong-password response (`tcp.rs:105-109`). This prevents an attacker from distinguishing a correct password hash from an incorrect one based on error type.

### TLS

`trojan_connect` (`tcp.rs:221-266`) performs TLS via `eggress_transport_tls::TlsClientConfigBuilder` with system root certificates. A default config is memoized in a `OnceLock<Arc<rustls::ClientConfig>>` (`tcp.rs:230`). Only successful builds are cached; transient failures do not poison the singleton.

## Feature gates

No optional features. The crate has a fixed dependency set: `sha2`, `subtle`, `rustls`, `tokio-rustls`, `eggress-transport-tls`.

## Security notes

- SHA224 is preimage-resistant but not designed for password storage (no salt, no iterations). This is the Trojan protocol design, not an eggress choice.
- `encode_trojan_request` rejects domains longer than 255 bytes or empty domains (`tcp.rs:43-48`).
- The TLS transport provides confidentiality and integrity for the handshake and all subsequent data.
- `OnceLock` memoization (`tcp.rs:230`) means the default TLS config is built at most once per process; if `with_system_roots()` fails on first call, subsequent calls with `tls_config=None` will also fail (the `OnceLock` remains unset).

## Test coverage map

| Category | Location | What it covers |
|---|---|---|
| KAT (password hash) | `hash.rs:31-46` | Known SHA224 hex for "password" and "" |
| Constant-time property | `hash.rs:77-119` | Single-bit difference detection, `ct_eq` delegation |
| Request encoding layout | `tcp.rs:294-417` | Domain, IPv4-as-domain, IPv6-as-domain, ATYP 0x03 enforcement, length boundaries (255 OK, 256 reject, empty reject) |
| Accept roundtrip | `tcp.rs:646-703` | IPv4, domain, IPv6: encode then accept, verify target normalization |
| Auth failure paths | `tcp.rs:706-764` | Wrong password, bad ATYP, non-CONNECT command |
| CRLF oracle resistance | `tcp.rs:988-1032` | Missing CRLF, corrupt CRLF after correct hash both return `AuthFailed` |
| TLS integration | `tcp.rs:420-506` | Self-signed cert, full connect/accept roundtrip through TLS |
| TLS SNI mismatch | `tcp.rs:769-817` | Wrong server name fails TLS verification |
| Custom CA trust | `tcp.rs:822-896` | CA-signed cert chain validation |
| Oversized/malformed | `tcp.rs:900-1095` | Truncated hash, oversized ATYP, non-UTF8 domain, empty domain, empty stream |
| Diagnostic codes | `error.rs:70-131` | All 5 TrojanError variants map to correct codes, display is snake_case |
| Property tests | `tests/request_properties.rs` | 6 proptest properties: hash length, hex-only, deterministic, distinct inputs, even length |
| Fuzz smoke | `tests/fuzz_smoke.rs` | Request encode, password hash, accept parser |
| Fuzz targets | `fuzz/fuzz_targets/trojan_request.rs`, `trojan_accept.rs` | Arbitrary bytes to `encode_trojan_request`, `password_hash`, `trojan_accept` |

## Reviewer gotchas

1. **All ATYP 0x03**: `encode_trojan_request` never emits ATYP 0x01 or 0x04. Servers that only accept native IP ATYP will reject these requests. This is intentional for pproxy compatibility.
2. **Domain normalization on accept**: `trojan_accept` parses ATYP 0x03 domains and normalizes numeric strings back to IP (`tcp.rs:167-169`). A request encoded with an IP-as-domain will be accepted as an IP target.
3. **CRLF after hash is security-critical**: the check at `tcp.rs:105-109` must return `AuthFailed` (not `Protocol`) to prevent hash-validity oracles.
4. **`OnceLock` is not `get_or_init`**: the code uses `get()` then `set()` separately (`tcp.rs:234-247`). A race between two threads could result in both building configs, but only one being cached. This is harmless.
5. **`trojan_accept` reads exactly 58 bytes first**: if the stream is shorter than 58 bytes, the error is `TrojanError::Io` (unexpected EOF), not `AuthFailed`. This is tested at `tcp.rs:901-916`.

## See also

- [protocols-shadowsocks.md](protocols-shadowsocks.md)
- [overview.md](overview.md)
