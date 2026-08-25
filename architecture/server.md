# eggress-server — Connection Orchestration

The reusable data plane: accept a connection, detect/handshake the inbound
protocol, route, open the upstream (direct or chained), reply, relay, and
report. Both the CLI service and the embed API drive connections through this
crate.

## Module map

| File | Role |
|------|------|
| `src/lib.rs` | `serve_connection()` entry point; `ConnectionConfig`, `ConnectionContext`, `SessionMetrics` trait (12 methods), `UdpService` trait, `UdpAssociationHandle`, `NoopMetrics` |
| `src/accept.rs` | Protocol detection and handshake: `AcceptedSession` (4 variants), `TunnelProtocol` (10 variants), `ReplyContext` (9 variants), `InboundAuthentication`, `AuthReuseCache` (IP-keyed, 4096 entries), `AcceptError`, `PrefixedStream`, `MAX_HEAD_SIZE` (32 KiB), `MAX_HEADER_LINES` (128) |
| `src/execute.rs` | `execute()` dispatcher; `open_route()` with connect-timeout; `build_chain_executor()`; `SessionReport`, `SessionOutcome` (7 variants), `FailureCategory` (14 variants), `PooledH2Stream`, `HttpOnlyStream`/`HttpOnlyHopHandler` |
| `src/reply.rs` | Protocol-correct success/failure replies: `send_tunnel_success()`, `send_tunnel_failure()`, `send_http_forward_failure()`, `send_http_expectation_failed()` (417), `send_http_upgrade_unsupported()` (501) |
| `src/error.rs` | `SessionOpenError` with `From` impls for `ConnectError`, `ChainError`, `HttpError`, `Socks5Error` |
| `src/advanced.rs` | `serve_h2_connection()` (H2 multiplexing), `serve_websocket_connection()` (WS upgrade). Gated on `feature = "extended"`. |
| `src/listener/unix.rs` | `UnixListener` with lifecycle management; refuses to unlink non-socket files or symlinks even when `unlink_existing=true` |
| `src/listener/transparent.rs` | Linux `SO_ORIGINAL_DST` retrieval — workspace's single documented `unsafe` block (ADR at `docs/adr/ADR_transparent_proxy_unsafe_boundary.md`) |

## Public API surface

```rust
pub async fn serve_connection(client: BoxStream, config: ConnectionConfig) -> SessionReport;

pub struct ConnectionConfig {
    pub routing: Arc<dyn RouteService>,
    pub context: ConnectionContext,      // { source, listener, generation }
    pub handshake_timeout: Duration,
    pub connect_timeout: Duration,
    pub protocols: Arc<[ProtocolId]>,
    pub authentication: InboundAuthentication,
    pub metrics: Option<Arc<dyn SessionMetrics>>,
    pub udp: Option<Arc<dyn UdpService>>,
    pub tls_client_config: Option<Arc<rustls::ClientConfig>>,
    pub shadowsocks: Option<InboundShadowsocksConfig>,
    pub trojan: Option<InboundTrojanConfig>,
    pub fixed_target: Option<TargetAddr>,
    pub local_bind: Option<String>,
    #[cfg(feature = "ssh")]      pub ssh_sessions: Option<Arc<SshSessionCache>>,
    #[cfg(feature = "extended")] pub shadowsocks_metrics: Option<Arc<ShadowsocksMetrics>>,
    #[cfg(not(feature = "extended"))] pub shadowsocks_metrics: Option<()>,
}

pub trait SessionMetrics: Send + Sync {
    // Core 6 — no defaults: record_session_start, record_session,
    //   record_route_decision, record_upstream_open, record_upstream_failure,
    //   record_auth_failure
    // Optional 6 — default no-op: record_platform_capability_check_failure,
    //   record_unix_listener_connection_accepted, record_reload,
    //   set_config_generation, record_udp_association_created, render_prometheus
}
pub trait UdpService: Send + Sync { /* create_association, is_enabled, active_count */ }
```

## How it works — `serve_connection` pipeline

