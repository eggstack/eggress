# Eggress Architecture Overview

Eggress is a Rust-native, embeddable, multi-protocol proxy framework and CLI
targeting practical and behavioral compatibility with Python `pproxy==2.7.9`.
It is built on Tokio around one central design decision: **everything is a
boxed byte stream**. Protocols, TLS, SSH, QUIC, and chain hops all consume an
`AsyncRead + AsyncWrite` stream and return an upgraded one, so any listener
protocol can be paired with any upstream chain without generics leaking
through the stack.

This document is the bird's-eye map and the index into per-component deep
dives. Each component below links to its own file **in this directory** for a
focused review session.

## The system at a glance

```
 entry points                    composition / policy             data plane
┌─────────────────────┐      ┌──────────────────────────┐    ┌─────────────────────────┐
│ eggress CLI         │      │ runtime supervisor       │    │ protocol crates          │
│ compat pproxy CLI   │─────▶│  · snapshot compilation  │───▶│ http/socks/shadowsocks/  │
│ embed API (Rust)    │      │  · reload, signals       │    │ trojan/websocket/raw/    │
│ Python (PyO3)       │      │  · shutdown ordering     │    │ reverse/h3               │
└─────────────────────┘      ├──────────────────────────┤    └───────────┬─────────────┘
                             │ server: accept→route→    │                │ BoxStream
        config ─────────────▶│ relay (per connection)   │    ┌───────────▼─────────────┐
   (eggress-config TOML)     ├──────────────────────────┤    │ transports              │
                             │ routing: rules → groups  │    │ tls (always available)  │
        observability ──────▶│ → schedulers → health    │    │ ssh / quic / h3 (opt-in)│
   (admin HTTP + metrics)    ├──────────────────────────┤    └─────────────────────────┘
                             │ admin · metrics · udp ·  │
                             │ system-proxy · reverse   │
                             └──────────────────────────┘
```

## How a TCP connection flows

```
Client → TcpListener (optional TLS unwrap, Unix/transparent variants)
  → serve_connection()                        [server]
      → accept(): sniff via ReplayStream + ProtocolDispatcher, auth check,
        bounded by handshake timeout → AcceptedSession
      → RouteRequest { target, source, listener, protocol, identity, transport }
      → RouteService::route()                 [routing]
        rules first-match-wins → group → scheduler picks member (health-aware)
        → SelectedRoute::Direct | Upstream{chain} (+ PendingLease)
      → open_route()
          Direct: DirectConnector (DNS rebinding-guarded)
          Upstream: ChainExecutor — each HopHandler consumes prior stream;
                    PendingLease → ActiveLease on success
      → deferred success reply to client
      → relay() both directions with half-close + byte counts
      → SessionReport { outcome, failure category, bytes, rule/group/upstream }
  → SessionMetrics recorded exactly once
```

UDP follows the same routing engine per datagram (see [udp.md](udp.md));
reverse/backward traffic uses routing as an authorization gate
(see [protocols-reverse.md](protocols-reverse.md)).

## Lifecycle: startup, reload, shutdown

The runtime compiles validated TOML into one `CompiledRuntimeSnapshot`
(router + shared upstream `Arc`s + health plan + listeners + PAC). Routing,
health, admin, and metrics all read the SAME snapshot; reload swaps it
atomically via arc-swap after the candidate compiles cleanly. Shutdown is an
enforced order — readiness false → listeners stop → UDP drain → connection
drain/cancel → admin last. Details: [runtime.md](runtime.md).

---

## Component index

### Foundation

| Component | Crate(s) | Role | Deep dive |
|---|---|---|---|
| Core types & streams | `eggress-core` | BoxStream, targets, relay, detection/dispatch, ChainExecutor, rebinding guard | [core.md](core.md) |
| URI grammar | `eggress-uri` | ProxyChainSpec AST, `+`/`__` syntax, redaction | [uri.md](uri.md) |
| Configuration | `eggress-config` | TOML schema, validation, secrets, compilation | [config.md](config.md) |

### Policy & observability

| Component | Crate(s) | Role | Deep dive |
|---|---|---|---|
| Routing engine | `eggress-routing` | Matchers, schedulers, health hysteresis, leases, explanation | [routing.md](routing.md) |
| Metrics | `eggress-metrics` | Prometheus registry, subsystem bridges, delta promotion | [metrics.md](metrics.md) |

### Data plane & lifecycle

| Component | Crate(s) | Role | Deep dive |
|---|---|---|---|
| Connection orchestration | `eggress-server` | serve_connection pipeline, session reports, reply semantics, Unix/transparent listeners | [server.md](server.md) |
| Runtime supervisor | `eggress-runtime` | Snapshots, reload, signals, shutdown ordering, reverse integration | [runtime.md](runtime.md) |
| Admin HTTP | `eggress-admin` | /-/endpoints, /metrics, PAC, route-explain | [admin.md](admin.md) |
| UDP subsystem | `eggress-udp` | Associations, flows, SOCKS5/SS upstream relay, standalone modes | [udp.md](udp.md) |
| System proxy | `eggress-system-proxy` | OS proxy inspect/apply/rollback per platform | [system-proxy.md](system-proxy.md) |

### Protocol crates (each depends only on core + uri)

