# eggress

A Rust-native, embeddable, multi-protocol proxy framework and CLI targeting practical and behavioral parity with Python `pproxy`.

> Status: Phase 36 final parity release audit complete. Frozen targets, final parity report, platform support matrix, migration guide, release notes, and go/no-go checklist are in `docs/release/`. CLI tier tightened (17 entries moved from `compatible` to `supported` because their evidence was synthetic); manifest validator extended to mechanically enforce category, platform constraints, intent-vs-evidence consistency, and reject file-path / CI-workflow test references.

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
- Structured diagnostics for unsupported protocols (SSH, Unix upstream)
- Differential tests verifying behavioral parity with Python `pproxy` (HTTP, SOCKS4/4a, SOCKS5, standalone UDP)
- Standalone UDP mode (`-ul`/`-ur` flags) properly classified as compatible in manifest
- Python bindings for translation helpers (`translate_pproxy_args`, `translate_pproxy_uri`)
- Python convenience API (`start_pproxy`, `EggressService.from_pproxy_args`)
- Python API parity specification with tier classification (Phase 29) — 424-line inventory covering 114 pproxy API entries across exports, protocols, ciphers, scheduling, lifecycle, and error surfaces

### Phase 28: CLI compatibility enhancements

- `eggress pproxy translate/check/run` subcommands translate pproxy CLI args to eggress TOML config
- Structured diagnostic codes for all CLI operations with `--json` flag support
- Exit code differentiation for process lifecycle and error handling (see `docs/cli/EXIT_CODES.md`)
- Full CLI flag inventory documenting parity with pproxy (`docs/cli/PPROXY_CLI_INVENTORY.md`)
- SSR and SSH URIs rejected with structured diagnostics (intentional non-parity)

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
- [x] PyPI package and wheels (Phase 15)
- [x] `pproxy`-compatible CLI shell
- [x] Typed URI parser
- [x] Multi-hop chain parser
- [x] Redacted configuration display
- [x] TCP listener
- [x] Unix-domain listener
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
- [x] Persistent HTTP forwarding
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
- [ ] SOCKS4 BIND (intentionally deferred: returns REP_COMMAND_NOT_SUPPORTED)

### SOCKS5

- [x] SOCKS5 CONNECT server
- [x] SOCKS5 CONNECT client
- [x] SOCKS5 no-auth
- [x] SOCKS5 username/password authentication
- [x] SOCKS5 IPv4 targets
- [x] SOCKS5 IPv6 targets
- [x] SOCKS5 domain targets
- [ ] SOCKS5 BIND (intentionally deferred: returns REP_COMMAND_NOT_SUPPORTED)
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
- [x] Standalone UDP relay (`mode = "standalone_pproxy_udp"`) — pproxy-compatible
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

- [x] Shadowsocks TCP client (standard SIP003 AEAD framing; wire-compatible with standard implementations; aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305)
- [x] Shadowsocks TCP server (inbound listener with standard SIP003 AEAD; single-hop upstream)
- [x] Shadowsocks UDP client (standard AEAD format: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305)
- [x] Shadowsocks UDP server (inbound Shadowsocks UDP association listener)
- [x] AEAD cipher support (individual encrypt/decrypt operations)
- [x] Modern default cipher suite
- [x] Legacy stream cipher diagnostics (rejected with clear error)
- [x] OTA intentionally excluded (intentional non-parity)
- [x] Password/key derivation compatibility
- [x] Interoperability with `shadowsocks-rust` (standard SIP003 AEAD framing)
- [x] Standard SIP003 AEAD framing (wire-compatible with standard implementations)

### ShadowsocksR (SSR) — Intentionally Unsupported

SSR URIs (`ssr://`) are recognized and rejected with clear diagnostics. SSR is a non-standard extension with no RFC; eggress does not implement SSR. See ADR at `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`.

- [ ] SSR client — not implemented; SSR URIs produce clear rejection
- [ ] SSR server — not implemented
- [ ] SSR UDP — not implemented
- [ ] `plain` — not implemented
- [ ] `origin` — not implemented
- [ ] `http_simple` — not implemented
- [ ] `tls1.2_ticket_auth` — not implemented
- [ ] `verify_simple` — not implemented
- [ ] `verify_deflate` — not implemented
- [ ] SSR compatibility feature gate — not implemented

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

