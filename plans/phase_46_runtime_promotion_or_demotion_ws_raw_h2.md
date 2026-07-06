# Phase 46: runtime promotion or demotion for WebSocket, raw, tunnel, and H2

## Goal

Resolve the current protocol-crate-only ambiguity for WebSocket, WSS, raw, tunnel, and HTTP/2 CONNECT support. These capabilities should either be promoted through config/compiler/runtime/CLI/Python with real tests, or deliberately demoted so the manifest, docs, and pproxy compatibility layer stop implying near-runtime parity.

The outcome should be explicit: every affected scheme is either runtime-supported with evidence or refused with clear diagnostics and rationale.

## Current baseline

Known state from prior audits:

- WebSocket/WSS protocol pieces exist in protocol crates.
- Raw/tunnel protocol pieces exist in protocol crates.
- H2 CONNECT protocol pieces exist in protocol crates.
- Runtime/config compiler refuses these schemes today.
- Manifest classifies them as native-equivalent/protocol-crate-only caveats, not drop-in.

## Decision framework

For each scheme/capability, answer:

1. Does pproxy support this surface in the pinned version?
2. Is the protocol crate implementation complete enough for runtime exposure?
3. Can config represent it without introducing ambiguous routing semantics?
4. Can runtime supervisor dispatch it safely?
5. Can CLI pproxy translator map it unambiguously?
6. Can Python `check_pproxy_args` and `PPProxyService.from_args` report it accurately?
7. Can tests exercise it without external infrastructure?

If the answer is no for any critical layer, demote/refuse rather than overclaim.

## Workstream A: Inventory and design record

Create a short design record for each group:

- `docs/adr/ADR_ws_wss_runtime_parity.md`
- `docs/adr/ADR_raw_tunnel_runtime_parity.md`
- `docs/adr/ADR_h2_connect_runtime_parity.md`

Each ADR should state:

- pproxy behavior;
- current eggress implementation state;
- security implications;
- runtime dispatch model;
- config model;
- test strategy;
- promote/demote decision.

## Workstream B: Promotion path if accepted

For any accepted scheme, complete these layers.

### Config/compiler

- Add explicit schema entries for the scheme.
- Validate required target/fixed-target fields.
- Reject unsupported combinations at compile time.
- Add redacted config displays.

### Runtime supervisor

- Wire listener or upstream creation.
- Ensure mixed inbound detection does not misclassify opaque streams.
- Apply connection limits, handshake limits, and shutdown semantics.
- Add metrics labels.

### pproxy compatibility layer

- Update URI parsing/translation to generate working TOML.
- Add diagnostics for unsupported role/composition.
- Support chains only where runtime works.
- Update `pproxy check --json` metadata.

### Python layer

- Ensure `check_pproxy_args` reports correct tiers.
- Add tests that `PPProxyService.from_args` preserves these schemes.

### Tests

- Unit tests for URI translation.
- Config compile tests.
- Runtime smoke tests.
- Differential tests if pproxy supports equivalent behavior.

## Workstream C: Demotion path if not accepted

For any scheme not promoted:

- Change manifest tier to `unsupported` or `intentional_non_parity`, not `native_equivalent`, unless there is a genuine native equivalent.
- Add stable diagnostic codes.
- Update `docs/parity/PPROXY_PARITY_REPORT.md` via generator.
- Update CLI inventory and README language.
- Ensure parser accepts only enough to give a good error; do not generate TOML.
- Ensure Python reports unsupported before service startup.

## Scheme-specific notes

### WebSocket / WSS

Potential value:

- useful for browser-friendly proxy traversal;
- likely relevant to pproxy users if pproxy supports WS transport.

Risks:

- HTTP upgrade semantics;
- TLS/WSS certificate handling;
- backpressure and close-frame semantics;
- chain composition ambiguity.

Promotion requires end-to-end WS byte-stream tests and WSS tests.

### Raw / tunnel

Potential value:

- simple fixed-target TCP forwarding;
- useful for embedding and test harnesses.

Risks:

- may bypass proxy protocol semantics;
- needs explicit target config;
- pproxy compatibility may not map 1:1.

Promotion requires clear config names and a no-ambiguous-default policy.

### H2 CONNECT

Potential value:

- modern HTTP proxy transport;
- potential bridge toward H3/MASQUE later.

Risks:

- ALPN/TLS negotiation;
- flow-control interactions;
- multiplexing lifecycle;
- higher test complexity.

Promotion requires h2 end-to-end CONNECT tests and clear pooling/multiplexing semantics.

## Files to inspect/change

- `crates/eggress-protocol-websocket/`
- `crates/eggress-protocol-raw/`
- `crates/eggress-protocol-http/`
- `crates/eggress-config/`
- `crates/eggress-runtime/`
- `crates/eggress-server/`
- `crates/eggress-pproxy-compat/`
- `crates/eggress-python/`
- `python/eggress/pproxy.py`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- `docs/PARITY_MATRIX.md`

## Acceptance criteria

- No WS/raw/H2 capability remains in an ambiguous protocol-crate-only state without a documented promote/demote decision.
- Promoted schemes work through config, runtime, CLI check/translate, and Python report.
- Demoted schemes fail early with stable diagnostics.
- Generated report places each caveat in the correct category from Phase 43.
- Manifest/report/docs agree.

## Verification commands

```bash
cargo fmt --all -- --check
cargo test -p eggress-protocol-websocket
cargo test -p eggress-protocol-raw
cargo test -p eggress-protocol-http
cargo test -p eggress-config
cargo test -p eggress-runtime
cargo test -p eggress-pproxy-compat ws raw h2
cargo test --workspace
python -m pytest python/tests/test_pproxy_dropin.py -v
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

## Non-goals

- Do not implement QUIC/H3 in this phase.
- Do not add HTTP/2 pooling as a hidden behavior unless explicitly designed.
- Do not promote parser-only support to runtime parity.
