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

## Body Framing

- `Content-Length` body copying with bounded buffers
- `Transfer-Encoding: chunked` with CRLF validation and extension support
- Hop-by-hop header filtering

## Dependencies

- `eggress-core` — `BoxStream`, `ProtocolId`

See [overview.md](overview.md) for context.
