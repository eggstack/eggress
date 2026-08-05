# Eggress Architecture Overview

Eggress is a Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical compatibility with Python `pproxy==2.7.9`. Built on Tokio, it uses stream-native composition: protocols and transports operate on boxed async byte streams (`BoxStream`), enabling arbitrary multi-hop chaining without generics propagating through the architecture.

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

---

## Module Overview

The workspace contains 24 crates organized into six categories: foundation, protocols, infrastructure, entry points, compatibility, and tooling. Each crate has a detailed architecture document linked in the [Deep Dive Index](#deep-dive-index).

### Foundation Layer

These crates define the shared types and abstractions used everywhere.

| Crate | Purpose | Deep Dive |
|---|---|---|
| **eggress-core** | Core types, traits, stream abstractions, protocol identifiers, error types. Defines `BoxStream`, `TargetAddr`, `ProtocolId`, `SessionContext`, `RouteAction`, relay, chain execution, protocol detection, and dispatch. Every other crate depends on it. | [core.md](core.md) |
| **eggress-uri** | URI parsing with typed AST. Parses `protocol+protocol://user:pass@host:port?rule#local` syntax with `__` hop separator. Produces `ProxyChainSpec` → `ProxyHopSpec` → `ProtocolSpec`/`EndpointSpec`/`CredentialSpec`. `RedactedUri` replaces credentials for safe logging. | [uri.md](uri.md) |
| **eggress-transport-tls** | Shared TLS transport layer using rustls. `TlsClientConfigBuilder` (system roots, custom CA, ALPN, insecure mode) and `TlsServerConfigBuilder` (cert/key PEM). `tls_connect()` / `tls_accept()` wrap `BoxStream` in TLS. Used by listener TLS, upstream chain hops, and Trojan. | [transport-tls.md](transport-tls.md) |

### Protocol Crates

Each protocol crate provides detection, server-side accept, client-side connect, and chain hop handler functionality. Protocol crates depend only on `eggress-core` (and sometimes `eggress-uri`); they do not depend on each other.

| Protocol | Crate | Inbound | Outbound | Chain Hop | Deep Dive |
|---|---|---|---|---|---|
| HTTP/1.1 CONNECT | eggress-protocol-http | Yes | Yes (client) | Yes | [protocols-http.md](protocols-http.md) |
| HTTP forward proxy | eggress-protocol-http | Yes | — | — | [protocols-http.md](protocols-http.md) |
| H2 CONNECT | eggress-protocol-http | — | Yes (pooled) | Yes | [protocols-http.md](protocols-http.md) |
| SOCKS4/4a | eggress-protocol-socks | Yes | Yes | Yes | [protocols-socks.md](protocols-socks.md) |
| SOCKS5 | eggress-protocol-socks | Yes | Yes | Yes | [protocols-socks.md](protocols-socks.md) |
| Shadowsocks (AEAD) | eggress-protocol-shadowsocks | Yes | Yes | Yes | [protocols-shadowsocks.md](protocols-shadowsocks.md) |
| Trojan | eggress-protocol-trojan | Yes | Yes | Yes | [protocols-trojan.md](protocols-trojan.md) |
| WebSocket tunnel | eggress-protocol-websocket | Yes | Yes (via tungstenite) | Yes | [protocols-websocket.md](protocols-websocket.md) |
| Raw/tunnel passthrough | eggress-protocol-raw | — | — | Yes | [protocols-raw.md](protocols-raw.md) |
| Reverse proxy | eggress-protocol-reverse | Yes (server) | Yes (client) | — | [protocols-reverse.md](protocols-reverse.md) |

### Transport Layer

| Transport | Crate | Role | Deep Dive |
|---|---|---|---|
| TLS (rustls) | eggress-transport-tls | Client/server TLS config, PEM loading, system roots | [transport-tls.md](transport-tls.md) |
| UDP | eggress-udp | SOCKS5 UDP ASSOCIATE, standalone/echo/fixed-target UDP, per-target flow model, upstream SOCKS5 relay | [udp.md](udp.md) |

### Infrastructure Layer

These crates provide the runtime machinery, configuration, routing, metrics, and admin capabilities.

| Crate | Purpose | Deep Dive |
|---|---|---|
| **eggress-routing** | Rule engine with first-match-wins evaluation. `MatchExpr` supports host (exact/suffix/regex), CIDR, port, source, listener, protocol, identity, and transport matchers. Upstream groups with schedulers (first-available, round-robin, random, least-connections). Active health state machine with hysteresis (Unknown → Healthy ↔ Suspect → Unhealthy → Recovering). `SharedRoutingService` with `ArcSwap` for atomic config reload. Route explanation for operator debugging. | [routing.md](routing.md) |
| **eggress-config** | TOML configuration schema, validation, and compilation. Versioned schema with recursive matcher expressions. Validates duplicate IDs, unknown references, URI syntax, regex, CIDR, duration strings. Secret sources (inline, env var, file). Compiles to `RuntimeConfig`. | [config.md](config.md) |
| **eggress-server** | Connection orchestration. `serve_connection()` is the top-level entry: detect → accept (with timeout) → route → reply → relay. `AcceptedSession` (Tunnel or HttpForward), `SessionReport` (structured outcome with protocol, target, bytes, failure category). Protocol enforcement and handshake timeout. | [server.md](server.md) |
| **eggress-runtime** | Service supervisor and lifecycle. `ServiceSupervisor::run()` manages startup, shutdown ordering (readiness false → stop listeners → drain → force-cancel → stop admin), and hot-reload via SIGHUP. `CompiledRuntimeSnapshot` is the single authoritative runtime state. `RuntimeState` shares snapshot via `ArcSwap`. | [runtime.md](runtime.md) |
| **eggress-admin** | Local admin HTTP server. Endpoints: `/-/ready`, `/-/health`, `/-/status`, `/-/routes`, `/-/upstreams`, `/-/config`, `/-/udp`, `/-/reverse`, `/metrics`, `/proxy.pac`. `AdminSnapshotProvider` trait lets runtime expose live data. Route explanation, PAC generation, static content serving. | [admin.md](admin.md) |
| **eggress-metrics** | Prometheus-compatible metrics registry. Session counters/durations/bytes, route decision labels, upstream health gauges, config generation tracking, reload counters. Bridges UDP, Shadowsocks, and H2 protocol metrics. Bounded label cardinality. | [metrics.md](metrics.md) |

### Entry Points and Embedding

| Crate | Purpose | Deep Dive |
|---|---|---|
| **eggress-cli** | CLI binary targets `eggress` (native) and `pproxy` (compatibility). Modes: config file, direct args, route-explain, upstream-test, system-proxy inspect. | [cli.md](cli.md) |
| **eggress-embed** | Stable Rust in-process API. `EggressConfig::from_toml_str()`, `EggressService::new().start().await` (async) or `.start_blocking()` (blocking). `EggressHandle` for bound addresses, status, metrics, hot-reload, shutdown. Also provides `OutboundConnector` for native outbound TCP connections. | [embed.md](embed.md) |
| **eggress-python** | PyO3 bindings wrapping the embed API. Classes: `Config`, `Service`, `Handle`, `Connection`, `OutboundConnector`, `OutboundStream`. pproxy-compatible `Server` class. Protocol client classes (`HTTP`, `Socks4`, `Socks5`). URI helpers, diagnostics, route explanation. GIL release on all blocking Rust calls. | [python.md](python.md) |

### Compatibility Layer

| Crate | Purpose | Deep Dive |
|---|---|---|
| **eggress-pproxy-compat** | pproxy 2.7.9 compatibility layer. Translates pproxy CLI args and URIs to eggress TOML config. `translate_pproxy_args()`, `translate_from_uris()`. Parity classification tiers (compatible, supported, partial, intentional_non_parity, experimental, unsupported). Structured diagnostics with stable codes. Regex compatibility for pproxy rule files. | [pproxy-compat.md](pproxy-compat.md) |

### Tooling and Test Infrastructure

| Component | Location | Purpose | Deep Dive |
|---|---|---|---|
| Testkit | `crates/eggress-testkit/` | Echo servers, port allocators, oracle/differential harnesses, manifest validation | [testkit.md](testkit.md) |
| System proxy | `crates/eggress-system-proxy/` | Platform proxy detection (macOS/Windows/Linux) and apply/rollback | [system-proxy.md](system-proxy.md) |
| Scripts | `scripts/` | Interoperability tests, certification probes, smoke clients | [tools-and-scripts.md](tools-and-scripts.md) |
| Fuzz targets | `fuzz/` | Standalone fuzz workspace for parser targets | — |
| Benchmarks | `benches/` | Criterion benchmarks: tcp_relay, udp_relay, route_match, http_connect_upstream | — |

---

## Workspace Layout

```
eggress/
├── crates/                    # 24 Rust crates (see below)
│   ├── eggress-core/          # Foundation: types, traits, relay, detection
│   ├── eggress-uri/           # URI parsing and typed AST
│   ├── eggress-routing/       # Rule engine, schedulers, health state
│   ├── eggress-config/        # TOML config parsing and validation
│   ├── eggress-server/        # Connection orchestration
│   ├── eggress-runtime/       # Service supervisor and lifecycle
│   ├── eggress-admin/         # Admin HTTP server
│   ├── eggress-metrics/       # Prometheus metrics
│   ├── eggress-transport-tls/ # TLS transport (rustls)
│   ├── eggress-udp/           # UDP associations and relay
│   ├── eggress-protocol-http/     # HTTP/1.1, H2 CONNECT
│   ├── eggress-protocol-socks/    # SOCKS4/4a, SOCKS5
│   ├── eggress-protocol-shadowsocks/ # Shadowsocks AEAD
│   ├── eggress-protocol-trojan/   # Trojan protocol
│   ├── eggress-protocol-websocket/ # WebSocket tunnels
│   ├── eggress-protocol-raw/      # Raw TCP passthrough
│   ├── eggress-protocol-reverse/  # Reverse/backward proxy
│   ├── eggress-embed/         # Rust embed API
│   ├── eggress-python/        # PyO3 Python bindings
│   ├── eggress-pproxy-compat/ # pproxy compatibility layer
│   ├── eggress-cli/           # CLI binary targets
│   ├── eggress-system-proxy/  # System proxy inspection
│   └── eggress-testkit/       # Test utilities and oracle
├── python/                    # Python package (eggress/ + pproxy/)
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

---

## Crate Dependency Graph

```
                          ┌─────────────┐
                          │  eggress-   │
                          │    uri      │
                          └──────┬──────┘
                                 │
                          ┌──────▼──────┐
                  ┌───────│  eggress-   │───────┐
                  │       │    core     │       │
                  │       └──────┬──────┘       │
                  │              │              │
         ┌────────▼────┐  ┌─────▼──────┐  ┌────▼─────────┐
         │  transport-  │  │  routing   │  │  protocol-*  │
         │    tls       │  │            │  │  (each)      │
         └──────┬──────┘  └─────┬──────┘  └────┬─────────┘
                │               │              │
                │        ┌──────▼──────┐       │
                │        │   config    │       │
                │        └──────┬──────┘       │
                │               │              │
         ┌──────▼───────────────▼──────────────▼──────┐
         │              eggress-server                 │
         └────────────────────┬───────────────────────┘
                              │
                ┌─────────────▼──────────────┐
                │       eggress-runtime      │
                │  (supervisor + lifecycle)   │
                └─────┬──────────┬──────┬────┘
                      │          │      │
               ┌──────▼───┐ ┌───▼───┐ ┌▼──────────┐
               │  embed   │ │ admin │ │  metrics   │
               └────┬─────┘ └───────┘ └─────┬──────┘
                    │                       │
            ┌───────┴────────┐    ┌─────────┼──────────────┐
            │                │    │         │              │
      ┌─────▼─────┐   ┌─────▼──────┐ ┌────▼─────┐ ┌──────▼──────┐
      │   cli     │   │  python    │ │   UDP    │ │  protocol-* │
      └───────────┘   └────────────┘ │ assocs+  │ │  (metrics)  │
                                     │  relay   │ └─────────────┘
                                     └──────────┘
```

**Protocol crates** depend only on `eggress-core` (and sometimes `eggress-uri`); they do not depend on each other.

**Leaf crates** (no eggress dependencies): `eggress-uri`, `eggress-system-proxy`, `eggress-testkit`.

---

## Platform Constraints

- Rust edition 2021, MSRV 1.75
- `unsafe_code = "deny"` at workspace level
- No OpenSSL, no C dependencies (`deny.toml` bans `openssl-sys`, `native-tls`, `aws-lc-sys`, `cmake`)
- TLS via rustls only

## Feature Groups and Lean Builds

The workspace defines bounded feature groups that control which protocol families and operational integrations are compiled:

| Group | Scope | Contents |
|-------|-------|----------|
| `common` | runtime, cli, embed | HTTP/SOCKS core, TLS transport, UDP, raw |
| `extended` | runtime, server, metrics, cli, embed | Shadowsocks, Trojan, WebSocket |
| `operations` | runtime, cli | System proxy |
| `reverse` | runtime, cli | Reverse/backward proxy control-channel |
| `pproxy-compat` | cli, embed | pproxy compatibility translator and binary |
| `full` | all | Union of all (default) |

Admin and metrics remain required dependencies for the snapshot invariant. The `extended` feature gates protocol accept paths, chain executor handlers, and metrics bridging at composition boundaries. A disabled feature fails with a structured diagnostic, never silently degrading.

Lean builds exclude optional protocol families:

```bash
# Lean local HTTP/SOCKS build
cargo build -p eggress-cli --release --no-default-features --features common

# Optional smallest optimization profile
cargo build -p eggress-cli --profile release-small --no-default-features --features common
```

Release profiles are defined at the workspace root:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"

[profile.release-small]
inherits = "release"
opt-level = "z"
lto = true
```

---

## Deep Dive Index

Each component links to its detailed architecture document for in-depth review:

| # | Component | File | What It Covers |
|---|---|---|---|
| 1 | Core types & traits | [core.md](core.md) | `BoxStream`, `TargetAddr`, `ProtocolId`, `SessionContext`, relay, chain execution, detection, dispatch |
| 2 | URI parsing | [uri.md](uri.md) | `ProxyChainSpec`, hop/protocol/credential parsing, redacted display, native grammar |
| 3 | TLS transport | [transport-tls.md](transport-tls.md) | rustls config builders, PEM/system root loading, accept/connect wrappers |
| 4 | Routing engine | [routing.md](routing.md) | Rule matching, upstream selection, health state, schedulers, leases, route explanation |
| 5 | Configuration | [config.md](config.md) | TOML schema, validation, compilation, secret sources |
| 6 | Server orchestration | [server.md](server.md) | `serve_connection()`, session lifecycle, accept/execute/reply, failure categories |
| 7 | Runtime supervisor | [runtime.md](runtime.md) | Startup, shutdown ordering, reload, snapshot compilation, reverse proxy integration |
| 8 | Admin HTTP server | [admin.md](admin.md) | Endpoints, PAC, metrics, route explanation, reverse registry |
| 9 | Metrics | [metrics.md](metrics.md) | Prometheus registry, session/UDP/Shadowsocks metric bridging |
| 10 | HTTP protocol | [protocols-http.md](protocols-http.md) | CONNECT, forward proxy, H2, body framing, auth |
| 11 | SOCKS protocols | [protocols-socks.md](protocols-socks.md) | SOCKS4/4a, SOCKS5, UDP ASSOCIATE, datagram codec |
| 12 | Shadowsocks protocol | [protocols-shadowsocks.md](protocols-shadowsocks.md) | AEAD ciphers, key derivation, address encoding, legacy detection |
| 13 | Trojan protocol | [protocols-trojan.md](protocols-trojan.md) | SHA224 auth, TLS transport, wire format, domain validation |
| 14 | WebSocket tunnel | [protocols-websocket.md](protocols-websocket.md) | ws/wss upgrade, stream adapter, chain composition |
| 15 | Raw/tunnel | [protocols-raw.md](protocols-raw.md) | TCP passthrough, no protocol overhead |
| 16 | Reverse proxy | [protocols-reverse.md](protocols-reverse.md) | NAT traversal, control/external channels, auth, runtime integration |
| 17 | UDP subsystem | [udp.md](udp.md) | Association registry, target flows, upstream relay, security policy |
| 18 | Embed API | [embed.md](embed.md) | Rust in-process API, async/blocking start, handle lifecycle, outbound connector |
| 19 | Python bindings | [python.md](python.md) | PyO3 wrappers, pproxy Server, OutboundConnector, GIL release |
| 20 | pproxy compatibility | [pproxy-compat.md](pproxy-compat.md) | CLI translation, URI parsing, tier classification, diagnostics |
| 21 | CLI binary | [cli.md](cli.md) | Binary modes, pproxy compat binary, upstream-test, system-proxy |
| 22 | Testkit | [testkit.md](testkit.md) | Test servers, oracle harness, manifest validation, differential testing |
| 23 | System proxy | [system-proxy.md](system-proxy.md) | Platform detection, inspect/apply/rollback |
| 24 | Tools & scripts | [tools-and-scripts.md](tools-and-scripts.md) | Interop tests, certification probes, smoke clients, fuzz targets |

---

## Design Principles

1. **Stream-native composition** — protocols and transports operate on `BoxStream`, enabling arbitrary chaining
2. **Separate protocol from transport** — protocols run over arbitrary streams; TLS is a wrapper, not a protocol
3. **Preserve unresolved targets** — domain names stay as domains until resolution is required
4. **Box streams at boundaries** — avoid propagating generic stream types through the architecture
5. **No unsafe in core crates** — `unsafe_code = "deny"` workspace-wide
6. **Credentials never logged** — redacted `Display` implementations
7. **Bounded everything** — sniff buffers, headers, credentials, handshake timeouts
8. **Normalized failure categories** — structured outcomes for metrics and diagnostics
9. **Immutable routing snapshots** — atomic swap via `ArcSwap` for lock-free reads
10. **Health-aware scheduling** — upstream eligibility based on health state machine
11. **Operator explainability** — route explanation without debug logs
12. **Graceful shutdown ordering** — readiness false → stop listeners → drain → force-cancel → stop admin
13. **Atomic reload** — compile candidate before swap, reject unsupported changes
14. **Fallible supervisor** — startup errors return `RuntimeError` instead of panicking
