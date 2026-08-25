# eggress-protocol-reverse -- Reverse / Backward Proxy (NAT Traversal)

Implements pproxy's backward model: a client behind NAT dials OUT to an
acceptor; external clients hit the acceptor and their sessions are tunneled
through control channels back to the NAT'd host. Also ships pproxy-wire
compatible adapters (raw and SOCKS5-framed backward channels).

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | Native auth handshake: `write_auth` sends `user:pass\n`, `read_handshake` reads 1-byte verdict (0x01 accept / 0x00 reject), `server_auth_handshake` orchestrates full server-side flow, `redact_auth` for logs, `ControlState` enum, half-close-preserving `relay_bidirectional_with_timeout`, auth payload cap 4096 bytes, 100 ms delay on auth failure |
| `src/server.rs` | `ReverseServer`: control listener pools authenticated channels via `mpsc`; external connections pop a channel and relay. `ReverseServerConfig::validate()` enforces defense-in-depth: non-loopback external bind requires BOTH auth credentials AND non-empty `allow_bind` allowlist. `ReverseServerState` atomic counters for admin hooks |
| `src/client.rs` | `ReverseClient`: connect, auth, resolve target via `TargetResolver` trait, relay, reconnect. Backoff: 1 s initial, doubling, 30 s cap. `DefaultTargetResolver` returns configured host/port or rejects |
| `src/compat_pproxy.rs` | `PproxyBackwardClient`/`PproxyBackwardServer`: raw byte-pipe and SOCKS5-framed channel adapters matching upstream pproxy backward wires. Auth bytes are NOT newline-terminated (differs from native protocol) |
| `src/metrics.rs` | `ReverseMetrics`: control conns active/accepted/rejected, auth failures, reconnects, heartbeat failures, streams opened/closed, per-state timing, Prometheus export |

## Public API surface

### Native protocol (`lib.rs`)

| Symbol | Kind | Notes |
|---|---|---|
| `HANDSHAKE_ACCEPT` | const | `0x01` |
| `HANDSHAKE_REJECT` | const | `0x00` |
| `ControlState` | enum | `Disconnected`, `Connecting`, `Authenticating`, `Ready`, `Draining`, `Closed` |
| `ProtocolError` | enum | `AuthFailed`, `AuthRequired`, `ConnectionClosed`, `BindDenied(SocketAddr)`, `ConfigInvalid(String)`, `Io(io::Error)` |
| `write_auth(stream, user, pass)` | async fn | Writes `user:pass\n` to TCP stream |
| `read_handshake(stream)` | async fn | Reads 1 byte; returns `AuthFailed` if reject |
| `write_handshake_accept(stream)` | async fn | Sends `0x01` |
| `write_handshake_reject(stream)` | async fn | Sends `0x00` |
| `client_auth_handshake(stream, user, pass)` | async fn | Client-side: write auth, read verdict |
| `server_auth_handshake(stream, expected_user, expected_pass)` | async fn | Server-side: read auth (capped 4 KiB), validate, respond. Returns redacted `user:****` string |
| `redact_auth(auth)` | fn | Returns `user:****` for logging; never leaks password |
| `relay_bidirectional(a, b)` | async fn | Wrapper around `relay_bidirectional_with_timeout` with no timeout |
| `relay_bidirectional_with_timeout(a, b, idle_timeout)` | async fn | Half-close-preserving relay; both sides drain independently |

### Server (`server.rs`)

| Symbol | Kind | Notes |
|---|---|---|
| `ReverseServerConfig` | struct | `control_bind`, `external_bind`, `auth_*`, `max_control_connections`, `read_timeout_ms`, `allow_bind`, `max_listeners_per_client`, `max_streams_per_listener`, `max_pending_external` |
| `ReverseServerConfig::validate()` | method | Fails if non-loopback external bind without auth + allowlist |
| `ReverseServerConfig::is_bind_allowed(addr)` | method | Checks `allow_bind` list; `None`/empty means all allowed |
| `ReverseServer` | struct | `new`, `set_metrics`, `state_handle`, `cancel_token`, `run`, `shutdown` |
| `ReverseServerState` | struct | Atomic counters: `active_control`, `active_streams`, `pending_external`, `denied_bind`, `dropped_stream_limit`, `dropped_pending_limit` |

