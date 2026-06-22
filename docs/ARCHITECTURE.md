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

### eggress-udp
UDP association management and direct forwarding:
- `UdpAssociation` — association state machine with ownership by TCP control connection
- `UdpAssociationRegistry` — bounded association tracking with global and per-listener limits
- `UdpTargetFlow` — connected UDP socket per target for reliable response demux
- `UdpLimits` — configurable association, datagram, and idle constraints
- `UdpMetrics` — Prometheus-compatible counters and gauges for UDP operations
- `validate_target` — security policy rejecting multicast, broadcast, unspecified, and port zero
- `testkit` — UDP echo server helper for integration tests

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

## UDP Data Flow

```
TCP SOCKS5 client → UDP ASSOCIATE command → server creates UdpAssociation
    → reply with UDP relay bind address
    → client sends SOCKS5 UDP datagrams to relay address
    → association decodes datagram, validates client ownership
    → route engine decides Direct/Reject/UnsupportedUpstream
    → direct UDP socket sends payload to target (one connected socket per target flow)
    → response from target mapped back to client
    → SOCKS5 UDP response datagram sent to pinned client address
    → idle timeout or TCP control close tears down association
```

### UDP association ownership

A UDP association is owned by the TCP control connection that issued the SOCKS5 UDP ASSOCIATE command. The association is closed when:

- The TCP control connection closes.
- The idle timeout expires.
- Runtime shutdown begins.
- Association or target-flow limits are exceeded.

If the control connection closes, the association is torn down immediately. If the UDP association idles out, the TCP control connection is eventually closed by the server.

### Datagram codec boundaries

The SOCKS5 UDP datagram codec is independent of socket I/O. It operates on byte slices and produces `Socks5UdpRequest` values containing a target address and payload. The codec enforces:

- RSV must be 0x0000.
- FRAG must be 0x00 (fragmentation unsupported).
- Maximum datagram size from configured limits.
- Valid ATYP values (IPv4, IPv6, domain).
- Nonzero domain length.

Encoding produces a SOCKS5 UDP response datagram with the same structure. The codec does not allocate per packet beyond the domain name representation.

### Target-flow model

Each unique target address within an association gets its own `UdpTargetFlow` backed by a connected UDP socket. This design:

- Simplifies response demultiplexing (replies arrive on the target-specific socket).
- Makes client address pinning straightforward.
- Bypasses the need for shared-socket peer mapping.

The target flow count per association is bounded by `max_targets_per_association`. Flows are cleaned up by idle expiry or association close.

### Routing per datagram

Every UDP datagram is routed through the Phase 2 rule engine using a `RouteRequest` with `TransportKind::Udp`. The route decision can be:

- `Direct` — forward to target via direct UDP socket.
- `Reject` — drop the packet, increment drop metric.
- `UnsupportedUpstream` — drop the packet because no UDP-capable upstream path exists.

Route rules can match on `transport` to distinguish UDP from TCP traffic. Route changes via SIGHUP take effect on subsequent datagrams without restarting the UDP listener.

### Direct-only limitation

Phase 3 supports only direct UDP forwarding. There is no relay through upstream SOCKS5, HTTP, or Shadowsocks proxies. If a rule selects an upstream group, the packet is dropped with an `unsupported_upstream` metric. This is the explicit, safe default for a connectionless protocol.

### Security defaults

- Client address pinning is enabled by default. The first valid UDP packet from a client pins the association to that address; subsequent packets from different addresses are dropped.
- Multicast, broadcast, unspecified targets, and port zero are rejected by default.
- Datagram size is bounded by configuration.
- Association and target-flow counts are bounded.
- Bind to loopback by default for non-internet-facing deployments.

### Reload limitations

- UDP bind address changes require a restart.
- UDP advertise address changes require a restart if the socket bind changes.
- UDP limit changes apply only to new associations.
- Route changes apply immediately to future UDP packets.

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