1. **Metrics start** — `record_session_start()` if metrics configured (`lib.rs:125`).
2. **Handshake with timeout** — `tokio::time::timeout(handshake_timeout, accept_with_fixed_target_for_peer(...))` wraps the entire accept phase (`lib.rs:129-142`). Timeout → `HandshakeTimedOut`.
3. **Protocol detection** (`accept.rs`) — first byte: `0x05` → SOCKS5, `0x04` → SOCKS4, otherwise → HTTP method detection via `detect_http_method()` (up to 64 bytes prefix). Single-protocol listeners (Shadowsocks, Trojan, Raw, Echo) skip detection.
4. **Authentication** — per-connection or `AuthReuseCache` lookup. SOCKS5/4/HTTP use `subtle::ConstantTimeEq`.
5. **Dispatch** — `execute()` on `AcceptedSession`: Tunnel → `execute_tunnel`, HttpForward → `execute_http_forward`, UdpAssociate → `execute_udp_associate`, Echo → `execute_echo`.
6. **Route open** — `open_route()` calls `routing.route()` then `DirectConnector.connect_with_options()` (direct) or `ChainExecutor.execute()` (upstream). Wrapped in `tokio::time::timeout(connect_timeout, ...)` (`execute.rs:349`). Does NOT cover HTTP body upload (`execute.rs:640-643`).
7. **Deferred success reply** — sent only after route opens: HTTP 200, SOCKS4 granted, SOCKS5 REP=0x00, Shadowsocks/Trojan/Raw: no reply.
8. **Relay** — `eggress_core::relay::relay()` bidirectional half-close-aware copy.
9. **Failure reply** — `send_tunnel_failure()` (`reply.rs:54`) maps `SessionOpenError` to per-protocol codes.
10. **Metrics end** — exactly one `record_session(&report)` before returning (`lib.rs:192`). Every code path reaches this block.

## Error & failure model

### `SessionOutcome` (7 variants)

| Variant | When |
|---------|------|
| `Completed` | Relay finished normally |
| `ClientProtocolError` | Malformed handshake, unsupported expectation, or failed reply write |
| `AuthenticationFailed` | Inbound credentials rejected |
| `HandshakeTimedOut` | Accept phase exceeded `handshake_timeout` |
| `RouteFailed` | `open_route()` error or policy rejection |
| `RelayFailed` | `relay()` terminated with error |
| `Cancelled` | Session cancelled (e.g., shutdown drain) |

### `FailureCategory` (14 variants)

| Category | Source | `SessionOpenError` mapping |
|----------|--------|---------------------------|
| `Protocol` | Client protocol errors | — |
| `Authentication` | Inbound auth failure | — |
| `HandshakeTimeout` | Accept timeout | — |
| `Dns` | DNS resolution failed | `Dns` |
| `ConnectionRefused` | Upstream refused | `Refused` |
| `NetworkUnreachable` | ICMP unreachable | `NetworkUnreachable` |
| `HostUnreachable` | ICMP host unreachable / NXDOMAIN | `HostUnreachable` |
| `RouteTimeout` | Connect/upstream timeout | `Timeout` |
| `RouteHop` | Chain hop failure | `Hop { .. }` |
| `UpstreamAuthentication` | Upstream proxy auth rejected | `UpstreamAuthentication` |
| `PolicyDenied` | Router rejected | `PolicyDenied` |
| `Relay` | I/O error, reset, other IO | `Other(_)` |
| `Cancelled` | Session cancelled | — |
| `Internal` | Invariant violation | — |

`FailureCategory::from_io_error()`: `ConnectionRefused` → `ConnectionRefused`, everything else → `Relay`.

### Reply mapping (tunnel protocols)

| `SessionOpenError` | HTTP CONNECT | SOCKS5 REP | SOCKS4 | SS/Trojan/Raw |
|--------------------|-------------|-----------|--------|---------------|
| `Timeout` | 504 | 0x06 | Failed | close |
| `PolicyDenied` | 403 | 0x02 | Failed | close |
| `Refused` | 502 | 0x05 | Failed | close |
| `Dns` | 502 | 0x04 | Failed | close |
| `NetworkUnreachable` | 502 | 0x03 | Failed | close |
| `HostUnreachable` | 502 | 0x04 | Failed | close |
| Other (`Hop`, `UpstreamAuth`, `Other`) | 502 | 0x01 | Failed | close |

