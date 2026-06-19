# Eggress Roadmap

This document references the main roadmap in [EGGRESS_ROADMAP.md](../EGGRESS_ROADMAP.md).

## Current Phase

Phase 3: Advanced features (planned)

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

### Phase 2: Routing, health, and operations — COMPLETE

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

## Next Phase

Phase 3: Advanced features (UDP, TLS listeners, system-proxy configuration)

See the main roadmap for detailed descriptions of each phase.
