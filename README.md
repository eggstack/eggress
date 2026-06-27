# eggress

A Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical and behavioral parity with Python `pproxy`.

> Status: Phase 14 complete — Python bindings via PyO3 wrapping `eggress-embed` with `EggressConfig`, `EggressService`, `EggressHandle`, exception hierarchy, GIL release, context manager support, and 14 passing tests. Phase 13 complete — Rust embed API stabilization with `eggress-embed` crate providing stable blocking/async start, bound address discovery, metrics/status, reload, error redaction, and comprehensive integration tests. Phase 12 complete — scheduler parity audit, multi-hop TCP chain tests, failure semantics documentation, retry/fallback behavior tests, observability semantics tests, and differential test extensions. Phase 11 complete — remaining protocol parity: all pproxy protocols classified, lightweight aliases (socks4a, https), Shadowsocks upstream fully supported, unsupported feature diagnostics, and comprehensive documentation. Phase 8 complete — pproxy compatibility CLI with URI-mode command translation, configuration migration support, and differential tests against Python `pproxy`. Phase 7 complete — pproxy parity specification with compatibility tier taxonomy, expanded parity matrix, refactored differential test harness with reusable primitives, and black-box probe tests. Phase 6 complete — hardening: property tests for codecs/parsers (proptest), fuzz harness smoke foundation, runtime lifecycle invariant tests, observability/metrics/admin tests, security invariant tests, pproxy differential tests (gated), benchmarks (criterion) and load tests, deny.toml explicit dependency bans, and comprehensive documentation (CI_STATUS, SECURITY_REVIEW, PARITY_MATRIX, CONFIG_REFERENCE, METRICS, OPERATIONS, TESTING, RELEASE_READINESS). Phase 5 complete — broader upstream protocol parity with HTTP CONNECT upstream polish, SOCKS4/SOCKS4a upstream polish, Shadowsocks TCP/UDP foundation (AEAD methods), Trojan TCP foundation, and upstream capability classification. Phase 4 complete — one-hop SOCKS5 UDP upstream relay with capability classification, flow model, upstream metrics, and integration tests. Phase 3 complete — UDP foundation with SOCKS5 UDP ASSOCIATE, direct forwarding, association lifecycle management, idle timeout, target-flow reaping, per-listener TOML configuration, task tracking, metrics bridging, routing fallback, and admin visibility. Phase 2 complete — policy-driven routing with rule engine, upstream groups, health-aware scheduling, TOML configuration, metrics, admin API, PAC/static serving, scoped atomic reload, route explanation (including source- and identity-based rules), runtime supervisor with fallible startup, and integration tests covering startup, routing, health, admin, reload, shutdown, PAC/static, and bind-conflict paths.

eggress will preserve the compact URI-driven workflow of `pproxy` while using explicit Rust abstractions for listeners, application proxy protocols, transport wrappers, routing, proxy chains, UDP associations, and platform integration.

## Design goals

- nearly identical common CLI usage to `pproxy`;
- mixed-protocol listeners;
- arbitrary compatible multi-hop proxy chains;
- TCP and UDP;
- secure defaults with explicit legacy compatibility;
- embeddable Rust library;
- resource-bounded hostile-input handling;
- pure Rust dependencies wherever practical;
- differential interoperability tests against Python `pproxy`;
- Linux, macOS, and Windows support where the underlying capability exists.

## pproxy compatibility

The `eggress-pproxy-compat` crate provides:

- URI-mode command translation from `pproxy` to `eggress` syntax (including `socks4a`, `https`, `direct`, `ss` scheme aliases)
- CLI flag translation with structured warnings for unsupported features
- Structured diagnostics for unsupported protocols (SSH, Unix, redir)
- Differential tests verifying behavioral parity with Python `pproxy`

## Usage

```text
eggress
eggress -l http://:8080
eggress -l socks4://:1080
eggress -l socks5://:1080
eggress -l http+socks4+socks5://:8080
eggress -l http+socks5://user:pass@:8080
eggress -r http://proxy.example:8080
eggress -r socks5://proxy.example:1080
eggress -r socks5://hop1:1080__http://hop2:8080
eggress pproxy translate -- -l http://:8080 -r socks5://proxy:1080
eggress pproxy check -- -l socks5://:1080 -r http://proxy:8080
eggress pproxy run -- -l socks5://:1080 -r http://proxy:8080
```

