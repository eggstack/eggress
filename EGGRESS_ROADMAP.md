# eggress Full-Parity Roadmap

## Project definition

**eggress** is a Rust-native, embeddable, multi-protocol proxy framework and CLI intended to reach practical and behavioral parity with Python's `pproxy` while preserving a nearly identical command-line and URI-driven usage model.

The project should not be structured as a literal source rewrite. `pproxy` combines several distinct concerns—protocol termination, transport composition, proxy chaining, routing, UDP association management, encryption, transparent proxying, administration, and platform integration—behind a compact interface. eggress should preserve that interface while replacing the internals with explicit, typed Rust abstractions.

The implementation should prefer pure Rust dependencies whenever a mature and auditable option exists. Native libraries, C bindings, shell commands, and platform FFI should be isolated behind narrow adapters and used only where the operating system or protocol ecosystem makes a pure Rust implementation impractical.

## Primary goals

1. Reach full practical capability parity with `pproxy`.
2. Preserve nearly identical CLI flags and URI syntax.
3. Support use as both a standalone binary and an embeddable Rust library.
4. Support arbitrary compatible combinations of inbound protocols, outbound protocols, transport wrappers, and proxy hops.
5. Provide secure defaults without preventing explicit legacy-compatibility operation.
6. Remain resource-bounded under hostile network input.
7. Maintain a README capability matrix that is updated as functionality lands.
8. Use differential interoperability tests against Python `pproxy`.
9. Prefer pure Rust dependencies and RustCrypto implementations.
10. Keep platform-specific and unsafe code isolated.

## Non-goals

The initial project is not intended to be:

- a packet-level VPN;
- a general-purpose service mesh;
- a browser-oriented content-filtering proxy;
- an interception proxy that dynamically generates certificates;
- a replacement for kernel routing, WireGuard, Tor, or full VPN software;
- an unrestricted native plugin host with an unstable Rust ABI.

These capabilities may later integrate with eggress, but they should not distort the core architecture.

## Compatibility target

Parity has four dimensions:

### Syntax parity

Existing common `pproxy` commands should require no changes beyond replacing the executable name.

Examples:

```text
eggress
eggress -l http+socks4+socks5://:8080
eggress -r socks5://proxy.example:1080
eggress -l ss://chacha20:password@:8388
eggress -r ss://hop1:8388__http://hop2:8080
eggress -ul socks5://:1080 -ur ss://server:8388
```

### Protocol parity

eggress should eventually support all major protocol and transport classes exposed by `pproxy`:

- direct TCP;
- direct UDP;
- HTTP forwarding;
- HTTP CONNECT;
- HTTPS-wrapped proxy transports;
- HTTP/2 CONNECT;
- HTTP/3 CONNECT;
- SOCKS4;
- SOCKS4a;
- SOCKS5 CONNECT;
- SOCKS5 BIND where practical;
- SOCKS5 UDP ASSOCIATE;
- Shadowsocks stream and AEAD modes;
- Shadowsocks UDP;
- legacy Shadowsocks OTA;
- ShadowsocksR compatibility;
- Trojan;
- SSH `direct-tcpip`;
- raw TCP forwarding;
- raw UDP forwarding;
- WebSocket tunnels;
- QUIC tunnels;
- Unix-domain listeners and connectors;
- transparent proxying;
- reverse/backward connections;
- multi-hop chains.

### Behavioral parity

Parity includes:

- mixed listener protocol autodetection;
- remote and local authentication;
- route rules;
- upstream groups;
- first-available, round-robin, random, and least-connections scheduling;
- upstream health checking;
- TCP and UDP chaining where technically valid;
- PAC serving;
- traffic statistics;
- system-proxy configuration;
- static HTTP endpoints;
- compatibility logging;
- equivalent default behavior.

### Interoperability parity

A protocol is not considered complete until eggress can interoperate in both directions with:

- Python `pproxy`, where applicable;
- one or more established third-party clients or servers;
- fragmented and partial network reads;
- IPv4, IPv6, and domain-name destinations;
- authentication and unauthenticated variants;
- both success and failure paths.

## Architectural principles

### Separate protocol from transport

