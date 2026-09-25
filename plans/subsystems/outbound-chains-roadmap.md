# Outbound Chains and Connectors Roadmap

Status: closed

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariants 1–2, 6–8)
- `plans/001-terminology-and-domain-model.md` §2–§3 (Chain, Hop, OutboundConnector, Snapshot)
- `plans/002-long-term-roadmap.md` (Phases 1, 5, 18–42)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

Owns listener-free chain execution: concrete hops, executor factory, shared classifier, `OutboundConnector` (`eggress-outbound` is the single implementation authority; `eggress-embed::outbound` re-exports). Includes the SSH session cache when `ssh` is enabled (verified known-hosts for native/TOML; compatibility host-key behavior only via `from_pproxy_uri()` with both `ssh` and `pproxy-compat`). Socket metadata is captured from the established `TcpStream` before boxing. MUST NOT own protocol framing or transport session internals beyond the policy-scoped contract.

Architecture: `architecture/outbound.md` (+ `server.md` for listener sessions, `embed.md` for the facade).

## 2. Work classification

### Invariants

- Chain metadata describes the actual first-hop TCP socket; never re-resolve a hostname to report metadata; address-lookup failure stays observational.
- Nested hops unpooled; hop-zero reuse within stable SSH cache / shared TLS-policy scope; `local_bind` disables hop-zero SSH/H2 reuse.

### Capabilities

- `OutboundConnector` chain execution (native/TOML + `from_pproxy_uri`), typed connect errors, executor factory.

### Infrastructure

- Hop handlers, shared classifier, H2 registries keyed by TLS config identity, Eggfetch CONNECT consolidation adapter.

### Polish

- Typed-error ergonomics, footprint reduction, docs.

## 3. Non-goals

- Listener accept path (server subsystem); translator flag semantics (parity subsystem).

## 4. Current state

Crate extraction, facade cleanup, SSH closure/feature-boundary passes, TCP socket-metadata recovery + release qualification, pooled-transport truthfulness/route-isolation correctives, Eggfetch CONNECT consolidation + correctives all complete. 1.0.10 qualified.

## 5. Target architecture

One authoritative outbound implementation (`eggress-outbound`) serving listener, listener-free, and embed callers — attained.

## 6. Dependency graph

```text
Chain executor + hops (hard)
    `--> Outbound crate extraction (hard)
             +--> SSH closure/boundary (soft, feature-gated)
             +--> Socket-metadata recovery (hard)
             +--> Pooled-transport correctives (hard for 1.0.10)
             `--> Eggfetch CONNECT consolidation (soft)
```

## 7. Milestones

### Milestone 1 — Chain execution and crate extraction

Class: infrastructure/capability. Objective: executor + `eggress-outbound` authority + facade cleanup. Exit: `OUTBOUND_EXECUTION_CRATE_EXTRACTION.md`, `OUTBOUND_FACADE_*` closed. Status: closed (historical, archive).

### Milestone 2 — Correctives and consolidation

Class: invariant/polish. Objective: SSH passes, socket metadata, pool truthfulness/isolation, Eggfetch consolidation. Exit: listed archive records implemented + 1.0.10 evidence. Status: closed. Evidence: archive `OUTBOUND_*`, `POOLED_TRANSPORT_*`, `EGGFETCH_*` records; `c5826b3`; CI `36032622797`.

## 8. Cross-cutting requirements

Config: per-hop validation incl. `tls_override`/`insecure` rejection. Compat: `from_pproxy_uri()` compat host-key behavior only with `ssh`+`pproxy-compat`. Security: known-hosts policy, redaction. Concurrency: pool scoping. Feature boundaries: no-default `toml`/`pproxy-compat`/`ssh`/`udp` compile slices checked. Docs: `architecture/outbound.md`, `embed.md`.

## 9. Verification strategy

Outbound feature-slice checks (`cargo check -p eggress-outbound --locked --no-default-features [--features …]`), embed OpenSSH regression for boundary changes (`EGRESS_REQUIRE_OPENSSH_TESTS=1 … --test ssh`), workspace suite for broad changes.

## 10. Risks and decision points

None active. New pooling or buffer changes require measured evidence.

## 11. Completion definition

Single-authority outbound execution with truthful metadata and policy-scoped reuse — attained.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive outbound records | archive + Git history | — |
| 2 | closed | archive outbound/pool/eggfetch records | registry control points | — |
