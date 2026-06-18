# eggress

A Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical and behavioral parity with Python `pproxy`.

> Status: Phase 1 complete — externally interoperable core TCP proxy with mixed HTTP/SOCKS listeners, ordinary HTTP forwarding, and HTTP/SOCKS chaining.

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
- [x] Graceful shutdown
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
- [ ] SOCKS5 UDP ASSOCIATE server
- [ ] SOCKS5 UDP ASSOCIATE client

### Routing and scheduling

- [x] Direct routes
- [x] Ordered upstream routes
- [ ] Regex compatibility rules (and all other rule types)
- [ ] Exact-host rules
- [ ] Domain-suffix rules
- [ ] CIDR rules
- [ ] Port rules
- [ ] Reject rules
- [x] First-available scheduling
- [ ] Round-robin scheduling
- [ ] Random scheduling
- [ ] Least-connections scheduling
- [ ] Active health checking
- [ ] Health hysteresis
- [x] Direct fallback
- [ ] Route explanation command

### Proxy chaining

- [x] HTTP → destination
- [x] SOCKS4a → destination
- [x] SOCKS5 → destination
- [x] HTTP → SOCKS5
- [x] SOCKS5 → HTTP
- [x] HTTP → HTTP
- [x] SOCKS5 → SOCKS5
- [x] Three-or-more-hop TCP chains
- [x] Per-hop timeout and diagnostics
- [x] Chain capability validation

### UDP

- [ ] Direct UDP
- [ ] UDP association table
- [ ] Per-client association limits
- [ ] Global association limits
- [ ] Idle expiry
- [ ] Target-aware reply demultiplexing
- [ ] UDP upstream routing
- [ ] UDP chain validation
- [ ] UDP metrics
- [ ] Packet-size and amplification limits

### TLS

- [ ] rustls client transport
- [ ] rustls server transport
- [ ] System root certificates
- [ ] Custom CA roots
- [ ] SNI
- [ ] ALPN
- [ ] Secure certificate verification default
- [ ] Explicit insecure compatibility mode
- [ ] Certificate reload
- [ ] HTTPS proxy server
- [ ] HTTPS proxy client
- [ ] TLS-wrapped SOCKS
- [ ] TLS-wrapped custom protocols

### Shadowsocks

- [ ] Shadowsocks TCP client
- [ ] Shadowsocks TCP server
- [ ] Shadowsocks UDP client
- [ ] Shadowsocks UDP server
- [ ] AEAD cipher support
- [ ] Modern default cipher suite
- [ ] Legacy stream cipher compatibility
- [ ] OTA compatibility
- [ ] Password/key derivation compatibility
- [ ] Interoperability with `shadowsocks-rust`
- [ ] Interoperability with Python `pproxy`

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

- [ ] Trojan client
- [ ] Trojan server
- [ ] Trojan authentication
- [ ] Trojan TCP target framing
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

- [ ] TOML configuration
- [ ] Configuration validation
- [ ] Configuration reload
- [x] Human-readable structured logs
- [ ] JSON logs
- [x] Secret redaction for URIs, authentication, and runtime logs
- [x] Traffic counters for TCP relay and HTTP forward sessions
- [ ] Per-upstream metrics
- [ ] Prometheus endpoint
- [ ] Local admin API
- [ ] PAC generation
- [ ] PAC serving
- [ ] Static HTTP endpoint
- [ ] Upstream test command
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
- [x] Dependency audit in CI
- [ ] Fuzzing corpus
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

### Phase 1 limitations

- One ordinary HTTP request is processed per client connection.
- Persistent proxy connections and pipelining are not yet supported.
- Unsupported transfer codings are rejected.
- TLS interception is not supported; HTTPS uses CONNECT tunneling.

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

## Documentation

- [Full roadmap](docs/ROADMAP.md)
- [Phase 1 plan](docs/PHASE_1_PLAN.md)

## Status discipline

README boxes are changed only in the same pull request that adds the implementation, tests, and documentation. Partial capabilities remain unchecked and describe the current limitation.