## Capability status

A capability is checked only when implementation, tests, documentation, and applicable interoperability tests are complete.

Legend:

- `[x]` complete;
- `[ ]` not complete;
- partial work remains unchecked and includes a note.

### Core

- [x] Rust workspace and CI
- [x] Embeddable library API (eggress-server crate)
- [x] `eggress-embed` library API
- [x] Python bindings (PyO3)
- [x] `pproxy`-compatible CLI shell
- [x] Typed URI parser
- [x] Multi-hop chain parser
- [x] Redacted configuration display
- [x] TCP listener
- [ ] Unix-domain listener
- [x] Direct TCP connector
- [x] Replayable protocol sniff buffer
- [x] Mixed inbound protocol autodetection
- [x] Half-close-aware bidirectional relay
- [x] Graceful shutdown (drain-first, cancel-after-deadline)
- [x] Connection limits
- [x] Handshake limits and timeouts

### HTTP/1

- [x] HTTP CONNECT server
- [x] HTTP CONNECT client
- [x] Single-exchange ordinary HTTP forward-proxy server
- [x] Absolute-form to origin-form rewriting
- [x] HTTP proxy Basic authentication
- [ ] Persistent HTTP forwarding
- [x] Hop-by-hop request-header filtering
- [x] HTTP upstream chaining
- [x] Content-Length request bodies
- [x] Chunked request bodies
- [x] Deferred CONNECT success reply

### SOCKS4

- [x] SOCKS4 CONNECT server
- [x] SOCKS4 CONNECT client
- [x] SOCKS4 user ID
- [x] SOCKS4a domain targets
- [ ] SOCKS4 BIND

### SOCKS5

- [x] SOCKS5 CONNECT server
- [x] SOCKS5 CONNECT client
- [x] SOCKS5 no-auth
- [x] SOCKS5 username/password authentication
- [x] SOCKS5 IPv4 targets
- [x] SOCKS5 IPv6 targets
- [x] SOCKS5 domain targets
- [ ] SOCKS5 BIND
- [x] SOCKS5 UDP ASSOCIATE server
- [x] SOCKS5 UDP ASSOCIATE client

### Routing and scheduling

- [x] Direct routes
- [x] Ordered upstream routes
- [x] Regex compatibility rules
- [x] Exact-host rules
- [x] Domain-suffix rules
- [x] CIDR rules
- [x] Port rules
- [x] Reject rules
- [x] First-available scheduling
- [x] Round-robin scheduling
- [x] Random scheduling
- [x] Least-connections scheduling
- [x] Active health checking (with health config from TOML)
- [x] Health hysteresis
- [x] Direct fallback (with SelectionReason::DirectFallback)
- [x] Route explanation command
- [x] Scheduler parity audit (Phase 12)
- [x] Multi-hop TCP chain tests (Phase 12)
- [x] Failure semantics documentation (Phase 12)
- [x] Retry/fallback behavior tests (Phase 12)

### Proxy chaining

- [x] HTTP → destination
- [x] SOCKS4a → destination
- [x] SOCKS5 → destination
- [x] HTTP → SOCKS5
- [x] SOCKS5 → HTTP
- [x] HTTP → HTTP
- [x] SOCKS5 → SOCKS5
- [x] Shadowsocks → destination
- [x] Three-or-more-hop TCP chains
- [x] Per-hop timeout and diagnostics
- [x] Chain capability validation

### Upstream protocol capability matrix

| Upstream protocol | TCP CONNECT | UDP relay | Phase |
|---|---|---|---|
| Direct | yes | yes | 3 |
| HTTP CONNECT | yes | no | 5 |
| SOCKS4/SOCKS4a | yes | no | 5 |
| SOCKS5 | yes | one-hop yes | 4 |
| Shadowsocks | yes (aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305) | yes (standard AEAD format) | 5/10 |
| Trojan | TCP yes (rustls) | no | 5 |

