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

The prepared 1.0.9 release is blocked on fresh release qualification after
the two pooled-transport correctives registered by the canonical
`docs/ROADMAP.md`:

1. [`POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md`](POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md) — **IMPLEMENTED**. Hop-zero reuse remains; nested SSH/H2 consumes the supplied chain stream without cross-execution physical reuse.
2. [`OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md`](OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md) — **IMPLEMENTED**. Reused hop-zero SSH/H2 no longer report candidate-socket addresses as active metadata.
3. [`OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`](OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md) — **READY FOR REQUALIFICATION; NOT RELEASE QUALIFIED**. Both prerequisites are complete; 1.0.9 remains unpublished.

No publication or release tagging is authorized by these plans.

## Recently completed

- [`POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md`](POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md) and [`OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md`](OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md) — **IMPLEMENTED** at `dfc19a0390e241f5255c8ad78dc2b50e214e537f`. Fresh 1.0.9 release/package requalification is now unblocked and remains pending.

- [`OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md`](OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md) — **IMPLEMENTED AND QUALIFIED** at `253370450dc76c16aa1a3987591089010183d3b3`. Direct and TCP-backed chain metadata now comes from the established socket, with unchanged public connection signatures and boxed-stream boundary.
- [`OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`](OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md) — **ORIGINAL QUALIFICATION SUPERSEDED BY POST-QUALIFICATION AUDIT**. The 1.0.9 tree was package/test qualified at `fc47c2dba39c2a8a7b1ef31a6daa46b67fb6ad37`, but publication is now blocked pending the two active pooled-transport correctives and fresh requalification.

- [`PYPI_WHEEL_MATRIX_EXPANSION.md`](PYPI_WHEEL_MATRIX_EXPANSION.md) — **IMPLEMENTED**. Tier A ten-family `cp39-abi3` matrix plus sdist, matrix-driven `packaging`-based validation, native ARM smokes, musl/ARMv7 execution smokes, ordinary CPython 3.9–3.15 qualification (3.15 RC), pinned maturin, Tier B deferred per sequencing.
- [`MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md`](MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md) — **IMPLEMENTED**. Graph-derived resumable local helper (`scripts/publish-crates.py`) replacing the hand-maintained tier table and fixed delays; crates.io remains manual. Native workspace publication is nightly-only on stable 1.89, hence Outcome B.

- [`EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md`](EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md) — **IMPLEMENTED**. Reconciled the implemented corrective acceptance/evidence state and removed stale live `RUSTSEC-2025-0134` cargo-audit guidance without reopening runtime, dependency, API, capability, or parity scope.
- [`EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`](EGGFETCH_0_2_CORRECTIVE_CLOSURE.md) — **IMPLEMENTED**. Restored pre-migration outbound CONNECT request-size compatibility, removed the obsolete `RUSTSEC-2026-0009` exception under MSRV 1.89, and reconciled planning state without reopening the implemented Outcome B architecture.
- [`EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md) — **IMPLEMENTED — response-parser Outcome B**. `eggfetch-http-connect 0.2.0` owns outbound H1 CONNECT authority/request framing; Eggress intentionally retains its public-limit-compatible response parser.
