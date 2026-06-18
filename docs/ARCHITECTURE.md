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
- `ChainExecutor` — multi-hop proxy chain execution

### eggress-server
Server orchestration library providing the reusable connection-handling API:
- `AcceptedSession` — typed inbound session (tunnel or HTTP forward)
- `PendingTunnel` / `PendingHttpForward` — parsed requests before route opening
- `RequestBodyKind` — explicit body framing type
- `serve_connection()` — main entry point: detect → accept → route → reply → relay
- `SessionReport` — structured connection outcome with protocol, target, route, byte counts
- `SessionOpenError` — normalized route failure types with protocol-specific reply mapping
- Deferred success replies — success is sent only after outbound route is established
- Common route opening — both tunnel and HTTP forward use the same `open_route()` function

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
Route resolution (first-available scheduling, direct fallback).

### eggress-protocol-http
HTTP/1 protocol implementation:
- CONNECT server and client with Basic auth
- Absolute-form forwarding with origin-form conversion
- Bounded header parsing via httparse

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
    → accept() — protocol detection and parsing (no replies sent)
    → open_route() — direct or chain to target
    → send success/failure reply
    → relay() or HTTP forward exchange
    → SessionReport
```

## Design Principles

1. **Separate protocol from transport** — protocols run over arbitrary streams
2. **Preserve unresolved targets** — domain names stay as domains until resolution is required
3. **Box streams at boundaries** — avoid propagating generic stream types
4. **No unsafe in core crates** — `unsafe_code = "forbid"`
5. **Credentials never logged** — redacted Display implementations
6. **Bounded everything** — sniff buffers, headers, credentials, handshake timeouts
