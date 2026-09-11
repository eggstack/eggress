# Eggress Architecture Overview

Eggress is a Rust-native, embeddable, multi-protocol proxy framework and CLI
targeting practical and behavioral compatibility with Python `pproxy==2.7.9`.
It is built on Tokio around one central design decision: **everything is a
boxed byte stream**. Protocols, TLS, SSH, QUIC, and chain hops all consume an
`AsyncRead + AsyncWrite` stream and return an upgraded one, so any listener
protocol can be paired with any upstream chain without generics leaking
through the stack.

This document is the bird's-eye map and the index into per-component deep
dives. Each section below gives a 2–4 sentence overview of one discrete
module/component and links to its dedicated file **in this directory** for a
focused review session. Start here for orientation; go to the linked file for
module maps, APIs, control flow, and reviewer gotchas.

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

#### Core types & streams — `eggress-core` → [core.md](core.md)

Root dependency of nearly every crate. Defines the universal `BoxStream`
boundary, typed destinations (`TargetAddr`/`TargetHost`), client identity,
semaphore-bounded `TcpListener`, `DirectConnector` with DNS-rebinding
defense, `ReplayStream`/`ProtocolDispatcher` sniffing, bidirectional `relay()`,
`ChainExecutor`/`HopHandler` multi-hop execution, and static TCP/UDP
capability classification.

#### URI grammar — `eggress-uri` → [uri.md](uri.md)

Leaf crate with no eggress dependencies. Parses proxy URIs into a typed AST
(`ProxyChainSpec` → `ProxyHopSpec` → `ProtocolSpec`/`EndpointSpec`/
`CredentialSpec`): `+` separates protocols within a hop, `__` separates hops.
Owns shared `syntax` lexing primitives, canonical protocol-name recognition,
and redacted display so secrets never reach logs.

#### Configuration — `eggress-config` → [config.md](config.md)

The single place the configuration surface is defined. Turns user TOML into
a validated `RuntimeConfig` (versioned schema, recursive matchers, secret
sources, health/PAC/static sections, CLI-compat compilation). Everything
invalid fails before any socket binds; the compiled config is the handoff to
startup and to the atomic reload transaction.

### Policy & observability

#### Routing engine — `eggress-routing` → [routing.md](routing.md)

Policy engine deciding Direct / UpstreamGroup / Reject per request.
First-match-wins rules over host/CIDR/port/source/listener/protocol/identity
matchers, upstream groups with persistent schedulers
(first-available, round-robin, random, least-connections), health state
machine with hysteresis plus active TCP probes, `PendingLease`/`ActiveLease`
concurrency accounting, and route-explain tooling. Hot-reload safe via
`ArcSwap`.

#### Metrics — `eggress-metrics` → [metrics.md](metrics.md)

Single Prometheus `MetricsRegistry` owning every metric family. Implements
the server's `SessionMetrics` trait so the data plane records without knowing
about Prometheus. Bridges live atomics from UDP relay, Shadowsocks, H2, and
transparent-proxy subsystems with delta-promotion to avoid double-counting;
bounded label cardinality throughout.

### Data plane & lifecycle

#### Connection orchestration — `eggress-server` → [server.md](server.md)

The reusable per-connection pipeline: `serve_connection()` detects the
inbound protocol (with timeout + auth), builds a `RouteRequest`, opens the
route via shared `open_route()` (direct or chained), sends the success reply
only after the upstream is established, then relays with byte counting.
Emits structured `SessionReport`s (outcome + failure category) and supports
Unix-socket and transparent-listener variants.

#### Runtime supervisor — `eggress-runtime` → [runtime.md](runtime.md)

Process-level composition: snapshot compilation (`CompiledRuntimeSnapshot`
with `Arc`-identity reuse), listener pre-bind, five `CancellationToken`s plus
`TaskTracker`s, SIGHUP reload through one canonical
`apply_compiled_config` transaction, signal handling, health-manager wiring,
reverse routing gate, system-proxy post-bind hook, and the enforced ordered
shutdown (admin stops last).

