# Eggress Architecture Overview

A Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical compatibility with Python `pproxy==2.7.9`. Built on Tokio with stream-native composition: protocols and transports operate on boxed async byte streams, enabling arbitrary multi-hop chaining without generics propagating through the architecture.

## System at a Glance

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
│  │  server      │  │  routing       │  │  admin       │          │
│  │  (conn mgr)  │  │  (policy+sel)  │  │  (HTTP API)  │          │
│  └───────┬──────┘  └────────┬───────┘  └───────┬──────┘          │
│          │                  │                   │                  │
│          └──────────────────┼───────────────────┘                  │
│                              │                                      │
│                    ┌─────────▼──────────┐                           │
│                    │   eggress-core     │                           │
│                    │ (types, traits,    │                           │
│                    │  relay, detection) │                           │
│                    └─────────┬──────────┘                           │
│                              │                                      │
│  ┌───────────┬───────────┬───┴───┬───────────┬───────────┐        │
│  │  HTTP     │  SOCKS    │  SS   │  Trojan   │ WebSocket │ ...   │
│  │  CONNECT  │  4/4a/5   │  AEAD │  protocol │  tunnel   │        │
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

All three converge on the same runtime:

| Entry Point | Crate | Description | Deep Dive |
|---|---|---|---|
| CLI binary | `eggress-cli` | `eggress` and `pproxy` binaries | [cli.md](../docs/architecture/cli.md) |
| Rust embed API | `eggress-embed` | In-process `EggressService::start()` | [embed.md](../docs/architecture/embed.md) |
| Python bindings | `eggress-python` | PyO3 wrapping the embed API | [python.md](../docs/architecture/python.md) |

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
                ┌─────────▼──────────────┐
                │       eggress-runtime   │
                │  (supervisor+lifecycle) │
                └─────┬──────────┬──────┬┘
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

Protocol crates depend only on `eggress-core` (and sometimes `eggress-uri`); they do not depend on each other. Leaf crates (no eggress dependencies): `eggress-uri`, `eggress-system-proxy`, `eggress-testkit`.

---

## Component Deep Dive Index

### Foundation Layer

| # | Component | Crate | What It Does | Deep Dive |
|---|---|---|---|---|
| 1 | Core types & traits | `eggress-core` | `BoxStream`, `TargetAddr`, `ProtocolId`, `SessionContext`, relay, chain execution, detection, dispatch | [core.md](../docs/architecture/core.md) |
| 2 | URI parsing | `eggress-uri` | `ProxyChainSpec`, hop/protocol/credential parsing, redacted display, native grammar | [uri.md](../docs/architecture/uri.md) |
| 3 | TLS transport | `eggress-transport-tls` | rustls config builders, PEM/system root loading, accept/connect wrappers | [transport-tls.md](../docs/architecture/transport-tls.md) |
| 4 | QUIC transport | `eggress-transport-quic` | Optional Quinn QUIC streams and HTTP/3 CONNECT adapters | [transport-quic.md](../docs/architecture/transport-quic.md) |
| 5 | SSH transport | `eggress-transport-ssh` | Optional russh client transport for pproxy-compatible SSH upstream chains | [transport-ssh.md](../docs/architecture/transport-ssh.md) |

### Protocol Crates

| # | Protocol | Crate | Inbound | Outbound | Chain Hop | Deep Dive |
|---|---|---|---|---|---|---|
| 6 | HTTP/1.1 CONNECT | `eggress-protocol-http` | Yes | Yes | Yes | [protocols-http.md](../docs/architecture/protocols-http.md) |
| 7 | HTTP forward proxy | `eggress-protocol-http` | Yes | — | — | [protocols-http.md](../docs/architecture/protocols-http.md) |
| 8 | H2 CONNECT | `eggress-protocol-http` | — | Yes (pooled) | Yes | [protocols-http.md](../docs/architecture/protocols-http.md) |
| 9 | SOCKS4/4a | `eggress-protocol-socks` | Yes | Yes | Yes | [protocols-socks.md](../docs/architecture/protocols-socks.md) |
| 10 | SOCKS5 | `eggress-protocol-socks` | Yes | Yes | Yes | [protocols-socks.md](../docs/architecture/protocols-socks.md) |
| 11 | Shadowsocks (AEAD) | `eggress-protocol-shadowsocks` | Yes | Yes | Yes | [protocols-shadowsocks.md](../docs/architecture/protocols-shadowsocks.md) |
| 12 | Trojan | `eggress-protocol-trojan` | Yes | Yes | Yes | [protocols-trojan.md](../docs/architecture/protocols-trojan.md) |
| 13 | WebSocket tunnel | `eggress-protocol-websocket` | Yes | Yes | Yes | [protocols-websocket.md](../docs/architecture/protocols-websocket.md) |
| 14 | Raw/tunnel passthrough | `eggress-protocol-raw` | — | — | Yes | [protocols-raw.md](../docs/architecture/protocols-raw.md) |
| 15 | Reverse proxy | `eggress-protocol-reverse` | Yes (server) | Yes (client) | — | [protocols-reverse.md](../docs/architecture/protocols-reverse.md) |

