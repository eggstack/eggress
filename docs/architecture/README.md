# docs/architecture — Earlier Per-Crate Notes (Superseded)

> Historical snapshot only. The maintained architecture reference is the
> top-level [`architecture/`](../../architecture/overview.md) index (last
> verified against the 1.0.7 tree on 2026-09-22: 28 workspace crates, 26
> deep-dive files, 11 fuzz targets, 5 Criterion benches, 4 workflows).
>
> This directory is retained for provenance and link targets. Prefer
> `architecture/overview.md` plus the per-component deep dives there. Known
> structural drift here includes singular `transport-*.md` names (maintained:
> `transports-*.md`), split `protocols-raw/websocket` notes (maintained:
> `protocols-tunnels.md`), `python.md` (maintained: `python-bindings.md`),
> `tools-and-scripts.md` + `testkit.md` (maintained: `testing-and-tooling.md`),
> and missing `outbound.md` / `relay.md` coverage for the newest crates.
