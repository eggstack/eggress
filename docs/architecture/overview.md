# Eggress Architecture Overview

Eggress is a Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical compatibility with Python `pproxy`. It is built on Tokio and designed around stream-native composition — protocols and transports operate on boxed async byte streams, enabling arbitrary chaining.

## System Context

```
┌─────────────────────────────────────────────────────────────────────┐
│                          eggress system                             │
│                                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌───────────────────┐  │
│  │   CLI    │  │  Embed   │  │ Python   │  │  System Proxy     │  │
│  │ (binary) │  │ (Rust)   │  │ (PyO3)   │  │  (inspector)      │  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └─────────┬─────────┘  │
│       │              │              │                  │            │
│       └──────────────┴──────────────┴──────────────────┘            │
│                              │                                      │
│                    ┌─────────▼──────────┐                           │
│                    │  eggress-runtime   │                           │
│                    │  (supervisor)      │                           │
│                    └─────────┬──────────┘                           │
│                              │                                      │
│          ┌───────────────────┼───────────────────┐                  │
│          │                   │                   │                  │
│  ┌───────▼──────┐  ┌────────▼───────┐  ┌───────▼──────┐          │
│  │ eggress-     │  │ eggress-       │  │ eggress-     │          │
│  │ server       │  │ routing        │  │ admin        │          │
│  │ (conn mgr)   │  │ (policy+sel)   │  │ (HTTP API)   │          │
│  └───────┬──────┘  └────────┬───────┘  └───────┬──────┘          │
│          │                  │                   │                  │
│          └──────────────────┼───────────────────┘                  │
│                              │                                      │
│       ┌──────────────────────┼──────────────────────┐              │
│       │                      │                      │              │
│  ┌────▼──────────────────────▼──────────────────────▼────┐        │
│  │                    eggress-core                       │        │
│  │  (types, traits, relay, detection, chain execution)   │        │
│  └──────────────────────────┬────────────────────────────┘        │
│                              │                                      │
│  ┌───────────┬───────────┬───┴───┬───────────┬───────────┐        │
│  │  HTTP     │  SOCKS    │  SS   │  Trojan   │ WebSocket │ ...   │
│  │  protocol │  4/4a/5   │  AEAD │  protocol │  tunnel   │        │
│  └───────────┴───────────┴───────┴───────────┴───────────┘        │
│                                                                     │
│  ┌──────────────┐  ┌──────────┐  ┌────────────┐                   │
│  │transport-tls │  │   UDP    │  │   URI      │                   │
│  │  (rustls)    │  │ assocs+  │  │  parser    │                   │
│  │              │  │  relay   │  │            │                   │
│  └──────────────┘  └──────────┘  └────────────┘                   │
└─────────────────────────────────────────────────────────────────────┘
```

## Entry Points

Three ways to run eggress, all converging on the same runtime:

| Entry Point | Crate | Description |
|---|---|---|
| CLI binary | [`eggress-cli`](cli.md) | `eggress` and `pproxy` binaries with clap-derived args |
| Rust embed API | [`eggress-embed`](embed.md) | In-process `EggressService::start()` with handle for status/reload/shutdown |
| Python bindings | [`eggress-python`](python.md) | PyO3 wrapping the embed API for Python consumers |

## Core Abstractions

| Concept | Location | Description |
|---|---|---|
| `BoxStream` | [`eggress-core`](core.md) | `Pin<Box<dyn AsyncRead + AsyncWrite + Send>>` — the universal stream type |
| `TargetAddr` | [`eggress-core`](core.md) | Typed destination preserving domain names until resolution |
| `ProtocolId` | [`eggress-core`](core.md) | Enum identifying detected inbound protocol (Http, Socks4, Socks5, Raw, Echo, etc.) |
| `RouteAction` | [`eggress-core`](core.md) | What to do with a connection: direct, upstream, or reject |
| `ProxyChainSpec` | [`eggress-uri`](uri.md) | Parsed multi-hop proxy chain from URI syntax |
| `MatchExpr` | [`eggress-routing`](routing.md) | Composite matcher: host, port, CIDR, protocol, identity, transport |
| `CompiledRule` | [`eggress-routing`](routing.md) | First-match-wins routing rule with upstream group binding |
| `SessionContext` | [`eggress-core`](core.md) | Per-connection metadata (target, client identity, listener) |

## Data Flow

```
Client connects
  → TcpListener (accepts, optional TLS unwrap)
  → serve_connection()  [eggress-server]
      → accept() — protocol detection with timeout and authentication
          → ReplayStream sniffs initial bytes
          → ProtocolDispatcher tries each detector in order
          → returns AcceptedSession (Tunnel or HttpForward)
      → RouteRequest built from session metadata
      → Router.decide() — evaluates rules, returns RouteDecision
      → Router.select() — scheduler picks upstream, returns SelectedRoute with ActiveLease
      → open_route() — DirectConnector or ChainExecutor
          → for chains: each HopHandler performs protocol handshake on prior stream
      → send success/failure reply to client
      → relay() or HTTP forward exchange (with byte counting)
      → SessionReport (protocol, target, route, bytes, outcome, failure category)
```

