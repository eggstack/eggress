# Trojan Protocol

## Overview

Trojan proxy protocol implementation. Uses TLS for transport security and
SHA224 password hashing for authentication. Connects to a Trojan server over
TLS and sends a CONNECT request with the target address.

Source: `crates/eggress-protocol-trojan/src/`

## Wire Format

### Handshake

```
+-------------------+------+---------+------+-----+------+
| SHA224(Password)  | CRLF | CONNECT | Addr | Port| CRLF |
+-------------------+------+---------+------+-----+------+
     56 hex chars      2      1        var    2      2
```

- **Password Hash**: 56-character hex-encoded SHA224 hash of the password
- **CRLF**: `\r\n` separator
- **Command**: `0x01` (CONNECT only)
- **Address**: Target address in SOCKS5-compatible format (ATYP + address)
- **Port**: Target port (big-endian, 2 bytes)
- **Terminal CRLF**: `\r\n`

### Address Format (ATYP)

| ATYP | Value | Address Length |
|------|-------|----------------|
| 0x01 | IPv4  | 4 bytes        |
| 0x03 | Domain | 1 byte length + domain bytes |
| 0x04 | IPv6  | 16 bytes       |

## TLS Configuration

- Library: `rustls` with `webpki-roots` for root certificates
- No client authentication (`with_no_client_auth`)
- Server name taken from `ProxyHopSpec.server_name` (falls back to endpoint host)
- TLS handshake performed before sending any Trojan protocol bytes

```rust
// Example: trojan_connect(stream, &target, "password", "server.example.com")
```

## Credential Model

Trojan uses:
- `hop.credentials.password` — the Trojan password (SHA224-hashed for auth)
- `hop.server_name` — the TLS server name for SNI and certificate verification
  (falls back to `hop.endpoint.host` if not set)

URI format: `trojan://password@server.example:443`

Note: The `username` field in credentials is not used by Trojan. The password
must be provided via the password field.

## Password Hash

SHA224 of the password, hex-encoded (56 characters):

```
SHA224("password") = "d63dc919e201d7bc4c825630d2cf25fdc93d4b2f0d46706d29038d01"
```

Source: `crates/eggress-protocol-trojan/src/hash.rs`

## Test Coverage

- Password hash length (56 hex chars)
- Password hash determinism (same input = same output)
- Password hash uniqueness (different inputs = different outputs)
- Known test vectors for `password_hash("password")` and `password_hash("")`
- `encode_trojan_request()` layout matches the expected wire format for
  domain, IPv4, and IPv6 targets
- `encode_trojan_request()` rejects empty domain (`TrojanError::Protocol`)
- `encode_trojan_request()` rejects 256-byte domain (`TrojanError::Protocol`)
- `encode_trojan_request()` accepts 255-byte domain
- `trojan_connect()` happy path through a synthetic TLS server: server observes
  the password hash, CRLF, command, ATYP, target, port, and trailing CRLF
  produced by `trojan_connect()`; client reads back the server's echo
- `trojan_connect()` rejects 256-byte domain (covered by `encode_trojan_request()`
  validation path exercised from `trojan_connect`)
- `trojan_connect()` rejects empty domain (covered by `encode_trojan_request()`
  validation path exercised from `trojan_connect`)
- `trojan_connect()` accepts 255-byte domain end-to-end through TLS
- Wrong-password server-side rejection

Test count: 15 tests across `eggress-protocol-trojan`
(`hash::tests` + `tcp::tests`).

## Current Status

The foundation is implemented and tested through the exported function:

- Password hashing: complete
- `encode_trojan_request()` helper: complete; sole encoder path
- `trojan_connect()`: delegates to `encode_trojan_request()` after TLS handshake
- TLS handshake with rustls: complete
- Request sending: complete
- Domain-length validation (1-255 bytes): complete and tested through the
  exported function

End-to-end interoperability tests against a real third-party Trojan server are
not yet included.

## Limitations

- No fallback routing (no TLS fallback to plain HTTP)
- No UDP support (TCP only)
- No server-side implementation (client/upstream only)
- No multi-hop support
- No response parsing (connection assumed successful after request send)
- No custom TLS certificate configuration (uses webpki roots only)
