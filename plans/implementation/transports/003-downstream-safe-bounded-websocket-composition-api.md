# Transports M003 — Downstream-Safe Bounded WebSocket Composition API

Status: ready

Planning baseline: 09a20c83e3c29914711cabf3c97cb8643dada186

Source roadmap:

- plans/subsystems/transports-roadmap.md
- plans/000-long-term-specification.md
- plans/002-long-term-roadmap.md

Primary class: infrastructure / polish

Hard dependency: published v1.0.11 baseline.

Primary downstream consumer: eggstack/eggtunnel M019.

## 1. Objective

Add an Eggress-owned bounded WebSocket configuration seam that lets downstream Rust consumers configure WebSocket message/frame ceilings without importing or naming `tokio_tungstenite::WebSocketConfig`.

The change must preserve current WebSocket behavior and boxed-stream ownership while making `eggress-protocol-websocket` the complete implementation boundary for Tungstenite configuration.

## 2. Why this milestone is dependency-ready

The current public API already owns WebSocket tunnel establishment:

- `WebSocketTunnelClient`;
- `WebSocketTunnelServer`;
- `WebSocketStreamAdapter`.

However, callers that need an explicit underlying frame/message limit must use:

- `connect_over_stream_with_config(..., tokio_tungstenite::...::WebSocketConfig)`;
- `accept_upgrade_with_config_over_stream(..., tokio_tungstenite::...::WebSocketConfig)`.

That leaks the implementation crate into downstream dependency graphs and forces consumers such as Eggtunnel to track the same Tungstenite API/version directly.

Eggtunnel currently requires a 1 MiB message and frame ceiling and cannot safely switch to the simpler methods without losing that explicit transport bound.

This is a concrete downstream API seam, not speculative abstraction work.

## 3. Invariants that cannot regress

- `eggress-protocol-websocket` remains a boxed byte-stream tunnel adapter.
- Existing public methods remain source-compatible.
- existing default maximum-message behavior remains unchanged for callers that do not opt into the new bounded surface.
- Ping/Pong/Close handling and write-backpressure semantics remain unchanged.
- credential redaction/auth behavior in authenticated server helpers remains unchanged.
- no browser-facing Origin policy is added implicitly.
- no unbounded frame/message configuration is introduced by the new API.
- no pproxy compatibility claim changes.
- no new non-Rust/native dependency.

## 4. In scope

- an Eggress-owned public limits/options type or equivalent primitive containing the bounds needed to construct Tungstenite configuration internally;
- client and server over-stream methods using that Eggress-owned type;
- ordinary connect/accept variants where useful for API symmetry;
- validation of zero/invalid/excessive values if the chosen type permits them;
- focused oversize/frame/message/backpressure tests;
- public Rust API documentation and architecture docs;
- downstream-shaped compile coverage that demonstrates callers need not depend directly on Tungstenite.

## 5. Out of scope

- removing or changing existing `*_with_config` methods;
- changing WebSocket wire behavior;
- extension/compression negotiation;
- browser Origin enforcement;
- changing DEFAULT_MAX_MESSAGE_SIZE semantics for existing methods;
- reverse-tunnel/session concepts;
- releasing/tagging a new Eggress version without separate maintainer authorization.

## 6. Required production changes

### A. Define an implementation-independent bounded configuration

Prefer a small public Eggress type such as `WebSocketLimits` / `WebSocketTunnelOptions` with fields sufficient for current downstream needs, at minimum:

- maximum message size;
- maximum frame size.

The type must not expose Tungstenite types.

Choose finite defaults and validation semantics deliberately. If `Option<usize>` would permit accidental unbounded input, prefer explicit finite `usize`/NonZero values or reject `None` in the downstream-safe path.

The existing default methods may retain their current behavior.

### B. Build Tungstenite configuration internally

Add client/server methods that translate the Eggress-owned limits into `tokio_tungstenite::tungstenite::protocol::WebSocketConfig` internally and then delegate to the existing handshake implementation.

Avoid duplicating handshake logic across old/new methods.