### Client (`client.rs`)

| Symbol | Kind | Notes |
|---|---|---|
| `ReverseClientConfig` | struct | `server_addr`, `auth_*`, `reconnect_initial_ms` (1000), `reconnect_max_ms` (30000), `default_target_*`, `read_timeout_ms`, `drain_grace_ms`, `target_connect_timeout_ms` |
| `TargetResolution` | enum | `Connect { host, port }` or `Reject { reason }` |
| `TargetResolver` | trait | `fn resolve(&self) -> TargetResolution` |
| `DefaultTargetResolver` | struct | Returns configured target or Reject |
| `ReverseClient` | struct | `new`, `set_metrics`, `set_resolver`, `cancel_token`, `run`, `shutdown` |

### Compat adapters (`compat_pproxy.rs`)

| Symbol | Kind | Notes |
|---|---|---|
| `PproxyBackwardClient` | struct | One persistent worker per `+in` occurrence; auth bytes written raw (no newline, no accept/reject byte) |
| `PproxyBackwardServer` | struct | Accepts control channels, pairs with external clients |
| `PproxyBackwardFraming` | enum | `Raw` (byte pipe) or `Socks5` (pproxy 2.7.9 `+in` interop) |
| `raw_auth(user, pass)` | fn | Builds `user:pass` bytes (no newline) |

## Wire format / protocol mechanics

### Native handshake

```
Client                              Server
  |                                    |
  |--- user:pass\n ------------------>|  (write_auth)
  |                                    |  server_auth_handshake reads up to 4096 bytes
  |                                    |  until \n; validates with ConstantTimeEq
  |<----------- 0x01 (accept) --------|  (or 0x00 reject + 100ms delay)
  |                                    |
  |===== bidirectional relay ========|
```

The auth payload is capped at 4096 bytes (`MAX_AUTH_BYTES` in `lib.rs:109`). A payload without a trailing `\n` is rejected as `AuthFailed`. The newline is part of the wire framing; requiring it prevents truncated credentials from being accepted at EOF.

### Pproxy compat handshake

```
Client (PproxyBackwardClient)        Server (PproxyBackwardServer)
  |                                    |
  |--- raw auth bytes ---------------->|  (NO newline, NO accept/reject byte)
  |                                    |  server compares byte-for-byte
  |                                    |
  |===== channel paired with ========|
  |     external client               |
```

The pproxy compat adapter does NOT send or read the 0x01/0x00 accept/reject byte. Auth bytes are raw (`format!("{user}:{pass}").into_bytes()` in `compat_pproxy.rs:584`). The `Socks5` framing mode additionally negotiates SOCKS5 hello/methods/CONNECT during the channel setup phase.

### Half-close relay

`relay_bidirectional_with_timeout` uses `tokio::io::split` on both streams and runs two independent copy loops (`a_to_b`, `b_to_a`) via `tokio::join!`. When one direction observes EOF (or idle timeout), it calls `shutdown` on the write half of the other side. Both loops must complete before the function returns -- this preserves half-close semantics so the still-open direction can drain remaining bytes.

## How it works

### Server lifecycle (`ReverseServer::run`)

1. `validate()` is called: non-loopback external bind without auth + allowlist is rejected.
2. `allow_bind` is checked against the external bind address.
3. Control listener binds. External listener binds (if configured).
4. A bounded `mpsc` channel (capacity 256) carries authenticated `ControlStream` objects from the control acceptor to the external acceptor.
5. **Control acceptor**: accepts TCP, increments `active_control` (atomic fetch_add with AcqRel), enforces `max_control_connections` (TOCTOU-safe via atomic check-then-sub), authenticates via `server_auth_handshake`, sends to channel via `try_send`.
6. **External acceptor**: accepts TCP, enforces `max_streams_per_listener` and `max_pending_external` via `fetch_update`, receives a control stream from the channel, spawns `relay_bidirectional_with_timeout` in a `JoinSet`.
7. On `shutdown()`, the cancel token triggers. External accept loop aborts all relay tasks and joins them.