### UDP

- [x] Direct UDP
- [x] UDP association table
- [x] Per-client association limits
- [x] Global association limits
- [x] Association idle timeout (enforced in relay loop)
- [x] Target-flow idle cleanup (enforced in relay loop)
- [x] Target-aware reply demultiplexing
- [x] UDP routing with direct-fallback support
- [x] UDP relay tasks tracked via TaskTracker
- [x] UDP chain validation
- [x] UDP metrics (exposed via `/metrics`)
- [x] Packet-size and amplification limits
- [x] Per-listener TOML UDP configuration (`[listeners.udp]`)
- [x] Configurable relay bind and advertise address per listener
- [x] Association registry cleanup on close
- [x] SOCKS5 UDP ASSOCIATE server
- [x] Direct UDP forwarding
- [x] UDP through one-hop SOCKS5 upstream
- [x] UDP through one-hop Shadowsocks upstream (standard AEAD format)
- [ ] UDP through Trojan upstream
- [ ] UDP through multi-hop proxy chains
- [ ] UDP through HTTP/MASQUE/CONNECT-UDP

### TLS

- [x] rustls client transport (Trojan)
- [x] rustls server transport (TLS listener accept)
- [x] System root certificates (webpki-roots)
- [x] Custom CA roots (TlsClientConfigBuilder)
- [x] SNI (client-side via TlsConnector)
- [x] ALPN (configurable via builder)
- [x] Secure certificate verification default (rustls default)
- [x] Explicit insecure compatibility mode (TlsClientConfigBuilder::with_insecure)
- [ ] Certificate reload (deferred)
- [x] HTTPS proxy server (TLS listener + HTTP protocol)
- [x] HTTPS proxy client (TLS upstream wrapping)
- [x] TLS-wrapped SOCKS (hop.tls flag)
- [x] TLS-wrapped custom protocols (hop.tls flag)

### Shadowsocks

- [!] Shadowsocks TCP client (non-standard AEAD framing; not wire-compatible with standard implementations; aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305)
- [ ] Shadowsocks TCP server
- [x] Shadowsocks UDP client (standard AEAD format: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305)
- [ ] Shadowsocks UDP server
- [x] AEAD cipher support (individual encrypt/decrypt operations)
- [x] Modern default cipher suite
- [ ] Legacy stream cipher compatibility
- [ ] OTA compatibility
- [x] Password/key derivation compatibility
- [ ] Interoperability with `shadowsocks-rust` (TCP framing is non-standard; UDP is standard)
- [!] Interoperability with Python `pproxy` (synthetic-tested only; not wire-compatible for TCP)

### ShadowsocksR

- [ ] SSR client
- [ ] SSR server
- [ ] SSR UDP
- [ ] `plain`
- [ ] `origin`
- [ ] `http_simple`
- [ ] `tls1.2_ticket_auth`
- [ ] `verify_simple`
- [ ] `verify_deflate`
- [ ] SSR compatibility feature gate

### Trojan

- [x] Trojan client
- [ ] Trojan server
- [x] Trojan authentication
- [x] Trojan TCP target framing
- [x] Domain length validation (1-255 bytes) through `encode_trojan_request()`
- [x] Synthetic TLS happy-path test exercises `trojan_connect()` directly
  (server-observed request bytes asserted)
- [ ] Trojan fallback routing
- [ ] Trojan interoperability tests

### WebSocket

- [ ] WebSocket tunnel client
- [ ] WebSocket tunnel server
- [ ] WSS via rustls
- [ ] Binary-message byte-stream adapter
- [ ] Ping/pong handling
- [ ] Close and half-close mapping
- [ ] Fixed-target WebSocket tunnel
- [ ] WebSocket in proxy chains

### Raw forwarding

- [ ] Fixed-target TCP forwarding
- [ ] Fixed-target UDP forwarding
- [ ] Raw tunnel client
- [ ] Raw tunnel server

### SSH

