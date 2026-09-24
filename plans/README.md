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

The canonical roadmap has one release-blocking corrective on the prepared
1.0.10 line:

1. [`H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md`](H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md) — **IMPLEMENTED; RELEASE-QUALIFICATION PENDING**. ALPN adaptation now uses `eggress_transport_tls::client_config_with_alpn` (clones the existing `rustls::ClientConfig` via `ClientConfig::clone()` and only mutates `alpn_protocols`); `tls_override + insecure=true` is rejected explicitly; new custom-CA H2 ALPN and trust-boundary regressions are in `crates/eggress-outbound/src/executor.rs`. Full release qualification (CI green on the final SHA, package publish dry-run, `v1.0.10` tag/publish) is the next gate.
2. [`POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`](POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md) — **IMPLEMENTED BUT NOT RELEASE-QUALIFIED**. Pool scoping, bind isolation, route isolation, and 1.0.10 version roll-forward are implemented; release closure now depends on the TLS corrective above.

The workspace is 1.0.10. No `v1.0.10` tag or publication is authorized by
these plans.

## Recently completed

- [`H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md`](H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md) — **IMPLEMENTED** (workstreams 1-7) on `main`. New `client_config_with_alpn` helper in `eggress-transport-tls`; outbound TLS wrapper closure now uses it for `tls_override` and fails closed on `tls_override + insecure=true`. New regressions: `custom_ca_tls_override_survives_h2_alpn_adaptation`, `h2_pool_does_not_cross_tls_trust_policy`, `mtls_identity_survives_h2_alpn_adaptation`, `tls_override_plus_insecure_fails_closed`, `tls_override_plus_insecure_fails_closed_with_insecure_tls_feature` (gated). Full evidence in the plan's "Completion record — 2026-09-24" section. Release qualification (CI green on the pushed SHA, `v1.0.10` tag/publish) remains a separate authorization.

- [`POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md`](POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md) and [`OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md`](OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md) — **IMPLEMENTED** at `dfc19a0390e241f5255c8ad78dc2b50e214e537f`. Fresh 1.0.9 release/package requalification is now unblocked and remains pending.

- [`OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md`](OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md) — **IMPLEMENTED AND QUALIFIED** at `253370450dc76c16aa1a3987591089010183d3b3`. Direct and TCP-backed chain metadata now comes from the established socket, with unchanged public connection signatures and boxed-stream boundary.
- [`OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`](OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md) — **ORIGINAL QUALIFICATION SUPERSEDED BY POST-QUALIFICATION AUDIT**. The 1.0.9 tree was package/test qualified at `fc47c2dba39c2a8a7b1ef31a6daa46b67fb6ad37`, but publication is now blocked pending the two active pooled-transport correctives and fresh requalification.

- [`PYPI_WHEEL_MATRIX_EXPANSION.md`](PYPI_WHEEL_MATRIX_EXPANSION.md) — **IMPLEMENTED**. Tier A ten-family `cp39-abi3` matrix plus sdist, matrix-driven `packaging`-based validation, native ARM smokes, musl/ARMv7 execution smokes, ordinary CPython 3.9–3.15 qualification (3.15 RC), pinned maturin, Tier B deferred per sequencing.
- [`MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md`](MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md) — **IMPLEMENTED**. Graph-derived resumable local helper (`scripts/publish-crates.py`) replacing the hand-maintained tier table and fixed delays; crates.io remains manual. Native workspace publication is nightly-only on stable 1.89, hence Outcome B.

- [`EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md`](EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md) — **IMPLEMENTED**. Reconciled the implemented corrective acceptance/evidence state and removed stale live `RUSTSEC-2025-0134` cargo-audit guidance without reopening runtime, dependency, API, capability, or parity scope.
- [`EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`](EGGFETCH_0_2_CORRECTIVE_CLOSURE.md) — **IMPLEMENTED**. Restored pre-migration outbound CONNECT request-size compatibility, removed the obsolete `RUSTSEC-2026-0009` exception under MSRV 1.89, and reconciled planning state without reopening the implemented Outcome B architecture.
- [`EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md) — **IMPLEMENTED — response-parser Outcome B**. `eggfetch-http-connect 0.2.0` owns outbound H1 CONNECT authority/request framing; Eggress intentionally retains its public-limit-compatible response parser.