H2/WebSocket failure: stream shutdown (no framed error code). HTTP forward failures use the same `http_failure_status()` mapping.

### `SessionOpenError` — key `From` conversions

`ConnectError` maps `ConnectionRefused`→`Refused`, `Timeout`→`Timeout`, `DnsResolution`→`Dns`, `TlsHandshake`/`Io`/`ReservedTarget`→`Other(msg)`. `ChainError::ConnectFailed` maps to `Hop { hop_index, source: from(source) }`. `HttpError::AuthRequired`/`AuthFailed`→`UpstreamAuthentication`. `Socks5Error::AuthFailed`→`UpstreamAuthentication`. Full table in `error.rs`.

## Configuration & features

### `ConnectionConfig` fields

| Field | Purpose |
|-------|---------|
| `routing` | Compiled router for rule evaluation and upstream selection |
| `context` | `ConnectionContext { source: Option<SocketAddr>, listener: String, generation: u64 }` |
| `handshake_timeout` | Bounds accept/detection/handshake phase |
| `connect_timeout` | Bounds route opening (direct TCP or chain execution) |
| `protocols` | Protocol allow-list for this listener |
| `authentication` | Inbound auth policy (None, UsernamePassword, UsernamePasswordWithReuse) |
| `metrics` / `udp` | Optional metrics sink / UDP association service |
| `tls_client_config` | Optional TLS override for upstream (e.g., test-only insecure) |
| `shadowsocks` / `trojan` | Inbound protocol configs (method, password, fallback) |
| `fixed_target` | Override target for single-purpose listeners (Raw, WebSocket) |
| `local_bind` | Outgoing bind address for direct connections |
| `ssh_sessions` | SSH session cache (feature `ssh`) |
| `shadowsocks_metrics` | SS metrics (feature `extended`; `()` in lean builds) |

### Feature gates

| Feature | Adds |
|---------|------|
| `extended` | Shadowsocks/Trojan/WebSocket inbound+outbound, `advanced.rs` (H2/WS listeners), `ShadowsocksMetrics` |
| `pproxy-legacy` | ShadowsocksR (SSR) inbound/outbound |
| `ssh` | `SshHopHandler`, `SshSessionCache` |
| `quic` | `QuicHopHandler`, `H3HopHandler` |
| `legacy-crypto` | Legacy Shadowsocks cipher methods |

### Hop handler registry (`build_chain_executor`)

Handlers in fixed order (`execute.rs:980`): Http, HttpOnly, Socks5, Socks4, [Shadowsocks, Trojan, WebSocket] (extended), [ShadowsocksR] (pproxy-legacy), Raw, Unix, [Ssh] (ssh), H2, [Quic, H3] (quic). The executor also installs a TLS wrapper using system root CAs or the `tls_client_config` override.

## Security notes

- **Constant-time auth**: SOCKS5 username (`accept.rs:758`), HTTP Basic (`accept.rs:983`, `advanced.rs:96`) use `subtle::ConstantTimeEq`.
- **AuthReuseCache**: IP-keyed, max 4096, lazy expiry, LRU eviction (`accept.rs:31-75`). pproxy-compat only; native listeners authenticate every connection.
- **Header limits**: 32 KiB head (`MAX_HEAD_SIZE`), 128 lines (`MAX_HEADER_LINES`) (`accept.rs:1242-1245`).
- **Transparent unsafe**: workspace's single `unsafe` block — `getsockopt(SO_ORIGINAL_DST)` FFI. Two `#[allow(unsafe_code)]` annotations: `query_original_dst` (sockaddr init + getsockopt) and `parse_sockaddr` (sockaddr_in/in6 reinterpretation with length validation).
- **Unix socket safety**: `UnixListener::bind()` refuses to unlink non-socket files or symlinks (`listener/unix.rs:96-122`); only `FileType::is_socket()` passes.
- **Trojan fallback**: on password mismatch, if `fallback` is set, relay to fallback target instead of rejecting (`accept.rs:632-646`).
- **H2/WS listener auth**: `serve_h2_connection()` and `serve_websocket_connection()` perform per-stream/per-connection auth with the same CT comparison.