Application proxy protocols and byte transports must remain distinct.

Examples:

- SOCKS5 is a proxy protocol.
- HTTP CONNECT is a proxy protocol.
- TLS is a transport wrapper.
- TCP is a transport.
- QUIC is a transport and multiplexing substrate.
- WebSocket is a message transport adapted into a byte stream.
- SSH is a channel-producing upstream transport.

A protocol implementation must be able to run over an arbitrary compatible underlying stream.

### Preserve unresolved targets

Destinations should remain typed as either a domain name or an IP address until a connector explicitly requires local resolution. This preserves remote DNS semantics and permits hostname-based routing.

### Build chains from composable connectors

Each upstream hop should accept a stream to the hop and produce a stream to the next target. The route executor should fold over the hop list rather than special-case every chain combination.

### Treat UDP as a first-class subsystem

UDP should not be modeled as a small extension to the TCP relay. It needs explicit association identity, expiry, demultiplexing, target metadata, framing, and route state.

### Make compatibility explicit

Legacy ciphers, insecure TLS verification, source-IP authentication caching, SSR, and historical quirks should be available only through compatibility profiles or compile-time features.

### Keep the data plane small

Per-packet and per-chunk operations should avoid central locks, unbounded allocation, and heavyweight dynamic policy evaluation.

## Proposed workspace

```text
eggress/
├── Cargo.toml
├── README.md
├── LICENSE
├── SECURITY.md
├── CONTRIBUTING.md
├── docs/
│   ├── ROADMAP.md
│   ├── PHASE_1_PLAN.md
│   ├── COMPATIBILITY.md
│   ├── URI_GRAMMAR.md
│   ├── ARCHITECTURE.md
│   └── SECURITY_MODEL.md
├── crates/
│   ├── eggress-cli/
│   ├── eggress-core/
│   ├── eggress-config/
│   ├── eggress-uri/
│   ├── eggress-routing/
│   ├── eggress-transport/
│   ├── eggress-protocol-http/
│   ├── eggress-protocol-socks/
│   ├── eggress-protocol-shadowsocks/
│   ├── eggress-protocol-trojan/
│   ├── eggress-protocol-ssh/
│   ├── eggress-transport-tls/
│   ├── eggress-transport-websocket/
│   ├── eggress-transport-quic/
│   ├── eggress-transparent/
│   ├── eggress-admin/
│   └── eggress-testkit/
└── tests/
    ├── interoperability/
    ├── fixtures/
    └── packet-captures/
```

The workspace may begin with fewer crates and split as boundaries stabilize. The dependency graph should remain acyclic and layered:

```text
CLI/config
    ↓
routing/orchestration
    ↓
protocol and transport traits
    ↓
protocol/transport implementations
    ↓
platform adapters
```

## Dependency policy

### Preferred

Prefer dependencies that are:

- implemented in Rust;
- actively maintained;
- permissively licensed;
- fuzzed or widely deployed;
- compatible with Tokio;
- free of unnecessary native build dependencies;
- able to compile on Linux, macOS, and Windows.

Likely core dependencies:

| Capability | Preferred dependency |
|---|---|
| Async runtime | `tokio` |
| Cancellation/utilities | `tokio-util` |
| CLI | `clap` |
| Serialization | `serde` |
| TOML | `toml` |
| Structured errors | `thiserror` |
| Diagnostics | `miette` or project-local formatting |
| Logging | `tracing`, `tracing-subscriber` |
| HTTP types | `http` |
| HTTP/1 and HTTP/2 | `hyper`, `h2` |
| TLS | `rustls`, `tokio-rustls` |
| QUIC | `quinn` |
| HTTP/3 | `h3` |
| WebSocket | `tokio-tungstenite` |
| DNS | system resolver initially; `hickory-resolver` optionally |
| Regex | `regex`, `regex-automata` |
| Secrets | `secrecy`, `zeroize` |
| Socket options | `socket2` |
| OS interfaces | `nix` where pure Rust APIs are insufficient |
| Property tests | `proptest` |
| Fuzzing | `cargo-fuzz`, `libfuzzer-sys` |
| Benchmarks | `criterion` or `divan` |

### Allowed with review