### Infrastructure Layer

| # | Component | Crate | What It Does | Deep Dive |
|---|---|---|---|---|
| 16 | Routing engine | `eggress-routing` | Rule matching, upstream selection, health state machine, schedulers, leases, route explanation | [routing.md](../docs/architecture/routing.md) |
| 17 | Configuration | `eggress-config` | TOML schema, validation, compilation, secret sources | [config.md](../docs/architecture/config.md) |
| 18 | Server orchestration | `eggress-server` | `serve_connection()`, session lifecycle, accept/execute/reply, failure categories | [server.md](../docs/architecture/server.md) |
| 19 | Runtime supervisor | `eggress-runtime` | Startup, shutdown ordering, reload, snapshot compilation, reverse proxy integration | [runtime.md](../docs/architecture/runtime.md) |
| 20 | Admin HTTP server | `eggress-admin` | Endpoints, PAC, metrics, route explanation, reverse registry | [admin.md](../docs/architecture/admin.md) |
| 21 | Metrics | `eggress-metrics` | Prometheus registry, session/UDP/Shadowsocks metric bridging | [metrics.md](../docs/architecture/metrics.md) |
| 22 | UDP subsystem | `eggress-udp` | Association registry, target flows, upstream relay, security policy | [udp.md](../docs/architecture/udp.md) |
| 23 | System proxy | `eggress-system-proxy` | Platform detection, inspect/apply/rollback | [system-proxy.md](../docs/architecture/system-proxy.md) |

### Entry Points & Embedding

| # | Component | Crate | What It Does | Deep Dive |
|---|---|---|---|---|
| 24 | Embed API | `eggress-embed` | Rust in-process API, async/blocking start, handle lifecycle, outbound connector | [embed.md](../docs/architecture/embed.md) |
| 25 | Python bindings | `eggress-python` | PyO3 wrappers, pproxy Server, OutboundConnector, GIL release | [python.md](../docs/architecture/python.md) |
| 26 | CLI binary | `eggress-cli` | Binary modes, pproxy compat binary, upstream-test, system-proxy | [cli.md](../docs/architecture/cli.md) |

### Compatibility

| # | Component | Crate | What It Does | Deep Dive |
|---|---|---|---|---|
| 27 | pproxy compat | `eggress-pproxy-compat` | CLI translation, URI parsing, tier classification, diagnostics | [pproxy-compat.md](../docs/architecture/pproxy-compat.md) |

### Tooling & Test Infrastructure

| # | Component | Location | What It Does | Deep Dive |
|---|---|---|---|---|
| 28 | Testkit | `eggress-testkit` | Test servers, oracle harness, manifest validation, differential testing | [testkit.md](../docs/architecture/testkit.md) |
| 29 | Tools & scripts | `scripts/` | Interop tests, certification probes, smoke clients, fuzz targets | [tools-and-scripts.md](../docs/architecture/tools-and-scripts.md) |

---

## Core Abstractions

| Concept | Location | Description |
|---|---|---|
| `BoxStream` | `eggress-core` | `Pin<Box<dyn AsyncRead + AsyncWrite + Send>>` — the universal stream type |
| `TargetAddr` | `eggress-core` | Typed destination preserving domain names until resolution |
| `ProtocolId` | `eggress-core` | Enum identifying detected inbound protocol (Http, Socks4, Socks5, Raw, Echo, etc.) |
| `RouteAction` | `eggress-core` | What to do with a connection: direct, upstream, or reject |
| `ProxyChainSpec` | `eggress-uri` | Parsed multi-hop proxy chain from URI syntax |
| `MatchExpr` | `eggress-routing` | Composite matcher: host, port, CIDR, protocol, identity, transport |
| `CompiledRule` | `eggress-routing` | First-match-wins routing rule with upstream group binding |
| `SessionContext` | `eggress-core` | Per-connection metadata (target, client identity, listener) |

