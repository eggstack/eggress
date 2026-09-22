# plans/ — Historical Phase Records and Registered Handoffs

> Most entries here are completed, implemented, or superseded phase records
> retained for provenance. New implementation handoffs may temporarily live in
> this directory only when they are explicitly registered from
> `docs/ROADMAP.md`; the canonical roadmap remains the authority for whether a
> plan is active.
>
> Current authority: `docs/ROADMAP.md` (canonical roadmap),
> `docs/parity/pproxy_capability_manifest.toml` +
> `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` (compat contract),
> `docs/CI_STATUS.md` (verification policy), `docs/TESTING.md` (suite
> inventory). `EGRESS_ROADMAP.md` at root is the original roadmap, also
> retained for provenance.

## Active registered handoff

- [`EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md`](EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md) — **READY FOR IMPLEMENTATION**. Reconcile the implemented Eggfetch corrective acceptance/evidence state and remove stale live `RUSTSEC-2025-0134` cargo-audit guidance without reopening runtime, dependency, API, capability, or parity scope.

## Recently completed

- [`EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`](EGGFETCH_0_2_CORRECTIVE_CLOSURE.md) — **IMPLEMENTED**. Restored pre-migration outbound CONNECT request-size compatibility, removed the obsolete `RUSTSEC-2026-0009` exception under MSRV 1.89, and reconciled planning state without reopening the implemented Outcome B architecture.
- [`EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md) — **IMPLEMENTED — response-parser Outcome B**. `eggfetch-http-connect 0.2.0` owns outbound H1 CONNECT authority/request framing; Eggress intentionally retains its public-limit-compatible response parser.
