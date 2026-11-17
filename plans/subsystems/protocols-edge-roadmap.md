# Edge Protocols Roadmap

Status: closed — M003 implemented and verified 2026-11-16

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

A 2026-11-16 security audit found two inbound-enforcement gaps: SOCKS4 auth compares only the username (the configured password is never enforced and a success can seed the shared IP reuse cache), and `allow_private_egress = false` is bypassable for UDP domain targets because the promised post-DNS check is absent from the direct flow. The same audit found the shared private/reserved-IP classifier missing CGNAT and IPv4-embedding IPv6 forms, and no warning when non-loopback HTTP/SOCKS password auth runs without TLS. M003 closes those gaps without changing the pproxy default or adding a protocol.

## 5. Target architecture

Any listener protocol pairable with any valid upstream chain through the boxed boundary — attained within documented tiers.

## 6. Dependency graph

```text
TCP listener protocols (hard)
    +--> UDP datagram plane (hard)
    `--> Upstream client roles (hard)
             `--> Reverse composition (soft)
                      `--> Strict-phase corrections (soft)
                               `--> M003 SOCKS4/UDP/auth-transport corrective (implemented)
```

## 7. Milestones

### Milestone 1 — Listener and upstream protocol coverage

Class: capability. Objective: protocol set integrated on both sides of the chain boundary. Exit: Phase 5 + UDP phases complete. Status: closed.

### Milestone 2 — Protocol correctness and strict-phase closure

Class: capability/polish. Objective: post-parity SOCKS5, reverse/UDP, SSR/plugin bounds, legacy-cipher boundaries. Exit: strict phases closed per manifest. Status: closed. Evidence: archive `POST_PARITY_*`, `PPROXY_STRICT_PHASE_*` records.

### Milestone 3 — SOCKS4 credential enforcement and UDP private-egress policy

Class: invariant. Status: closed 2026-11-16 — closure record `plans/closure/protocols-edge/003-status.md` (coordinator workspace-wide gate passed).

Implementation plan:

- `plans/implementation/protocols-edge/003-socks4-auth-and-udp-private-egress.md`

Objective: enforce the full pproxy SOCKS4 user-ID credential, apply `allow_private_egress` after DNS on all locally resolving UDP paths, extend CGNAT/NAT64/6to4/Teredo classification, and warn when non-loopback HTTP/SOCKS auth has no TLS.

Exit: SOCKS4 username-only/wrong-password attempts fail without seeding the reuse cache; private-resolving UDP domains are dropped when the policy disallows private egress; classification tests cover the new ranges; the auth-without-TLS warning is emitted; existing `allow_private_egress = true` behavior remains green. No release/tag.

## 8. Cross-cutting requirements

Config: per-listener protocol validation. Compat: manifest tiers per protocol behavior. Security: auth checks, post-DNS destination policy, bounded handshakes, redaction; M003 adds the SOCKS4 full-credential rule and resolved-target UDP enforcement. Concurrency: association/flow bounds, shutdown drain. Performance: `udp_relay`/`http_connect_upstream` benches informational. Docs: protocol deep dives + `udp.md`.

## 9. Verification strategy

Protocol unit/integration tests, UDP relay flow tests, differential suites for HTTP/SOCKS (`EGRESS_REQUIRE_EXTERNAL_INTEROP` gated interop only when the claim changed). M003 additionally requires SOCKS4 auth tests, private-domain UDP drop tests, classifier tables, and the auth-without-TLS config warning.

## 10. Risks and decision points

M003's SOCKS4 change is a compatibility-sensitive restore of the pinned pproxy full-credential user-ID behavior; if the manifest claim changes, the oracle/differential gate applies. UDP domain filtering must reuse the existing lookup and must not change the default `allow_private_egress = true`. CIDR matcher semantics and admin exposure are owned by Routing M003. New listener roles or framing changes require composition validation plus manifest review.

## 11. Completion definition

Protocol coverage closed within tiered boundaries with differential/interop evidence, plus M003 inbound-auth/UDP security correction closed — attained when both are true.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive phase records | archive phase records | — |
| 2 | closed (historical) | archive strict/post-parity records | Git history + suite evidence | — |
| 3 | closed | `plans/implementation/protocols-edge/003-socks4-auth-and-udp-private-egress.md` | `plans/closure/protocols-edge/003-status.md` | — |