---

## Feature Groups

| Group | Scope | Contents |
|-------|-------|----------|
| `common` | runtime, cli, embed | HTTP/SOCKS core, TLS transport, UDP, raw; admin and metrics remain required |
| `extended` | runtime, server, metrics, cli, embed | Shadowsocks, Trojan, WebSocket |
| `operations` | runtime, cli | System proxy |
| `reverse` | runtime, cli | Reverse/backward proxy control-channel |
| `pproxy-compat` | cli, embed | pproxy compatibility translator and binary |
| `ssh` | cli, embed, runtime, server, Python | Optional SSH upstream transport |
| `legacy-crypto` | cli, embed, runtime, server, Python | Optional legacy Shadowsocks ciphers |
| `pproxy-daemon` | cli, Python | Optional Linux safe re-exec daemon |
| `full` | all | Union of all (default) |

Lean build: `cargo build -p eggress-cli --release --no-default-features --features common`

---

## Platform Constraints

- Rust edition 2021, MSRV 1.85
- `unsafe_code = "deny"` workspace-wide
- No OpenSSL, no C dependencies (denied via `deny.toml`)
- TLS via rustls only

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

---

## Workspace Layout

```
eggress/
├── crates/                    # 26 Rust crates
│   ├── eggress-core/          # Foundation: types, traits, relay, detection
│   ├── eggress-uri/           # URI parsing and typed AST
│   ├── eggress-routing/       # Rule engine, schedulers, health state
│   ├── eggress-config/        # TOML config parsing and validation
│   ├── eggress-server/        # Connection orchestration
│   ├── eggress-runtime/       # Service supervisor and lifecycle
│   ├── eggress-admin/         # Admin HTTP server
│   ├── eggress-metrics/       # Prometheus metrics
│   ├── eggress-transport-tls/ # TLS transport (rustls)
│   ├── eggress-transport-ssh/ # Optional SSH transport
│   ├── eggress-transport-quic/# Optional QUIC transport
│   ├── eggress-udp/           # UDP associations and relay
│   ├── eggress-protocol-http/     # HTTP/1.1, H2 CONNECT
│   ├── eggress-protocol-socks/    # SOCKS4/4a, SOCKS5
│   ├── eggress-protocol-shadowsocks/ # Shadowsocks AEAD
│   ├── eggress-protocol-trojan/   # Trojan protocol
│   ├── eggress-protocol-websocket/ # WebSocket tunnels
│   ├── eggress-protocol-raw/      # Raw TCP passthrough
│   ├── eggress-protocol-reverse/  # Reverse/backward proxy
│   ├── eggress-protocol-h3/       # HTTP/3 (QUIC-based)
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
│   └── architecture/          # Per-component deep dives (26 files)
├── .skills/                   # Agent skills (task-specific guidance)
├── plans/                     # Historical phase plans
├── compat/                    # Compatibility test assets
└── example-config.toml        # Example TOML configuration
```

---

## Further Reading

- [Existing architecture docs](../docs/architecture/) — 26 detailed per-component documents
- [Agent skills](../.skills/) — task-specific development guidance (proxy dev, testing, security, config, routing, transports, reverse proxy, release)
- [pproxy parity spec](../docs/PPROXY_PARITY_SPEC.md) — compatibility vocabulary and tier definitions
- [Embed API reference](../docs/EMBED_API.md) — Rust in-process API
- [Python bindings reference](../docs/PYTHON_BINDINGS.md) — PyO3 API surface
- [Config reference](../docs/CONFIG_REFERENCE.md) — TOML schema
- [URI grammar](../docs/URI_GRAMMAR.md) — proxy chain URI syntax
- [Testing guide](../docs/TESTING.md) — local, specialized, interoperability testing
- [Security review](../docs/SECURITY_REVIEW.md) — threat model and mitigations
