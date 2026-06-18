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
- `FailureCategory` — detailed failure diagnostics: Protocol, Authentication, HandshakeTimeout, Dns, ConnectionRefused, NetworkUnreachable, HostUnreachable, RouteTimeout, UpstreamAuthentication, Relay, Internal
- `SessionOpenError` — normalized route failure types with protocol-specific reply mapping
- Deferred success replies — success is sent only after outbound route is established
- Common route opening — both tunnel and HTTP forward use the same `open_route()` function
- Protocol enforcement — listener configuration restricts which protocols are accepted
- Handshake timeout — configurable timeout for inbound protocol establishment

### eggress-cli
CLI binary with `clap`-derived arguments:
- `-l` / `--listen` — listener URIs (multiple allowed)
- `-r` / `--remote` — upstream proxy URIs (chains with `__`)
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
- Upstream groups with scheduler selection (first-available, round-robin, random, least-connections)
- Active connection accounting with `PendingLease`/`ActiveLease`
- Health state machine with hysteresis (Unknown, Healthy, Suspect, Unhealthy, Recovering, Disabled)
- Active TCP health probes with configurable intervals and jitter
- `RouteService` trait for pluggable routing backends
- `SharedRoutingService` with `ArcSwap` for atomic config reload
- Route explanation tooling for operator debugging
- Compatibility regex parser for pproxy-style rule files

### eggress-config
TOML configuration with validation:
- Versioned schema with typed runtime model
- Validation: duplicate IDs, unknown references, invalid URIs, duration parsing
- Secret sources (inline, environment variable, file)
- CLI compatibility compilation

### eggress-metrics
Prometheus-compatible metrics:
- Connection counters, byte counters, route decision labels
- Upstream health gauges, config generation tracking
- Reload success/failure counters
- Bounded label cardinality

### eggress-admin
Local admin HTTP server:
- Health/readiness endpoints
- Status, routes, upstreams, config JSON endpoints
- Prometheus metrics endpoint
- PAC generation and serving
- Static content serving

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
