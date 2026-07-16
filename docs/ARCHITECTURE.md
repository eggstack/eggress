# Eggress Architecture

## Overview

Eggress is a multi-protocol TCP proxy framework built on Tokio. It supports mixed-protocol listeners (HTTP CONNECT, SOCKS4/4a, SOCKS5) with direct or chained upstream connections.

## Crate Structure

### eggress-core
Core types, traits, and infrastructure:
- `TargetAddr`, `TargetHost` — typed destination addresses preserving domain names
- `ClientIdentity` — anonymous or authenticated client identity
- `SessionContext` — per-connection metadata
- `BoxStream` — boxed async byte stream trait alias
- `TcpListener` — connection-accepting listener with semaphore limits
- `DirectConnector` — TCP connector with DNS resolution
- `relay()` — bidirectional half-close-aware data relay
- `ReplayStream` — bounded sniff buffer for protocol detection
- `ProtocolDispatcher` — ordered protocol detection and dispatch
- `ProtocolId` — typed protocol identifier enum (Http, Socks4, Socks5)
- `ChainExecutor` — multi-hop proxy chain execution

### eggress-server
Server orchestration library providing the reusable connection-handling API:
- `AcceptedSession` — typed inbound session (tunnel or HTTP forward)
- `PendingTunnel` / `PendingHttpForward` — parsed requests before route opening
- `RequestBodyKind` — explicit body framing type
- `InboundAuthentication` — listener authentication policy (none or username/password)
- `AcceptError` — accept-phase error types including authentication failure
- `serve_connection()` — main entry point: detect → accept (with timeout) → route → reply → relay
- `SessionReport` — structured connection outcome with protocol, target, route, byte counts, and failure category
- `SessionOutcome` — normalized outcomes: Completed, ClientProtocolError, AuthenticationFailed, HandshakeTimedOut, RouteFailed, RelayFailed, Cancelled
- `FailureCategory` — detailed failure diagnostics: Protocol, Authentication, HandshakeTimeout, Dns, ConnectionRefused, NetworkUnreachable, HostUnreachable, RouteTimeout, UpstreamAuthentication, RouteHop, Cancelled, Relay, Internal
- `SessionOpenError` — normalized route failure types with protocol-specific reply mapping
- `SessionMetrics` — trait for recording session metrics (latency, bytes, outcome)
- Deferred success replies — success is sent only after outbound route is established
- Common route opening — both tunnel and HTTP forward use the same `open_route()` function
- Protocol enforcement — listener configuration restricts which protocols are accepted
- Handshake timeout — configurable timeout for inbound protocol establishment

### eggress-runtime
Service supervisor and composition layer:
- `CompiledRuntimeSnapshot` — single authoritative runtime snapshot
- `compile_runtime_snapshot()` — builds shared upstream registry, router, and health plan
- `RuntimeState` — shared state with snapshot, readiness, and snapshot-based generation
- `ServiceSupervisor::run()` returns `Result<(), RuntimeError>`; bind conflicts and tokio runtime init errors are structured, not panics
- Pre-bind listeners before readiness
- Separate cancellation tokens for listeners, connections, health, admin
- Shutdown sequence: readiness false → stop listeners → drain → force-cancel → stop admin (admin stays up through drain so /-/ready, /metrics, /-/status remain queryable)
- Signal handling — SIGHUP for reload, SIGTERM/SIGINT for graceful shutdown
- Health manager integration — background health probes use each upstream's compiled `HealthConfig`
- Metrics integration — session metrics recording via `SessionMetrics` trait

### eggress-cli
CLI binary with `clap`-derived arguments:
- `-l` / `--listen` — listener URIs (multiple allowed)
- `-r` / `--remote` — upstream proxy URIs (chains with `__`)
- `--config` — TOML configuration file (runtime mode)
- `--admin` — admin endpoint for route explanation
- `upstream-test` — test upstream reachability (connect or proxy mode)
- `route-explain` — explain routing decision for a target
- Default: mixed HTTP listener on 127.0.0.1:8080

### eggress-uri
URI parser with typed AST:
- `ProxyChainSpec` → `ProxyHopSpec` → `ProtocolSpec`, `EndpointSpec`, `CredentialSpec`
- `+` separates protocols within a hop
- `__` separates proxy hops
- Redacted Display implementation for secret-safe logging

