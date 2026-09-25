# Eggress Long-Term Implementation Roadmap

Status: execution roadmap for `plans/000-long-term-specification.md`

Terminology: `plans/001-terminology-and-domain-model.md`

This roadmap orders the work needed to reach the Eggress end state. It is dependency-ordered, not calendar-ordered. Each phase MUST leave the repository in a coherent state and MUST include focused implementation plans, migrations, tests, documentation, and closure evidence before the next dependent phase is treated as available.

Historical per-phase evidence lives in `plans/archive/phase-records/` and Git history. This document states ordering and exit criteria; it does not duplicate that evidence.

## Cross-phase execution rules

Every phase MUST:

1. preserve the boxed-stream and fail-closed composition invariants;
2. redact credentials in logs, diagnostics, and evidence;
3. validate protocol/transport composition before execution;
4. keep listener topology fixed and swap only routing/upstream/group/health state;
5. follow the enforced shutdown order;
6. keep compat claims manifest-governed (manifest first, reports derived);
7. add bounded parsing and typed diagnostics for new protocol surface;
8. update `architecture/` deep dives and relevant skills with code;
9. record explicit exit evidence in the implementation plan or closure record.

## Phase 1 — Core TCP proxy foundation — closed

Objective: repository/compat skeleton, URI grammar, core stream/relay, replay/dispatch, SOCKS4/4a, SOCKS5 CONNECT, HTTP CONNECT, ordinary HTTP forwarding, chain executor, CLI integration, session-model corrective closure.

Exit: ordinary TCP listener→route→chain→relay path operational with deferred replies and bounded framing. Historical record: `plans/archive/phase-records/` Phase 1 entries + `docs/ROADMAP.md` §Completed Milestones.

## Phase 2 — Routing, health, and operations — closed

Objective: rule engine, upstream groups/schedulers, server routing integration, health state machine, TOML config, metrics/logging, admin/PAC, reload/graceful shutdown, route explanation/`upstream test`, phase closure plus corrective integration tests.

Exit: snapshot-compiled routing with health-aware scheduling, atomic reload, and ordered shutdown. Historical record: archive Phase 2 entries.

## Phases 3–4 — UDP foundation and upstream relay — closed

Objective: SOCKS5 UDP ASSOCIATE, datagram codec, association registry, direct forwarding, capability/flow model, upstream client, relay integration, metrics, security, shutdown, integration tests.

Exit: standalone and upstream UDP relay flows managed with bounded relay accounting. Historical record: archive Phase 3–4 entries.

## Phase 5 — Upstream protocol parity — closed

Objective: capability matrix, HTTP/SOCKS polish, Shadowsocks TCP/UDP, Trojan TCP, URI/config integration, chain executor integration, protocol docs.

Exit: upstream protocol set integrated behind the chain boundary with documented capability matrix. Historical record: archive Phase 5 entries.

## Phases 7–8 — Parity specification and compat CLI/URI translation — closed

Objective: parity spec, tier taxonomy, expanded matrix, differential harness primitives, `eggress-pproxy-compat` crate, CLI subcommands, flag translation, redaction, migration guide, integration tests.

Exit: translator surface operational with tiered diagnostics; manifest/validator own claim semantics. Historical record: archive Phase 7–8 entries.

## Phases 13–17 — Embed, Python, packaging, parity audit — closed

Objective: `eggress-embed` crate + lifecycle API, PyO3 module + exception hierarchy, wheel/PyPI pipeline, pproxy library helpers, parity-matrix/runtime/evidence/security/packaging audits, oracle + differential harness.

Exit: embeddable Rust/Python surfaces qualified with manifest-tied evidence. Historical record: archive Phase 13–17 entries.

## Phases 18–42 — Closure, hardening, transports, corrective passes — closed

Objective: oracle/differential evidence, HTTP/SOCKS baseline closure, standalone UDP, evidence cleanup, transparent/Unix/reverse/H2/WS/Raw runtime roles, QUIC/H3 rejection paths, CLI native-equivalent closure, URI/chain semantics, Python migration API, API-boundary/interop maintenance, Eggfetch 0.2 consolidation + correctives, pooled-transport/H2/ALPN correctives, 1.0.10 qualification, distribution/release maintenance (wheel matrix, crates.io helper).

Exit (attained): 1.0.10 runtime/TLS implementation, package dry-run, and CI qualified; physical H2 isolation proven by handshake counts with shared-registry control; workspace prepared but unpublished; no active evidence blocker. Historical record: archive entries including `H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md`, `ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md`, `H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md`, `POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`, `DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md`, and the `plans/README.md` legacy index preserved at `plans/archive/phase-records-README-legacy.md`.

## Next — no active phase

There is no active implementation phase. Post-milestone work (advanced transport hardening, Python async refinements, release automation) is ongoing improvement handled as new subsystem milestones when proposed, not as blocking phases. Publication/tagging of the prepared 1.0.10 tree remains a separate maintainer-authorized release action.

A new milestone may begin only via the `plans/003-planning-process.md` lifecycle: subsystem roadmap → bounded implementation plan → registration in `plans/registry.md` and `docs/ROADMAP.md` → implementation → closure record.