### Client lifecycle (`ReverseClient::run`)

1. Loop: `run_session()` attempts connect, auth, resolve target, relay.
2. On success (session ended cleanly): backoff resets to `reconnect_initial_ms`, reconnect immediately.
3. On error: backoff doubles (1s -> 2s -> 4s ... -> 30s cap), sleeps with cancel-aware `tokio::select!`.
4. Target resolution uses the injected `TargetResolver`. Production deployments inject a resolver that consults the route engine via `SharedRoutingService::decide()`.

### allow_bind enforcement

`ReverseServerConfig::validate()` (`server.rs:95-121`) enforces:

| External bind | Auth configured | allow_bind non-empty | Result |
|---|---|---|---|
| Loopback | Any | Any | OK |
| Non-loopback | No | Any | **Rejected** |
| Non-loopback | Yes | No/empty | **Rejected** |
| Non-loopback | Yes | Yes | OK |

`is_bind_allowed` (`server.rs:72-78`) checks exact IP+port match. `same_bind` compares IPv4/IPv6 separately (no mapped-address normalization).

## Error and failure model

### Protocol errors (`ProtocolError`)

| Variant | When | Effect |
|---|---|---|
| `AuthFailed` | Credentials mismatch or missing trailing `\n` | 100 ms delay, connection closed |
| `AuthRequired` | Auth expected but not provided | Connection closed |
| `ConnectionClosed` | Empty auth payload (EOF before `\n`) | Connection closed |
| `BindDenied(addr)` | External bind not in `allow_bind` | Server refuses to start |
| `ConfigInvalid(msg)` | Validation failure (e.g., non-loopback without auth) | Server refuses to start |
| `Io(e)` | Underlying TCP error | Propagated |

### Reconnect backoff parameters

| Parameter | Default | Notes |
|---|---|---|
| `reconnect_initial_ms` | 1000 | Reset on clean session end |
| `reconnect_max_ms` | 30000 | Ceiling for exponential backoff |
| Backoff formula | `min(initial * 2^n, max)` | Doubles each failure, resets on success |
| Target connect timeout | 10000 ms | Per `target_connect_timeout_ms` |
| Drain grace | 50 ms (hardcoded in `run`) | After cancel, before returning |

## Security notes