### eggress-routing
Policy-driven routing and upstream selection:
- Rule AST: `CompiledRule`, `MatchExpr` (host exact/suffix/regex, CIDR, port, source, listener, protocol, identity)
- First-match-wins rule evaluation with configurable default action
- Upstream groups with persistent scheduler instances (first-available, round-robin, random, least-connections)
- Active connection accounting with `PendingLease`/`ActiveLease`
- Health state machine with hysteresis (Unknown, Healthy, Suspect, Unhealthy, Recovering, Disabled)
- Active TCP health probes with configurable intervals and jitter
- `RouteService` trait for pluggable routing backends
- `SharedRoutingService` with `ArcSwap` for atomic config reload
- Route explanation tooling for operator debugging
- `SelectionReason` variants: Normal, DirectFallback, UnhealthyFallback
- Health configuration per upstream
- Compatibility regex parser for pproxy-style rule files

### eggress-config
TOML configuration with validation:
- Versioned schema with typed runtime model
- Recursive matcher expressions (`all`, `any_of`, `not`)
- Expanded leaf matchers (host, port range, port set, CIDR, listener, protocol, identity)
- Validation: duplicate IDs, unknown references, invalid URIs, duration parsing, regex validation, CIDR validation
- Secret sources (inline, environment variable, file)
- Health configuration per upstream
- PAC/static content configuration
- CLI compatibility compilation

### eggress-metrics
Prometheus-compatible metrics:
- `SessionMetrics` trait for recording session outcomes
- Connection counters, byte counters, route decision labels
- Upstream health gauges, config generation tracking
- Reload success/failure counters
- Bounded label cardinality

### eggress-admin
Local admin HTTP server:
- `AdminSnapshotProvider` trait with `AdminSnapshot` (generation, router, pac, static_routes, listeners)
- Runtime implements the trait so admin handlers see live data from the current `CompiledRuntimeSnapshot` on every request; reloads take effect without restarting admin
- `StaticAdminSnapshot` for tests that need a fixed view
- Readiness reflects runtime state
- Health/readiness endpoints
- Status, routes, upstreams, config JSON endpoints
- Prometheus metrics endpoint
- PAC generation and serving
- Static content serving
- Body size limits for route explanation
- Route explanation supports optional `source` (SocketAddr) and `identity` (Username, 1-256 bytes) fields
- `RouteService` trait for pluggable routing backends
- `SharedRoutingService` with `ArcSwap` for atomic config reload

### eggress-protocol-http
HTTP/1 protocol implementation:
- CONNECT server and client with Basic auth
- Absolute-form forwarding with origin-form conversion
- Bounded header parsing
- Request body framing validation (Content-Length, Transfer-Encoding)
- Bounded chunked body copying with extensions, CRLF validation, and limits
- Byte-counting response forwarding

### eggress-protocol-socks
SOCKS4/4a and SOCKS5 protocol implementations:
- Server and client for both protocol versions
- SOCKS4a domain preservation for remote DNS
- SOCKS5 method negotiation, no-auth and username/password auth
- Bounded credentials (255 bytes)
- SOCKS5 UDP ASSOCIATE command and reply
- SOCKS5 UDP datagram codec (encode/decode with IPv4, IPv6, and domain targets)

### eggress-protocol-shadowsocks
Shadowsocks protocol implementation:
- AEAD cipher methods: AES-128-GCM, AES-256-GCM, ChaCha20-IETF-Poly1305
- Key derivation via HKDF-SHA256
- Shadowsocks address encoding/decoding (IPv4, IPv6, domain)
- TCP CONNECT with encrypted address header
- UDP packet encode/decode with random nonce
- Legacy method detection (`is_legacy_method()`) producing `LegacyMethodUnsupported` errors
- SSR detection producing `SsrUnsupported` errors
- Synthetic server tests for all methods

### eggress-protocol-trojan
Trojan protocol implementation:
- SHA224 password hash authentication
- `encode_trojan_request()` — single helper that validates domain length (1-255)
  and produces the wire format (hash + CRLF + CONNECT + address + port + CRLF)
- TLS transport via shared `eggress-transport-tls` layer
- `trojan_connect()` — performs TLS handshake via the shared client connector
  and delegates request encoding to `encode_trojan_request()`
- Accepts optional `Arc<ClientConfig>` for shared config (falls back to system roots)
- Hash tests, encoder tests (domain length validation, IPv4/IPv6/domain layout),
  and synthetic TLS happy-path test that exercises `trojan_connect()` directly
  and asserts the server-observed request bytes

### Advanced Transport Upstream Support (Phase B3) — Stream-Native Composition