Dependencies containing limited unsafe Rust are acceptable where they are established and audited. The project goal is to prefer pure Rust over C dependencies, not to prohibit all unsafe internals.

### Avoid unless unavoidable

- OpenSSL bindings when rustls is sufficient;
- libcurl bindings;
- native SSH libraries when `russh` can satisfy requirements;
- C-based SOCKS or HTTP parser libraries;
- system shell commands for core proxy behavior;
- embedded scripting runtimes in the data plane;
- dynamic Rust plugins.

### Platform exceptions

Transparent proxying and system-proxy configuration may require:

- direct system calls;
- `libc`;
- `nix`;
- Windows API bindings;
- macOS SystemConfiguration or PF-specific FFI.

These must be isolated to platform crates or modules and never leak platform-specific types into core traits.

## Capability completion criteria

A capability may be checked off in the README only after all applicable criteria are satisfied:

1. URI/config parsing implemented.
2. Client role implemented.
3. Server role implemented.
4. Authentication behavior implemented.
5. IPv4 tested.
6. IPv6 tested.
7. Domain targets tested.
8. Fragmented I/O tested.
9. Failure behavior tested.
10. Interoperability test with Python `pproxy` completed.
11. Interoperability test with an independent implementation completed where available.
12. Metrics and structured errors added.
13. Resource limits documented.
14. User documentation added.
15. No unresolved high-severity security findings.

Partial capabilities should remain explicitly marked as partial.

## Roadmap overview

| Phase | Name | Core outcome |
|---|---|---|
| 0 | Compatibility baseline | Frozen target behavior, test corpus, URI grammar, architecture decisions |
| 1 | Core TCP proxy foundation | Mixed HTTP/SOCKS listener, direct and chained TCP, initial compatible CLI |
| 2 | Routing, health, and operations | Rules, scheduling, health checks, metrics, configuration, admin endpoints |
| 3 | UDP foundation | Direct UDP, SOCKS5 UDP, association lifecycle, UDP routing |
| 4 | UDP upstream relay | one-hop SOCKS5 UDP upstream relay with capability classification |
| 5 | Upstream protocol parity | HTTP/SOCKS4/SOCKS5 polish, Shadowsocks/Trojan foundations, capability classifier |
| 5A | TLS transport (corrective) | rustls wrapping, HTTPS proxy transport, secure certificate policy |
| 6 | WebSocket, raw tunnels, and SSH | WS tunnels, fixed forwarding, SSH channels, connection reuse |
| 7 | HTTP/2 and multiplexed streams | H2 CONNECT client/server, pooling, stream adapters |
| 8 | QUIC and HTTP/3 | QUIC tunnels, H3 CONNECT, datagrams, multiplexing |
| 9 | Reverse and backward connections | Reverse registration, multiplexing, reconnect, NAT traversal workflows |
| 10 | Transparent proxying | Linux REDIRECT/TPROXY and macOS PF support |
| 11 | Legacy and edge parity | SSR, OTA, legacy ciphers, remaining historical quirks |
| 12 | Hardening and parity release | Full interoperability matrix, fuzzing, soak tests, audits, packaging |

---

# Phase 0: Compatibility baseline

## Objective

Create an executable specification of the target before implementation diverges from actual `pproxy` behavior.

## Deliverables

- frozen Python `pproxy` version or commit;
- captured `--help` output;
- protocol and flag inventory;
- URI grammar specification;
- valid and invalid URI fixture corpus;
- behavior matrix for defaults;
- packet captures for representative handshakes;
- reference Docker or virtual-environment harness;
- differential test runner skeleton;
- architecture decision records;
- dependency review policy;
- initial README capability checklist.

## Required investigations

- exact scheme names and aliases;
- URI delimiter escaping;
- `+` protocol autodetection order;
- `__` hop semantics;
- rule matching order;
- authentication cache behavior;
- UDP map identity and expiry behavior;
- SSL versus secure verification semantics;
- tunnel destination syntax;
- reverse/backward connection framing;
- QUIC framing and authentication;
- transparent destination recovery;
- PAC and static endpoint behavior;
- system proxy state changes.

## Exit criteria

