# eggress-protocol-websocket

`crates/eggress-protocol-websocket/`

WebSocket tunnel adapter wrapping `tokio-tungstenite` for proxy chaining.

## Key Types

| Type | Description |
|---|---|
| `WebSocketStreamAdapter` | Wraps `WebSocketStream` as `AsyncRead + AsyncWrite` |
| `WebSocketTunnelServer` | Accept WebSocket upgrade on server side |
| `WebSocketTunnelClient` | Connect to WebSocket endpoint |
| `WebSocketError` | WebSocket-specific error type |

## Stream Adapter

`WebSocketStreamAdapter` converts `tokio-tungstenite::WebSocketStream` into a `BoxStream` by:
- Mapping `AsyncRead` to WebSocket message reading
- Mapping `AsyncWrite` to WebSocket message writing
- Handling ping/pong frames transparently

## Client Operations

| Method | Description |
|---|---|
| `connect(url)` | Open WebSocket connection to URL |
| `connect_over_stream(stream)` | Upgrade existing stream to WebSocket |

## Compatibility listener

The pproxy compatibility path supports `ws{host:port}://listener` and
`wss{host:port}://listener` as fixed-target listeners. The upgrade is handled
by `accept_upgrade_with_auth`; binary frames become the proxied byte stream,
while ping/pong and close frames remain standards-compliant. WSS uses the
existing listener TLS configuration and `http/1.1` ALPN.

The `connect_over_stream` method is critical for chain composition — it performs the WebSocket handshake over a prior-hop stream.

## Configuration

- Max message size configurable
- `ws://` and `wss://` schemes supported

## Dependencies

- `eggress-core` — `BoxStream`, `ProtocolId`

See [overview.md](overview.md) for context.
