# eggress-protocol-http — HTTP/1.1 CONNECT, Forward Proxy, H2 CONNECT

HTTP protocol family: server-side CONNECT accept, client-side CONNECT,
absolute-form forward proxying with origin-form conversion, and HTTP/2 CONNECT
with a connection pool. Detection (`HttpDetector`) distinguishes proxy-usable
HTTP from other protocols by method/response shape.

## Module map

| File | Role |
|---|---|
| `src/connect/server.rs` | `handle_connect`: parse CONNECT head (bounds: 32 KiB head, 128 header lines), Basic auth via constant-time compare |
| `src/connect/client.rs` | `http_connect`: send CONNECT + read reply (status line ≤ 1 KiB, headers ≤ 32 KiB / 100 count); `validate_credentials` rejects control characters |
| `src/forward/server.rs` | `forward_request`/`forward_response`: absolute→origin form, hop-by-hop header filtering (RFC 2616 §13.5.1), body framing via Content-Length or chunked; chunk caps (64 MiB payload, 64 KiB trailers, ≤ 8 informational responses) |
| `src/h2_connect.rs` | H2 CONNECT client/server/relay; `H2ConnectionPool`/`H2PoolRegistry` keyed by endpoint + SHA-256 of credentials (identity isolation); exports `H2_PROTOCOL_METRICS` global atomics consumed by eggress-metrics |
| `src/detect.rs`, `src/error.rs`, `src/connect/test_server.rs` | Detection, `HttpError` with status mapping, synthetic test server |

## Invariants worth reviewing

- Bounded parsing everywhere (no unbounded header accumulation).
- Auth comparisons constant-time; upstream credentials validated before wire.
- Hop-by-hop semantics preserved while keeping `Transfer-Encoding: chunked`.
- H2 relay drains with a bounded timeout to avoid stuck streams.

## Review entry points

- Verify: `cargo test -p eggress-protocol-http`; fuzz targets
  `http_connect_response`, `h2_connect_authority`.