Phase 0 is complete when every advertised `pproxy` feature has a corresponding test case, planned test case, or documented exclusion.

---

# Phase 1: Core TCP proxy foundation

## Objective

Produce a usable Rust proxy that preserves the common `pproxy` invocation model and supports mixed HTTP/SOCKS TCP proxying with direct or chained upstreams.

## Included

- Rust workspace and CI;
- CLI compatibility shell;
- URI parser and typed AST;
- TCP and Unix listeners;
- direct connector;
- HTTP CONNECT server and client;
- basic HTTP forward-proxy server;
- SOCKS4/SOCKS4a server and client;
- SOCKS5 CONNECT server and client;
- no-auth and username/password authentication;
- protocol sniffing with replay buffer;
- target preservation for remote DNS;
- direct, HTTP, and SOCKS chains;
- bidirectional relay with half-close support;
- connection timeouts;
- initial structured logging;
- basic counters;
- exact first-available scheduling;
- testkit and differential tests.

## Excluded

- UDP;
- TLS;
- Shadowsocks;
- Trojan;
- SSH;
- WebSocket;
- HTTP/2;
- HTTP/3;
- QUIC;
- reverse tunnels;
- transparent proxying;
- PAC/system proxy;
- SSR.

## Exit criteria

Commands equivalent to the following operate successfully:

```text
eggress
eggress -l http://:8080
eggress -l socks4+socks5://:1080
eggress -l http+socks4+socks5://:8080
eggress -r http://proxy:8080
eggress -r socks5://proxy:1080
eggress -r socks5://hop1:1080__http://hop2:8080
```

Mixed listeners must interoperate with curl, a browser or standard HTTP client, a standard SOCKS client, and Python `pproxy`.

---

# Phase 2: Routing, health, and operations

## Objective

Make eggress operable as a long-running service with route policy, upstream groups, diagnostics, and administration.

## Included

- regex compatibility rules;
- exact host, suffix, CIDR, and port rules;
- reject and direct actions;
- first-available scheduler;
- round-robin scheduler;
- random scheduler;
- least-connections scheduler;
- active connection accounting;
- health checking with compatibility mode;
- richer health state with hysteresis;
- TOML configuration;
- configuration validation;
- Prometheus-compatible metrics;
- JSON logs;
- local admin API;
- static HTTP endpoint;
- PAC generation and serving;
- graceful shutdown;
- configuration reload where safe;
- route explanation command.

## Exit criteria

Upstream selection and rule behavior match reference fixtures, all schedulers have deterministic tests, and long-running service behavior passes restart and shutdown tests.

---

# Phase 3: UDP foundation

## Objective

Add resource-bounded UDP proxying and establish a reusable association model.

## Included

- direct UDP relay;
- typed datagram target model;
- association table;
- configurable idle expiry;
- global and per-client limits;
- SOCKS5 UDP ASSOCIATE server;
- SOCKS5 UDP ASSOCIATE client;
- UDP upstream routing;
- domain target preservation where the protocol permits;
- NAT-reuse-safe association keys;
- metrics and eviction visibility;
- packet-size and amplification limits;
- interoperability tests.

## Exit criteria

SOCKS5 UDP operates through direct and SOCKS5 upstream routes under packet loss, reordering, IPv4, IPv6, and domain destinations.

---

# Phase 4: UDP upstream relay

## Objective

Extend direct UDP forwarding to one-hop SOCKS5 upstream relay with capability classification.

## Included

- [x] UDP capability model (UdpRelayCapability);
- [x] SOCKS5 upstream client (handshake, auth, UDP ASSOCIATE);
- [x] Flow model (UdpFlowKind, UdpFlowKey, per-target upstream association);
- [x] Relay integration (handle_client_datagram refactor);
- [x] Upstream metrics and admin visibility;
- [x] Codec rename with backward-compatible wrappers;
- [x] Synthetic test server (Socks5UdpTestServer);
- [x] Integration tests (socks5_upstream, udp_upstream).

## Exit criteria

SOCKS5 UDP operates through direct and SOCKS5 upstream routes under packet loss, reordering, IPv4, IPv6, and domain destinations.

## Status

Phase 4 complete. All items implemented and tested.

---

