# eggress-protocol-http -- HTTP/1.1 CONNECT, Forward Proxy, H2 CONNECT

HTTP protocol family: server-side CONNECT accept, client-side CONNECT,
absolute-form forward proxying with origin-form conversion, and HTTP/2
CONNECT with a connection pool. Detection (`HttpDetector`) distinguishes
proxy-usable HTTP from other protocols by method/response shape.

## Module map

| File | Role | Key lines |
|---|---|---|
| `connect/server.rs` | `handle_connect`: bounded CONNECT head, Basic auth constant-time compare, 200/407 | `MAX_HEAD_SIZE` (:10), `MAX_HEADER_LINES` (:13), `handle_connect` (:35), `parse_authority` (:168), `parse_basic_auth` (:249) |
| `connect/client.rs` | `http_connect`: send CONNECT, read reply; `validate_credentials` rejects control chars | `HttpConnectLimits` (:9), `validate_credentials` (:31), `http_connect` (:52), `parse_status_code` (:154) |
| `forward/server.rs` | Absolute-to-origin form, hop-by-hop filter, body framing, chunk caps, informational bound | `BodyCopyLimits` (:7), `determine_request_body_kind` (:269), `filter_hop_by_hop` (:398), `forward_response` (:625), `parse_header_line` (:1053) |
| `h2_connect.rs` | H2 CONNECT client/server/relay; `H2ConnectionPool`/`H2PoolRegistry` keyed by endpoint + SHA-256 cred hash; `H2_PROTOCOL_METRICS` | `h2_connect_relay` (:167), `H2PoolKey` (:429), `H2ConnectionPool` (:537) |
| `detect.rs` | `HttpDetector`: confidence 100 for methods, 95 for responses | `HttpDetector` (:7), `HTTP_METHODS` (:9) |
| `error.rs` | `HttpError` with `status_code()` mapping | `HttpError` (:3), `status_code` (:82) |
| `connect/test_server.rs` | Synthetic CONNECT proxy (Success/AuthRequired/Forbidden/MalformedStatus/SlowResponse/HeadersTooLarge) | `ProxyMode` (:7) |

## Public API surface

Re-exported from `lib.rs` (:12-27): `handle_connect`, `ConnectRequest`,
`http_connect`, `validate_credentials`, `HttpConnectLimits`,
`build_origin_request`, `copy_request_body`, `determine_request_body_kind`,
`filter_hop_by_hop`, `forward_request`/`forward_request_stream`,
`forward_response`, `has_unsupported_expectation`, `BodyCopyLimits`,
`ForwardRequest`, `ForwardResponse`, `ForwardResult`, `RequestBodyKind`,
`h2_connect_client`, `h2_connect_client_pooled`, `h2_connect_relay`,
`H2ConnectError`, `H2PoolGuard`, `H2PoolKey`, `H2PoolRegistry`,
`H2ProtocolMetrics`, `H2StreamRead`, `H2StreamWrite`,
`H2_POOL_REGISTRY`, `H2_PROTOCOL_METRICS`, `HttpDetector`, `HttpError`.

## Wire format

### CONNECT request (client-to-proxy)

```
CONNECT host:port HTTP/1.1\r\n
[Proxy-Authorization: Basic <base64(user:pass)>\r\n]
\r\n
```

Server limits: head <= 32 KiB (`MAX_HEAD_SIZE`, :10), headers <= 128
(`MAX_HEADER_LINES`, :13). Authority (`:168`): `host:port`, `[ipv6]:port`;
domain-only returns error.

### Client-side CONNECT response limits (`HttpConnectLimits` defaults, :19-25)

| Limit | Default |
|---|---|
| `max_status_line` | 1024 B |
| `max_headers_bytes` | 32 KiB |
| `max_header_count` | 100 |

Status mapping (:93-100): 200-299 success, 407/403/502/504 to typed errors,
other codes `UnexpectedStatus`.

### Forward request

`build_origin_request` (:428) converts `GET http://host:port/path HTTP/1.1`
to `GET /path HTTP/1.1`, strips hop-by-hop headers, appends `Connection:
close`. `Proxy-Authorization` removed at parse time (:911).

### Forward response

Response head bounded at 32 KiB (`MAX_RESPONSE_HEAD_SIZE`, :342), headers
<= 128 lines (:349). Informational (1xx except 101) capped at 8
(:346). 101 returns `UpgradeUnsupported`.

### Body limits

Request-side (`BodyCopyLimits` defaults): chunk-size-line 1024 B, chunk-size
64 MiB, decoded-body 64 MiB, trailer-line 8192 B, trailers 32 KiB.
Response-side constants: `MAX_RESPONSE_CHUNK_SIZE` 64 MiB (:352),
`MAX_TRAILER_BYTES` 64 KiB (:356).

### H2 CONNECT

`h2_connect_relay` (:167): bidirectional H2-to-TCP relay. Drain timeout
`H2_RELAY_DRAIN_TIMEOUT` = 5s (:22). `H2StreamWrite` (:88) implements
capacity-aware `AsyncWrite`.

## How it works

### Accept flow (CONNECT)

1. `handle_connect` (:35) reads head byte-by-byte until `\r\n\r\n` or limit.
2. Validates HTTP/1.0 or HTTP/1.1 (:140). Parses authority via `parse_authority`
   (:168). If auth required, verifies via `subtle::ConstantTimeEq` (:47-49);
   sends 407 on mismatch. Writes `200 Connection Established\r\n\r\n` and
   returns the upgraded stream.

### Forward flow

1. `forward_request` (:821) reads absolute-form request.
2. `parse_absolute_uri` (:949) extracts target and path.
3. `determine_request_body_kind` (:269) resolves framing; rejects TE+CL,
   conflicting CL, non-chunked TE. Body copied via `copy_request_body` (:45).
