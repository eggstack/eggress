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

- [`EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md) — **READY FOR IMPLEMENTATION**. Narrow adoption of `eggfetch-http-connect 0.2.0` for outbound HTTP/1 CONNECT wire ownership, with Rust 1.89/Base64 0.23 alignment and behavior-preserving conformance gates.

When this handoff is implemented and closed, move its status to the historical
state in place and remove it from this active subsection rather than creating a
second completion document.