The public implementation-dependent `*_with_config` methods remain for compatibility; the new API becomes the preferred downstream seam where callers only need bounded tunnel behavior.

### C. Keep stream adapter limits coherent

Ensure the `WebSocketStreamAdapter` logical message bound and the underlying Tungstenite message/frame bounds cannot silently diverge in a way that weakens the caller-selected ceiling.

Document the exact relationship.

### D. Add downstream ownership evidence

Add a focused compile/test path demonstrating a consumer can:

- accept a WebSocket upgrade over an existing `BoxStream`/async stream;
- connect over an existing stream;
- enforce a finite message/frame ceiling;
- do so without importing `tokio_tungstenite`.

This does not require adding Eggtunnel as a dependency.

## 7. Ordered work packages

1. Pin the existing WebSocket default/config behavior with focused tests.
2. Add the implementation-independent bounded options/limits type.
3. Add client/server over-stream bounded methods by delegating to existing handshake machinery.
4. Add oversize, frame/message, Ping/Pong, close and backpressure regression tests.
5. Add downstream-shaped public API compile coverage.
6. Update `architecture/protocols-tunnels.md`, public rustdoc and `docs/RUST_API.md`.
7. Run workspace/Clippy/fmt and relevant optional-feature gates.
8. Create closure evidence; leave release publication to the normal authorized release process.

## 8. Failure and cancellation semantics

The API itself owns no timeout/cancellation policy beyond the underlying async handshake; downstream callers continue to wrap it in their own deadline/cancellation primitives.

Invalid limits fail before or at handshake construction, not after accepting unbounded data.

Oversized frames/messages fail with the existing WebSocket error/I/O behavior; there is no silent truncation or fallback.

## 9. Required focused tests

- selected finite limits are reflected in the internal Tungstenite config;
- payload below/equal to the chosen limit succeeds;
- oversized message fails;
- oversized frame fails where distinguishable by the underlying implementation;
- multi-frame valid payload behavior remains correct;
- Ping receives Pong with the same payload;
- close frame yields EOF as before;
- write backpressure does not duplicate frames;
- existing `*_with_config` methods remain source-compatible;
- a downstream-shaped test imports only Eggress WebSocket/core types.

## 10. Broad verification

At minimum:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check -p eggress-cli --locked --no-default-features --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins
```

Run narrower WebSocket tests first. No parity/oracle suite is required unless implementation changes a compatibility claim.

Dependency/security checks are required if dependency versions change; this plan should not need such a change merely to add the wrapper API.

## 11. Compatibility and migration effects

The API is additive.

Existing callers may continue to use the Tungstenite-config methods. Downstream consumers can migrate to the new Eggress-owned bounded methods and remove their direct Tungstenite dependency.

No configuration, protocol or compatibility-manifest change.

## 12. Documentation updates

Update:

- `architecture/protocols-tunnels.md`;
- `architecture/overview.md` only if the public seam summary needs it;
- `docs/RUST_API.md`;
- crate rustdoc/examples describing preferred bounded downstream composition.

## 13. Acceptance criteria

M003 may close only when:

- downstream callers can set finite message and frame ceilings without naming Tungstenite;
- client and server over-stream paths both support the new bounded seam;
- existing WebSocket behavior and public methods remain compatible;
- oversize/backpressure/Ping/close tests pass;
- broad Rust gates pass;
- no compatibility claim changes or unresolved high/medium finding remain.

## 14. Stop conditions

Stop and split/review if:

- the API requires exposing a broader generic WebSocket implementation abstraction;
- supporting the bounds requires changing existing default semantics;
- a public breaking change becomes necessary;
- the work expands into browser security, routing, reverse-session or protocol-compat behavior.

## 15. Closure evidence required

Create `plans/closure/transports/003-status.md` with:

- baseline/final head;
- public API before/after;
- bounded oversize/frame/message test evidence;
- downstream-shaped compile evidence;
- broad verification results;
- known limitations;
- release/publication status explicitly distinguished from source closure.

## 16. Handoff notes

The new surface should be small. The purpose is to hide the implementation dependency while preserving explicit hostile-input bounds, not to create a general WebSocket configuration framework.
