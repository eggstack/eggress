# HTTP CONNECT Upstream Protocol

## Overview

TCP CONNECT via HTTP/1.1 proxy. Sends a `CONNECT` request to the upstream proxy
and establishes a bidirectional tunnel upon receiving a `2xx` response.

Source: `crates/eggress-protocol-http/src/connect/client.rs`

## Supported Subset

- TCP CONNECT only (HTTP/1.1)
- Authentication via `Proxy-Authorization: Basic` header
- No persistent connections (one request per stream)
- No HTTP/2 support

## Wire Format

```
CONNECT host:port HTTP/1.1\r\n
Host: host:port\r\n
[Proxy-Authorization: Basic <base64(user:pass)>\r\n]
\r\n
```

Authority and `Host` are produced from one helper so they agree. Domains
and IPv4 use `host:port`; IPv6 uses bracketed `[addr]:port`. Empty hosts
and request-line splitting bytes fail before any wire bytes are sent.

The client expects a response head terminated by `\r\n\r\n`. Any `2xx` status
code indicates success; the stream is returned for bidirectional forwarding.
Only the status line must be UTF-8; header values may contain obs-text
bytes. Bytes already buffered past the terminator are replayed first from
the returned stream.

## Authentication

Credentials are base64-encoded and sent via the `Proxy-Authorization` header.
Control characters (bytes < 0x20 or 0x7F) in username or password are rejected
before sending any bytes to the upstream.

```rust
// Example: http_connect(stream, &target, Some(("user", "pass")), &limits)
```

## Parser Limits

Configurable via `HttpConnectLimits`:

| Field               | Default    | Description                                         |
|---------------------|------------|-----------------------------------------------------|
| `max_status_line`   | 1024       | Maximum length of the status line (before CRLF)     |
| `max_headers_bytes` | 32,768     | Maximum total bytes for the response head           |
| `max_header_count`  | 100        | Maximum number of actual header fields (status line and terminal empty line excluded) |

Exceeding size limits returns `HeaderTooLarge`; exceeding the field count
returns `TooManyHeaders`. Truncated or malformed heads return
`MalformedResponse`.

## Error Mapping

| HTTP Status | Error Variant    | Description                       |
|-------------|------------------|-----------------------------------|
| 200-299     | (success)        | Connection established            |
| 407         | `AuthRequired`   | Proxy Authentication Required     |
| 403         | `AuthFailed`     | Forbidden                         |
| 502         | `BadGateway`     | Bad Gateway                       |
| 504         | `GatewayTimeout` | Gateway Timeout                   |
| Other       | `UnexpectedStatus` | Upstream returned unexpected code |

## Test Coverage

- Synthetic proxy server with configurable modes (Success, AuthRequired, Forbidden, MalformedStatus, SlowResponse, HeadersTooLarge)
- Base64 encoding correctness
- Status code parsing (valid, invalid, too long)
- Credential validation (control chars rejected, normal accepted)
- Full connect flow: 200 success, 407 auth required, 403 forbidden
- Auth with correct and wrong credentials
- Malformed response handling
- Slow response timeout (external timeout)
- Header size limit enforcement
- Authority formatting (domain/IPv4/IPv6 bracketed, injection rejected)
- Wire agreement (request-line authority matches `Host`, non-default ports)
- Pre-write credential rejection and secret redaction in errors
- Status policy (201/204 success, 502/504/arbitrary mappings, truncated/overlong heads)
- Header-count boundaries (exactly 100 accepted, 101 rejected)
- Non-UTF-8 header acceptance and pipelined read-ahead preservation

Test count: run `cargo test -p eggress-protocol-http` for the current total.

## Limitations

- No persistent connection support (each CONNECT is one-shot)
- No HTTP/2 CONNECT or extended CONNECT
- No chunked transfer encoding in the CONNECT response
- No proxy chaining within the HTTP protocol layer
- Subsequent data after the 200 response is forwarded as-is (no encryption)
