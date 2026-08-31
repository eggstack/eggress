# eggress-core -- Foundation Types, Streams, Relay, Chain Execution

Root dependency of nearly every other crate. Defines the universal stream
boundary (`BoxStream`), destination/identity types, the bidirectional relay,
protocol sniffing/dispatch, the direct connector with DNS-rebinding protection,
and the multi-hop `ChainExecutor` used for all upstream chains.

## Module map

| File | Role |
|---|---|
| `src/lib.rs` | Core types: `BoxStream`, `TargetAddr`/`TargetHost`, `ClientIdentity`, `SessionContext`, `ProtocolId`, `UpstreamId`, `RejectReason`, `RouteAction`; error enums (`ConnectError`, `ProtocolError`, `AuthError`, `RelayError`); `AsyncStream` blanket trait; crate re-exports |
| `src/listener.rs` | `TcpListener`: semaphore-bounded accept via `PermitStream` wrapper that holds the permit until the connection drops |
| `src/connector.rs` | `DirectConnector` with `ConnectOptions`; `is_reserved_or_private_ip` / `is_dns_rebinding_risk` for all IPv4 and IPv6 private/reserved/special-use ranges including IPv4-mapped v6 |
| `src/relay.rs` | `relay()`: bidirectional copy using `JoinSet` with half-close awareness (shutdown write half on EOF) and `AtomicU64` byte counters; returns `RelayResult` with `TerminationReason` |
| `src/replay.rs` | `ReplayStream`: bounded sniff buffer (default 8 KiB, 2048-byte stack read chunks) that replays consumed bytes so detection is single-pass |
| `src/detect.rs` | `ProtocolDetector` trait (`id()` + `detect(prefix) -> DetectResult`); `PrefixDetector` with configurable min_length |
| `src/dispatch.rs` | `ProtocolDispatcher`: ordered detector list, first match wins; `DispatchError` with Timeout, BufferOverflow, NoMatch |
| `src/chain.rs` | `ChainExecutor` + `HopHandler` trait: walks a chain of hops, each handler consumes the prior hop's stream; TLS wrapping, QUIC/H3 transport open, Unix socket, local_bind support |
| `src/capability.rs` | `classify_upstream_chain()`: static classification of TCP/UDP capability for a `ProxyChainSpec` |

## Public API surface

### Identity & addressing (`lib.rs`)

| Type | Variants/Fields | Notes |
|---|---|---|
| `ProtocolId` | Http, Socks4, Socks5, Shadowsocks, ShadowsocksR, Trojan, Http2, Http3, Quic, WebSocket, Raw, Echo, Reverse | 13 variants; `Debug`/`Display` |
| `UpstreamId` | newtype `Arc<str>` | `Serialize`, `FromStr`, `Display` |
| `TargetHost` | `Ip(IpAddr)` / `Domain(String)` | Domains stay unresolved until dial |
| `TargetAddr` | `host: TargetHost`, `port: u16` | `FromStr` parses `[ipv6]:port` / `host:port`; rejects unbracketed IPv6 |
| `ClientIdentity` | Anonymous / Username(String) / Opaque(String) | `Debug` does not leak credentials |
| `SessionContext` | session_id (u64), client_identity, target_addr | `Debug`, `Clone` |
| `RouteAction` | Direct / Upstream(UpstreamId) / Reject(RejectReason) | Routing decision |
| `RejectReason` | UnsupportedProtocol, AuthRequired, AccessDenied, Blocked, InternalError | Human-readable `Display` |

### Stream types (`lib.rs`)

- `AsyncStream` = blanket trait for `AsyncRead + AsyncWrite + Send + Unpin`
- `BoxStream` = `Box<dyn AsyncStream>` -- the universal stream boundary

### Error enums (`lib.rs`)

| Enum | Variants |
|---|---|
| `ConnectError` | ConnectionRefused, Timeout, DnsResolution(String), TlsHandshake(String), ReservedTarget(IpAddr), Io(io::Error) |
| `ProtocolError` | MalformedMessage, UnsupportedVersion, MethodNotSupported, AddressTypeNotSupported, Io(io::Error) |
| `AuthError` | InvalidCredentials, MethodNotSupported, Required, Io(io::Error) |
| `RelayError` | ConnectionClosed, Io(io::Error) |

### Listener (`listener.rs`)

- `TcpListenerConfig`: bind_addr, protocols, auth_required, handshake_timeout, connection_limit
- `AcceptedConnection`: stream (BoxStream), peer_addr, local_addr
- `TcpListener::new()` / `new_with_reuse_port()`: binds via `socket2` with `SO_REUSEADDR` always; `SO_REUSEPORT` on Unix only
- `accept()`: acquires `OwnedSemaphorePermit` *before* TCP accept, wraps stream in `PermitStream` -- permit lives until stream drop

