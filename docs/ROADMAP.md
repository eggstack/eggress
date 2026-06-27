# Eggress Roadmap

This document references the main roadmap in [EGGRESS_ROADMAP.md](../EGGRESS_ROADMAP.md).

## Current Phase

Phase 13: Rust embed API stabilization (complete)

## Completed Milestones

### Phase 1: Core TCP proxy foundation

- [x] 1.1: Repository and compatibility skeleton
- [x] 1.2: URI grammar and validation
- [x] 1.3: Core stream and relay
- [x] 1.4: Replay stream and protocol dispatch
- [x] 1.5: SOCKS4/SOCKS4a
- [x] 1.6: SOCKS5 CONNECT
- [x] 1.7: HTTP CONNECT
- [x] 1.8: Ordinary HTTP forwarding
- [x] 1.9: Chain executor
- [x] 1.10: CLI integration
- [x] 1.11: Corrective closure — session model, deferred replies, body framing, header filtering, external interop

### Phase 2: Routing, health, and operations — complete

- [x] 2.1: Routing rule engine (matchers, first-match-wins, route explanation)
- [x] 2.2: Upstream groups and schedulers (first-available, round-robin, random, least-connections)
- [x] 2.3: Server routing integration (RouteService trait, protocol-correct rejects, connect timeout)
- [x] 2.4: Health management (state machine, TCP probes, hysteresis, eligibility)
- [x] 2.5: TOML configuration (validation, secret sources, CLI compatibility)
- [x] 2.6: Metrics and JSON logging (Prometheus registry, bounded cardinality)
- [x] 2.7: Admin API, PAC, and static content (health, status, metrics, PAC serving)
- [x] 2.8: Reload and graceful shutdown (ArcSwap, SIGHUP, drain timeout)
- [x] 2.9: Route explanation and upstream test command (human/JSON output, reachability testing)
- [x] 2.10: Phase closure (README, ARCHITECTURE, AGENTS.md updates)
- [x] 2.11: Corrective integration — scheduler persistence, lease lifecycle, group fallback, connect timeout, context propagation, stable IDs, runtime supervisor, metrics trait, expanded TOML matchers
- [x] 2.12: Shared runtime snapshot and upstream registry
- [x] 2.13: Health configuration from TOML
- [x] 2.14: Pre-bind listeners before readiness
- [x] 2.15: Unified generation and live admin state
- [x] 2.16: Correct graceful shutdown sequencing
- [x] 2.17: Atomic reload with topology validation
- [x] 2.18: PAC/static content TOML configuration
- [x] 2.19: Direct fallback metadata and failure categories
- [x] 2.20: End-to-end integration tests

### Phase 3: UDP foundation — complete

- [x] 3.1: SOCKS5 UDP ASSOCIATE server
- [x] 3.2: UDP datagram codec (IPv4, IPv6, domain)
- [x] 3.3: Association registry with bounded limits
- [x] 3.4: Direct UDP forwarding
- [x] 3.5: Transport-aware routing
- [x] 3.6: UDP metrics and admin visibility
- [x] 3.7: Security controls (client pinning, multicast rejection, size limits)
- [x] 3.8: Shutdown integration
- [x] 3.9: Corrective closure — lifecycle, config, metrics, routing, docs

### Phase 4: UDP upstream relay — complete

- [x] 4.1: UDP capability model (UdpRelayCapability)
- [x] 4.2: SOCKS5 upstream client (handshake, auth, UDP ASSOCIATE)
- [x] 4.3: Flow model (UdpFlowKind, UdpFlowKey, per-target upstream association)
- [x] 4.4: Relay integration (handle_client_datagram refactor)
- [x] 4.5: Upstream metrics and admin visibility
- [x] 4.6: Codec rename with backward-compatible wrappers
- [x] 4.7: Synthetic test server (Socks5UdpTestServer)
- [x] 4.8: Integration tests (socks5_upstream, udp_upstream)

### Phase 5: Upstream protocol parity — complete

- [x] 5.1: Upstream capability matrix (classify_upstream_chain)
- [x] 5.2: HTTP CONNECT upstream polish (HttpConnectLimits, credential validation, synthetic server tests)
- [x] 5.3: SOCKS4/SOCKS4a upstream polish (synthetic server tests, all status codes)
- [x] 5.4: Shadowsocks TCP foundation (AEAD methods, key derivation, address encoding, tcp connect)
- [x] 5.5: Shadowsocks UDP foundation (packet encode/decode, one-hop upstream)
- [x] 5.6: Trojan TCP foundation (SHA224 hash, wire format, rustls TLS)
- [x] 5.7: URI/config integration (ProtocolSpec::Shadowsocks, ProtocolSpec::Trojan)
- [x] 5.8: Chain executor integration (ShadowsocksHopHandler, TrojanHopHandler)
- [x] 5.9: Protocol documentation (HTTP_CONNECT.md, SOCKS4.md, SHADOWSOCKS.md, TROJAN.md)

### Phase 7: pproxy parity specification — complete

- [x] 7.1: pproxy parity spec (`docs/PPROXY_PARITY_SPEC.md`)
- [x] 7.2: Compatibility tier taxonomy
- [x] 7.3: Expanded parity matrix (`docs/PARITY_MATRIX.md`)
- [x] 7.4: Refactored differential test harness primitives
- [x] 7.5: Black-box probe tests for ambiguous pproxy behavior
- [x] 7.6: Intentional non-parity documentation

### Phase 8: pproxy-compatible CLI and URI translation — complete

- [x] 8.1: `eggress-pproxy-compat` crate with URI parser and TOML translator
- [x] 8.2: CLI subcommands (`pproxy translate`, `pproxy check`, `pproxy run`)
- [x] 8.3: Flag translation with structured warnings (`-v`, `-s`, `-a`, `--ssl`, `-b`)
- [x] 8.4: Unknown flag detection and warnings
- [x] 8.5: Credential redaction in all output
- [x] 8.6: Migration guide (`docs/PPROXY_MIGRATION.md`)
- [x] 8.7: CLI integration tests for pproxy subcommands

### Phase 13: Rust embed API stabilization — complete

- [x] 13.1: `eggress-embed` crate skeleton with public error types
- [x] 13.2: Config constructors (`from_toml_str`, `from_toml_file`, `source_toml`)
- [x] 13.3: Async start/handle/status/bound-address API
- [x] 13.4: Blocking owned-runtime API (`start_blocking`, `shutdown_blocking`)
- [x] 13.5: Metrics, status, and reload APIs
- [x] 13.6: Integration tests (start/stop, proxy traffic, reload, metrics, error redaction)
- [x] 13.7: Documentation (`EMBED_API.md`) and workspace updates

## Remaining Work

None — Phases 1–8, 13 are complete.

## Next Phase

Phase 9: (to be determined)

See the main roadmap for detailed descriptions of each phase.