#### Admin HTTP — `eggress-admin` → [admin.md](admin.md)

Hyper-based local operational server: `/-/health`, `/-/ready`, `/-/status`,
`/-/routes`, `/-/upstreams`, `/-/config`, `/metrics`, PAC generation/serving,
static content, `/-/route-explain` dry-run routing, `/-/udp` association
status, and reverse state. Reads the live snapshot per request via
`AdminSnapshotProvider`, so reloads take effect without restarting admin.

#### UDP subsystem — `eggress-udp` → [udp.md](udp.md)

Association management (`UdpAssociationRegistry`), per-target connected flows
(`UdpTargetFlow`), SOCKS5 UDP codec, recursive SOCKS5/Shadowsocks upstream
framing, direct forwarding, standalone relay modes, client-pin + target
validation security policy, bounded limits/idle reaping, and Prometheus
bridging. Every datagram is routed through the full rule engine; unsupported
chains drop with metrics, never silent fallback.

#### System proxy — `eggress-system-proxy` → [system-proxy.md](system-proxy.md)

Leaf crate reading/mutating OS proxy configuration. Powers
`eggress system-proxy inspect` and the pproxy-compatible `--sys` flag
(apply bound listener after bind, rollback on shutdown). Structured
`Command { program, args }` execution only, per-platform capability
classification, credential redaction.

### Protocol crates (each depends only on core + uri)

#### HTTP/1.1 CONNECT + forward + H2 pool — `eggress-protocol-http` → [protocols-http.md](protocols-http.md)

Server-side CONNECT accept, client-side CONNECT hop, absolute-form forward
proxying with origin-form conversion, bounded header/body/chunk parsing, and
HTTP/2 CONNECT with a pooled `H2HopHandler`. Detection distinguishes
proxy-usable HTTP by method/response shape.

#### SOCKS4/4a + SOCKS5 — `eggress-protocol-socks` → [protocols-socks.md](protocols-socks.md)

Full server + client for SOCKS4/4a (with 4a domain preservation) and SOCKS5
(method negotiation, no-auth + username/password, CONNECT, UDP ASSOCIATE
reply). Owns the SOCKS5 UDP datagram codec (IPv4/IPv6/domain) consumed by the
UDP subsystem; bounded 255-byte credentials, constant-time auth.

#### Shadowsocks AEAD (+legacy/SSR gates) — `eggress-protocol-shadowsocks` → [protocols-shadowsocks.md](protocols-shadowsocks.md)

Native path is AEAD-only (AES-GCM family, ChaCha20-IETF-Poly1305) with
pproxy-compatible EVP_BytesToKey→HKDF-SHA1 key derivation, encrypted TCP
address header, UDP packet encode/decode, address codec, and metrics.
Legacy stream ciphers sit behind `legacy-crypto`; SSR framing plus six
built-in plugins sit behind `pproxy-legacy`; both fail closed when off.

#### Trojan — `eggress-protocol-trojan` → [protocols-trojan.md](protocols-trojan.md)

Small focused crate: SHA224 password-hash auth and the Trojan request wire
format. Server accept runs on an already-TLS stream; the client connector
performs TLS through the shared transport layer. Client + server roles
implemented natively.

#### WebSocket tunnel + raw passthrough — `eggress-protocol-websocket`, `eggress-protocol-raw` → [protocols-tunnels.md](protocols-tunnels.md)

Two thin stream-native tunnel wrappers usable as listener protocols or chain
hops. WebSocket performs the ws/wss upgrade over the prior-hop stream and
returns a byte stream; raw passes the prior-hop stream through (fixed-target
listener on the inbound side). Both consume the prior hop's stream — no
independent dials mid-chain.

#### Reverse / backward — `eggress-protocol-reverse` → [protocols-reverse.md](protocols-reverse.md)

pproxy's backward model for NAT traversal: a client behind NAT dials OUT to
an acceptor; external sessions arriving at the acceptor are paired with
pooled control channels and relayed back. Includes auth handshake, control
state, metrics, optional server-authenticated TLS / mTLS on native control
channels (pproxy-compat wire stays plaintext), plus raw and SOCKS5-framed
pproxy-wire adapters. TCP only, one session per control connection.