### Connector (`connector.rs`)

- `Connector` trait (dyn-compatible via `trait_variant`): `connect(&self, target: &TargetAddr) -> Result<BoxStream, ConnectError>`
- `DirectConnector::connect_with_options()`: optional local_bind, optional DNS-rebinding check
- `is_reserved_or_private_ip()`: covers loopback, link-local, private, unspecified, multicast, broadcast, documentation (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24, 192.88.99.0/24), benchmarking (198.18.0.0/15), reserved-future (240.0.0.0/4), this-network (0.0.0.0/8), IPv6 loopback/link-local/unique-local/unspecified/multicast/documentation/discard; IPv4-mapped v6 addresses are translated and checked against v4 ranges. Domain lookups are rejected if any returned address is reserved, even when the same response also contains a public address; this conservative split-horizon policy prevents an unsafe answer from being selected during DNS rebinding checks.

### Relay (`relay.rs`)

- `relay(client: BoxStream, server: BoxStream) -> RelayResult`
- Splits both streams via `io::split`, spawns two copy tasks in a `JoinSet`
- `copy_direction()`: 8 KiB buffer; on read EOF, calls `writer.shutdown()` (half-close); on error, aborts both tasks
- `RelayResult`: bytes_upstream, bytes_downstream, termination_reason
- `TerminationReason`: ClientClosed, ServerClosed, BothClosed, Error, Cancelled

### Replay (`replay.rs`)

- `ReplayStream::new(stream)` / `with_max_buffer(stream, max_buffer)`
- Default max_buffer: 8192 bytes; internal read chunk: 2048 bytes (stack-allocated)
- Two modes: sniffing (buffer + return to caller) and pass-through
- `finish_sniff()`: switches to pass-through; buffer retained for inspection
- `buffered_remaining()`: unconsumed bytes in buffer
- `into_inner()`: consumes wrapper; inner stream is positioned after all sniffed bytes

### Detection (`detect.rs`)

- `DetectResult`: `Match { confidence: u8 }` / `NeedMore { minimum: usize }` / `NoMatch`
- `ProtocolDetector` trait: `id() -> ProtocolId`, `detect(&[u8]) -> DetectResult`
- `PrefixDetector::new()` / `with_min_length()`: simple byte-prefix matching

### Dispatch (`dispatch.rs`)

- `ProtocolDispatcher::new(detectors, max_sniff, handshake_timeout)` / `with_defaults()`
- `dispatch(stream) -> Result<(ProtocolId, ReplayStream), DispatchError>`
- Flow: read into `ReplayStream` up to max_sniff bytes, run all detectors in order on accumulated prefix; first `Match` wins; `NeedMore` continues reading; timeout wraps the whole operation
- `DispatchError`: Timeout, BufferOverflow(usize), NoMatch, Io(io::Error)

### Chain (`chain.rs`)

- `HopHandler` trait (dyn-compatible): `protocol()`, `open()` (optional, for QUIC/H3), `handshake(stream, target, hop, hop_index)`
- `ChainExecutor::new(handlers)`, `with_tls_wrapper()`, `with_shared_tls_config()`
- `execute(chain, target) -> Result<BoxStream, ChainError>`
- Execution flow: pre-flight handler validation, connect to hop 0 (TCP/QUIC/Unix), then for each hop: TLS wrap if `hop.tls` → application handshake → next hop
- TLS wrapping: uses `hop.server_name` (defaults to endpoint host), sets H2 ALPN for Http2 protocol
- `ChainError`: EmptyChain, ConnectFailed{hop_index, endpoint, source}, HandshakeFailed{hop_index, protocol, source}, InvalidChain{reason}
- `HandshakeError`: Io, Protocol, ConnectionRefused, AuthFailed, Other
- `TlsWrapper`: boxed async closure `(BoxStream, String, Option<Vec<Vec<u8>>>) -> Result<BoxStream, ...>`

### Capability (`capability.rs`)

- `classify_upstream_chain(chain: &ProxyChainSpec) -> UpstreamCapabilities`
- `UpstreamCapabilities`: tcp_connect + udp_associate, each a `CapabilityResult` (Supported / UnsupportedProtocol{protocol} / UnsupportedChain{reason})
- Single-hop rules: HTTP/Socks4/Trojan/H2/H3/WebSocket/Raw/Ssh/Unix = TCP only; Socks5/Shadowsocks = TCP + UDP; SSR = TCP only; QUIC alone = no TCP
- Multi-hop: TCP supported; UDP supported only when every hop is single-protocol Socks5 or Shadowsocks
- QUIC at first hop: TCP supported, UDP unsupported ("QUIC UDP stream mapping")
- Zero hops (direct): both unsupported ("direct")

## How it works (control flow)

