# ADR B4: HTTP/2 CONNECT Runtime Promotion

## Status

Accepted

## Context

Phase B3 promoted WebSocket (ws/wss) and Raw (raw/tunnel) transport protocols from protocol-crate-only support to runtime-integrated upstream protocols. H2 CONNECT was explicitly deferred to B4 because it "requires complex bidirectional stream adaptation" (ADR B3).

Eggress already contained H2 CONNECT server/relay primitives in `eggress-protocol-http::h2_connect`:
- `handle_h2_connect()` — server-side H2 connection accept loop
- `h2_connect_relay()` — bidirectional relay between H2 stream and TCP target
- `H2StreamWrite` — AsyncWrite adapter for h2::SendStream with flow control

What was missing:
- H2 CONNECT client (sending CONNECT to an upstream H2 proxy)
- AsyncRead adapter for h2::RecvStream
- HopHandler integration in the chain executor
- End-to-end test evidence

## Decision

Promote H2 CONNECT to `drop_in` tier as an upstream-only protocol, following the B3 pattern established by WebSocket and Raw.

### Implementation

1. **`h2_connect_client<S>`** — Generic H2 CONNECT client function in `eggress-protocol-http::h2_connect`. Accepts any `AsyncRead + AsyncWrite + Unpin + Send + 'static` stream (TCP or TLS-wrapped). Performs h2 client handshake, sends CONNECT request with optional Basic auth, returns `(SendStream, RecvStream, JoinHandle)`.

2. **`H2StreamRead`** — AsyncRead adapter for `h2::RecvStream` with internal byte buffering. Handles partial reads across H2 data frames.

3. **`H2HopHandler`** — Implements `HopHandler` for `ProtocolSpec::Http2`. Creates its own TCP connection to the endpoint (ignoring the chain executor's input stream, following the B3 pattern). If `hop.tls`, wraps with TLS using h2 ALPN via `TlsClientConfigBuilder::with_h2_alpn()`. Performs H2 CONNECT handshake, wraps streams as bidirectional `BoxStream` via `tokio::io::join`.

4. **Chain integration** — Registered in `build_chain_executor()` handlers vec alongside WebSocket and Raw handlers.

### Connection Pooling (B4 completion)

5. **`H2ConnectionPool`** — Bounded connection pool with idle timeout and GOAWAY-aware retirement. Keyed by `(endpoint_host, endpoint_port, use_tls, server_name, auth_hash)`. Default pool size: 4, idle timeout: 60s, max concurrent streams per connection: 100.

6. **`H2PoolRegistry`** — Global static registry (`H2_POOL_REGISTRY`) mapping pool keys to pool instances. Pools are created lazily on first use.

7. **`h2_connect_client_pooled<S>`** — Pooled variant that acquires a connection from the pool (or creates a new one), sends CONNECT, and returns streams with an `H2PoolGuard`. The guard releases the connection back to the pool when dropped.

8. **`PooledH2Stream`** — Wrapper that holds an `H2PoolGuard` alongside the bidirectional stream, ensuring the pooled connection is released only when the stream is dropped.

9. **`H2HopHandler` integration** — Builds a `H2PoolKey` from the hop spec and calls `h2_connect_client_pooled`. The pool guard is held by `PooledH2Stream` for the connection lifetime.

### Design Choices

- **Ignored input stream**: Like WebSocket and Raw handlers, H2HopHandler creates its own connection. The chain executor's generic TLS wrapper doesn't support ALPN, so H2 manages TLS internally.
- **Connection pooling**: Global static pool keyed by endpoint. Bounded by semaphore (pool_size). Idle timeout reaps unused connections. GOAWAY/connection errors retire entries and create new connections.
- **Cleartext H2**: Supported when `hop.tls = false` (e.g., for local/testing scenarios). Production use requires TLS per spec.
- **Auth**: Basic auth via `Proxy-Authorization` header, consistent with HTTP CONNECT handler.

## Consequences

- `h2://` upstream URI now launches real listener/upstream services with connection pooling
- Composition matrix updated: H2 upstream cell changed from `intentional_non_parity` to `drop_in`
- Capability manifest updated: `uri.scheme_h2` and `protocol.h2_runtime` promoted to `drop_in`
- `protocol_crate_only` constraint emptied (all protocols now runtime-integrated)
- Chain cells added: `socks5→h2`, `http→h2`, `socks5→h2→http` (3-hop)
- Existing H2 unit tests continue to pass
- Connection pool reuses H2 connections across requests to the same endpoint
- 20 integration tests covering: basic echo, auth success/failure, concurrent streams, chain combinations, connection loss recovery, connection reuse, flow control pressure, concurrent stream limits, config validation