4. `forward_response` (:625) reads upstream, filters hop-by-hop, writes to
   client. Returns `ForwardResult` with `upstream_alive`/`client_should_close`.

### H2 connect flow

1. `h2_connect_client` (:363): H2 handshake + CONNECT with optional Basic auth.
2. `h2_connect_client_pooled` (:815): acquires from pool or creates new.
   Pool key includes SHA-256 of credentials (:463-469).
3. `H2PoolGuard` (:778) releases on drop.
4. `h2_connect_relay` (:167): domain targets checked against DNS rebinding;
   IP literals connect directly (NOT a policy boundary, see :152-166).

## Error and failure model

`HttpError` maps to HTTP status codes via `status_code()` (:error.rs:82):
400 for malformed/invalid framing, 403/407 for auth, 417 for expectation,
431 for header limits, 500 for other, 501 for upgrade, 502 for upstream/
connection errors, 504 for timeout. `H2ConnectError` adds `PoolExhausted`
and `DnsRebinding`.

## Security notes

| Resource | Limit | Enforced at |
|---|---|---|
| CONNECT head | 32 KiB | `read_connect_request` (:84) |
| CONNECT headers | 128 lines | `read_connect_request` (:110) |
| Forward req/resp head | 32 KiB | `:844`, `:471` |
| Forward headers | 128 lines | `:872`, `:526` |
| Client CONNECT status | 1024 B | `HttpConnectLimits` (:21) |
| Client CONNECT headers | 32 KiB / 100 lines | `:22`, `:23` |
| Informational responses | 8 | `forward_response` (:638) |
| Request/response chunk | 64 MiB | `BodyCopyLimits`, `:352` |
| Request/response trailers | 32/64 KiB | `BodyCopyLimits` (:22), `:356` |

**Constant-time auth**: CONNECT server uses `subtle::ConstantTimeEq`
(:connect/server.rs:47-49). `validate_credentials` (:connect/client.rs:31)
rejects bytes < 0x20 or 0x7F before wire.

**Header validation**: forward `parse_header_line` (:forward/server.rs:1053)
rejects NUL/CR/LF in names/values per RFC 7230 s3.2.4. Reason phrase validated
as printable ASCII (:forward/server.rs:573). `Proxy-Authorization` stripped
at parse (:forward/server.rs:911).

**Hop-by-hop filter** (:forward/server.rs:362): removes `connection`,
`keep-alive`, `proxy-authenticate`, `proxy-authorization`, `te`, `trailers`,
`upgrade`, `proxy-connection`, non-`chunked` TE. `Connection`-nominated
headers removed per RFC 7230 s6.1 (:384).

**Body framing**: TE+CL rejected (:306-308), conflicting CL rejected
(:298-299), equal-duplicate CL accepted (:297), chunked must be final
(:314-318), only `chunked` supported (:322-326). Response TE takes
precedence over CL per RFC 7230 s3.3.3 (:558-559).

**H2 pool isolation**: `H2PoolKey` hashes creds via SHA-256 (:463-469).
Key includes `hop_index` for cross-chain isolation. Idle connections reaped
at `idle_timeout / 2`.

## Test coverage

- `connect/server.rs`: `parse_authority`, `parse_header_line`,
  `parse_basic_auth`, head-size/header-count rejection.
- `connect/client.rs`: `parse_status_code`, `validate_credentials`,
  synthetic server integration (200/407/403/malformed/slow/headers-too-large).
- `forward/server.rs`: `parse_absolute_uri`, `filter_hop_by_hop`,
  `determine_request_body_kind` (all branches), body copy (chunked/CL/EOF,
  premature EOF, oversized chunk, decoded limit, trailers), `forward_response`
  (Connection:close, HTTP/1.1 keepalive, informational bound, 101, invalid CL,
  conflicting CL, invalid chunk size).
- `h2_connect.rs`: error variants, pool key isolation (auth/TLS/SNI/hop index),
  pool stats, registry get-or-create.
- `detect.rs`: method/response match, partial prefix, empty, no-match.
- `lib.rs`: CONNECT integration (echo, auth, domain, IPv6, invalid method,
  header size), forwarding (GET, proxy-auth removal, origin-form, POST).

**Fuzz targets**: `http_connect_response` exercises `parse_status_code`,
`parse_authority`, `parse_header_line`, `parse_basic_auth`.
`h2_connect_authority` exercises those plus `validate_credentials` with
default and restrictive limits.

## Reviewer gotchas

1. **Two `parse_header_line` fns**: `connect/server.rs:239` (public, no
   control-char check, for fuzzing) vs `forward/server.rs:1053` (private,
   rejects NUL/CR/LF). Different contexts.
2. **Request vs response trailer limits differ**: request-side 32 KiB
   (`BodyCopyLimits`, :22) vs response-side 64 KiB (:356). Both chunk-size
   limits are 64 MiB.
3. **`parse_authority` requires port**: Unlike `parse_authority_with_default`
   (:forward/server.rs:979), the CONNECT server's version errors on missing
   port.
4. **H2 relay is NOT a policy boundary**: checks DNS rebinding for domains
   but NOT IP literals (:152-166). Callers must screen IPs.
5. **EOF framing**: No CL and no TE (:forward/server.rs:739) means body read
   until close; marks `upstream_alive = false`.
6. **HTTP/1.0 keep-alive**: Default is close; alive only with explicit
   non-empty `Keep-Alive` (:forward/server.rs:762-765).

## See also
[Overview](overview.md), [Server](server.md), [UDP](udp.md),
[Routing](routing.md), [Transports TLS](transports-tls.md).
