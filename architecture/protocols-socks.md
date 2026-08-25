# eggress-protocol-socks — SOCKS4/4a and SOCKS5

Full server + client implementations of SOCKS4/4a and SOCKS5, including the
SOCKS5 UDP datagram codec used by the UDP subsystem.

## Module map

| File | Role |
|---|---|
| `src/detector.rs` / `src/lib.rs` | Version-byte sniffers (`Socks4Detector` sees 0x04, `Socks5Detector` sees 0x05) |
| `src/socks4/server.rs` | `read_socks4_request` / `write_socks4_reply`; 4a domain extension when IP = `0.0.0.x` (x≠0); USERID ≤ 255 bytes; BIND rejected |
| `src/socks4/client.rs` | `socks4_connect` |
| `src/socks5/server.rs` | Full handshake: method negotiation → optional RFC 1929 username/password → CONNECT; `SocksAddr` (IPv4/IPv6/domain); unsupported command answered REP=0x07 before close |
| `src/socks5/client.rs` | `socks5_connect` (greeting + auth + request) |
| `src/socks5/udp_codec.rs` | `decode_/encode_socks5_udp_datagram`: RSV must be 0x0000, FRAG must be 0x00 (no fragmentation), ATYP-valid, domain ≤ 255, payload ≤ 65535 |
| `src/error.rs` | Shared `Socks5Error` taxonomy |

## Security-relevant details

- Password comparison constant-time (`subtle`).
- Credential lengths capped at 255 both directions.
- Pure sync parse functions exist alongside async handlers specifically so fuzz
  targets can hammer the parsers without I/O (`parse_method_negotiation`,
  `parse_connect_request`).

## Review entry points

- Verify: `cargo test -p eggress-protocol-socks`; fuzz targets
  `socks5_handshake`, `socks5_udp_datagram`.
