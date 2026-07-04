# Failure Semantics

This document maps internal failure conditions to client-visible protocol replies
for each supported inbound protocol. It serves as the authoritative reference for
predictable client behavior across failure scenarios.

## Failure Condition Taxonomy

| Condition | Source | Description |
|-----------|--------|-------------|
| Timeout | Connection setup exceeded deadline | Upstream or target did not respond in time |
| Connection refused | TCP RST or ICMP unreachable | Target or upstream actively refused connection |
| Network unreachable | Routing failure | No route to target network |
| Host unreachable | Routing failure or DNS | Target host cannot be reached |
| DNS resolution failed | Name resolution | Domain name could not be resolved |
| Policy denied | Routing rule | Request rejected by routing policy |
| Upstream auth failed | Protocol handshake | Upstream proxy rejected credentials |
| Protocol error | Handshake failure | Malformed protocol exchange |
| Internal error | Bug or resource exhaustion | Unexpected internal condition |

## SOCKS5 Reply Codes

| Internal condition | SOCKS5 REP code | Hex | Description |
|-------------------|----------------|-----|-------------|
| Success | `REP_SUCCESS` | `0x00` | Connection established |
| Policy denied | `REP_NOT_ALLOWED` | `0x02` | Operation not permitted |
| Network unreachable | `REP_NETWORK_UNREACHABLE` | `0x03` | Network unreachable |
| DNS resolution failed | `REP_HOST_UNREACHABLE` | `0x04` | Host unreachable |
| Host unreachable | `REP_HOST_UNREACHABLE` | `0x04` | Host unreachable |
| Connection refused | `REP_CONNECTION_REFUSED` | `0x05` | Connection refused |
| Timeout | `0x06` | `0x06` | TTL expired (closest match) |
| BIND command (unsupported) | `REP_COMMAND_NOT_SUPPORTED` | `0x07` | Command not supported |
| Other error | `REP_GENERAL_FAILURE` | `0x01` | General failure |
| Unknown address type | `REP_ADDRESS_TYPE_NOT_SUPPORTED` | `0x08` | Address type not supported |

### SOCKS5 Authentication Failure

When authentication is required and the client provides invalid credentials:
- Server sends method `0xFF` (no acceptable methods) during method negotiation
- Connection is closed without a reply message

### SOCKS5 Reply Wire Format

```
+----+-----+-------+------+----------+----------+
|VER | REP |  RSV  | ATYP | BND.ADDR | BND.PORT |
+----+-----+-------+------+----------+----------+
| 1  |  1  | X'00' |  1   | Variable |    2     |
+----+-----+-------+------+----------+----------+
```

## HTTP CONNECT Status Codes

| Internal condition | HTTP Status | Description |
|-------------------|-------------|-------------|
| Success | `200 Connection Established` | Tunnel established |
| Policy denied | `403 Forbidden` | Request blocked by policy |
| Timeout | `504 Gateway Timeout` | Upstream or target timed out |
| All other errors | `502 Bad Gateway` | Upstream returned invalid response |

### HTTP Error Status Codes (Protocol Level)

| Error condition | HTTP Status | Description |
|----------------|-------------|-------------|
| Malformed request | `400 Bad Request` | Request syntax error |
| Auth required | `407 Proxy Authentication Required` | Credentials needed |
| Auth failed | `403 Forbidden` | Invalid credentials |
| Connection refused | `502 Bad Gateway` | Upstream connection refused |
| Gateway timeout | `504 Gateway Timeout` | Upstream timed out |
| Headers too large | `431 Request Header Fields Too Large` | Header overflow |
| Invalid credentials | `400 Bad Request` | Malformed auth data |
| Unsupported transfer encoding | `400 Bad Request` | Invalid transfer coding |
| Other errors | `500 Internal Server Error` | Unexpected error |

### HTTP Forward Proxy Failure

For HTTP forward proxy (non-CONNECT) requests, failures are reported via:
- `403 Forbidden` — policy denied
- `502 Bad Gateway` — upstream error
- `504 Gateway Timeout` — timeout

## SOCKS4 Reply Codes

| Internal condition | SOCKS4 Status | Code | Description |
|-------------------|---------------|------|-------------|
| Success | `Granted` | `90` | Request granted |
| General failure | `Failed` | `91` | Request failed |
| Client ident required | `FailedNoIdent` | `92` | Client cannot be identified |
| Different user required | `FailedDifferentUser` | `93` | User ID mismatch |

### SOCKS4 Reply Wire Format

```
+----+----+----+----+----------+----------+
|VN  |CD  |DSTPORT|DSTIP        |          |
+----+----+----+----+----------+----------+
| 1  | 1  |  2  |  4  |          |
+----+----+----+----+----------+----------+
```

## Shadowsocks Failure Modes

Shadowsocks does not define application-layer error codes. All failure
conditions result in immediate connection close without a reply message.

| Condition | Client-observed behavior |
|-----------|--------------------------|
| Decrypt failure (wrong key, corrupted data) | Connection closed |
| Unsupported cipher method | Connection closed |
| Invalid address encoding | Connection closed |
| Upstream connect failure | Connection closed |

Shadowsocks AEAD framing uses encrypted length fields, so the proxy cannot
distinguish between truncation and other network errors. The client observes
a connection reset or timeout.

## Failure Propagation Through Chains

When a request traverses a multi-hop chain, failures at intermediate hops
are propagated back to the client as the corresponding protocol reply:

| Hop failure | SOCKS5 reply | HTTP reply | SOCKS4 reply |
|-------------|-------------|------------|--------------|
| Hop N connect refused | `0x05` | `502` | `91` |
| Hop N timeout | `0x06` | `504` | `91` |
| Hop N auth failed | `0x01` | `502` | `91` |
| Hop N protocol error | `0x01` | `502` | `91` |

Chain errors include hop index and endpoint metadata for diagnostics, but the
client-visible reply is always a standard protocol reply code.

## Security Invariants

- Credentials are never included in failure replies or error messages
- Error messages are bounded in length
- Internal error details are logged but not exposed to clients
- Upstream authentication failures do not reveal which credential was wrong
- Policy denied responses do not reveal the matching rule

## Comparison with pproxy

| Behavior | pproxy | Eggress | Notes |
|----------|--------|---------|-------|
| SOCKS5 refused | `0x05` | `0x05` | Match |
| SOCKS5 DNS failure | `0x04` | `0x04` | Match |
| SOCKS5 auth failure | `0xFF` method | `0xFF` method | Match |
| HTTP auth failure | `407` | `407` | Match |
| HTTP upstream failure | `502` | `502` | Match |
| SOCKS5 timeout | Connection reset | `0x06` | Eggress uses TTL expired code |
| HTTP timeout | Connection reset | `504` | Eggress uses explicit timeout code |
| Invalid SOCKS version | Connection reset | Connection reset | Match |