WebSocket (`ws://`, `wss://`) and raw/tunnel (`raw://`, `tunnel://`) schemes are
integrated as runtime-integrated upstream protocols. H2 CONNECT (`h2://`) is also
runtime-integrated. All three intermediate-hop handlers now consume the prior-hop
stream instead of opening independent connections, enabling true stream-native
composition across all chain types.

### eggress-transport-tls
Shared TLS transport layer:
- `TlsClientConfigBuilder` — builds `Arc<ClientConfig>` from system roots, custom CA PEM, ALPN, insecure mode, server name override
- `TlsServerConfigBuilder` — builds `Arc<ServerConfig>` from cert chain and key PEM
- `tls_connect` / `tls_accept` — wraps `BoxStream` in TLS
- `load_system_roots` / `load_pem_roots` / `load_pem_certs` — root certificate loading
- `TlsError` — structured error type for TLS operations
- Used by: `eggress-runtime` (listener TLS), `eggress-server` (upstream TLS), `eggress-protocol-trojan` (Trojan TLS)

### eggress-pproxy-compat
pproxy compatibility layer:
- `pproxy translate` — converts pproxy CLI args to TOML configuration
- `pproxy check` — validates translated configuration
- `pproxy run` — runs eggress with pproxy-style arguments (translated internally)
- URI translation from pproxy listen/remote format to eggress TOML
- Flag mapping: `-l`, `-r`, `-s`, `-v`, `-a`, `--ssl`, `-b`, `--rulefile`, `-a`, `--pac`, `--test`, `--sys`
- Default port inference for pproxy URI schemes (`default_port_for_scheme()`)
- `__` chain separator parsing
- Structured diagnostics with stable `DiagnosticCode` enum and `StructuredDiagnostic` JSON output

### eggress-embed
Rust embed API for in-process embedding:
- `EggressConfig::from_toml_str()` / `from_toml_file()` — parse and validate config
- `EggressService::new(config).start_blocking()` — blocking start, returns `EggressHandle`
- `EggressService::new(config).start().await` — async start within a Tokio runtime
- `handle.bound_addresses()` — discover listener ports (supports port-0)
- `handle.status()` — generation, readiness, uptime, active connections
- `handle.metrics_text()` — Prometheus metrics without HTTP
- `handle.reload_toml_str()` — hot-reload routing/upstreams
- `handle.shutdown()` / `shutdown_blocking()` — graceful shutdown (idempotent)
- Thread ownership: async path uses Tokio blocking-pool thread + dedicated OS thread; blocking path uses outer startup thread + inner run thread

### Native OutboundConnector

`eggress-embed::outbound` provides `OutboundConnector` for native Rust outbound connections without temporary local listeners:
- `OutboundConnector::from_toml(toml)` — create from TOML config
- `OutboundConnector::from_pproxy_uri(uri)` — create from pproxy URI
- `connector.connect_tcp(target)` — connect to TCP target
- `connector.connect_tcp_timeout(target, timeout)` — connect with explicit timeout

The Python binding exposes the same connector through `PyOutboundConnector` and
returns a `PyOutboundStream`. The pure-Python `OutboundStream` and
`AsyncOutboundStream` wrappers provide read/write/half-close/close operations,
release the GIL around native blocking work, and do not bind a temporary local
listener. `ProxyConnection` delegates to this path. UDP remains listener-based;
`associate_udp()` is intentionally not advertised as a completed Python API.

The canonical `eggress` wheel owns only the `eggress` namespace. The separate
`eggress-pproxy-compat` distribution installs the top-level `pproxy` package for
the certified subset, pins the matching `eggress` version, and declares the
`cryptography` dependency used by the supported AEAD cipher objects. It does
not use `sys.modules` aliasing or import-time namespace mutation.

### eggress-python
Python bindings via PyO3 wrapping `eggress-embed`:
- `EggressConfig`, `EggressService`, `EggressHandle` — direct Rust wrappers
- `PPProxyService` — pproxy-compatible service builder (`from_args`, `from_uri`, `from_toml`, `from_file`, `start`, context manager)
- `PPProxyHandle` — alias for `EggressHandle`
- `CompatibilityReport`, `FeatureInfo` — tier classification and diagnostics
- `start_pproxy()` — multi-mode convenience function (args, local/remote, config, config_path)
- `Server` — pproxy-compatible server wrapper with sync/async context managers, observability (`status()`, `sessions`, `last_error`), hot-reload, and resource management
- URI helpers: `check_pproxy_uri`, `redact_pproxy_uri`, `diagnostics_for_uri`, `supported_features`
- Config explanation: `explain_config_toml`, `explain_pproxy_args`, `explain_pproxy_uri`
- Translation: `translate_pproxy_args`, `translate_pproxy_uri`, `check_pproxy_args`
- Route/upstream: `route_explain`, `test_upstream_connect`
- GIL release via `py.detach()` on all blocking Rust calls
- `.pyi` type stubs for all public modules
- Package: `eggress` on PyPI, wheels for Linux/macOS/Windows, `py.typed` PEP 561 marker