- [x] WebSocket tunnel client (Phase 26, protocol-crate only)
- [x] WebSocket tunnel server (Phase 26, protocol-crate only)
- [x] WSS via rustls (Phase 26, protocol-crate only)
- [x] Binary-message byte-stream adapter (Phase 26, protocol-crate only)
- [x] Ping/pong handling (Phase 26, protocol-crate only)
- [x] Close and half-close mapping (Phase 26, protocol-crate only)
- [x] Fixed-target WebSocket tunnel (Phase 26, protocol-crate only)
- [x] WebSocket in proxy chains (Phase 26, protocol-crate only)

Note: `ws://`/`wss://` URIs are accepted by the URI parser and translator but are
**refused** as listener/upstream protocols through the runtime supervisor and
config compiler (Phase 25-28 hardening H5/H6/H7). Use as direct protocol-crate
callers only.

### Raw forwarding

- [x] Fixed-target TCP forwarding (Phase 26, protocol-crate only)
- [ ] Fixed-target UDP forwarding
- [x] Raw tunnel client (Phase 26, protocol-crate only)
- [x] Raw tunnel server (Phase 26, protocol-crate only)

Note: `raw://`/`tunnel://` URIs are accepted by the URI parser and translator
but are **refused** as listener/upstream protocols through the runtime
supervisor and config compiler (Phase 25-28 hardening H5/H6/H7). Use as direct
protocol-crate callers only.

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

- [x] HTTP/2 CONNECT server (Phase 26, protocol-crate only)
- [x] HTTP/2 CONNECT client (Phase 26, protocol-crate only)
- [x] Stream adapter (Phase 26, protocol-crate only)
- [x] Flow-control integration (Phase 26, protocol-crate only)
- [x] Stream reset propagation (Phase 26, protocol-crate only)
- [x] GOAWAY handling (Phase 26, protocol-crate only)
- [x] Upstream connection pooling (Phase 26, protocol-crate only)
- [x] H2-over-TLS ALPN (Phase 26, protocol-crate only)
- [x] H2 authentication (Phase 26, protocol-crate only)

Note: `h2://` URIs are accepted by the URI parser and translator but are
**refused** as listener/upstream protocols through the runtime supervisor and
config compiler (Phase 25-28 hardening H5/H6/H7). Use as direct protocol-crate
callers only.

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

- [x] Reverse acceptor (control channel + external listener)
- [x] Reverse control client (outbound dial + auto-reconnect)
- [x] Plaintext control-channel handshake (1-byte accept/reject)
- [x] Raw user:pass auth bytes
- [x] Exponential reconnect backoff (1s -> 30s cap)
- [x] ReverseMetrics struct (control-accepted/rejected, reconnects, streams)
- [x] pproxy URI translation (`socks5+in://`, `bind://`, `listen://`, `backward://`, `rebind://`)
- [x] TOML `[reverse_servers]` / `[reverse_clients]` config model
- [ ] Logical stream multiplexing (intentional -- pproxy uses one session per control channel)
- [ ] Built-in TLS for control channel (use stunnel or external TLS)
- [ ] Multi-channel concurrency (parallel `+in+in+in`)
- [ ] Jump chain composition on relayed streams
- [ ] Reverse UDP (intentional -- pproxy does not support UDP reverse)
- [x] Reverse listener access policy (allowlist of bind addresses)
- [x] Reverse integration into eggress-runtime supervisor (autonomous mode managed by ServiceSupervisor)
- [x] Reverse admin endpoints (reverse session listing in admin API)
- [ ] Python `pproxy` backward-mode compatibility (reverse URI translation only; no Python API yet)

### Transparent proxying

- [x] Linux `SO_ORIGINAL_DST`
- [ ] Linux IPv6 original destination
- [x] Linux REDIRECT workflow
- [ ] Linux TPROXY workflow
- [ ] Linux transparent bind
- [ ] macOS PF original-destination recovery
- [ ] PF integration tests
- [x] Startup capability checks

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
- [x] System-proxy configuration on macOS (inspection, dry-run apply commands)
- [x] System-proxy configuration on Windows (inspection, dry-run apply commands)
- [x] System-proxy state restoration (rollback state save/load)

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
- [x] Python package on PyPI (wheels for Linux/macOS/Windows)
- [x] PyPI release workflow (GitHub Actions)
- [x] PyPI release documentation
- [x] Python import strategy and packaging docs (Phase 32)
- [x] Python wheel smoke tests (Phase 32)

### pproxy compatibility

