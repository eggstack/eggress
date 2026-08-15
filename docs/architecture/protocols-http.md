# eggress-protocol-http

`crates/eggress-protocol-http/`

HTTP/1.1 and H2 proxy protocol implementation.

## Capabilities

| Feature | Direction | Description |
|---|---|---|
| HTTP CONNECT | Inbound + Outbound | Tunnel establishment via CONNECT method |
| HTTP forward proxy | Inbound | Absolute-form URI to origin-form conversion |
| H2 CONNECT | Outbound | HTTP/2 CONNECT with connection pooling |
| Basic auth | Both | Proxy-Authorization header support |

## Key Functions

| Function | Description |
|---|---|
| `handle_connect()` | Server-side CONNECT handling with auth |
| `http_connect()` | Client-side CONNECT establishment |
| `forward_request()` | Convert absolute-form to origin-form |
| `forward_response()` | Forward response with byte counting |
| `h2_connect_client()` | H2 CONNECT with connection pooling |
| `h2_connect_client_pooled()` | Pooled H2 CONNECT |

## Key Types

| Type | Description |
|---|---|
| `ConnectRequest` | Parsed CONNECT request (target, auth) |
| `HttpConnectLimits` | Header size limits, body size limits |
| `HttpDetector` | Protocol detection for HTTP |
| `ForwardRequest` | Forward proxy request |
| `ForwardResponse` | Forward proxy response with body framing |
| `H2PoolRegistry` | H2 connection pool registry |

## Compatibility H2 listener

The pproxy compatibility path exposes `h2://listener` as an H2 CONNECT
listener. `serve_h2_connection` drives one parent connection and routes each
CONNECT stream independently, validating `:authority` and
`proxy-authorization`. DATA receive capacity is released as bytes are read,
and GOAWAY or transport loss ends the parent without cancelling unrelated
streams that have already been handed to the relay executor.

## Body Framing

- `Content-Length` body copying with bounded buffers
- `Transfer-Encoding: chunked` with CRLF validation and extension support
- Hop-by-hop header filtering

## Forwarding safety policy

The HTTP/1.1 forwarder is deliberately half-duplex. It rejects any non-empty
`Expect` value, including `100-continue`, with `417 Expectation Failed` and
closes the client connection before opening an origin route. Request-body
upload and the final upstream flush use the configured connect timeout; a
timeout or write failure drops both streams and cannot reuse buffered bytes as
another request.

Informational responses (`1xx`, except `101`) are forwarded in order and are
bounded to eight heads before one final response is processed. `101 Switching
Protocols` is explicitly rejected with `501 Not Implemented`; ordinary
forward-proxy upgrade tunneling is not implemented. An early final response
while a request body is uploading terminates the session within the upload
timeout rather than attempting a general full-duplex pump.

## Dependencies

- `eggress-core` — `BoxStream`, `ProtocolId`

See [overview.md](overview.md) for context.