| Component | Crate | Inbound | Outbound | Chain hop | UDP | Deep dive |
|---|---|---|---|---|---|---|
| HTTP/1.1 CONNECT + forward + H2 pool | `eggress-protocol-http` | yes | yes | yes | — | [protocols-http.md](protocols-http.md) |
| SOCKS4/4a + SOCKS5 | `eggress-protocol-socks` | yes | yes | yes | codec | [protocols-socks.md](protocols-socks.md) |
| Shadowsocks AEAD (+legacy/SSR gates) | `eggress-protocol-shadowsocks` | yes | yes | yes | yes | [protocols-shadowsocks.md](protocols-shadowsocks.md) |
| Trojan | `eggress-protocol-trojan` | yes | yes (TLS) | yes | — | [protocols-trojan.md](protocols-trojan.md) |
| WebSocket tunnel | `eggress-protocol-websocket` | yes | yes | yes | — | [protocols-tunnels.md](protocols-tunnels.md) |
| Raw passthrough | `eggress-protocol-raw` | fixed-target listener | — | yes | — | [protocols-tunnels.md](protocols-tunnels.md) |
| Reverse / backward | `eggress-protocol-reverse` | acceptor | NAT'd client | — | — | [protocols-reverse.md](protocols-reverse.md) |

### Transports

| Component | Crate(s) | Feature | Deep dive |
|---|---|---|---|
| TLS (rustls only) | `eggress-transport-tls` | always built | [transports-tls.md](transports-tls.md) |
| SSH channels | `eggress-transport-ssh` | `ssh` | [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) |
| QUIC streams | `eggress-transport-quic` | `quic` | [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) |
| HTTP/3 CONNECT | `eggress-protocol-h3` | `quic` | [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) |

### Entry points & compatibility

| Component | Crate(s)/tree | Role | Deep dive |
|---|---|---|---|
| CLI binaries | `eggress-cli` (`eggress`, compat `pproxy`) | flags/subcommands, exit codes, lean builds | [cli.md](cli.md) |
| Embed API | `eggress-embed` | in-process service lifecycle, OutboundConnector | [embed.md](embed.md) |
| Python bindings + package | `eggress-python`, `python/` | PyO3 `_eggress`, pure-Python wrappers, asyncio bridge | [python-bindings.md](python-bindings.md) |
| pproxy compat | `eggress-pproxy-compat`, `python-pproxy-compat/` | translate/check/run, tier tiers, gate, `pproxy` namespace dist | [pproxy-compat.md](pproxy-compat.md) |

### Verification infrastructure

| Component | Location | Deep dive |
|---|---|---|
| Testkit, fuzz targets, benches, scripts, oracle assets, CI policy | `crates/eggress-testkit`, `fuzz/`, `benches/`, `scripts/`, `compat/`, `.github/workflows` | [testing-and-tooling.md](testing-and-tooling.md) |

---

## Cross-cutting invariants (hold everywhere)

1. Streams are boxed at every protocol/transport boundary.
2. Domains stay unresolved until dial time; DNS results are screened against
   private/reserved ranges (rebinding defense).
3. Credentials are redacted in logs, errors, diagnostics, and metric labels.
4. Parsers are bounded (heads, credentials, chunks, datagrams) — every parser
   has a matching fuzz target.
5. Auth comparisons are constant-time (`subtle`) across all protocols.
6. One compiled snapshot feeds routing + health + admin + metrics.
7. Listeners are not hot-reloadable; policy/upstreams/groups/health are.
8. Unsupported transports/features fail with structured diagnostics and
   stable exit codes — never silent fallback.
9. `unsafe_code = "deny"` workspace-wide; no OpenSSL/C dependencies; rustls
   only.

## Build profiles

Default features = `full` (common+extended+operations+reverse+pproxy-compat).
Optional: `ssh`, `quic`, `legacy-crypto`, `pproxy-daemon`. Lean build:
`cargo build -p eggress-cli --release --no-default-features --features common`.
MSRV 1.85; release profiles use thin-LTO/symbol-stripping.

## Repository layout

```
eggress/
├── crates/                 # 26 workspace crates (see index above)
├── python/                 # canonical Python package (eggress/) + pproxy shim sources
├── python-pproxy-compat/   # opt-in distribution owning top-level `pproxy`
├── architecture/           # THIS directory: overview + per-component reviews
├── docs/                   # canonical reference docs (ARCHITECTURE, parity manifests, specs)
├── tests/                  # cross-implementation Python tests (tests/compat)
├── fuzz/                   # standalone libfuzzer workspace (11 targets)
├── benches/                # Criterion benchmarks (root pkg eggress-bench)
├── scripts/                # interop/certification/probe/evidence tooling
├── compat/pproxy-2.7.9/    # frozen oracle provenance + baselines
└── example-config.toml     # annotated configuration tour
```

## Related material (outside this directory)

- `docs/ARCHITECTURE.md` — long-form canonical architecture narrative
- `docs/parity/pproxy_capability_manifest.toml`,
  `docs/parity/pproxy_2_7_9_strict_manifest.toml` — compatibility contracts
- `docs/PPROXY_PARITY_SPEC.md` — tier vocabulary used throughout
- `.skills/` — task-specific agent guides (rust-proxy-dev, python-bindings,
  testing, security-dev, …); mirrored into `.agents/skills/` and
  `.opencode/skills/` via relative symlinks
- Earlier per-crate notes also exist under `docs/architecture/`; treat this
  directory as the maintained review index.