### Transports

#### TLS (rustls only) — `eggress-transport-tls` → [transports-tls.md](transports-tls.md)

The only TLS implementation in the workspace — no OpenSSL anywhere.
Client/server config builders (system roots, custom CA PEM, ALPN, SNI
override, insecure opt-in), `tls_connect`/`tls_accept` over `BoxStream`s.
Consumed by listener inbound, upstream `+tls` hops, and Trojan.

#### SSH channels — `eggress-transport-ssh` (`ssh` feature) → [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md)

Upstream-only SSH transport (no SSH listeners): session cache, password and
key auth, channel-per-connection over the prior-hop stream. Opt-in via the
`ssh` feature; insecure host-key acknowledgement is an explicit narrow hook,
never default.

#### QUIC streams — `eggress-transport-quic` (`quic` feature) → [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md)

QUIC transport producing streams as `BoxStream`s so the rest of the stack
stays transport-agnostic. Bound to the `quic` feature; `insecure-quic`
(test-only cert bypass) is never part of the product gate.

#### HTTP/3 CONNECT — `eggress-protocol-h3` (`quic` feature) → [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md)

H3 CONNECT handshake over QUIC streams; TLS ALPN handled by the chain
executor. Shares the deep dive with SSH/QUIC because the three are one
feature-gated transport story.

### Entry points & compatibility

#### CLI binaries — `eggress-cli` (`eggress` + compat `pproxy`) → [cli.md](cli.md)

One crate installing two binaries that converge on the same
`ServiceSupervisor` and differ only in how arguments reach config. Native
`eggress`: `-l`/`-r`/`--config`/`--rules-file`, `route`, `upstream test`,
`pproxy translate|check|run`, `system-proxy inspect`, stable exit codes
(0/1/2/3/5/130/143), lean `--no-default-features --features common` builds.
Compat `pproxy`: frozen 2.7.9 flag parser with fail-closed gate and Linux
`--daemon` re-exec behind `pproxy-daemon`.

#### Embed API — `eggress-embed` → [embed.md](embed.md)

Stable in-process Rust API and the binding target for PyO3: parse/validate
config from TOML string or file, `start()`/`start_blocking()`, discover
bound addresses (port-0 friendly), `status()`/`metrics_text()`, hot-reload
routing/upstreams via `reload_toml_str`, idempotent shutdown. Plus
`OutboundConnector` for listener-free TCP chains (`from_pproxy_uri` with
`__` multi-hop, fail-closed) and idempotent UDP association.

#### Python bindings + package — `eggress-python`, `python/` → [python-bindings.md](python-bindings.md)

Two layers: compiled PyO3 `_eggress` extension (service, connection,
outbound, compat/translate/explain/test helpers, system-proxy, 18 functions
+ 18 exception types, GIL released on blocking calls) and the canonical
pure-Python `python/eggress` package (service/handles, `Connection`,
`OutboundConnector`/`OutboundStream`, pproxy facade, protocol/cipher/plugin/
wrapper object model, `AsyncBridge`/`CloseWaiter` asyncio pattern, `.pyi`
stubs). maturin builds the `eggress` wheel (abi3-py39); it never owns the
top-level `pproxy` namespace.

#### pproxy compat — `eggress-pproxy-compat`, `python-pproxy-compat/` → [pproxy-compat.md](pproxy-compat.md)

Evidence-backed compatibility contract, not a claim: frozen 2.7.9
argument/URI parsing over shared `syntax` primitives, dual renderers (TOML
presentation + native compilation from shared intermediates), five-tier
classification (`drop_in` … `unsupported`), 26 stable diagnostic codes,
fail-closed execution gate, 10 stable exit codes, regex/rule-file compat.
The opt-in `eggress-pproxy-compat` distribution owns the top-level `pproxy`
shim namespace and must never be installed beside upstream `pproxy`.
Contract truth lives in `docs/parity/pproxy_capability_manifest.toml` + the
practical compatibility matrix.