1. `TcpListener::accept()` acquires a semaphore permit, accepts the TCP socket, wraps it in `PermitStream`
2. `ProtocolDispatcher::dispatch()` wraps the stream in `ReplayStream`, reads up to max_sniff bytes, runs ordered detectors
3. On match, returns `(ProtocolId, ReplayStream)` -- the ReplayStream preserves sniffed bytes for the protocol handler
4. The protocol handler reads from the ReplayStream (buffered bytes replayed first, then live stream)
5. For outbound chains, `ChainExecutor::execute()` connects to hop 0, applies TLS if configured, runs each `HopHandler::handshake()` in sequence
6. `relay()` bidirectionally copies data between client and server streams with half-close support

## Error & failure model

- All errors are `thiserror`-derived with structured variants
- `ChainError` carries hop_index for precise failure location
- `DispatchError` distinguishes timeout, buffer overflow, and no-match conditions
- `ConnectError::ReservedTarget` is the DNS-rebinding rejection variant
- `relay()` returns `TerminationReason` rather than erroring on normal EOF

## Configuration/features

- No feature flags in eggress-core itself
- `socket2` for socket options; `tokio-util` for `CancellationToken`
- Workspace `unsafe_code = "deny"` applies

## Security notes

- DNS-rebinding protection: `is_reserved_or_private_ip` covers all RFC-reserved ranges; IPv4-mapped IPv6 addresses are converted to v4 before checking
- `ConnectOptions::enforce_dns_rebinding_check` is opt-in (default false) -- callers must explicitly enable it
- `CredentialSpec::Debug` and `RedactedUri::Display` never emit plaintext passwords
- `ClientIdentity::Debug` does not redact (identities are not secrets)

## Concurrency & lifecycle

- `PermitStream` holds `OwnedSemaphorePermit` until the connection is dropped -- connection limit applies to the whole session, not just the accept call
- `relay()` uses `JoinSet` for two copy tasks; on error, `abort_all()` cancels both directions
- `CancellationToken` on `TcpListener` allows graceful shutdown of the accept loop
- `ReplayStream` is `!Sync` (owns `Vec<u8>` buffer) -- safe because it moves through single-task protocol handling

## Test coverage map

| Module | Test count (lib) | Key coverage |
|---|---|---|
| `lib.rs` | 9 | TargetAddr display/FromStr, IPv6 bracketing, RejectReason display |
| `listener.rs` | 4 | Accept, cancellation, connection_limit held until drop, SO_REUSEPORT |
| `connector.rs` | 19 | Echo connect, DNS-rebinding for domains, reserved IPv4/IPv6 ranges (loopback, private, link-local, multicast, broadcast, documentation, benchmarking, reserved-future, this-network, discard prefix, IPv4-mapped) |
| `relay.rs` | 3 | Echo relay, half-close, cancellation |
| `replay.rs` | 7 | Buffer during sniff, partial reads, into_inner, write delegation, finish_sniff, custom max_buffer, empty read |
| `detect.rs` | 5 | Prefix match/no-match, need-more, empty input, exact match, custom min_length |
| `dispatch.rs` | 9 | HTTP/Socks5/SSH detection, no-match, timeout, buffer overflow, ordered detection, fragmented, stream close |
| `chain.rs` | ~30 | Empty/invalid chains, missing handlers, connect/handshake failures, domain preservation through 1-3 hops, credentials, handler selection, error indexing, TLS wrapping |
| `capability.rs` | 10 | Per-protocol classification, multi-hop, empty chain, QUIC at first hop, reason label stability |
| **Total** | **105** | |

## Reviewer gotchas

- `ChainExecutor::execute()` does **not** apply DNS-rebinding checks -- that is `DirectConnector`'s responsibility. Chain handlers connect to hop endpoints, not to the final target.
- `ReplayStream::into_inner()` does not replay buffered bytes to the inner stream; they were already returned to callers during sniff reads. The inner stream is positioned right after the sniffed prefix.
- `detect.rs` `DetectResult::Match` carries a `confidence: u8` field, but `ProtocolDispatcher` ignores confidence -- first match wins unconditionally.
- `relay()` reports `TerminationReason::BothClosed` even if one side closed first and the other followed; `Error` is only set when a task panics or returns an IO error.
- `HopHandler::open()` is only used for QUIC/H3 transport opening; TCP-backed handlers use the default (returns `None`) and rely on `DirectConnector`.

## See also

- [overview.md](overview.md) -- system architecture
- [uri.md](uri.md) -- proxy chain URI grammar consumed by `ChainExecutor`
- [config.md](config.md) -- TOML schema and compilation
- [routing.md](routing.md) -- rule matching and route selection
- [server.md](server.md) -- connection orchestration driving core types
- [runtime.md](runtime.md) -- supervisor lifecycle and runtime snapshot
