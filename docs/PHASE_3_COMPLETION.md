# Phase 3 Completion: UDP Foundation

## Status

Phase 3 UDP corrective closure is complete.

## Completed

Phase 3 added UDP foundation support to eggress, enabling SOCKS5 UDP ASSOCIATE and direct UDP forwarding.

### What was implemented

1. **SOCKS5 UDP ASSOCIATE** — Full UDP ASSOCIATE command parsing and session lifecycle
2. **SOCKS5 UDP Datagram Codec** — Encode/decode SOCKS5 UDP datagrams with IPv4, IPv6, and domain targets
3. **UDP Association Registry** — Bounded association tracking with global and per-listener limits
4. **Direct UDP Forwarding** — Connected UDP socket per target flow with DNS resolution
5. **Transport-Aware Routing** — Route rules can match on TCP vs UDP transport
6. **UDP Metrics** — Prometheus-compatible counters and gauges for UDP operations
7. **Admin Visibility** — `/-/udp` endpoint for active association summary
8. **Security Controls** — Client pinning, multicast/broadcast rejection, size limits, idle expiry
9. **Shutdown Integration** — UDP associations closed during graceful shutdown, tasks tracked and drained

### Corrective closure items

The following items were completed during the corrective closure pass:

1. **Association registry cleanup** — Every close path (TCP close, idle timeout, shutdown) removes the association from the registry via `registry.remove(id)`. Active counts return to zero.
2. **Association idle timeout** — Enforced in the relay loop via periodic tick against `last_activity().elapsed()`.
3. **Target-flow idle cleanup** — Enforced in the relay loop via `reap_idle_flows()`. `max_targets_per_association` bounds active flows, not lifetime history.
4. **UDP task tracking** — Relay tasks spawned via `TaskTracker` (not bare `tokio::spawn`). Shutdown drains tracked tasks within the grace period.
5. **TOML UDP configuration** — Nested `[listeners.udp]` section with `enabled`, `bind`, `advertise`, `idle_timeout`, `target_idle_timeout`, `max_associations`, `max_targets_per_association`, `max_datagram_size`, `client_pin`. Legacy `udp_enabled = true` compatibility preserved.
6. **Per-listener bind/advertise** — Relay bind address and advertised SOCKS5 reply address derived from listener UDP config. Advertise IP follows: explicit config > bind IP > loopback derivation.
7. **Metrics bridging** — `UdpMetrics` bridged into `MetricsRegistry` via `set_udp_metrics()`. `/metrics` exposes live UDP counters (associations, packets, bytes, drops, decode errors, target flows).
8. **Routing fallback** — UDP routing uses full `route()` with fallback semantics. `DirectFallback` is honored and logged.
9. **Admin endpoint hardened** — `/-/udp` reports live data from `MetricsRegistry` without exposing client/target addresses.

### Architecture decisions

- TCP control connection owns the UDP association lifetime
- One outbound UDP socket per target flow for reliable response demux
- Connected UDP sockets for simplified response filtering
- Client address pinning enabled by default
- Direct-only forwarding (no upstream UDP relay in this phase)
- Broadcast, multicast, unspecified targets and port zero rejected by default
- UDP relay tasks tracked via `TaskTracker` for supervised shutdown
- Full `route()` evaluation for UDP preserves upstream-group fallback semantics

### Limitations

- No UDP relay through HTTP, SOCKS4, or multi-hop upstream proxies (one-hop SOCKS5 added in Phase 4)
- No QUIC, HTTP/3, MASQUE, or CONNECT-UDP support
- No transparent UDP proxying
- No UDP fragmentation/reassembly
- UDP bind changes require restart
- No UDP chain validation

## Exit criteria status

1. [x] SOCKS5 UDP ASSOCIATE is parsed and authenticated correctly
2. [x] Server replies with a usable UDP relay address
3. [x] SOCKS5 UDP datagram codec supports IPv4, IPv6, and domain targets
4. [x] Nonzero FRAG is rejected or dropped with metrics
5. [x] Direct UDP forwarding works through a local SOCKS5 UDP association
6. [x] Association lifecycle is owned by the TCP control connection
7. [x] Idle timeout is enforced in the relay loop
8. [x] Global and per-listener association limits are enforced
9. [x] Target-flow limits are enforced and freed by idle cleanup
10. [x] Client address pinning is enabled by default and tested
11. [x] Broadcast, multicast, unspecified targets, and port zero are rejected by default
12. [x] UDP datagrams are routed through the Phase 2 routing engine with fallback
13. [x] Reject rules drop UDP packets and increment metrics
14. [x] Unsupported upstream UDP paths are explicit and metriced
15. [x] UDP metrics expose bounded-cardinality counters and gauges
16. [x] `/metrics` exposes live UDP counters via MetricsRegistry bridge
17. [x] Admin `/-/udp` reports live association summary without client/target addresses
18. [x] UDP relay tasks tracked via TaskTracker and drained on shutdown
19. [x] Per-listener TOML UDP configuration with `[listeners.udp]` section
20. [x] UDP relay bind and advertise configurable per listener
21. [x] Association registry entries removed on every close path
22. [x] Reload semantics for UDP config are explicit and tested
23. [x] README and architecture docs accurately describe UDP limitations
24. [x] All workspace tests, lint, audit, and applicable interoperability checks pass
25. [x] No unsafe Rust, OpenSSL dependency, or native dependency is introduced

## Completion record

Implemented by commits:

- Phase 3 foundational commits — SOCKS5 UDP ASSOCIATE, datagram codec, association registry, direct forwarding, routing, metrics, admin, security, shutdown
- Phase 3 corrective closure — association registry cleanup, idle timeout enforcement, target-flow reaping, TaskTracker, TOML UDP config, bind/advertise, metrics bridging, routing fallback, admin hardening
- Phase 3 docs — documentation accuracy corrections and completion record
