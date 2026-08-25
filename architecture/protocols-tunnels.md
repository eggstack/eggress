# Protocol Tunnels -- WebSocket and Raw Passthrough

Two thin tunnel wrappers used as chain hops and listener protocols:
`eggress-protocol-websocket` (ws/wss upgrade wrapped as a byte stream) and
`eggress-protocol-raw` (fixed-target passthrough listener).

## Module map

| File | Role |
|---|---|
| `websocket/src/lib.rs` | `WebSocketStreamAdapter` (AsyncRead+AsyncWrite over binary frames), `WebSocketTunnelServer`, `WebSocketTunnelClient`, free `accept_upgrade_with_auth` function, `parse_basic_auth` |
| `websocket/src/error.rs` | `WebSocketError` enum: `Handshake`, `Connect`, `Protocol`, `MessageTooLarge`, `Io` |
| `raw/src/tunnel.rs` | `RawTunnelListener`: bind, accept loop, semaphore-gated relay to fixed target |
| `raw/src/error.rs` | `RawTunnelError` enum: `NoTarget`, `TargetConnect`, `Io`, `DnsRebinding` |

## Public API surface

### WebSocket (`eggress-protocol-websocket`)

| Symbol | Kind | Notes |
|---|---|---|
| `WebSocketStreamAdapter<S>` | struct | Generic over underlying stream `S`; holds split read/write halves, `read_buf: BytesMut`, `max_message_size`, `write_flush_outstanding` flag |
| `WebSocketStreamAdapter::new(ws, max_message_size)` | constructor | Splits a `tokio_tungstenite::WebSocketStream<S>` into read/write halves |
| `WebSocketStreamAdapter::into_boxed(self)` | method | Wraps self into `BoxStream` (type alias for `Pin<Box<dyn AsyncRead + AsyncWrite + Send>>`) |
| `WebSocketTunnelServer` | struct | Holds `max_message_size`; provides `accept_upgrade`, `accept_upgrade_over_stream`, `accept_upgrade_with_config`, `accept_upgrade_with_config_over_stream` |
| `WebSocketTunnelClient` | struct | Holds `max_message_size`; provides `connect`, `connect_with_config`, `connect_over_stream`, `connect_over_stream_with_config` |
| `accept_upgrade_with_auth(stream, credentials)` | free fn | Server-side upgrade with optional Basic proxy auth; returns `(BoxStream, Option<String>)` where `String` is the authenticated username |
| `DEFAULT_MAX_MESSAGE_SIZE` | const | 16 MiB (16 * 1024 * 1024) |

### Raw (`eggress-protocol-raw`)

| Symbol | Kind | Notes |
|---|---|---|
| `RawTunnelListener` | struct | Holds `TcpListener`, `TargetAddr`, `Arc<Semaphore>` |
| `RawTunnelListener::bind(bind_addr, target)` | async fn | Binds TCP socket; semaphore defaults to 1024 permits |
| `RawTunnelListener::local_addr()` | method | Returns bound `SocketAddr` |
| `RawTunnelListener::run()` | async fn | Accept loop; spawns `handle_raw_connection` per peer |
| `DEFAULT_MAX_CONNECTIONS` | const | 1024 |

## Wire format / protocol mechanics

### WebSocket frame mapping

The adapter maps between WebSocket message types and byte-stream semantics:

| WS frame type | AsyncRead behavior | Notes |
|---|---|---|
| `Binary(data)` | Yields `data` bytes to the reader; oversized frames trigger `InvalidData` | Partial reads buffered in `read_buf` |
| `Text(_)` | Skipped with `tracing::warn` | Text frames are not valid for binary tunnel traffic |
| `Ping(_)` / `Pong(_)` | Skipped silently | Transparent keepalive handling |
| `Close(_)` | Returns `Ok(())` (EOF) | No error; clean stream termination |
| `Frame(_)` | Skipped silently | Raw frame type (tungstenite internal) |
| Stream ends (`None`) | Returns `Ok(())` (EOF) | Upstream closed without Close frame |

On the write side, every `poll_write` call wraps the buffer as `Message::Binary(buf.to_vec().into())` and flushes immediately. The `write_flush_outstanding` flag tracks pending flushes to avoid double-flushing.

### Raw tunnel wire format

