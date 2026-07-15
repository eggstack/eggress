# Trojan Protocol

## Overview

Trojan proxy protocol implementation. Uses TLS for transport security and
SHA224 password hashing for authentication. Supports both **upstream** (client
connecting to a Trojan server) and **inbound listener** (accepting Trojan
connections from clients).

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

### Upstream (Client)

- Library: `rustls` with `webpki-roots` for root certificates
- No client authentication (`with_no_client_auth`)
- Server name taken from `ProxyHopSpec.server_name` (falls back to endpoint host)
- TLS handshake performed before sending any Trojan protocol bytes

### Inbound Listener (Server)

- TLS termination happens in the runtime supervisor before `accept()`
- `trojan_accept()` reads the Trojan handshake from the TLS stream
- Password verification: SHA224 hash comparison
- Only CONNECT command (0x01) supported
- No response sent on success (silent protocol)

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
- `trojan_accept()` round-trip with `encode_trojan_request()` for IPv4, domain, and IPv6 targets
- `trojan_accept()` returns `AuthFailed` for wrong password
- `trojan_accept()` returns `Protocol` error for invalid ATYP
- `trojan_accept()` returns `Protocol` error for non-CONNECT command
- `trojan_accept()` accepts 255-byte domain end-to-end through TLS
- Wrong-password server-side rejection

Test count: 29 tests across `eggress-protocol-trojan`
(`hash::tests` + `tcp::tests`).

## Current Status

### Upstream (Client)

- Password hashing: complete
- `encode_trojan_request()` helper: complete; sole encoder path
- `trojan_connect()`: delegates to `encode_trojan_request()` after TLS handshake
- TLS handshake with rustls: complete
- Request sending: complete
- Domain-length validation (1-255 bytes): complete and tested

### Inbound Listener (Server)

- `trojan_accept()`: reads and validates Trojan handshake from TLS stream
- Password verification via SHA224 hash comparison
- Single-protocol listener mode (no mixed-protocol detection)
- Config: `[listeners.trojan]` section with password field
- TLS required (validated at config level)
- Config validation: Trojan requires TLS + password section

End-to-end interoperability tests against a real third-party Trojan server are
provided via a gated integration test (`interoperability_trojan.rs`).

## Verification Evidence

### Unit Tests

- `eggress-protocol-trojan` crate: 29+ tests across `hash::tests` and `tcp::tests`
  covering password hashing, wire format encoding/decoding, ATYP variants (IPv4,
  domain, IPv6), domain length validation (1-255 bytes), CRLF verification,
  error classification, and malformed-frame rejection.
- `eggress-runtime/tests/trojan.rs`: 4 runtime integration tests covering
  correct-password relay, wrong-password rejection, fallback-on-auth-failure,
  and auth failure metrics.

### Integration Tests (Runtime)

- **`trojan_correct_password`**: Full TLS→Trojan handshake→relay→echo round-trip
  through the runtime supervisor.
- **`trojan_auth_rejected`**: Wrong password with no fallback — connection is
  closed without relay.
- **`trojan_fallback_on_auth_failure`**: Wrong password with fallback configured —
  connection is relayed to the fallback target (data is echoed back).
- **`trojan_auth_failure_metrics`**: Wrong password increments the
  `eggress_auth_failures_total` Prometheus counter.

### Chain Support

- **`socks5_to_trojan_chain`** (`crates/eggress-runtime/tests/multihop_tcp.rs:661`):
  SOCKS5 inbound listener → Trojan upstream → TCP echo target. Verifies
  bidirectional data relay through a two-hop chain.

### Differential / Oracle

- Trojan URI parsing is covered by the oracle scenario harness (TOML scenarios).
- Trojan upstream config validation is tested in lifecycle invariant and
  upstream protocol tests.

### Constant-Time Authentication

Password verification uses `subtle::ConstantTimeEq` for comparison of the
56-character SHA224 hash. This prevents timing side-channel attacks that could
leak hash bytes through response-time variation. The `trojan_check_password()`
function in `hash.rs` delegates to `ct_eq()` from the `subtle` crate, which
is specifically designed for constant-time operations independent of input
values.

Tests verify:
- Correct passwords match through the constant-time path
- Single-bit differences are detected
- The comparison does not short-circuit on differing first bytes

### Fallback Mechanism

When a Trojan listener is configured with `fallback = "<target>"`, connections
that fail authentication (wrong password) are transparently relayed to the
fallback target instead of being closed. This provides censorship resistance
by making Trojan servers indistinguishable from normal TLS services under
active probing. Tested in `trojan_fallback_on_auth_failure`.

### Independent Interop Status

A gated integration test (`crates/eggress-cli/tests/interoperability_trojan.rs`)
exercises eggress as a Trojan upstream against a third-party `trojan-go` server.
Tests cover bidirectional TCP relay, large payloads, and wrong-password
rejection. The test is gated behind `EGRESS_REQUIRE_TROJAN_INTEROP=1` and
requires `trojan-go` on PATH. The inbound listener path is also tested
with a raw TLS client sending a Trojan handshake.

### TCP-Only Status

Trojan is TCP-only — no UDP support. This matches the original Trojan protocol
specification and pproxy's Trojan implementation. UDP via Trojan is explicitly
rejected at config validation time.

## Limitations

- No fallback routing (no TLS fallback to plain HTTP)
- No UDP support (TCP only)
- No multi-hop support
- No response parsing (connection assumed successful after request send)
- No custom TLS certificate configuration (uses webpki roots only for upstream)