# Phase 5: Upstream protocol parity

## Objective

Add encrypted proxy protocol foundations — upstream/client roles, AEAD
ciphers, and basic TCP/UDP relay — while preserving modern security
defaults.

## Included (upstream/client only)

- Shadowsocks target framing;
- Shadowsocks AEAD TCP (aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305);
- Shadowsocks AEAD UDP (one-hop upstream relay);
- password/key derivation through RustCrypto;
- modern cipher suite only;
- Trojan TCP client with rustls;
- SHA224 password hash authentication;
- upstream protocol capability classification;
- metrics and admin integration for new protocols;
- config validation for unsupported protocol/transport combos.

## Deferred (moved to later phases)

- Shadowsocks TCP/UDP server (inbound listener role);
- legacy stream cipher compatibility;
- OTA compatibility wrappers;
- Trojan server role;
- Trojan fallback routing;
- interoperability tests with `shadowsocks-rust` and `pproxy`;
- ShadowsocksR.

## Dependency strategy

Pure Rust crypto via RustCrypto (aes-gcm, chacha20poly1305, hkdf, sha2).
`rustls` for TLS. No OpenSSL or native-tls.

## Exit criteria

Upstream protocol parity for all five supported protocols (HTTP, SOCKS4,
SOCKS5, Shadowsocks, Trojan) with capability classification, metrics,
and protocol docs.

---

## Phase 5A: TLS transport (corrective)

TLS transport work was originally planned as Phase 4 but was reclassified as a corrective sub-phase after Phase 5. A shared `eggress-transport-tls` crate provides rustls client/server wrappers, certificate loading, SNI, ALPN, HTTPS proxy transport, and TLS-wrapped SOCKS/custom protocols. Certificate reload remains deferred.

---

# Phase 6: WebSocket, raw tunnels, and SSH

## Objective

Cover fixed-target and message-framed transports plus SSH proxy chaining.

## Included

- raw TCP forwarding;
- raw UDP forwarding;
- fixed destination listeners;
- WebSocket client and server;
- WSS through rustls;
- byte-stream adapter over binary WebSocket messages;
- ping/pong and close handling;
- SSH client transport;
- password and key authentication;
- host-key verification;
- `direct-tcpip`;
- SSH connection pooling;
- keepalives and reconnect;
- chaining SSH through prior hops.

## Exit criteria

Raw, WS/WSS, and SSH routes operate both directly and inside compatible multi-hop chains.

---

# Phase 7: HTTP/2 and multiplexed streams

## Objective

Add multiplexed CONNECT streams and reusable upstream pools.

## Included

- HTTP/2 CONNECT server;
- HTTP/2 CONNECT client;
- request-body/response-body stream adapter;
- stream and connection flow control;
- reset and half-close propagation;
- upstream connection pooling;
- maximum concurrent streams;
- GOAWAY handling;
- H2 authentication;
- H2-over-TLS ALPN;
- metrics per transport connection and logical stream.

## Exit criteria

Multiple simultaneous tunnels reuse one H2 connection and remain isolated under stream reset, cancellation, and backpressure.

---

# Phase 8: QUIC and HTTP/3

## Objective

Implement QUIC-native multiplexed tunnels and HTTP/3 CONNECT.

## Included

- Quinn-based QUIC transport;
- versioned eggress tunnel framing;
- Python `pproxy` compatibility transport where its framing can be reproduced;
- QUIC stream lifecycle;
- QUIC datagrams;
- authentication;
- connection pooling;
- HTTP/3 CONNECT client/server;
- H3 stream adapter;
- migration and idle handling;
- route-planner validation of unsupported chain combinations.

## Exit criteria

QUIC and H3 interoperability pass cross-implementation tests, and unsupported tunnel combinations fail during configuration validation.

---

# Phase 9: Reverse and backward connections

## Objective

Support proxy service behind NAT through outbound persistent control connections.

## Included

- reverse endpoint registration;
- authenticated control channel;
- logical stream opening;
- multiplexing;
- heartbeats;
- reconnect and exponential backoff;
- re-registration;
- graceful draining;
- per-reverse-listener policy;
- reverse UDP where feasible;
- compatibility adapter for Python behavior;
- observability and operator controls.