- **Plaintext auth.** Credentials cross the wire as `user:pass\n` with no challenge. Captured handshakes are replayable. Wrap the control channel in TLS when it leaves a trusted network.
- **Defense-in-depth validation.** `ReverseServerConfig::validate()` refuses non-loopback external binds without BOTH auth AND an explicit `allow_bind` allowlist. This is enforced at startup, not per-connection.
- **Constant-time comparison.** `server_auth_handshake` uses `subtle::ConstantTimeEq` for credential validation.
- **Auth failure delay.** 100 ms sleep (`AUTH_FAILURE_DELAY` at `server.rs:16`) after failed auth to slow brute-force attempts.
- **Auth payload cap.** 4096 bytes maximum (`MAX_AUTH_BYTES` at `lib.rs:109`). Prevents unbounded memory growth from malicious clients.
- **One session per control connection.** Each control channel carries exactly one proxy session (matching pproxy's backward model). Parallelism requires N control connections.

## Concurrency and lifecycle

- `ReverseServer` uses `tokio::spawn` per control connection and per relay. `JoinSet` manages relay tasks; `abort_all()` on shutdown.
- `active_control` is decremented on: auth failure, channel-full, relay completion, and drain. The decrement uses `fetch_sub` (Relaxed ordering) after the counter was incremented with `AcqRel`.
- `active_streams` uses `fetch_update` (AcqRel/Acquire) for the check-and-increment, avoiding TOCTOU races on `max_streams_per_listener`.
- `pending_external` similarly uses `fetch_update` for `max_pending_external`.
- `control_tx.try_send` is used (not `send().await`) to avoid blocking the accept loop when the channel is full.

## Test coverage map

### Unit tests (`lib.rs`)

| Test | What it covers |
|---|---|
| `parse_auth_str_*` | Auth string parsing (normal, empty, no-colon, multiple colons) |
| `redact_auth_*` | Redaction behavior (basic, no-colon, empty, password-with-colon, no-leak) |
| `handshake_constants` | Accept/reject byte values |
| `control_state_variants` | All 6 `ControlState` variants constructible |
| `auth_handshake_success` | Full round-trip: client writes, server validates, accept sent |
| `auth_handshake_failure` | Wrong password produces error |
| `auth_no_credentials_configured` | No-auth mode: server accepts without checking |
| `relay_bidirectional_data` | Bidirectional byte relay between two TCP streams |

### Unit tests (`server.rs`)

| Test | What it covers |
|---|---|
| `is_bind_allowed_*` | Allowlist matching (None, empty, match, mismatch) |
| `state_snapshot_round_trip` | Atomic counter snapshot |
| `validate_*` | 7 validation cases: loopback OK, no external OK, non-loopback without auth rejected, non-loopback with auth but no allowlist rejected, non-loopback with both OK, IPv6 loopback OK, IPv6 non-loopback rejected |

### Unit tests (`client.rs`)

| Test | What it covers |
|---|---|
| `default_resolver_*` | Configured target, unset, partial |
| `custom_resolver_can_reject` | Custom resolver returning Reject |

### Unit tests (`compat_pproxy.rs`)

| Test | What it covers |
|---|---|
| `raw_auth_is_not_newline_terminated` | Confirm no `\n` in pproxy auth bytes |
| `client_cancellation_is_prompt` | Cancel token stops reconnect loop |
| `raw_backward_client_and_server_relay_*` | Full end-to-end: client auth, server accept, external relay |

### Unit tests (`metrics.rs`)

| Test | What it covers |
|---|---|
| `new_counters_are_zero` | All counters start at 0 |
| `record_*` | Per-counter increment behavior |
| `record_error_truncates_long_messages` | 256-char truncation |
| `record_error_handles_multibyte_messages` | UTF-8 character truncation |
| `snapshot_*` | Snapshot construction, Display, Serialize, Clone |
| `prometheus_output_contains_expected_names` | Prometheus text format correctness |

## Reviewer gotchas

- **Auth format differs between native and pproxy.** Native sends `user:pass\n` (with newline, validated via `server_auth_handshake`). Pproxy compat sends raw `user:pass` (no newline, byte-for-byte comparison). Do not mix these.
- **`read_handshake` with no auth.** When no auth is configured, the server sends `0x01` and the client reads it via `read_handshake`. This works because `read_handshake` only rejects `0x00`.
- **`relay_bidirectional` is not strictly half-close.** Both copy loops run to completion, and each side calls `shutdown` on its write half when its read loop ends. But `tokio::join!` waits for both, so the function only returns when both directions finish.
- **`try_send` not `send`.** The control channel uses `try_send` (non-blocking). If the channel is full, the authenticated control connection is dropped and the counter is decremented. This avoids head-of-line blocking in the accept loop.
- **`allow_bind` is IP+port exact match.** `same_bind` does not normalize IPv4-mapped IPv6 addresses. `[::1]:8080` and `127.0.0.1:8080` are different entries.
- **Backoff resets on clean session end.** If the external client disconnects (normal session end), the client reconnects immediately without backoff. Backoff only applies on errors.
- **Metrics `heartbeat_failures_total` is tracked but never incremented** in the current codebase. The counter exists for future use; do not interpret its zero value as "no heartbeats configured."
- **`PproxyBackwardClient::run_connection` sends auth bytes directly** without the native protocol's newline or accept/reject handshake. This is by design for pproxy wire compatibility.

## See also

- [protocols-tunnels.md](protocols-tunnels.md) -- WebSocket and raw tunnel protocols.
- [runtime.md](runtime.md) -- `TargetResolver` implementation via `SharedRoutingService::decide()`.
- [pproxy-compat.md](pproxy-compat.md) -- pproxy compatibility layer overview.
- [metrics.md](metrics.md) -- metrics architecture and Prometheus integration.
