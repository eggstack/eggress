# ADR: WebSocket/WSS, Raw/Tunnel, H2 CONNECT — Protocol-Crate-Only Demotion

| Field | Value |
|-------|-------|
| Status | Accepted |
| Date | Phase 46 |
| Decision makers | Eggress maintainers |
| Related | `docs/protocols/ADVANCED_TRANSPORTS.md`, `docs/parity/pproxy_capability_manifest.toml`, `docs/PARITY_MATRIX.md`, `docs/PHASE_25_28_HARDENING_COMPLETION.md` |

## Context

Phase 26 implemented protocol crates for WebSocket tunnels (`eggress-protocol-websocket`),
Raw fixed-target tunnels (`eggress-protocol-raw`), and HTTP/2 CONNECT (`eggress-protocol-http::h2_connect`).
These protocol crates are complete with working implementations and unit tests.

However, none of these transports are wired through the config compiler (`compile_protocol()`),
runtime supervisor, or CLI as usable listener or upstream protocols. The Phase 25-28 hardening
pass (H5/H6/H7) explicitly refused them at these layers with structured diagnostics.

The pproxy capability manifest currently classifies all 8 related capabilities as
`native_equivalent` tier with `caveat_class = "protocol_crate_only"`. This overclaims parity:
`native_equivalent` implies the feature works end-to-end like pproxy, but these transports
only work when used directly as Rust library types — not through the TOML config, CLI,
or runtime supervisor that users actually interact with.

pproxy 2.7.9 accepts `ws://`, `wss://`, `h2://`, `raw://`, and `tunnel://` as **upstream-only**
URI schemes (not listeners). No differential tests exist for any of these transports.

## Decision

**All three transport groups are demoted from `native_equivalent` to `intentional_non_parity`.**

The protocol crate implementations remain in place. The config compiler, runtime supervisor,
and CLI continue to refuse these transports with structured diagnostics. The tier change
reflects an honest assessment: these are deliberate architectural choices, not near-parity gaps.

Specifically:
- `ws://`, `wss://`, `h2://`, `raw://`, `tunnel://` scheme capabilities → `intentional_non_parity`
- `protocol.ws_runtime`, `protocol.raw_runtime`, `protocol.h2_runtime` → `intentional_non_parity`
- ADR rationale documented here and in the manifest

## Rationale

### Honest Tier Classification

`native_equivalent` means "works like pproxy with minor mechanism differences." These
transports do not work through any user-facing interface (config, CLI, Python). The only
way to use them is to import the Rust protocol crate directly — a developer API, not a
user feature. `intentional_non_parity` accurately describes "we chose not to wire these
through the runtime."

### Protocol Crates Are Complete but Standalone

The implementations are real and tested:
- WebSocket: 11 unit tests (echo, max message size, close frame, ping/pong, large payload, client connect)
- Raw/Tunnel: 6 unit tests (bind, relay, concurrency, error handling)
- H2 CONNECT: 5 tests (error display, full client→server handshake)

These are not stubs. They are production-quality protocol implementations that happen to
not be integrated into the runtime pipeline.

### Promotion Cost Exceeds Benefit

Promoting to `drop_in` would require:
1. Config compiler: new protocol variants + config schema for fixed targets
2. Runtime supervisor: new upstream dispatch paths
3. CLI: URI-to-config translation for each transport
4. Python bindings: new status reporting
5. Integration tests for each transport through the full pipeline
6. WSS/TLS certificate configuration

The protocol crates already serve developers who need these transports. The runtime pipeline
serves end users who configure via TOML/CLI. These are different audiences.

### pproxy Uses These as Upstream-Only

pproxy accepts `ws://`, `wss://`, `h2://`, `raw://`, `tunnel://` only as upstream targets,
not as listener protocols. Even if eggress promoted these, parity would be limited to
upstream-only support — a narrow slice of the full transport story.

### No User Demand Evidence

No issues, discussions, or feature requests indicate user demand for runtime-integrated
WebSocket, Raw, or H2 CONNECT transports. The protocol crates exist because Phase 26
scoped them as transport wrappers, not because users requested runtime integration.

## Consequences

### Positive

- **Honest parity accounting**: The manifest and report accurately reflect what users can
  actually configure and run.
- **No false expectations**: Users reading `intentional_non_parity` understand these are
  deliberate choices, not implementation gaps.
- **Reduced maintenance surface**: No runtime/config/CLI plumbing to maintain, test, or
  document for these transports.
- **Clear ADR record**: This decision is documented and won't be re-litigated without
  new evidence (user demand, pproxy changes).

### Negative

- **Users cannot configure these via TOML/CLI**: The only way to use WebSocket, Raw, or
  H2 CONNECT is through the Rust API directly.
- **No Python binding support**: Python users cannot access these transports.

### Neutral

- **Protocol crates remain**: The implementations are not removed. They continue to work
  as Rust library types for developers who need them.
- **Manifest tier change is metadata-only**: No code changes are required. The tier
  classification is a documentation concern.

## Alternatives Considered

### 1. Promote All Three to `drop_in`

Wire WebSocket, Raw, and H2 CONNECT through the config compiler, runtime supervisor, CLI,
and Python bindings with full integration tests.

**Rejected because**: The promotion cost (6+ work items per transport) exceeds the benefit
for transports with no user demand evidence. The protocol crates already serve the developer
audience. The runtime pipeline serves a different audience.

### 2. Promote Only Raw/Tunnel (Simplest Case)

Raw/Tunnel is the simplest transport (fixed TCP target, no protocol negotiation) and could
be promoted with minimal effort.

**Rejected because**: Even the simplest transport requires config schema, supervisor dispatch,
CLI translation, and integration tests. Without user demand, the maintenance cost is not
justified. The protocol crate works for developers who need raw tunneling.

### 3. Keep as `native_equivalent` with Caveat

Maintain the current tier classification with the `protocol_crate_only` caveat.

**Rejected because**: `native_equivalent` overclaims parity. The caveat is a footnote that
most readers will miss. `intentional_non_parity` is the honest classification for features
that are deliberately not wired through the user-facing pipeline.

## References

- `docs/protocols/ADVANCED_TRANSPORTS.md` — Transport wrapper architecture
- `docs/PHASE_25_28_HARDENING_COMPLETION.md` — H5/H6/H7 refusal enforcement
- `docs/parity/pproxy_capability_manifest.toml` — Capability manifest (updated in Phase 46)
- `docs/PARITY_MATRIX.md` — Feature parity tracking (updated in Phase 46)
- `docs/PPROXY_PARITY_SPEC.md` — Tier taxonomy
