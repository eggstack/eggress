# eggress-protocol-socks

`crates/eggress-protocol-socks/`

SOCKS4/4a and SOCKS5 proxy protocol implementations.

## Capabilities

| Feature | Description |
|---|---|
| SOCKS4 | Classic SOCKS4 with IP targets |
| SOCKS4a | Domain name extension for remote DNS |
| SOCKS5 | Full SOCKS5 with method negotiation |
| SOCKS5 Auth | Username/password authentication |
| SOCKS5 UDP ASSOCIATE | UDP relay via SOCKS5 |

## Key Types

| Type | Description |
|---|---|
| `Socks4Detector` | Protocol detection for SOCKS4 |
| `Socks5Detector` | Protocol detection for SOCKS5 |
| `Socks4Request` | Parsed SOCKS4 request |
| `Socks4Status` | SOCKS4 reply status codes |
| `Socks5Error` | SOCKS5 error type |

## SOCKS4/4a

- `socks4_connect()` — client-side SOCKS4 CONNECT
- `read_socks4_request()` / `write_socks4_reply()` — server-side
- Domain preservation for SOCKS4a (remote DNS)

## SOCKS5

- Method negotiation (no-auth, username/password)
- Bounded credentials (255 bytes max)
- CONNECT, BIND, UDP ASSOCIATE commands
- UDP datagram codec: encode/decode with IPv4, IPv6, and domain targets

## UDP ASSOCIATE

SOCKS5 UDP ASSOCIATE is the primary UDP entry point:
1. Client sends UDP ASSOCIATE command
2. Server replies with relay bind address
3. Client sends SOCKS5 UDP datagrams to relay
4. Association decodes, routes, and forwards

## Dependencies

- `eggress-core` — `BoxStream`, `ProtocolId`

See [overview.md](overview.md) for context.
