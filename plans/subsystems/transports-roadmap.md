# Transports Roadmap

Status: active — M003 downstream-safe bounded WebSocket composition API ready

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariants 1–2, 7–8)
- `plans/001-terminology-and-domain-model.md` §2–§3 (BoxStream upgrades, policy scoping)
- `plans/002-long-term-roadmap.md` (Phases 25–28, 1.0.10 correctives)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

Owns transport upgrades over the byte stream: TLS (`eggress-transport-tls`, always available), SSH (`eggress-transport-ssh`, opt-in `ssh`), QUIC/H3 (`eggress-transport-quic`, `eggress-protocol-h3`, opt-in `quic`). Upgrades consume and return `BoxStream`. MUST NOT own protocol framing or chain orchestration; H2 pool registries live under policy-scoped ownership shared with outbound.

Architecture: `architecture/transports-tls.md`, `architecture/transports-ssh-quic-h3.md`.

## 2. Work classification

### Invariants

- Policy-scoped SSH/H2 reuse (nested hops unpooled; `local_bind` disables hop-zero SSH/H2 reuse; insecure H2 unpooled; registries keyed by TLS config identity).
- ALPN adaptation preserves all `ClientConfig` trust fields; `tls_override + insecure=true` fails closed.

### Capabilities

- TLS listener/upstream roles; SSH upstream compat (listeners unsupported); QUIC/H3 optional roles with rejection paths.

### Infrastructure

- `client_config_with_alpn` helper, H2 handshake-count evidence fixtures, transport benches (`tls_setup` informational).

### Polish

- Transport diagnostics and docs.

## 3. Non-goals

- Chain composition and socket-metadata capture (outbound subsystem); compat-tier assignment (parity subsystem).

## 4. Current state

H2 physical-session evidence, ALPN preservation, and pooled-transport policy-identity correctives shipped in published `v1.0.10`. The workspace and published line are now `v1.0.11`. SSH/QUIC remain intentionally feature-gated.

A downstream audit in Eggtunnel found one concrete remaining WebSocket ownership leak: consumers that require an explicit frame/message ceiling must currently pass `tokio_tungstenite::WebSocketConfig` into Eggress's `*_with_config` methods. Eggtunnel therefore carries a direct Tungstenite dependency solely to preserve its 1 MiB hostile-input bound. M003 adds a small implementation-independent bounded WebSocket configuration seam so downstream consumers can remain behind the Eggress adapter boundary.

## 5. Target architecture

Policy-scoped, trust-preserving transport upgrades remain behind the boxed boundary. For WebSocket tunnel composition, downstream callers that only need finite transport bounds should not need to name Tungstenite implementation types. M003 closes that remaining public composition seam without changing existing methods or defaults.

## 6. Dependency graph

```text
TLS upgrade (hard)
    +--> SSH upstream compat [feature-gated] (soft)
    +--> QUIC/H3 roles [feature-gated] (soft)
    +--> H2 pool/ALPN correctives (closed for 1.0.10)
    `--> M003 bounded WebSocket composition seam (ready; additive)
```

## 7. Milestones

### Milestone 1 — Transport roles and hardening

Class: capability. Objective: TLS/SSH/QUIC-H3 roles with rejection paths. Exit: Phases 25–28 transport scope complete. Status: closed.

### Milestone 2 — 1.0.10 transport correctives

Class: invariant. Objective: physical H2 isolation, ALPN trust preservation, pool policy identity + roll-forward. Exit: handshake-count evidence with shared-registry control; full suite/Clippy/fmt green. Status: closed. Evidence: archive `H2_*`, `POOLED_TRANSPORT_*` records; implementation `c5826b3`; Rust CI `36032622797`.

### Milestone 3 — Downstream-safe bounded WebSocket composition API

Class: infrastructure / polish. Status: ready.

Implementation plan:

- `plans/implementation/transports/003-downstream-safe-bounded-websocket-composition-api.md`

Objective: let downstream consumers select finite WebSocket message/frame ceilings through an Eggress-owned API without importing `tokio_tungstenite::WebSocketConfig`.

Exit: additive client/server over-stream bounded APIs, oversize/backpressure/Ping/close regressions green, downstream-shaped compile evidence with no direct Tungstenite import, broad Rust gates green. Source closure does not imply a release; publication remains a separate maintainer-authorized action.

## 8. Cross-cutting requirements

Config: per-hop TLS/SSH/QUIC validation; insecure combinations rejected explicitly. Compat: manifest transport entries. Security: trust-policy isolation, no silent insecure substitution. Concurrency: pool scoping under route isolation. Docs: transport deep dives.

## 9. Verification strategy

Transport unit tests (including `custom_ca_tls_override_survives_h2_alpn_adaptation`, `h2_pool_does_not_cross_*`, `tls_override_plus_insecure_fails_closed*`), `cargo bench --bench tls_setup` (informational), feature-gated CLI check for `ssh,quic` paths.

## 10. Risks and decision points

None active. New reuse/pooling semantics require evidence and must not change the 64 KiB relay buffer or add UDP pooling without proof.

## 11. Completion definition

Transport upgrades trust-preserving and policy-scoped with 1.0.10 evidence — attained.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive phase records | archive phase records | — |
| 2 | closed | archive H2/pool/ALPN records | §Controlling evidence in `plans/registry.md` | — |
| 3 | ready | `plans/implementation/transports/003-downstream-safe-bounded-websocket-composition-api.md` | pending | Published v1.0.11 baseline; no hard blocker. |
