# eggress-protocol-trojan — Trojan over TLS

Small, sharp crate: SHA224 password-hash authentication and the Trojan request
wire format. Server-side accept operates on an already-TLS stream provided by
the caller; the client connector performs TLS itself via the shared transport.

## Module map

| File | Role |
|---|---|
| `src/hash.rs` | `password_hash` (SHA224 → 56 hex chars), `trojan_check_password` (constant-time 56-byte compare) |
| `src/tcp.rs` | `encode_trojan_request` (validates domain length 1–255), `trojan_accept` (parses hash+CRLF+CMD+ATYP+addr+port+CRLF), `trojan_connect` (TLS handshake via eggress-transport-tls + request write); memoized TLS config via `OnceLock` |
| `src/error.rs` | `TrojanError` + stable `TrojanDiagnosticCode` classification |

## Wire format & deliberate choices

- Request: `SHA224(password)hex + CRLF + CMD(0x01) + ATYP + addr + port + CRLF`.
- **All** targets — including literal IPs — are encoded as ATYP 0x03 (domain)
  to match pproxy behavior; `trojan_accept` normalizes numeric strings back to
  IP form.
- A correct hash followed by a bad CRLF fails identically to a wrong password
  (`AuthFailed`) so attackers can't oracle hash validity.
- Non-CONNECT commands are rejected; no fallback behavior.

## Review entry points

- Verify: `cargo test -p eggress-protocol-trojan`; fuzz targets
  `trojan_request`, `trojan_accept`. Interop suite:
  `interoperability_trojan.rs` in eggress-cli (opt-in).