### Verification infrastructure & tools

Covered in depth by [testing-and-tooling.md](testing-and-tooling.md);
summary of the discrete pieces:

- **Test support — `crates/eggress-testkit`.** Test-only library used as a
  dev-dependency: echo/half-close servers, free-port allocation, oracle
  interpreter resolution (`$EGRESS_ORACLE_PYTHON` → `$EGRESS_PYTHON_BIN` →
  discovery), pproxy 2.7.9 oracle process management, differential harness,
  manifest/corpus/fixture/report helpers.
- **Fuzz — `fuzz/` (standalone workspace, 11 libfuzzer targets).** One target
  per bounded parser (SOCKS5 handshake + UDP datagram, HTTP CONNECT
  response, Trojan request/accept, route match, URI parse, Shadowsocks
  frame, TOML config, WebSocket handshake, H2 authority). Not covered by
  workspace commands; check with
  `cargo check --manifest-path fuzz/Cargo.toml --bins`.
- **Benchmarks — `benches/` (root package `eggress-bench`, Criterion).**
  Four suites: `route_match` (decision latency), `tcp_relay` (1 KiB/64 KiB
  throughput), `udp_relay` (codec), `http_connect_upstream` (CONNECT lifecycle).
- **Scripts — `scripts/`.** Grouped helpers: strict pproxy probes
  (`strict_*_probe.py`), interop runners (`compat_shadowsocks.sh`,
  `compat_udp_pproxy.sh`), certification (`run_pproxy_certification.sh`,
  `run_strict_pproxy_*`), evidence/validation
  (`validate_pproxy_parity_manifest.py`, `compare_observations.py`,
  regression-injection demos), release smoke (`release_artifact_smoke.py`,
  `test_wheel.sh`), perf/soak (`scripts/perf/`), snapshots
  (`snapshot_pproxy_api.py`, `pproxy_surface_probe.py`).
- **Frozen oracle — `compat/pproxy-2.7.9/`.** Immutable reference data:
  provenance/hashes, known defects, CLI + namespace baselines, fixture
  manifest, recorded observations, oracle tests/examples. Prebuilt venvs
  (`.venv-oracle`, `.venv-pproxy-279`) already exist at root.
- **Cross-implementation tests — `tests/compat/`.** API-contract validation
  against the extracted 2.7.9 contract plus URI/CLI/Python-API fixtures and
  behavioral docs; regression-injection modules prove the harness catches
  mutations. Python tiers 0–5 live under `python/tests/`
  (`TEST_TAXONOMY.md`); `pytest.ini` forces `--import-mode=importlib` so the
  source tree can't shadow the built extension.
- **CI — exactly 3 workflows.** `ci.yml` (Ubuntu Rust smoke: fmt, clippy,
  workspace tests, bounded optional-compat compile gate, fuzz-target
  compilation), `python-test.yml` (path-scoped 3.12 wheel smoke),
  `publish-python.yml` (fires on every `v*` tag push — tags publish to PyPI).
  Policy: `docs/CI_STATUS.md`; suite inventory: `docs/TESTING.md`.
- **Container — `Containerfile`.** Multi-stage
  `rust:1.85-slim` → distroless nonroot; ports 8080/1080/9090; entrypoint
  `/eggress`.

---

## Capabilities at a glance

- **Protocols:** mixed-protocol listeners (HTTP CONNECT/forward, SOCKS4/4a,
  SOCKS5, Shadowsocks AEAD, Trojan, WebSocket, raw, reverse acceptor) and
  arbitrary compatible multi-hop chains (`__`); H2 CONNECT pooled, H3 behind
  `quic`, SSH upstream-only behind `ssh`.
- **TCP + UDP:** full TCP relay with half-close; UDP via SOCKS5 ASSOCIATE
  with per-datagram routing, SOCKS5/Shadowsocks upstream recursion, direct
  fallback, standalone relay modes. HTTP/SOCKS4/Trojan/H2/WS hops are
  explicitly UDP-rejected with metrics.