### eggress-udp
UDP association management and direct forwarding:
- `UdpAssociation` — association state machine with ownership by TCP control connection
- `UdpAssociationRegistry` — bounded association tracking with global and per-listener limits
- `UdpTargetFlow` — connected UDP socket per target for reliable response demux
- `UdpFlowKind` — enum distinguishing direct and SOCKS5 upstream flows
- `UdpFlowKey` — typed flow key for direct and upstream flows
- `UdpLimits` — configurable association, datagram, and idle constraints
- `UdpMetrics` — Prometheus-compatible counters and gauges for UDP operations
- `UdpRelayCapability` — classifies proxy chains as UDP-supported or unsupported
- `validate_target` — security policy rejecting multicast, broadcast, unspecified, and port zero
- `upstream_socks5` — SOCKS5 upstream client with handshake and UDP ASSOCIATE
- `testkit` — UDP echo server and SOCKS5 UDP test server for integration tests

### eggress-testkit
Test utilities:
- Echo server, half-close server
- Temporary port allocator

## Data Flow

```
Client → TcpListener → serve_connection()
    → accept() — protocol detection with timeout and authentication
    → RouteRequest — build from session metadata
    → Router.decide() — evaluate rules, return RouteDecision
    → Router.select() — scheduler picks upstream, returns SelectedRoute with ActiveLease
    → open_route() — direct or chain via SelectedRoute
    → send success/failure reply
    → relay() or HTTP forward exchange (with byte counting)
    → SessionReport (with rule ID, upstream group, byte counts, failure category)
```

## TLS Transport Layer

TLS is applied at two points in the data flow:

### Listener TLS (inbound)

```
TCP accept → raw TcpStream
    → if listener has TLS config: tls_accept() → TlsStream<TcpStream>
    → protocol detection on the unwrapped stream
    → route → relay
```

Configured via `[listeners.tls]` in TOML with cert/key PEM files.

### Upstream TLS (outbound)

```
ChainExecutor::execute()
    → DirectConnector.connect(first_hop) → BoxStream
    → for each hop:
        if hop.tls: TlsWrapper(stream, server_name) → BoxStream (TLS)
        handler.handshake(stream, target, creds) → BoxStream (protocol)
```

Configured via `+tls` URI suffix (e.g., `socks5+tls://proxy:1080`) or `tls = true` on the hop spec.

### Chain handler stream usage

Each `HopHandler` implementation receives a `BoxStream` from the prior hop (or from the `DirectConnector` for the first hop). All handlers are **stream-consuming** — they perform a protocol handshake on the provided stream and return the upgraded stream:

- `HttpHopHandler` — writes `CONNECT` request to the stream, reads 200 response, returns the stream
- `Socks5HopHandler` — performs SOCKS5 greeting + CONNECT on the stream, returns the upgraded stream
- `Socks4HopHandler` — performs SOCKS4 CONNECT on the stream, returns the upgraded stream
- `ShadowsocksHopHandler` — encrypts the stream with AEAD, returns the encrypted stream
- `TrojanHopHandler` — performs TLS handshake + Trojan request on the stream, returns the encrypted stream
- `RawHopHandler` — passes through the prior-hop stream directly (raw passthrough, no protocol overhead)
- `WebSocketHopHandler` — performs WebSocket handshake over the prior-hop stream via `connect_over_stream()`
- `H2HopHandler` — performs H2 CONNECT handshake over the prior-hop stream; TLS ALPN is handled by the chain executor

All intermediate-hop chains (socks5→ws, http→ws, socks5→raw, http→raw, socks5→h2, http→h2) are now classified as `drop_in` in the parity manifest.

Test coverage: `crates/eggress-runtime/tests/upstream_protocols.rs` includes tests that verify stream consumption for all handler types (`chain_http_connect_consumes_prior_hop_stream`, `chain_ws_consumes_prior_hop_stream`, `chain_raw_consumes_prior_hop_stream`, `chain_h2_consumes_prior_hop_stream`).

