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

The client expects a response head terminated by `\r\n\r\n`. Any `2xx` status
code indicates success; the stream is returned for bidirectional forwarding.

## Authentication

Credentials are base64-encoded and sent via the `Proxy-Authorization` header.
Control characters (bytes < 0x20 or 0x7F) in username or password are rejected
before sending any bytes to the upstream.

```rust
// Example: http_connect(stream, &target, Some(("user", "pass")), &limits)
```

## Parser Limits

Configurable via `HttpConnectLimits`:

| Field               | Default    | Description                         |
|---------------------|------------|-------------------------------------|
| `max_status_line`   | 1024       | Maximum length of the status line   |
| `max_headers_bytes` | 32,768     | Maximum total bytes for headers     |
| `max_header_count`  | 100        | Maximum number of header lines      |

Exceeding these limits returns `HeaderTooLarge` or `TooManyHeaders`.

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

Test count: 76 tests across `eggress-protocol-http`.

## Limitations

- No persistent connection support (each CONNECT is one-shot)
- No HTTP/2 CONNECT or extended CONNECT
- No chunked transfer encoding in the CONNECT response
- No proxy chaining within the HTTP protocol layer
- Subsequent data after the 200 response is forwarded as-is (no encryption)