- [ ] SSH client transport
- [ ] Password authentication
- [ ] Public-key authentication
- [ ] Encrypted private keys
- [ ] Host-key verification
- [ ] SSH agent support
- [ ] `direct-tcpip`
- [ ] Connection pooling
- [ ] Keepalives
- [ ] Reconnect
- [ ] SSH through prior proxy hops

### HTTP/2

- [ ] HTTP/2 CONNECT server
- [ ] HTTP/2 CONNECT client
- [ ] Stream adapter
- [ ] Flow-control integration
- [ ] Stream reset propagation
- [ ] GOAWAY handling
- [ ] Upstream connection pooling
- [ ] H2-over-TLS ALPN
- [ ] H2 authentication

### QUIC

- [ ] QUIC client transport
- [ ] QUIC server transport
- [ ] Versioned tunnel framing
- [ ] Multiplexed streams
- [ ] QUIC datagrams
- [ ] Authentication
- [ ] Connection reuse
- [ ] Python `pproxy` QUIC compatibility
- [ ] Route validation for unsupported chain combinations

### HTTP/3

- [ ] HTTP/3 CONNECT server
- [ ] HTTP/3 CONNECT client
- [ ] H3 stream adapter
- [ ] H3 authentication
- [ ] H3 connection pooling
- [ ] HTTP/3 interoperability tests

### Reverse and backward proxying

- [ ] Reverse endpoint registration
- [ ] Authenticated control channel
- [ ] Logical stream multiplexing
- [ ] Heartbeats
- [ ] Reconnect
- [ ] Re-registration
- [ ] Graceful draining
- [ ] Reverse listener policy
- [ ] Reverse UDP
- [ ] Python `pproxy` backward-mode compatibility

### Transparent proxying

- [ ] Linux `SO_ORIGINAL_DST`
- [ ] Linux IPv6 original destination
- [ ] Linux REDIRECT workflow
- [ ] Linux TPROXY workflow
- [ ] Linux transparent bind
- [ ] macOS PF original-destination recovery
- [ ] PF integration tests
- [ ] Startup capability checks

### Administration and operations

- [x] TOML configuration
- [x] Configuration validation
- [x] Configuration reload (with explicit scope: routing/upstreams/groups, not listener topology)
- [x] Human-readable structured logs
- [x] JSON logs
- [x] Secret redaction for URIs, authentication, and runtime logs
- [x] Traffic counters for TCP relay and HTTP forward sessions
- [x] Per-upstream metrics
- [x] Prometheus endpoint
- [x] Local admin API
- [x] PAC generation
- [x] PAC serving
- [x] Static HTTP endpoint
- [x] Upstream test command
- [ ] System-proxy configuration on macOS
- [ ] System-proxy configuration on Windows
- [ ] System-proxy state restoration

### Security and robustness

- [x] Bounded parsers
- [x] Bounded replay buffer
- [x] Connection semaphore
- [ ] Per-source limits
- [ ] Authentication failure rate limiting
- [ ] Proxy-loop detection
- [ ] Private-network egress policy
- [ ] DNS policy
- [ ] DNS rebinding-aware routing
- [ ] Secret zeroization where practical
- [ ] Unsafe-code audit
- [x] Dependency audit in CI (deny.toml with explicit bans: openssl-sys, native-tls, aws-lc-sys, cmake)
- [x] Property tests for codecs/parsers
- [x] Fuzz harness smoke foundation
- [x] Security invariant tests
- [ ] Fuzzing corpus (seed corpus)
- [ ] Long-running soak tests
- [ ] Resource-exhaustion tests
- [ ] Security disclosure process

### Packaging

- [ ] Linux binaries
- [ ] macOS binaries
- [ ] Windows binaries
- [ ] Static or minimally dynamic builds where practical
- [ ] Container image
- [ ] Reproducible builds
- [ ] Signed release artifacts
- [ ] SBOM
- [ ] Crates.io packages
- [ ] Migration guide from Python `pproxy`

### pproxy compatibility

- [x] URI-mode command translation (`pproxy translate`)
- [x] CLI flag translation with warnings (`pproxy check`)
- [x] Differential tests against Python `pproxy` (gated)
- [x] Behavioral parity for common listener patterns
- [x] Complete URI option coverage (all pproxy flags)

