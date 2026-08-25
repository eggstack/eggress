# Protocol Tunnels — WebSocket and Raw Passthrough

Two thin tunnel wrappers used as chain hops and listener protocols:
`eggress-protocol-websocket` (ws/wss upgrade wrapped as a byte stream) and
`eggress-protocol-raw` (fixed-target passthrough listener).

## eggress-protocol-websocket

Single-file crate (`src/lib.rs`):

- `WebSocketStreamAdapter`: implements AsyncRead+AsyncWrite over binary
  WebSocket frames; Text frames skipped with warning; ping/pong handled
  transparently; Close frame ⇒ EOF. Partial reads buffered internally.
- Max message size 16 MiB default; oversized ⇒ InvalidData error.
- `accept_upgrade_with_auth` / `connect_over_stream` variants allow running WS
  over any prior stream (TLS, H2, plain TCP) — this is what makes
  `socks5__ws__...` chains stream-native.
- Optional Basic auth checked constant-time. Origin header intentionally not
  validated (non-browser tunnel usage).

## eggress-protocol-raw

- `RawTunnelListener` (`src/tunnel.rs`): binds a port, forwards EVERY accepted
  connection to one fixed target via `copy_bidirectional`. No handshake.
- Connection cap via semaphore (default 1024).
- Domain targets pass through DNS rebinding validation before connect.
- As a chain hop (`raw://`), it is a pure passthrough of the prior stream.

## Review entry points

- Verify: `cargo test -p eggress-protocol-websocket`,
  `cargo test -p eggress-protocol-raw`; fuzz target `websocket_handshake`.
