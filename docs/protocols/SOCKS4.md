# SOCKS4/SOCKS4a Upstream Protocol

## Overview

SOCKS4 and SOCKS4a CONNECT proxy protocol. Supports IPv4 targets natively and
domain targets via the SOCKS4a extension (using the `0.0.0.1` placeholder).

Source: `crates/eggress-protocol-socks/src/socks4/client.rs`

## Wire Format

### SOCKS4 CONNECT Request

```
+----+----+----+----+----+----+----+----+----+----+....+----+
| VN | CD | DSTPORT |      DSTIP        | USERID       |0x00|
+----+----+----+----+----+----+----+----+----+----+....+----+
  1    1      2            4              variable       1
```

- `VN`: `0x04` (version)
- `CD`: `0x01` (CONNECT command)
- `DSTPORT`: target port (big-endian)
- `DSTIP`: target IPv4 address (4 bytes)
- `USERID`: optional NUL-terminated user ID string

### SOCKS4a Extension

When the target is a domain, DSTIP is set to `0.0.0.1` and the domain name is
appended after the NUL-terminated user ID, followed by another NUL byte.

```
+----+----+----+----+----+----+----+----+----+....+----+------+
| VN | CD | DSTPORT |  0.0.0.1       | USERID       |0x00|DOMAIN|0x00|
+----+----+----+----+----+----+----+----+----+....+----+------+
```

### Response

```
+----+----+----+----+----+----+----+----+
| VN | CD | DSTPORT |      DSTIP        |
+----+----+----+----+----+----+----+----+
  1    1      2            4
```

- `VN`: `0x00` (null version)
- `CD`: status code (see below)

## Status Codes

| Code | Name                  | Description                           |
|------|-----------------------|---------------------------------------|
| 90   | `Granted`             | Request granted                       |
| 91   | `Failed`              | Request failed or rejected            |
| 92   | `FailedNoIdent`       | Failed: client not accepted (no identd) |
| 93   | `FailedDifferentUser` | Failed: different user ID expected    |

Unknown status codes return `UnknownStatus(code)`.

## User ID

- Optional, max 255 bytes
- NUL-terminated in the wire format
- Exceeding the length returns `UserIdTooLong` before sending any bytes

## Address Types

- **IPv4**: Sent directly in the DSTIP field
- **IPv6**: Not supported in SOCKS4; the client returns an error
- **Domain**: SOCKS4a mode with `0.0.0.1` placeholder and domain appended after user ID

## Test Coverage

- Synthetic SOCKS4 test server with configurable modes (Success, DomainSuccess, Rejected, NoIdent, DifferentUser, MalformedResponse, UnknownStatus, SlowResponse, NoReply)
- IPv4 CONNECT success with data echo
- SOCKS4a domain CONNECT success
- All status code paths (90, 91, 92, 93)
- Malformed response (wrong version byte)
- Unknown status code handling
- User ID length limit enforcement
- User ID sent correctly with CONNECT
- Slow response timeout (external timeout)
- No-reply causes EOF error

Test count: 92 tests across `eggress-protocol-socks` (includes SOCKS5 tests).

## Limitations

- No BIND command support
- No UDP ASSOCIATE support
- No IPv6 target support
- No authentication beyond the optional user ID field
- SOCKS4a domain length limited to 255 bytes