## Protocol Stack

Protocols are implemented as independent crates, each providing detection, server-side accept, client-side connect, and chain hop handler functionality:

| Protocol | Crate | Inbound | Outbound | Chain Hop |
|---|---|---|---|---|
| HTTP/1.1 CONNECT | [`eggress-protocol-http`](protocols-http.md) | Yes | Yes (client) | Yes |
| HTTP forward proxy | [`eggress-protocol-http`](protocols-http.md) | Yes | — | — |
| H2 CONNECT | [`eggress-protocol-http`](protocols-http.md) | — | Yes (pooled) | Yes |
| SOCKS4/4a | [`eggress-protocol-socks`](protocols-socks.md) | Yes | Yes | Yes |
| SOCKS5 | [`eggress-protocol-socks`](protocols-socks.md) | Yes | Yes | Yes |
| Shadowsocks (AEAD) | [`eggress-protocol-shadowsocks`](protocols-shadowsocks.md) | Yes | Yes | Yes |
| Trojan | [`eggress-protocol-trojan`](protocols-trojan.md) | Yes | Yes | Yes |
| WebSocket tunnel | [`eggress-protocol-websocket`](protocols-websocket.md) | Yes | Yes (via tungstenite) | Yes |
| Raw/tunnel passthrough | [`eggress-protocol-raw`](protocols-raw.md) | — | — | Yes |
| Reverse proxy | [`eggress-protocol-reverse`](protocols-reverse.md) | Yes (server) | Yes (client) | — |

## Transport Layer

| Transport | Crate | Role |
|---|---|---|
| TLS (rustls) | [`eggress-transport-tls`](transport-tls.md) | Client/server TLS config, PEM loading, system roots |
| UDP | [`eggress-udp`](udp.md) | SOCKS5 UDP ASSOCIATE, standalone UDP, per-target flow model |

## Infrastructure Crates

| Crate | Purpose |
|---|---|
| [`eggress-config`](config.md) | TOML parsing, semantic validation, compilation to `RuntimeConfig` |
| [`eggress-routing`](routing.md) | Rule engine, upstream selection, health state, schedulers |
| [`eggress-admin`](admin.md) | Local admin HTTP server (health, metrics, PAC, route explanation) |
| [`eggress-metrics`](metrics.md) | Prometheus-compatible counters and gauges |
| [`eggress-runtime`](runtime.md) | Service supervisor, snapshot compilation, reload, shutdown |
| [`eggress-pproxy-compat`](pproxy-compat.md) | pproxy CLI/URI translation and compatibility diagnostics |

## Tooling and Test Infrastructure

| Component | Location | Purpose |
|---|---|---|
| [`eggress-testkit`](testkit.md) | `crates/eggress-testkit/` | Echo servers, port allocators, oracle/differential harnesses |
| System proxy inspector | [`eggress-system-proxy`](system-proxy.md) | Platform proxy detection and apply/rollback |
| Scripts | [`scripts`](tools-and-scripts.md) | Interoperability tests, certification probes, smoke clients |
| Fuzz targets | `fuzz/` | Standalone fuzz workspace for parser targets |
| Benchmarks | `benches/` | Criterion benchmarks for TCP relay, UDP relay, route match, HTTP connect |

## Workspace Structure

```
eggress/
├── crates/                    # 24 Rust crates (see below)
├── python/                    # Python package (eggress/)
├── fuzz/                      # Standalone fuzz workspace
├── benches/                   # Criterion benchmarks
├── scripts/                   # Interoperability and testing scripts
├── tests/                     # Integration tests
├── docs/                      # Documentation
│   └── architecture/          # This directory
├── plans/                     # Historical phase plans
├── compat/                    # Compatibility test assets
└── example-config.toml        # Example TOML configuration
```

### Crate Dependency Graph (simplified)

```
eggress-core (foundation)
  ↑
  ├── eggress-uri
  ├── eggress-routing ──→ eggress-core, eggress-uri
  ├── eggress-config ──→ eggress-uri, eggress-routing, eggress-udp
  ├── eggress-server ──→ eggress-core, eggress-routing, eggress-udp
  ├── eggress-runtime ──→ eggress-config, eggress-server, eggress-routing,
  │                       eggress-admin, eggress-metrics, eggress-udp,
  │                       eggress-protocol-reverse, eggress-system-proxy
  ├── eggress-embed ──→ eggress-config, eggress-runtime
  ├── eggress-python ──→ eggress-embed, eggress-pproxy-compat
  ├── eggress-cli ──→ eggress-runtime, eggress-pproxy-compat, eggress-system-proxy
  └── eggress-admin ──→ eggress-routing, eggress-metrics, eggress-udp
```

