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

### Design Choices

- **Ignored input stream**: Like WebSocket and Raw handlers, H2HopHandler creates its own connection. The chain executor's generic TLS wrapper doesn't support ALPN, so H2 manages TLS internally.
- **No connection pooling**: Each CONNECT request establishes a fresh H2 connection. Pooling is deferred to a future phase.
- **Cleartext H2**: Supported when `hop.tls = false` (e.g., for local/testing scenarios). Production use requires TLS per spec.
- **Auth**: Basic auth via `Proxy-Authorization` header, consistent with HTTP CONNECT handler.

## Consequences

- `h2://` upstream URI now launches real listener/upstream services
- Composition matrix updated: H2 upstream cell changed from `intentional_non_parity` to `drop_in`
- Capability manifest updated: `uri.scheme_h2` and `protocol.h2_runtime` promoted to `drop_in`
- `protocol_crate_only` constraint emptied (all protocols now runtime-integrated)
- Chain cells added: `socks5→h2`, `http→h2`
- Existing H2 unit tests continue to pass
- New integration test: `h2_upstream_routes_tcp_echo`
