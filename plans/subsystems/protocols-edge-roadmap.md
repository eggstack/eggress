# Edge Protocols Roadmap

Status: closed

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariants 1–2, 10)
- `plans/001-terminology-and-domain-model.md` §2–§3 (Listener roles, chain composition)
- `plans/002-long-term-roadmap.md` (Phases 1, 5, 18–42)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

Owns listener-side and upstream protocol roles: HTTP, SOCKS, Shadowsocks, Trojan, WebSocket, Raw, Reverse/Backward, H3 (`eggress-protocol-*`), plus UDP datagram handling (`eggress-udp`) where it is protocol framing rather than chain execution. Consumes/produces `BoxStream` at the boundary. MUST NOT own TLS/SSH/QUIC session mechanics (transports) or chain orchestration (outbound).

Architecture: `architecture/protocols-http.md`, `protocols-socks.md`, `protocols-shadowsocks.md`, `protocols-trojan.md`, `protocols-tunnels.md`, `protocols-reverse.md`, `architecture/udp.md`.

## 2. Work classification

### Invariants

- Bounded protocol parsing; fail-closed on unsupported roles with structured diagnostics.
- SOCKS5 UDP ASSOCIATE association/flow accounting; reverse-path routing as authorization gate.

### Capabilities

- Listener roles for the protocol set; upstream client roles; standalone + upstream UDP relay.

### Infrastructure

- Datagram codecs, association registries, flow models, differential-harness protocol scenarios.

### Polish

- Protocol docs, diagnostic wording, benchmark coverage (`udp_relay`, `http_connect_upstream` informational).

## 3. Non-goals

- Transport session reuse policy (transports/outbound); compat-tier assignment (parity subsystem).

## 4. Current state

Phases 1/3/4/5 plus post-parity SOCKS5 correctness, reverse/UDP composition, and strict-phase protocol corrections complete. Standing feature-gated boundaries (SSR framing/plugins, legacy crypto, QUIC/H3 optional) are intentional and manifest-recorded.

## 5. Target architecture

Any listener protocol pairable with any valid upstream chain through the boxed boundary — attained within documented tiers.

## 6. Dependency graph

```text
TCP listener protocols (hard)
    +--> UDP datagram plane (hard)
    `--> Upstream client roles (hard)
             `--> Reverse composition (soft)
                      `--> Strict-phase corrections (soft)
```

## 7. Milestones

### Milestone 1 — Listener and upstream protocol coverage

Class: capability. Objective: protocol set integrated on both sides of the chain boundary. Exit: Phase 5 + UDP phases complete. Status: closed.

### Milestone 2 — Protocol correctness and strict-phase closure

Class: capability/polish. Objective: post-parity SOCKS5, reverse/UDP, SSR/plugin bounds, legacy-cipher boundaries. Exit: strict phases closed per manifest. Status: closed. Evidence: archive `POST_PARITY_*`, `PPROXY_STRICT_PHASE_*` records.

## 8. Cross-cutting requirements

Config: per-listener protocol validation. Compat: manifest tiers per protocol behavior. Security: auth checks, bounded handshakes, redaction. Concurrency: association/flow bounds, shutdown drain. Performance: `udp_relay`/`http_connect_upstream` benches informational. Docs: protocol deep dives + `udp.md`.

## 9. Verification strategy

Protocol unit/integration tests, UDP relay flow tests, differential suites for HTTP/SOCKS (`EGRESS_REQUIRE_EXTERNAL_INTEROP` gated interop only when the claim changed).

## 10. Risks and decision points

None active. New listener roles or framing changes require composition validation plus manifest review.

## 11. Completion definition

Protocol coverage closed within tiered boundaries with differential/interop evidence — attained.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive phase records | archive phase records | — |
| 2 | closed (historical) | archive strict/post-parity records | Git history + suite evidence | — |
