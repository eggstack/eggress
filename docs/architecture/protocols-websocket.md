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

The `connect_over_stream` method is critical for chain composition — it performs the WebSocket handshake over a prior-hop stream.

## Configuration

- Max message size configurable
- `ws://` and `wss://` schemes supported

## Dependencies

- `eggress-core` — `BoxStream`, `ProtocolId`

See [overview.md](overview.md) for context.