- [x] URI-mode command translation (`pproxy translate`)
- [x] CLI flag translation with warnings (`pproxy check`)
- [x] Differential tests against Python `pproxy` (gated)
- [x] Behavioral parity for common listener patterns
- [x] Complete URI option coverage (all pproxy flags)
- [x] Python pproxy translation helpers (`translate_pproxy_args`, `translate_pproxy_uri`)
- [x] Python convenience API (`start_pproxy`, `from_pproxy_args`)
- [x] Python async lifecycle (`astart`, `AsyncEggressHandle`)
- [x] Python pproxy compat tests (45 passing)
- [x] Python security/redaction tests
- [x] Python concurrency tests
- [x] Python packaging, import strategy, and distribution compatibility (Phase 32)
- [x] Structured diagnostics and exit codes (Phase 28) — `docs/cli/EXIT_CODES.md`, `docs/cli/PPROXY_CLI_INVENTORY.md`
- [x] `--json` flag for machine-readable pproxy check output (Phase 28)
- [x] CLI flag inventory: full parity documentation of translate/check/run subcommands (Phase 28)
- [x] Compatibility manifest tracking all parity features with evidence levels (`tests/compat/pproxy_manifest.toml`)
- Oracle process runner for real pproxy differential testing (`eggress-testkit`)
- Machine-readable parity reports generated after differential test runs

### Phase 1 limitations

- Unsupported transfer codings are rejected.
- TLS interception is not supported; HTTPS uses CONNECT tunneling.

> Persistent proxy connections and pipelining have been implemented since
> Phase 19 (`http_forward_persistent_connection` in the parity manifest;
> see `docs/COMPATIBILITY_EVIDENCE.md`). The bullet above is removed as of
> Phase 36.

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
- UDP is available on listeners with the `socks5` protocol or in standalone mode (`mode = "standalone_pproxy_udp"`).
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
- [Compatibility evidence](docs/COMPATIBILITY_EVIDENCE.md)
- [pproxy parity spec](docs/PPROXY_PARITY_SPEC.md)
- [pproxy migration](docs/PPROXY_MIGRATION.md)
- [Phase 7 pproxy parity spec](docs/PHASE_7_PPROXY_PARITY_SPEC_COMPLETION.md)
- [Failure semantics](docs/FAILURE_SEMANTICS.md)
- [Phase 36 final parity release audit](docs/release/) — frozen targets, final parity report, platform support matrix, migration guide, release notes, go/no-go checklist.
- [Phase 12 scheduler/chain/failure parity](docs/PHASE_12_SCHEDULER_CHAIN_FAILURE_PARITY_COMPLETION.md)
- [Python bindings](docs/PYTHON_BINDINGS.md)
- [Phase 16 Python pproxy library parity](docs/PHASE_16_PYTHON_PPROXY_LIBRARY_PARITY_COMPLETION.md)
- [Phase 17 true pproxy parity release candidate](docs/PHASE_17_TRUE_PPROXY_PARITY_RELEASE_CANDIDATE_COMPLETION.md)
- [Phase 17 RC polish](docs/PHASE_17_RC_POLISH_COMPLETION.md)
- [True pproxy parity release candidate](docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md)
- [Phase 18 pproxy oracle and evidence harness](plans/PHASE_18_PPROXY_ORACLE_AND_EVIDENCE_HARNESS.md)
- [Phase 19 HTTP/SOCKS baseline closure](docs/PHASE_19_HTTP_SOCKS_BASELINE_CLOSURE_COMPLETION.md)
- [Phase 25-28 hardening pass](docs/PHASE_25_28_HARDENING_COMPLETION.md)
- [Performance testing](docs/performance/README.md)
- [Benchmark inventory](docs/performance/BENCHMARK_INVENTORY.md)
- [Regression gate policy](docs/performance/REGRESSION_GATE_POLICY.md)
- [PyPI release procedure](docs/PYPI_RELEASE.md)
- [Wheel artifact audit](docs/WHEEL_AUDIT.md)
- [Import strategy](docs/python/IMPORT_STRATEGY.md)
- [Installation guide](docs/python/INSTALLATION.md)
- [Migration from pproxy](docs/python/MIGRATION_FROM_PPROXY.md)
- [Python packaging](docs/python/PACKAGING.md)
- [Release checklist](docs/python/RELEASE_CHECKLIST.md)
- [Python import/distribution ADR](docs/adr/ADR_python_import_and_distribution_strategy.md)

## Status discipline

README boxes are changed only in the same pull request that adds the implementation, tests, and documentation. Partial capabilities remain unchecked and describe the current limitation.