## UDP Data Flow

```
TCP SOCKS5 client → UDP ASSOCIATE command → server creates UdpAssociation
    → reply with UDP relay bind address (computed per listener config)
    → client sends SOCKS5 UDP datagrams to relay address
    → association decodes datagram, validates client ownership
    → route engine evaluates full route() with fallback support
    → direct: forward via direct UDP socket (one per target flow)
    → reject: drop with policy metric
    → unsupported upstream: drop (with direct fallback if configured)
    → response from target mapped back to client
    → SOCKS5 UDP response datagram sent to pinned client address
    → idle timeout or TCP control close tears down association
    → relay task removes association from registry
```

## UDP Upstream Relay

Eggress supports relaying UDP through one-hop SOCKS5 upstream proxies. The relay
works as follows:

1. Client sends SOCKS5 UDP datagram to Eggress
2. Eggress decodes the target address and payload
3. Routing selects a direct or upstream path
4. For SOCKS5 upstream paths:
   - Establishes TCP control connection to upstream
   - Performs SOCKS5 handshake (method negotiation + optional auth)
   - Sends UDP ASSOCIATE request to upstream
   - Creates per-target UDP association with upstream
   - Encodes and sends SOCKS5 UDP datagrams to upstream relay
   - Receives responses and forwards to client
5. For direct paths: sends directly via connected UDP socket

### Flow Model

Each target flow through a SOCKS5 upstream maintains:
- A TCP control connection (kept alive while flow is active)
- A UDP socket bound to 127.0.0.1:0
- An active lease held on the upstream

Flows are keyed by target address. On first packet to a target, a new upstream
association is established. Subsequent packets to the same target reuse the
existing flow until idle timeout.

### Unsupported Chains

HTTP, SOCKS4, and multi-hop chains are explicitly rejected for UDP with metrics.
No silent fallback to direct unless the routing policy selects a direct fallback.

### Reload Semantics

- Existing upstream flows keep their selected upstream until idle expiry
- New flows use the latest routing snapshot after reload
- Removed upstreams are unavailable for new flows but old flows drain

### Metrics

Upstream-specific metrics in `/metrics`:
- `eggress_udp_upstream_associations_active` - active upstream UDP associations
- `eggress_udp_upstream_packets_up_total` - packets sent upstream
- `eggress_udp_upstream_packets_down_total` - packets received upstream
- `eggress_udp_upstream_failures_total` - upstream handshake failures
- `eggress_udp_unsupported_upstream_total` - unsupported chain attempts

The `/-/udp` endpoint includes `upstream_flows_active` in its response.

### Configuration Example

```toml
[[listeners]]
name = "socks-in"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
advertise = "127.0.0.1"
idle_timeout = "60s"
target_idle_timeout = "30s"
max_associations = 512
max_targets_per_association = 32
max_datagram_size = 65535
client_pin = true

[[upstreams]]
id = "socks-upstream"
uri = "socks5://user:pass@127.0.0.1:19080"

[[upstream_groups]]
id = "udp-egress"
scheduler = "first-available"
members = ["socks-upstream"]
fallback = "reject"

[[rules]]
id = "udp-via-socks"
upstream_group = "udp-egress"

[rules.match]
all = [
  { transport = "udp" },
  { destination_port = 53 }
]
```

### UDP association lifecycle

A UDP association is created when a SOCKS5 client issues a UDP ASSOCIATE command. The lifecycle is:

1. **Create**: `UdpAssociationRegistry::create_association()` allocates a slot and returns an `Arc<UdpAssociation>`.
2. **Relay**: `udp_relay_loop()` runs as a tracked task (`TaskTracker`), decoding client datagrams, routing through the rule engine, forwarding to targets via connected sockets, and relaying responses back.
3. **Idle timeout**: A periodic tick checks `last_activity().elapsed()`. If the association exceeds `idle_timeout` without valid client or target activity, the relay loop breaks.
4. **Close**: The relay loop closes the association, aborts all target-flow recv tasks, and removes the association from the registry via `registry.remove(id)`.
5. **Registry cleanup**: Every close path (TCP control close, idle timeout, runtime shutdown) ensures the association is removed from the registry exactly once.

Activity is updated (`touch()`) for:
- Valid client datagrams after successful client-pin validation.
- Target responses sent back to the client.

Rejected packets from wrong client addresses do not update activity.

### Target-flow model