No handshake. The listener accepts a TCP connection, opens a TCP connection to the configured `TargetAddr`, and calls `tokio::io::copy_bidirectional`. Bytes flow in both directions with no protocol framing.

## How it works

### WebSocket adapter lifecycle

1. A `WebSocketStreamAdapter` is created from an already-accepted `WebSocketStream<S>` (via tungstenite). The stream is split into `SplitSink` and `SplitStream` halves.
2. `AsyncRead::poll_read` first drains the internal `read_buf` if non-empty, then loops calling `poll_next_message`. Each `Binary` frame is copied into the caller's buffer; excess bytes go into `read_buf`. Other frame types are consumed and discarded.
3. `AsyncWrite::poll_write` queues a `Binary` message via `start_send`, then attempts `poll_flush`. If the flush is pending, the write still reports `Ok(buf.len())` -- backpressure is applied on the *next* write via `write_flush_outstanding`.
4. `poll_shutdown` calls `poll_close` on the sink, sending a WebSocket Close frame.

### Server upgrade paths

`WebSocketTunnelServer` provides four accept variants:

- `accept_upgrade(stream)` -- raw TCP stream, default tungstenite config.
- `accept_upgrade_over_stream(stream)` -- any `AsyncRead+AsyncWrite` stream (TLS, H2, etc.).
- `accept_upgrade_with_config(stream, config)` -- explicit `WebSocketConfig`.
- `accept_upgrade_with_config_over_stream(stream, config)` -- both generic and explicit config.

The free function `accept_upgrade_with_auth` wraps `tokio_tungstenite::accept_hdr_async` and inspects the `Proxy-Authorization` header during the HTTP upgrade handshake. It uses constant-time comparison (`subtle::ConstantTimeEq`) for both username and password. On success the callback stores the username in an `Arc<Mutex<Option<String>>>`; on failure it returns `ErrorResponse::new(None)` (HTTP 401).

### Client connection paths

`WebSocketTunnelClient` provides four connect variants:

- `connect(url)` -- resolves URL and opens a new TCP connection.
- `connect_with_config(url, config)` -- explicit `WebSocketConfig`.
- `connect_over_stream(url, stream)` -- upgrades an existing stream (e.g., TLS or H2 CONNECT).
- `connect_over_stream_with_config(url, stream, config)` -- both.

### Raw tunnel lifecycle

1. `RawTunnelListener::bind` opens a TCP socket on `bind_addr` and stores the fixed `TargetAddr`.
2. `run()` enters an accept loop. For each peer, it acquires a semaphore permit (dropping the connection if at capacity) and spawns `handle_raw_connection`.
3. For IP targets, the upstream connection is opened directly. For domain targets, DNS resolution happens first, then `is_dns_rebinding_risk` is checked against the resolved IP.
4. `tokio::io::copy_bidirectional` relays bytes until one side closes.

## Error and failure model

### WebSocket errors (`WebSocketError`)

| Variant | When | Effect |
|---|---|---|
| `Handshake(msg)` | `accept_async` or `accept_hdr_async` fails | Connection rejected |
| `Connect(msg)` | Client-side `connect_async` fails | Connection failed |
| `Protocol(msg)` | Tungstenite sink flush/send errors | I/O error propagated to caller |
| `MessageTooLarge { size, max }` | Binary frame exceeds `max_message_size` | `io::ErrorKind::InvalidData` |
| `Io(e)` | Underlying stream error | Propagated directly |

### Raw tunnel errors (`RawTunnelError`)

| Variant | When | Effect |
|---|---|---|
| `NoTarget` | No target configured | Connection rejected |
| `TargetConnect(msg)` | Upstream TCP connect or DNS resolution fails | Connection dropped, logged |
| `DnsRebinding(ip)` | Resolved domain IP is private/reserved | Connection rejected (security) |
| `Io(e)` | Copy or accept error | Propagated |

Semaphore exhaustion in `RawTunnelListener` is not an error variant -- the connection is dropped and a warning is logged.

## Security notes