- **Routing:** first-match rules → groups → health-aware schedulers → lease
  accounting → explainable decisions (`route-explain`, admin endpoint).
- **Operations:** TOML config + CLI flags + rule files, atomic hot-reload of
  policy/upstreams/groups/health (listener topology is restart-only),
  admin HTTP + Prometheus, PAC/static serving, system-proxy inspect/apply,
  reverse NAT-traversal servers/clients, graceful ordered shutdown.
- **Compatibility posture:** tier vocabulary (`matched` /
  `supported_difference` / `platform_limited` / `intentional_non_parity` in
  operator docs; `drop_in` … `unsupported` `ManifestTier` in the crate —
  see [pproxy-compat.md](pproxy-compat.md)). Claim changes update the
  manifest + matrix and run the oracle/differential/interop suites; generated
  reports follow the manifest, never lead it. Deliberate boundaries include
  macOS PF transparency, four unavailable legacy cipher names, SOCKS BIND
  refusal, CONNECT-tunneling (no TLS MITM), TCP-only reverse.
- **Security defaults:** rustls-only TLS, bounded parsers (each with a fuzz
  target), constant-time auth, redacted credentials in logs/errors/metrics,
  DNS-rebinding guards on direct connect, client-pin + target validation on
  UDP, `unsafe_code = "deny"`, pure-Rust deps (no OpenSSL/C/build scripts
  without architectural reason).
- **Platforms:** Linux, macOS, Windows where the capability exists; SSH/QUIC/
  daemon/legacy-crypto are explicit opt-in features; MSRV 1.85, edition 2021.

Full checklists: `docs/CAPABILITIES.md`, `docs/OPERATIONS.md`,
`docs/SECURITY_REVIEW.md` + `docs/security/`, compatibility contract in
`docs/parity/`.

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
Optional: `ssh`, `quic`, `pproxy-legacy`, `legacy-crypto`, `pproxy-daemon`.
Lean build:
`cargo build -p eggress-cli --release --no-default-features --features common`.
MSRV 1.85; release profiles use thin-LTO/symbol-stripping. Never substitute
`--all-features` (drags in test-only `insecure-quic`); the bounded
`full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon` check covers the
product-relevant optional surface.

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
├── .skills/                # task-specific agent guides (mirrored into .agents/ + .opencode/)
└── example-config.toml     # annotated configuration tour
```

## How to use this index for review

Pick one component, read its 2–4 sentence summary above, then open the
linked deep dive — each follows the same shape (module map → API → control
flow → tests → gotchas → see-also). Suggested order for a first pass:
[core.md](core.md) → [uri.md](uri.md) → [config.md](config.md) →
[routing.md](routing.md) → [server.md](server.md) → [runtime.md](runtime.md),
then the protocol/transport of interest, then [cli.md](cli.md) /
[embed.md](embed.md) / [python-bindings.md](python-bindings.md) /
[pproxy-compat.md](pproxy-compat.md), finishing with [udp.md](udp.md),
[protocols-reverse.md](protocols-reverse.md), [admin.md](admin.md),
[metrics.md](metrics.md), [system-proxy.md](system-proxy.md),
[transports-tls.md](transports-tls.md),
[transports-ssh-quic-h3.md](transports-ssh-quic-h3.md), and
[testing-and-tooling.md](testing-and-tooling.md).

## Related material (outside this directory)

- `docs/ARCHITECTURE.md` — long-form canonical architecture narrative
- `docs/parity/pproxy_capability_manifest.toml`,
  `docs/parity/pproxy_2_7_9_strict_manifest.toml` — compatibility contracts
- `docs/parity/README.md` + `crates/eggress-pproxy-compat/src/tier.rs` — tier vocabulary and classification rules (`docs/PPROXY_PARITY_SPEC.md` is historical provenance only)
- `.skills/` — task-specific agent guides (rust-proxy-dev, python-bindings,
  testing, security-dev, …); mirrored into `.agents/skills/` and
  `.opencode/skills/` via relative symlinks
- Earlier per-crate notes also exist under `docs/architecture/`; treat this
  directory as the maintained review index.
