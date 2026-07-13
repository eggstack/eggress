# ADR: WebSocket and Raw Runtime Promotion (Phase B3)

| Field | Value |
|-------|-------|
| Status | Accepted |
| Date | Phase B3 |
| Decision makers | Eggress maintainers |
| Related | `docs/adr/ADR_ws_wss_raw_h2_protocol_crate_only.md`, `docs/parity/composition_matrix.toml`, `docs/parity/pproxy_capability_manifest.toml` |

## Context

Phase B3 promotes WebSocket (ws/wss) and Raw (raw/tunnel) from `intentional_non_parity` (protocol-crate-only) to `drop_in` tier. H2 CONNECT remains deferred.

The protocol crates were already complete with working implementations and unit tests. The promotion adds:
- Config compiler support (ws/wss/raw/tunnel accepted in TOML)
- CLI support (ProtocolSpec::WebSocket/Raw mapped to ProtocolId)
- Upstream HopHandler implementations (WebSocketHopHandler, RawHopHandler)
- Chain participation (ws/wss/raw/tunnel no longer blocked in chains)

## Decision

**WS/WSS and Raw/Tunnel are promoted to `drop_in` tier for upstream roles.**

Specifically:
- `ws://`, `wss://` upstream → `drop_in` with `evidence = "integration"`
- `raw://`, `tunnel://` upstream → `drop_in` with `evidence = "integration"`
- `h2://` upstream → remains `intentional_non_parity` (deferred to B4)
- Chain compositions: socks5→ws, http→ws, socks5→raw, http→raw → `drop_in`
- Listener roles remain unsupported (pproxy uses these as upstream-only)

## Rationale

### pproxy Behavior Match

pproxy accepts ws/wss/raw/tunnel as upstream-only URI schemes. The promotion matches this behavior exactly.

### Protocol Crates Were Complete

The implementations were production-quality with full test suites. The only gap was config/runtime wiring.

### Low Promotion Cost

Unlike the original ADR's estimate of "6+ work items per transport", the actual promotion required:
1. Config compiler: 3 new match arms (trivial)
2. CLI: 3 new match arms (trivial)
3. Upstream handlers: 2 new HopHandler impls (moderate)
4. Chain support: remove from unsupported list (trivial)

### No Listener Complexity

pproxy does not use these as listeners, so no accept-layer detection, no listener config schema, and no protocol-specific listener options are needed.

## Consequences

### Positive
- WS/WSS and Raw/Tunnel can be configured via TOML and CLI
- Python bindings can expose these transports
- Composition matrix and parity manifest accurately reflect capabilities

### Negative
- H2 remains deferred (requires bidirectional stream adaptation)
- No listener support (upstream-only, matching pproxy behavior)