Each unique target address within an association gets its own `UdpTargetFlow` backed by a connected UDP socket. This design:

- Simplifies response demultiplexing (replies arrive on the target-specific socket).
- Makes client address pinning straightforward.
- Bypasses the need for shared-socket peer mapping.

Target flows are bounded by `max_targets_per_association`. Idle target flows are reaped periodically via `target_idle_timeout`, freeing slots for new targets. Each flow has a dedicated recv task tracked via `JoinHandle`.

### Routing per datagram

Every UDP datagram is routed through the Phase 2 rule engine using `RouteService::route()` (full route selection, not just `decide()`). This preserves upstream-group fallback semantics:

- `SelectedRoute::Direct { selection_reason: Normal }` — forward to target via direct UDP socket.
- `SelectedRoute::Direct { selection_reason: DirectFallback }` — forward via direct, with fallback metric recorded.
- `SelectedRoute::Upstream { .. }` — forward via one-hop SOCKS5 upstream if capable; drop with `unsupported_upstream` metric for HTTP/SOCKS4/multi-hop chains.
- `RouteError::Rejected { .. }` — drop with policy metric.

Route rules can match on `transport` to distinguish UDP from TCP traffic. Route changes via SIGHUP take effect on subsequent datagrams without restarting the UDP listener.

### Supported upstream relay

Phase 4 supports one-hop SOCKS5 upstream relay for UDP. HTTP, SOCKS4, and
multi-hop chains are explicitly rejected for UDP with metrics. No silent
fallback to direct unless the routing policy selects a direct fallback.

### UDP task tracking

UDP relay tasks are spawned via `TaskTracker` (in `RuntimeState` and `RuntimeUdpService`), not bare `tokio::spawn`. During shutdown:

1. `udp_registry.close_all()` closes all associations.
2. `udp_tasks.close()` prevents new task spawns.
3. `tokio::time::timeout(grace, udp_tasks.wait())` waits for in-flight relay tasks to drain.

### Metrics bridging

UDP relay records into `eggress_udp::metrics::UdpMetrics`. The `MetricsRegistry` bridges these counters via `set_udp_metrics()`, so `/metrics` exposes live UDP counters (associations active/total, packets up/down, bytes, drops, decode errors, target flows). The bridged snapshot is refreshed on each Prometheus render.

### TOML configuration

UDP behavior is configurable per listener via `[listeners.udp]`:

```toml
[[listeners]]
name = "socks-in"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
advertise = "127.0.0.1"
idle_timeout = "60s"
target_idle_timeout = "30s"
max_associations = 1024
max_targets_per_association = 64
max_datagram_size = 65535
client_pin = true
```

The legacy `udp_enabled = true` flag is still supported as compatibility sugar. If `udp_enabled = true` and no `[listeners.udp]` section exists, default UDP config is synthesized. If both are present and disagree, config validation rejects.

### Advertise address derivation

The SOCKS5 UDP ASSOCIATE reply contains a computed relay address:

1. If `advertise` is configured, use it.
2. Else if UDP bind IP is not unspecified (`0.0.0.0` / `::`), use the bind IP.
3. Else if TCP peer is loopback, use the matching loopback address.
4. Otherwise, config validation requires an explicit `advertise` value.

This avoids advertising `0.0.0.0` to clients by default.

### UDP association ownership

A UDP association is owned by the TCP control connection that issued the SOCKS5 UDP ASSOCIATE command. The association is closed when:

- The TCP control connection closes.
- The idle timeout expires.
- Runtime shutdown begins.
- Association or target-flow limits are exceeded.

Every close path removes the association from the `UdpAssociationRegistry`, ensuring `active_count()` returns to zero after the relay task completes.

### Datagram codec boundaries

The SOCKS5 UDP datagram codec is independent of socket I/O. It operates on byte slices and produces `Socks5UdpRequest` values containing a target address and payload. The codec enforces:

- RSV must be 0x0000.
- FRAG must be 0x00 (fragmentation unsupported).
- Maximum datagram size from configured limits.
- Valid ATYP values (IPv4, IPv6, domain).
- Nonzero domain length.

Encoding produces a SOCKS5 UDP response datagram with the same structure. The codec does not allocate per packet beyond the domain name representation.

### Security defaults

- Client address pinning is enabled by default. The first valid UDP packet from a client pins the association to that address; subsequent packets from different addresses are dropped.
- Multicast, broadcast, unspecified targets, and port zero are rejected by default.
- Datagram size is bounded by configuration.
- Association and target-flow counts are bounded.
- Bind to loopback by default for non-internet-facing deployments.

