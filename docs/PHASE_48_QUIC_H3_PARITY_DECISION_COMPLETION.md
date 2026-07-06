# Phase 48: QUIC and HTTP/3 Parity Decision Completion Record

## Summary

Resolved the QUIC/HTTP/3 parity question by confirming the existing deferral decision. The ADR (`docs/adr/ADR_quic_h3_pproxy_parity.md`) already records the deferral rationale. This phase closed consistency gaps between the ADR, manifest, report, diagnostics, README, and skills documentation.

## Status: Complete

## Decision: Defer with ADR

QUIC and HTTP/3 remain deferred. Rationale (from ADR):

- pproxy's H3/QUIC behavior in v2.7.9 is experimental and unstable
- No interop evidence between pproxy H3/QUIC and standard QUIC implementations
- The `quinn` dependency stack is substantial for uncertain benefit
- No clear use case for typical proxy deployments (TCP with H2 is sufficient)
- No standard QUIC-based proxy protocol exists; CONNECT-UDP/MASQUE not implemented by pproxy

## Workstream verification

### A: pproxy behavior inventory

Documented in `docs/adr/ADR_quic_h3_pproxy_parity.md`. pproxy recognizes `h3://` scheme but behavior is undocumented, unstable, and not interoperable with standard QUIC implementations. The `aioquic` Python library has limited deployment.

### B: implementation feasibility

Evaluated in ADR §Rationale. `quinn` + `h3` + `h3-quinn` adds significant dependency weight (rustls, ring, webpki-roots, UDP socket management, connection migration, 0-RTT). No `quinn`/`h3`/`h3-quinn` dependencies exist in any `Cargo.toml`.

### C: security and operational model

Not applicable — no QUIC/H3 code to secure. If re-evaluated in the future, the implementation would need: certificate verification defaults, custom root support, SNI behavior, connection migration policy, 0-RTT disabled/enabled policy, congestion-control defaults, UDP socket binding policy, rate limits, amplification controls, and observability metrics.

### D: prototype/implementation path

Not applicable — deferral chosen.

### E: defer path (applied)

- Manifest: `protocol.quic` and `protocol.http3` are `intentional_non_parity` with `caveat_class = "deferred_by_adr"` and `runtime = "refused"` in `docs/parity/pproxy_capability_manifest.toml`
- Report: Listed under "Deferred Design Areas" and "Intentional Non-Parity (Caveat Classified)" in `docs/parity/PPROXY_PARITY_REPORT.md`
- Diagnostics: `quic://` and `h3://` schemes rejected with `UnsupportedProtocol` at URI parse time (`crates/eggress-uri/src/lib.rs`). Structured diagnostic tests: `test_quic_scheme_rejected_with_structured_diagnostic`, `test_h3_scheme_rejected_with_structured_diagnostic`
- Parser/translator: Do not silently generate nonfunctional QUIC/H3 configs — rejection happens at URI parse time before config generation

## Acceptance criteria

| Criterion | Status | Evidence |
|-----------|--------|----------|
| ADR exists and states decision | ✓ | `docs/adr/ADR_quic_h3_pproxy_parity.md` — "HTTP/3/QUIC implementation is DEFERRED" |
| Manifest/report/docs agree | ✓ | All classify as `intentional_non_parity` / `deferred_by_adr` with ADR reference |
| Parser/translator do not generate nonfunctional configs | ✓ | `quic://` and `h3://` rejected with `UnsupportedProtocol` at parse time |
| Diagnostics are clear and stable | ✓ | Structured `UnsupportedProtocol` error with scheme name |

## Documentation updates (Phase 48)

- **README.md**: Replaced unchecked QUIC/H3 feature items with deferral notice referencing ADR and re-evaluation triggers. Updated dependency policy to note Quinn/H3 as aspirational and deferred.
- **.skills/advanced-transports/skill.md**: Added QUIC/H3 deferral note with ADR reference and workspace status.
- **AGENTS.md**: No change needed — line 452 already references "QUIC/HTTP/3 deferred by separate ADR".
- **docs/protocols/ADVANCED_TRANSPORTS.md**: No change needed — already has deferral language.
- **docs/protocols/H2_H3_QUIC.md**: No change needed — already has deferred section.
- **docs/PARITY_MATRIX.md**: No change needed — already has deferral entries.
- **docs/ARCHITECTURE.md**: No QUIC/H3 mentions — no change needed.

## Verification commands

```bash
cargo fmt --all -- --check
cargo test -p eggress-uri test_quic_scheme_rejected_with_structured_diagnostic
cargo test -p eggress-uri test_h3_scheme_rejected_with_structured_diagnostic
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

## Re-evaluation triggers

H3/QUIC implementation will be reconsidered when all of the following are met:

1. pproxy H3/QUIC stabilizes with documented, stable behavior
2. Interop evidence exists with standard QUIC implementations
3. Differential testing is possible against pproxy's H3/QUIC
4. Multiple users request H3/QUIC with concrete use cases
5. CONNECT-UDP/MASQUE (RFC 9298) gains traction in the proxy ecosystem