### Phase 1 limitations

- One ordinary HTTP request is processed per client connection.
- Persistent proxy connections and pipelining are not yet supported.
- Unsupported transfer codings are rejected.
- TLS interception is not supported; HTTPS uses CONNECT tunneling.

### Phase 2 operational limitations

- Listener topology changes (count, names, bind addresses) require restart; only routing, upstreams, health config, and admin content are hot-reloadable.
- All other runtime state — router, upstream groups, health probes, PAC, static content, route-explain generation — is reloaded atomically on SIGHUP without dropping connections.

### Phase 3 UDP operational limitations

- UDP relay through HTTP, SOCKS4, and multi-hop upstream proxies is not supported; one-hop SOCKS5 upstream is supported.
- No QUIC, HTTP/3, MASQUE, or CONNECT-UDP transport.
- No transparent UDP proxying.
- No UDP fragmentation/reassembly (nonzero FRAG is rejected).
- UDP bind address changes require a restart.
- UDP limit changes apply only to new associations after reload.
- UDP is only available on listeners with the `socks5` protocol.
- No UDP chain validation (UDP cannot traverse multi-hop proxy chains).

## Dependency policy

eggress prefers pure Rust dependencies where mature implementations exist.

Preferred foundations include:

- Tokio for asynchronous I/O;
- rustls for TLS;
- Quinn for QUIC;
- Hyper/H2/H3 for HTTP transports;
- RustCrypto primitives;
- `russh` for SSH where it satisfies interoperability;
- reusable Rust crates from `shadowsocks-rust`;
- pure Rust parsers and codecs.

Native dependencies and platform FFI are reserved for operating-system facilities such as transparent proxying and system-proxy configuration.

Dependency hygiene is enforced via `deny.toml` at the workspace root. CI runs `cargo deny check` to block banned crates (openssl-sys, native-tls, aws-lc-sys, cmake) and audit advisories.

## Documentation

- [Full roadmap](docs/ROADMAP.md)
- [Architecture](docs/ARCHITECTURE.md)
- [URI grammar](docs/URI_GRAMMAR.md)
- [Phase 2 completion](docs/PHASE_2_COMPLETION.md)
- [Phase 3 completion](docs/PHASE_3_COMPLETION.md)
- [Phase 4 UDP upstream relay](docs/PHASE_4_UDP_UPSTREAM_RELAY_COMPLETION.md)
- [Phase 5 upstream protocol parity](docs/PHASE_5_UPSTREAM_PROTOCOL_PARITY_COMPLETION.md)
- [Testing](docs/TESTING.md)
- [Security review](docs/SECURITY_REVIEW.md)
- [Parity matrix](docs/PARITY_MATRIX.md)
- [Config reference](docs/CONFIG_REFERENCE.md)
- [Metrics](docs/METRICS.md)
- [Operations](docs/OPERATIONS.md)
- [Release readiness](docs/RELEASE_READINESS.md)
- [CI status](docs/CI_STATUS.md)
- [Protocol: HTTP CONNECT](docs/protocols/HTTP_CONNECT.md)
- [Protocol: SOCKS4](docs/protocols/SOCKS4.md)
- [Protocol: Shadowsocks](docs/protocols/SHADOWSOCKS.md)
- [Protocol: Trojan](docs/protocols/TROJAN.md)
- [pproxy parity spec](docs/PPROXY_PARITY_SPEC.md)
- [pproxy migration](docs/PPROXY_MIGRATION.md)
- [Phase 7 pproxy parity spec](docs/PHASE_7_PPROXY_PARITY_SPEC_COMPLETION.md)
- [Failure semantics](docs/FAILURE_SEMANTICS.md)
- [Phase 12 scheduler/chain/failure parity](docs/PHASE_12_SCHEDULER_CHAIN_FAILURE_PARITY_COMPLETION.md)

## Status discipline

README boxes are changed only in the same pull request that adds the implementation, tests, and documentation. Partial capabilities remain unchecked and describe the current limitation.
