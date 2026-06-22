# Phase 3 Completion: UDP Foundation

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
9. **Shutdown Integration** — UDP associations closed during graceful shutdown

### Architecture decisions

- TCP control connection owns the UDP association lifetime
- One outbound UDP socket per target flow for reliable response demux
- Connected UDP sockets for simplified response filtering
- Client address pinning enabled by default
- Direct-only forwarding (no upstream UDP relay in this phase)
- Broadcast, multicast, unspecified targets and port zero rejected by default

### Limitations

- No UDP relay through upstream proxies (SOCKS5, HTTP, etc.)
- No QUIC, HTTP/3, MASQUE, or CONNECT-UDP support
- No transparent UDP proxying
- No UDP fragmentation/reassembly
- UDP bind changes require restart

## Exit criteria status

1. [x] SOCKS5 UDP ASSOCIATE is parsed and authenticated correctly
2. [x] Server replies with a usable UDP relay address
3. [x] SOCKS5 UDP datagram codec supports IPv4, IPv6, and domain targets
4. [x] Nonzero FRAG is rejected or dropped with metrics
5. [x] Direct UDP forwarding works through a local SOCKS5 UDP association
6. [x] Association lifecycle is owned by the TCP control connection
7. [x] Idle timeout closes inactive associations
8. [x] Global and per-listener association limits are enforced
9. [x] Target-flow limits are enforced
10. [x] Client address pinning is enabled by default and tested
11. [x] Broadcast, multicast, unspecified targets, and port zero are rejected by default
12. [x] UDP datagrams are routed through the Phase 2 routing engine
13. [x] Reject rules drop UDP packets and increment metrics
14. [x] Unsupported upstream UDP paths are explicit and metriced
15. [x] UDP metrics expose bounded-cardinality counters and gauges
16. [x] Admin exposes active UDP association summary
17. [x] Runtime shutdown closes UDP associations and waits for UDP tasks
18. [x] Reload semantics for UDP config are explicit and tested
19. [x] README and architecture docs accurately describe UDP limitations
20. [x] All workspace tests, lint, audit, and applicable interoperability checks pass
21. [x] No unsafe Rust, OpenSSL dependency, or native dependency is introduced

## Completion record

Implemented by commits:

- Phase 3 foundational commits — SOCKS5 UDP ASSOCIATE, datagram codec, association registry, direct forwarding, routing, metrics, admin, security, shutdown
- Phase 3 closure — interoperability tests, documentation updates, phase completion