- **Origin header not validated.** `accept_upgrade_with_auth` does not check the `Origin` header. This is intentional for non-browser tunnel usage; exposing these endpoints to browsers permits cross-site WebSocket hijacking.
- **Constant-time auth.** Both `accept_upgrade_with_auth` (WebSocket) and `server_auth_handshake` (reverse) use `subtle::ConstantTimeEq` for credential comparison.
- **DNS rebinding.** `RawTunnelListener` calls `is_dns_rebinding_risk` on resolved domain IPs before connecting. IP targets bypass this check (they are already resolved).
- **Max message size.** The 16 MiB default prevents unbounded memory growth from malicious peers. Oversized frames produce a structured `MessageTooLarge` error.
- **No TLS built in.** WebSocket and raw tunnels do not perform TLS; wrap in `eggress-transport-tls` when needed (e.g., `wss://` via TLS listener).

## Concurrency and lifecycle

- `WebSocketStreamAdapter` is `Send + 'static` when `S: Send + 'static`. It is safe to hold across `.await` points.
- `RawTunnelListener::run()` is a long-lived accept loop. It uses `tokio::spawn` per connection and `Arc<Semaphore>` for connection limiting. The semaphore uses `acquire_owned` so permits move into spawned tasks.
- There is no built-in shutdown signal for `RawTunnelListener::run()` -- it runs until the task is cancelled externally.

## Test coverage map

### Unit tests (`websocket/src/lib.rs`)

| Test | What it covers |
|---|---|
| `test_websocket_echo` | Basic binary frame echo through adapter |
| `test_max_message_size_enforced` | Oversized binary frame returns error |
| `test_close_frame_yields_eof` | Close frame terminates read as clean EOF |
| `test_ping_pong_skipped` | Ping/pong frames do not reach the reader |
| `test_text_frame_skipped` | Text frames are skipped with warning |
| `test_partial_read_buffering` | Multi-frame reads with internal buffer |
| `test_accept_upgrade_with_config` | Explicit `WebSocketConfig` passed through |
| `test_bidirectional_large_payload` | 64 KiB round-trip echo |
| `test_websocket_error_display` | Error `Display` formatting |
| `test_websocket_client_new` | Client construction with custom and default size |
| `test_websocket_client_connect` | Client connect + bidirectional relay |
| `test_connect_over_stream` | `connect_over_stream` over raw TCP |

### Unit tests (`raw/src/tunnel.rs`)

| Test | What it covers |
|---|---|
| `test_bind_success` | Listener binds to port 0 |
| `test_local_addr_returns_listening_address` | `local_addr()` returns correct socket |
| `test_bind_failure_invalid_address` | Invalid bind string fails |
| `test_relay_bidirectional` | Full byte relay through tunnel |
| `test_upstream_connect_failure` | Upstream unreachable produces zero-byte read |
| `test_multiple_concurrent_connections` | 3 concurrent relays through semaphore |

### Fuzz target

`fuzz/fuzz_targets/websocket_handshake.rs` exercises:
- `WebSocketTunnelServer::accept_upgrade_over_stream` with fuzz bytes piped through a `duplex` stream (100ms timeout).
- `WebSocketError` variant construction and `Display` with fuzz strings.
- `WebSocketTunnelServer::new` and `WebSocketTunnelClient::new` with fuzz `usize` values.

## Reviewer gotchas

- `accept_upgrade_with_auth` is a **free function**, not a method on `WebSocketTunnelServer`. It uses `accept_hdr_async` directly, bypassing the server struct.
- `connect_over_stream` exists only on `WebSocketTunnelClient`, not on the server. The server has `accept_upgrade_over_stream` for the same purpose.
- The `write_flush_outstanding` flag in `AsyncWrite` means a `poll_write` can return `Ok(len)` even when the flush is still pending. The next write will block on the flush. This is correct backpressure but looks surprising.
- `RawTunnelListener::run()` has no graceful shutdown; the caller must cancel the task.
- `DEFAULT_MAX_MESSAGE_SIZE` is 16 MiB. If a chain hop uses `accept_upgrade_with_auth`, it also uses this default (hardcoded at `lib.rs:320`), not a configurable value.

## See also

- [protocols-reverse.md](protocols-reverse.md) -- reverse proxy control channel (different auth model).
- [transports-tls.md](transports-tls.md) -- wrap WebSocket/raw tunnels in TLS.
- [protocols-http.md](protocols-http.md) -- HTTP CONNECT tunnel (alternative upgrade path).
- [server.md](server.md) -- listener lifecycle that drives these protocols.