## Exit criteria

A proxy behind NAT can serve multiple concurrent remote clients through a reconnecting reverse transport without cross-stream corruption.

---

# Phase 10: Transparent proxying

## Objective

Recover original destinations from kernel redirection and route connections through eggress.

## Included

### Linux

- `SO_ORIGINAL_DST`;
- IPv6 original destination;
- REDIRECT workflows;
- TPROXY workflows;
- transparent bind support;
- startup capability checks;
- network namespace integration tests.

### macOS

- PF original-destination recovery;
- isolated FFI;
- PF configuration documentation;
- integration tests on supported macOS versions.

### Optional Windows investigation

Windows transparent interception should be evaluated separately and should not block parity if it is outside the Python target behavior.

## Exit criteria

Transparent TCP operation is reproducible from documented firewall configuration on Linux and macOS.

---

# Phase 11: Legacy and edge parity

## Objective

Close remaining compatibility gaps without weakening the default security profile.

## Included

- ShadowsocksR protocol compatibility;
- SSR obfuscation plugins;
- legacy Shadowsocks ciphers;
- exact OTA behavior;
- historical TLS mode compatibility;
- source-IP authentication cache compatibility;
- obscure URI aliases;
- remaining port-forwarding variants;
- exact log and error compatibility where useful;
- all remaining reference test cases.

## Packaging

Legacy features should be separated into Cargo features such as:

```text
legacy-ciphers
ssr-compat
insecure-tls-compat
pproxy-auth-cache
```

## Exit criteria

Every advertised reference capability is implemented, explicitly documented as incompatible, or rejected with a precise reason.

---

# Phase 12: Hardening and parity release

## Objective

Ship a defensible full-parity release.

## Included

- complete differential matrix;
- protocol fuzzing;
- URI fuzzing;
- long-duration soak tests;
- memory and descriptor leak testing;
- connection-flood tests;
- UDP state-exhaustion tests;
- slowloris tests;
- dependency audit;
- unsafe-code audit;
- cryptographic review;
- reproducible release builds;
- SBOM;
- signed artifacts;
- Linux, macOS, and Windows packages;
- container image;
- operational documentation;
- migration guide from Python `pproxy`;
- security disclosure process.

## Full-parity release criteria

A `1.0.0` release requires:

- all README parity boxes checked or explicitly marked unsupported with rationale;
- no known critical or high-severity security defects;
- interoperability matrix published;
- all default features implemented without mandatory C dependencies;
- graceful degradation where platform facilities are unavailable;
- stable CLI and configuration schema;
- stable embeddable library API for core connection and server operations.

## Cross-cutting workstreams

### Security

Every phase must include:

- bounded parsers;
- timeout policy;
- cancellation propagation;
- secret redaction;
- denial-of-service analysis;
- unsafe-code review;
- dependency audit updates.

### Documentation

Every capability must update:

- README checklist;
- CLI help;
- URI grammar;
- configuration reference;
- interoperability matrix;
- examples;
- security notes.

### Testing

Every phase must add:

- unit tests;
- fragmented-I/O tests;
- property tests where useful;
- integration tests;
- Python `pproxy` differential cases;
- at least one independent client/server interoperability case when available.

### Performance

Performance optimization should follow correctness, but each phase should track:

- allocations per connection;
- idle memory;
- active connection overhead;
- relay throughput;
- handshake latency;
- CPU per GiB transferred;
- UDP association overhead;
- multiplexing efficiency.

## README maintenance rule

The root README is the authoritative human-readable capability ledger.

A box may change from `[ ]` to `[x]` only in the same pull request that includes:

- implementation;
- tests;
- documentation;
- interoperability evidence.

Partially implemented work should use:

```text
- [ ] Capability name — partial: current limitation
```

Do not check off umbrella headings when only one sub-capability exists.

## Definition of done for eggress parity

eggress reaches full parity when it can serve as a practical replacement for `pproxy` across the documented protocol, chaining, routing, UDP, administrative, and platform workflows, with equivalent command-line usage for common invocations and published exceptions for any intentionally excluded insecure or unmaintainable legacy behavior.
