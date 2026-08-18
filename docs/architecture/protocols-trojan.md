# eggress-protocol-trojan

`crates/eggress-protocol-trojan/`

Trojan protocol implementation with TLS transport and SHA224 password authentication.

## Key Functions

| Function | Description |
|---|---|
| `trojan_accept()` | Server-side TLS + Trojan handshake |
| `trojan_connect()` | Client-side TLS + Trojan request |
| `trojan_check_password()` | Verify password against SHA224 hash |
| `encode_trojan_request()` | Produce wire format (hash + CRLF + CONNECT + address + CRLF) |

## Wire Format

```
SHA224(password) + \r\n
CONNECT host:port HTTP/1.1\r\n
\r\n
[data stream]
```

## Key Types

| Type | Description |
|---|---|
| `TrojanError` | Error type with diagnostic codes |
| `TrojanDiagnosticCode` | Structured diagnostic enum |
| `TrojanAcceptResult` | Result of server-side accept |

## Authentication

- Password hashed with SHA224
- Hash compared to expected hash from config
- Domain length validated (1-255 bytes)

Trojan clients may encode a literal IPv4/IPv6 target in the domain-form address
field, as pproxy does. The accept path normalizes numeric literals back to
`TargetHost::Ip` before routing, preserving literal-IP compatibility while
keeping DNS-rebinding checks for actual hostnames.

## TLS Transport

Trojan always uses TLS. Uses `eggress-transport-tls` for TLS handshake:
- Accepts optional `Arc<ClientConfig>` for shared config
- Falls back to system roots if no config provided

## Dependencies

- `eggress-core` — stream types (implicit)
- `eggress-transport-tls` — TLS handshake

See [overview.md](overview.md) for context.
