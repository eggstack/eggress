# Capability Status

A capability is checked only when implementation, tests, documentation, and applicable interoperability tests are complete.

Legend: `[x]` complete, `[ ]` not complete.

## Core

- [x] Rust workspace and CI
- [x] Embeddable library API (`eggress-embed`)
- [x] Python bindings (PyO3)
- [x] PyPI package and wheels
- [x] `pproxy`-compatible CLI shell
- [x] Typed URI parser
- [x] Multi-hop chain parser
- [x] Redacted configuration display
- [x] TCP listener
- [x] Unix-domain listener
- [x] Direct TCP connector
- [x] Native `OutboundConnector` — `from_toml()`, `from_pproxy_uri()`, `connect_tcp()`, `connect_tcp_timeout()`; Python sync/async native stream wrappers
- [x] Replayable protocol sniff buffer
- [x] Mixed inbound protocol autodetection
- [x] Half-close-aware bidirectional relay
- [x] Graceful shutdown (drain-first, cancel-after-deadline)
- [x] Connection limits
- [x] Handshake limits and timeouts

## HTTP/1

- [x] HTTP CONNECT server and client
- [x] Single-exchange ordinary HTTP forward-proxy server
- [x] Absolute-form to origin-form rewriting
- [x] HTTP proxy Basic authentication
- [x] Persistent HTTP forwarding
- [x] Hop-by-hop request-header filtering
- [x] HTTP upstream chaining
- [x] Content-Length and chunked request bodies
- [x] Deferred CONNECT success reply

## SOCKS4

- [x] SOCKS4 CONNECT server and client
- [x] SOCKS4 user ID
- [x] SOCKS4a domain targets
- [x] SOCKS4 BIND refusal (pproxy 2.7.9 does not implement BIND)

## SOCKS5

- [x] SOCKS5 CONNECT server and client
- [x] SOCKS5 no-auth and username/password authentication
- [x] SOCKS5 IPv4, IPv6, and domain targets
- [x] SOCKS5 BIND refusal (pproxy 2.7.9 does not implement BIND)
- [x] SOCKS5 UDP ASSOCIATE server and client

## Routing and scheduling

- [x] Direct routes and ordered upstream routes
- [x] Regex, exact-host, domain-suffix, CIDR, port, and reject rules
- [x] First-available, round-robin, random, and least-connections scheduling
- [x] Active health checking with hysteresis
- [x] Direct fallback
- [x] Route explanation command

## Proxy chaining

- [x] HTTP, SOCKS4a, SOCKS5, Shadowsocks -> destination
- [x] Cross-protocol chains (HTTP<->SOCKS5, HTTP->HTTP, SOCKS5->SOCKS5)
- [x] Three-or-more-hop TCP chains
- [x] Per-hop timeout and diagnostics
- [x] Chain capability validation

## Upstream protocol support

| Upstream | TCP CONNECT | UDP relay |
|----------|------------|-----------|
| Direct | yes | yes |
| HTTP CONNECT | yes | no |
| SOCKS4/SOCKS4a | yes | no |
| SOCKS5 | yes | one-hop and composed UDP-capable chains |
| Shadowsocks | yes (aes-128-gcm, aes-192-gcm, aes-256-gcm, chacha20-ietf-poly1305) | yes (standard AEAD upstream; pproxy PacketCipher standalone inbound) |
| Trojan | yes (rustls) | no |

## UDP

- [x] Direct UDP, UDP association table, per-client and global limits
- [x] Association idle timeout and target-flow idle cleanup
- [x] Target-aware reply demultiplexing
- [x] UDP routing with direct-fallback support
- [x] Packet-size and amplification limits
- [x] Per-listener TOML UDP configuration
- [x] SOCKS5 UDP ASSOCIATE server
- [x] UDP through one-hop SOCKS5 upstream
- [x] UDP through one-hop Shadowsocks upstream (standard AEAD)
- [x] Standalone UDP relay (`mode = "standalone_pproxy_udp"`)
- [ ] UDP through Trojan upstream
- [x] UDP through composed SOCKS5/Shadowsocks hop chains, with reverse-order response decoding
- [ ] UDP through HTTP/MASQUE/CONNECT-UDP

## TLS

- [x] rustls client and server transport
- [x] System root certificates and custom CA roots
- [x] SNI, ALPN (configurable)
- [x] Secure certificate verification default
- [x] Explicit insecure compatibility mode
- [x] HTTPS proxy server and client
- [x] TLS-wrapped SOCKS and custom protocols
- [ ] Certificate reload (deferred)

## Shadowsocks