## Concurrency & lifecycle

- **Exactly-once metrics**: `record_session_start()` at entry, exactly one `record_session(&report)` before every return. Enforced structurally and by `metrics_lifecycle_tests` (`lib.rs:1232+`).
- **Handshake timeout**: wraps `accept_with_fixed_target_for_peer()` — detection, auth, handshake all bounded.
- **Connect timeout**: wraps `open_route()` but NOT HTTP body upload (`execute.rs:640-643`).
- **Deferred success replies**: sent only after `open_route()` succeeds (`execute.rs:466`).
- **HTTP forward keep-alive**: loops over requests; breaks on `Connection: close`, upstream close, client EOF, or malformed request.
- **H2 listener** (`advanced.rs:56`): per-stream `TaskTracker` spawn with child-token cancellation.
- **WebSocket listener** (`advanced.rs:163`): single WS upgrade with fixed target.
- **UDP ASSOCIATE** (`execute.rs:937-961`): TCP control held alive; ends on client close or cancel. `connect_timeout` also bounds `create_association()`.
- **Shadowsocks metrics**: `record_tcp_session_closed()` + `record_tcp_flow_close()` after standard finalization (`lib.rs:196-202`).

## Test coverage map

| Area | Tests |
|------|-------|
| End-to-end | SOCKS5 direct, HTTP CONNECT direct, HTTP POST (Content-Length + chunked), HTTP GET |
| Handshake timeout | No bytes, partial HTTP, partial SOCKS5, completes before timeout |
| Metrics lifecycle | Balanced start/end for success, auth failure, timeout, route failure, cancelled, protocol error, no double finalization |
| Failure categories | DNS, refused, unreachable, timeout, upstream auth, IO errors, policy denied |
| Body upload | Connect timeout does NOT limit body upload; Expect 100-continue -> 417; upgrade unsupported -> 501 |
| Advanced | H2 listener routes CONNECT; WebSocket listener routes binary |
| Lean build | SS/Trojan/WS rejected in non-extended build; HTTP/SOCKS still works |
| HttpOnly | origin-form rewrite, header terminator preservation, mixed line endings |

Run: `cargo test -p eggress-server`

## Reviewer gotchas

1. `execute_tunnel` vs `execute_http_forward`: share `open_route()` but differ in relay strategy (raw relay vs HTTP pump with keep-alive).
2. `FailureCategory::Relay` covers both `ConnectionReset` and `TimedOut` I/O errors — no distinction.
3. `SessionReport::rejected()` produces `RouteFailed` + `PolicyDenied`; NOT a separate `SessionOutcome`.
4. Non-extended build: `shadowsocks_metrics` field becomes `Option<()>`.
5. `HttpOnlyHopHandler` rewrites origin-form to absolute-form for pproxy `httponly` compat — not a general rewriter.
6. `PooledH2Stream` wraps stream + pool guard; dropping releases connection back to H2 pool.
7. `PrefixedStream` (`accept.rs:263`) replays bytes consumed during detection — every accept path wraps the stream.
8. `open_route()` maps `RouteError::NoEligibleUpstream` and `RouteError::UnknownGroup` to `PolicyDenied` (`execute.rs:324-328`).

## See also

- [core.md](core.md) — relay, BoxStream, chain executor, hop handler trait
- [protocols-http.md](protocols-http.md) — HTTP CONNECT/forward, hop-by-hop filtering
- [protocols-socks.md](protocols-socks.md) — SOCKS4/5 protocol details
- [routing.md](routing.md) — rule evaluation and upstream selection
- [runtime.md](runtime.md) — supervisor lifecycle, shutdown ordering
- [metrics.md](metrics.md) — MetricsRegistry and Prometheus rendering
- [udp.md](udp.md) — UDP association lifecycle
