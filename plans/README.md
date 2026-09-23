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

Outbound TCP socket metadata recovery is active and registered by the canonical
`docs/ROADMAP.md`:

1. [`OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md`](OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md) — **IMPLEMENTED AND QUALIFIED** at `253370450dc76c16aa1a3987591089010183d3b3`. Capture actual TCP local/peer metadata before the stream is boxed, propagate first-hop metadata through `ChainExecutor`, and populate `OutboundInfo` from the socket actually used without changing existing connect signatures.
2. [`OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`](OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md) — **ACTIVE**. Prepare the next immutable lockstep patch release and package graph for maintainer-operated publication; no tag or registry mutation is authorized by the plan itself.

This is a bounded listener-free metadata correctness campaign. Resolver injection,
downstream authorization policy, retry/fallback changes, and new proxy capabilities
remain out of scope.

## Recently completed

- [`PYPI_WHEEL_MATRIX_EXPANSION.md`](PYPI_WHEEL_MATRIX_EXPANSION.md) — **IMPLEMENTED**. Tier A ten-family `cp39-abi3` matrix plus sdist, matrix-driven `packaging`-based validation, native ARM smokes, musl/ARMv7 execution smokes, ordinary CPython 3.9–3.15 qualification (3.15 RC), pinned maturin, Tier B deferred per sequencing.
- [`MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md`](MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md) — **IMPLEMENTED**. Graph-derived resumable local helper (`scripts/publish-crates.py`) replacing the hand-maintained tier table and fixed delays; crates.io remains manual. Native workspace publication is nightly-only on stable 1.89, hence Outcome B.

- [`EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md`](EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md) — **IMPLEMENTED**. Reconciled the implemented corrective acceptance/evidence state and removed stale live `RUSTSEC-2025-0134` cargo-audit guidance without reopening runtime, dependency, API, capability, or parity scope.
- [`EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`](EGGFETCH_0_2_CORRECTIVE_CLOSURE.md) — **IMPLEMENTED**. Restored pre-migration outbound CONNECT request-size compatibility, removed the obsolete `RUSTSEC-2026-0009` exception under MSRV 1.89, and reconciled planning state without reopening the implemented Outcome B architecture.
- [`EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md) — **IMPLEMENTED — response-parser Outcome B**. `eggfetch-http-connect 0.2.0` owns outbound H1 CONNECT authority/request framing; Eggress intentionally retains its public-limit-compatible response parser.