### Protocol crates → core dependencies

All protocol crates depend on `eggress-core` for `BoxStream`, `ProtocolId`, and error types. Protocol crates do not depend on each other.

### Platform Constraints

- Rust edition 2021, MSRV 1.75
- `unsafe_code = "deny"` at workspace level
- No OpenSSL, no C dependencies (deny.toml bans openssl-sys, native-tls, aws-lc-sys, cmake)
- TLS via rustls only

## Deep Dive Index

Each component below links to its detailed architecture document:

| Component | File | What It Covers |
|---|---|---|
| Core types & traits | [core.md](core.md) | `BoxStream`, `TargetAddr`, `ProtocolId`, `SessionContext`, relay, chain execution |
| URI parsing | [uri.md](uri.md) | `ProxyChainSpec`, hop/protocol/credential parsing, redacted display |
| Routing engine | [routing.md](routing.md) | Rule matching, upstream selection, health state, schedulers, leases |
| Configuration | [config.md](config.md) | TOML schema, validation, compilation, secret sources |
| Server orchestration | [server.md](server.md) | `serve_connection()`, session lifecycle, accept/execute/reply |
| Runtime supervisor | [runtime.md](runtime.md) | Startup, shutdown ordering, reload, snapshot compilation |
| Admin HTTP server | [admin.md](admin.md) | Endpoints, PAC, metrics, route explanation, reverse registry |
| Metrics | [metrics.md](metrics.md) | Prometheus registry, session/UDP/Shadowsocks metric bridging |
| TLS transport | [transport-tls.md](transport-tls.md) | rustls config builders, PEM/system root loading, accept/connect |
| HTTP protocol | [protocols-http.md](protocols-http.md) | CONNECT, forward proxy, H2, body framing |
| SOCKS protocols | [protocols-socks.md](protocols-socks.md) | SOCKS4/4a, SOCKS5, UDP ASSOCIATE codec |
| Shadowsocks protocol | [protocols-shadowsocks.md](protocols-shadowsocks.md) | AEAD ciphers, key derivation, address encoding |
| Trojan protocol | [protocols-trojan.md](protocols-trojan.md) | SHA224 auth, TLS transport, wire format |
| WebSocket tunnel | [protocols-websocket.md](protocols-websocket.md) | ws/wss upgrade, stream adapter |
| Raw/tunnel | [protocols-raw.md](protocols-raw.md) | TCP passthrough, no protocol overhead |
| Reverse proxy | [protocols-reverse.md](protocols-reverse.md) | NAT traversal, control/external channels, auth |
| UDP subsystem | [udp.md](udp.md) | Association registry, target flows, upstream relay, security |
| Embed API | [embed.md](embed.md) | Rust in-process API, async/blocking start, handle lifecycle |
| Python bindings | [python.md](python.md) | PyO3 wrappers, pproxy Server, OutboundConnector |
| pproxy compatibility | [pproxy-compat.md](pproxy-compat.md) | CLI translation, URI parsing, tier classification |
| CLI binary | [cli.md](cli.md) | Binary modes, pproxy compat binary, upstream-test |
| Testkit | [testkit.md](testkit.md) | Test servers, oracle harness, manifest validation |
| System proxy | [system-proxy.md](system-proxy.md) | Platform detection, inspect/apply/rollback |
| Tools & scripts | [tools-and-scripts.md](tools-and-scripts.md) | Interop tests, certification probes, smoke clients |

## Design Principles

1. **Stream-native composition** — protocols and transports operate on `BoxStream`, enabling arbitrary chaining
2. **Preserve unresolved targets** — domain names stay as domains until resolution is required
3. **Box streams at boundaries** — avoid propagating generic stream types through the architecture
4. **No unsafe in core crates** — `unsafe_code = "deny"` workspace-wide
5. **Credentials never logged** — redacted `Display` implementations
6. **Bounded everything** — sniff buffers, headers, credentials, handshake timeouts
7. **Normalized failure categories** — structured outcomes for metrics and diagnostics
8. **Immutable routing snapshots** — atomic swap via `ArcSwap` for lock-free reads
9. **Health-aware scheduling** — upstream eligibility based on health state machine
10. **Operator explainability** — route explanation without debug logs
11. **Graceful shutdown ordering** — readiness false → stop listeners → drain → force-cancel → stop admin
12. **Atomic reload** — compile candidate before swap, reject unsupported changes
13. **Fallible supervisor** — startup errors return `RuntimeError` instead of panicking