### Reload limitations

- UDP bind address changes require a restart.
- UDP advertise address changes require a restart if the socket bind changes.
- UDP limit changes (idle timeout, target idle timeout, max datagram size, client pin) apply only to new associations created after reload.
- Route changes apply immediately to future UDP packets.
- The legacy `udp_enabled = true` flag is retained for backward compatibility and synthesized to default UDP config when no `[listeners.udp]` section is present.

## Reverse/Backward Proxying

Eggress implements a reverse (backward) proxy model for NAT traversal. A server behind NAT connects outward to an acceptor in a datacenter, exposing internal services to external clients without inbound port forwarding on the server side.

### eggress-protocol-reverse

Crate structure:
- `lib.rs` — shared protocol primitives: auth handshake (`write_auth`, `read_handshake`, `client_auth_handshake`, `server_auth_handshake`), credential redaction, bidirectional relay, `ControlState` enum, and `ProtocolError` types
- `server.rs` — `ReverseServer` (acceptor side): binds a control listener and an external listener, authenticates incoming control connections, pairs them with external clients, and relays data bidirectionally
- `client.rs` — `ReverseClient` (server-behind-NAT side): connects to the acceptor, authenticates, resolves a target via the `TargetResolver` trait, connects to the local target, and relays data back through the control channel
- `metrics.rs` — `ReverseMetrics` with Prometheus exposition: control connections (active/accepted/rejected), auth failures, reconnects, streams opened/closed, state durations, and error tracking

### Runtime adapter

`eggress-runtime/src/reverse.rs` bridges the routing engine to the reverse client. `RouteEngineTargetResolver` implements `TargetResolver` by building a synthetic `RouteRequest` (transport = `ReverseTcp`, listener = reverse listener name) and gating the target through `SharedRoutingService::decide()`. Routing decisions of `Direct` or `UpstreamGroup` map to `TargetResolution::Connect`; `Reject` maps to `TargetResolution::Reject`.

### Data flow

```
External client → ReverseServer.external_listener
    → paired with available control connection (from pool)
    → relay_bidirectional(external_stream, control_stream)
    → ReverseClient resolves target via RouteEngineTargetResolver
    → ReverseClient connects to local target
    → relay_bidirectional(control_stream, target_stream)
```

Control connection lifecycle:
1. **Connect**: ReverseClient dials the acceptor's control listener
2. **Auth**: Client sends `user:pass\n`, server validates and responds with 0x01 (accept) or 0x00 (reject)
3. **Pool**: Authenticated control connection enters the server's unbounded channel
4. **Pair**: External client arrives, server receives a control connection from the channel
5. **Relay**: Bidirectional byte relay between external stream and control stream
6. **Reconnect**: On session end or failure, client reconnects with exponential backoff (1s initial, 30s cap, doubling)

Parallel connections: multiple `reverse_clients` entries or `parallel_connections` > 1 creates independent control channels, enabling concurrent external sessions.

### Security model

- **Plaintext by default** — no built-in TLS on control or external channels; wrap in an external TLS layer or deploy in a trusted network
- **Defense-in-depth validation** — `ReverseServerConfig::validate()` rejects non-loopback external binds without both authentication and an explicit `allow_bind` allowlist
- **Auth required for non-loopback** — non-loopback `external_bind` requires `auth_username`/`auth_password` and a non-empty `allow_bind`
- **Loopback exempt** — loopback binds skip auth/allowlist requirements for local development
- **allow_bind allowlist** — optional list of permitted external bind addresses; bind denied if address not in list
- **Bounded resources** — `max_control_connections`, `max_streams_per_listener`, `max_pending_external` prevent resource exhaustion
- **Credential redaction** — `redact_auth()` replaces passwords with `****` for logging; full credentials never appear in logs

### Configuration

```toml
[[reverse_servers]]
id = "acceptor"
control_bind = "0.0.0.0:8443"
external_bind = "0.0.0.0:9000"
auth_username = "user"
auth_password = "pass"

[[reverse_clients]]
id = "server-behind-nat"
server_addr = "acceptor-host:8443"
auth_username = "user"
auth_password = "pass"
default_target_host = "127.0.0.1"
default_target_port = 8080
reconnect_initial = "1s"
reconnect_max = "30s"
```

### Key limitations