- [x] TCP client and server (standard SIP003 AEAD framing)
- [x] UDP client and server (standard upstream AEAD plus pproxy-compatible standalone inbound format)
- [x] AEAD cipher support (aes-128-gcm, aes-192-gcm, aes-256-gcm, chacha20-ietf-poly1305)
- [x] Legacy stream ciphers and OTA behind the opt-in `legacy-crypto` feature
      (unauthenticated compatibility path with warnings and fail-closed HMAC)
- [x] Interoperability with `shadowsocks-rust`
- [x] pproxy 2.7.9 SSR address framing with IPv4, IPv6, domain, and optional auth prefix
- [x] Bounded pproxy SSR plugin codecs behind the opt-in `pproxy-legacy`
      feature (default CLI `full` no longer enables it; build with
      `--features pproxy-legacy` to opt in)
- [x] Ordered `plain`, `origin`, `http_simple`, `tls1.2_ticket_auth`, `verify_simple`, and `verify_deflate` plugin names

## Trojan

- [x] Trojan client, server, and authentication
- [x] Trojan TCP target framing
- [x] Domain length validation (1-255 bytes)
- [ ] Trojan fallback routing

## WebSocket

- [x] WebSocket tunnel client and server
- [x] WSS via rustls
- [x] Binary-message byte-stream adapter
- [x] Ping/pong handling, close and half-close mapping
- [x] Fixed-target WebSocket tunnel
- [x] WebSocket in proxy chains
- [x] Stream-native composition — WS handshake over prior-hop stream
- [x] Compatibility WS/WSS fixed-target listeners

## Raw forwarding

- [x] Fixed-target TCP forwarding
- [x] Raw tunnel client and server
- [x] Stream-native composition — raw passthrough over prior-hop stream
- [x] Bounded fixed-target UDP forwarding (explicit compatibility listener mode)

## HTTP/2

- [x] HTTP/2 CONNECT server and client
- [x] Stream adapter, flow-control integration, stream reset propagation
- [x] GOAWAY handling, upstream connection pooling
- [x] H2-over-TLS ALPN, H2 authentication
- [x] Stream-native composition — H2 handshake over prior-hop stream
- [x] Compatibility H2 CONNECT listener with per-stream routing

## Reverse and backward proxying

- [x] Reverse acceptor (control channel + external listener)
- [x] Reverse control client with auto-reconnect
- [x] Plaintext control-channel handshake
- [x] pproxy URI translation (`socks5+in://`, `bind://`, `listen://`, `backward://`, `rebind://`)
- [x] TOML `[reverse_servers]` / `[reverse_clients]` config model
- [x] Reverse listener access policy (allowlist)
- [x] Reverse admin endpoints
- [x] Real pproxy 2.7.9 oracle payload evidence for raw and SOCKS5 `+in`
      backward compositions via the gated `reverse_interop` tests
- [ ] Built-in TLS for control channel (use stunnel or external TLS)
- [ ] Reverse/backward TLS composition (unsupported; reuse the native
      access-policy and listener topology)
- [ ] Reverse UDP (intentional — pproxy does not support UDP reverse)

## Transparent proxying

- [x] Linux `SO_ORIGINAL_DST` and REDIRECT workflow
- [x] Startup capability checks
- [ ] Linux IPv6 original destination
- [ ] Linux TPROXY workflow
- [ ] macOS PF original-destination recovery (requires a future maintained safe
      `/dev/pf` wrapper; external `pfctl` setup remains the workaround)

## Administration and operations

- [x] TOML configuration with validation
- [x] Configuration reload (routing/upstreams/groups hot-swapped; listener topology requires restart)
- [x] Structured logs (human-readable and JSON)
- [x] Secret redaction for URIs, authentication, and runtime logs
- [x] Traffic counters, per-upstream metrics, Prometheus endpoint
- [x] Local admin API, PAC generation and serving, static HTTP endpoint
- [x] Upstream test command
- [x] System-proxy configuration on macOS and Windows

## Security and robustness

- [x] Bounded parsers and replay buffer
- [x] Connection semaphore
- [x] DNS rebinding-aware routing
- [x] Unsafe-code audit (`deny` level)
- [x] Dependency audit in CI (bans openssl-sys, native-tls, aws-lc-sys, cmake)
- [x] Property tests, fuzz harness smoke, security invariant tests
- [x] Soak tests, resource-exhaustion tests
- [x] Security disclosure process

## Packaging

- [x] Linux binaries (x86_64, aarch64), macOS binaries (x86_64, arm64), Windows binaries (x86_64)
- [x] Python package on PyPI (wheels for Linux/macOS/Windows)
- [ ] Crates.io packages (blocked: CLI depends on internal crates)
- [ ] Reproducible builds