- **TCP only** — no UDP reverse mode
- **No multiplexing** — each control connection carries exactly one proxy session (matching pproxy's backward model)
- **No built-in TLS** — control and external channels are plaintext; TLS must be added externally or via `+tls` transport
- **No heartbeat** — control state tracking exists but active keepalive probes are not yet implemented
- **Single session per connection** — no stream multiplexing over the control channel; parallel connections are achieved by spawning multiple independent control channels

## Design Principles

1. **Separate protocol from transport** — protocols run over arbitrary streams
2. **Preserve unresolved targets** — domain names stay as domains until resolution is required
3. **Box streams at boundaries** — avoid propagating generic stream types
4. **No unsafe in core crates** — `unsafe_code = "forbid"`
5. **Credentials never logged** — redacted Display implementations
6. **Bounded everything** — sniff buffers, headers, credentials, handshake timeouts
7. **Normalized failure categories** — structured outcomes for metrics and diagnostics
8. **Configured protocol sets** — listeners accept only configured protocols
9. **Immutable routing snapshots** — atomic swap via `ArcSwap` for lock-free reads
10. **Health-aware scheduling** — upstream eligibility based on health state
11. **Lease accounting** — `PendingLease`/`ActiveLease` track in-flight connections
12. **Operator explainability** — route explanation without debug logs
13. **Shared runtime snapshot** — one set of `Arc<UpstreamRuntime>` shared by router, health, admin, metrics
14. **Graceful shutdown ordering** — drain first, cancel second; admin stays up through drain
15. **Atomic reload** — compile candidate before swap, reject unsupported changes
16. **Single generation source** — `CompiledRuntimeSnapshot.generation` is the only authoritative externally visible generation
17. **Live admin reads** — admin handlers read PAC, static content, router, and listeners from the current snapshot per request via `AdminSnapshotProvider`
18. **Fallible supervisor** — startup errors return `RuntimeError` instead of panicking
19. **Manifest-driven evidence discipline** (Phase 36) — the parity contract is encoded in `tests/compat/pproxy_manifest.toml` and mechanically validated by `eggress-testkit::manifest::validate_manifest`. The validator enforces six tier statuses (`compatible`, `supported`, `partial`, `intentional_non_parity`, `experimental`, `unsupported`), category enumeration, evidence-level semantics, test-reference hygiene (no bare file paths or CI workflow references), and platform-constraint documentation. Adding or changing a feature requires editing the manifest; CI gates on `cargo test -p eggress-testkit --lib manifest`.
20. **Release artifact separation** (Phase 36) — the parity release contract is documented in `docs/release/`: frozen targets (`PARITY_TARGET_FREEZE.md`), final report (`FINAL_PPROXY_PARITY_REPORT.md`), platform matrix (`PLATFORM_SUPPORT_MATRIX.md`), migration guide (`MIGRATION_FROM_PPROXY_FINAL.md`), release notes (`RELEASE_NOTES_PARITY_RC.md`), and go/no-go (`PARITY_RELEASE_GO_NO_GO.md`). These are read-only contracts; behavior changes must be reflected here as well as in code.
21. **DNS rebinding protection** (Phase 50) — `DirectConnector` rejects DNS resolutions pointing to private/reserved IP ranges (loopback, link-local, RFC 1918, unique-local IPv6) to prevent DNS rebinding attacks. Applied to domain resolution only, not to explicit IP targets.
22. **Auth failure observability** (Phase 50) — all inbound authentication failures (SOCKS5 username/password, HTTP Proxy-Authorization, reverse proxy auth) increment `eggress_auth_failures_total` counter.
23. **Standalone UDP security** (Phase 50) — standalone UDP relay validates targets against private/reserved IP ranges via `validate_standalone_target()`, preventing DNS rebinding-style attacks over UDP.

### Track B/C release-candidate status

The Track B/C verification pass (2026-07-16) re-confirmed the release-candidate status with:

- 34 targeted Rust test suites passing (~1,663 tests, 0 failures)
- Full Python source-tree suite passing (1,400 tests, 20 skipped, 0 failures)
- 40 new native outbound stream lifecycle tests passing
- AEAD KAT tests passing for AES-256-GCM, AES-128-GCM (NIST SP 800-38D)
- 12 in-tree fuzz smoke tests across 5 crates passing
- Two cipher defects fixed with regression coverage
- Manifest/composition/report consistency validated

This is a **certified modern pproxy compatibility subset**, not strict full parity. See `docs/release/PARITY_RELEASE_GO_NO_GO.md` for the decision record.
