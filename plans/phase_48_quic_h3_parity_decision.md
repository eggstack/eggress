# Phase 48: QUIC and HTTP/3 parity decision

## Goal

Resolve QUIC and HTTP/3 parity deliberately. These features are high-risk and should not remain ambiguous pproxy blockers. This phase should produce either an implementation plan with scoped runtime support or an explicit deferred/intentional-non-parity decision backed by ADRs and manifest updates.

## Current baseline

- QUIC and HTTP/3 are currently unsupported/deferred.
- Manifest entries identify QUIC/H3 as non-drop-in and deferred by ADR language.
- HTTP/2 CONNECT work exists in protocol-crate form, but H3 is not implemented.
- QUIC/H3 should not block mainline HTTP/SOCKS/Shadowsocks/Trojan parity unless strict full pproxy protocol-surface parity is required.

## Primary decision

Choose one:

1. **Defer with ADR**: keep QUIC/H3 out of the near-term release, classify as deferred by ADR, and make docs clear.
2. **Prototype only**: add experimental native support behind an explicit feature flag, not pproxy drop-in parity.
3. **Implement parity**: add production QUIC/H3 runtime support, tests, docs, and packaging impact analysis.

Default recommendation: defer unless pproxy QUIC/H3 is a real user requirement and can be tested against the pinned pproxy version.

## Workstream A: pproxy behavior inventory

Document exactly what pproxy supports:

- URI schemes and modifiers;
- QUIC library/dependency behavior;
- TLS/ALPN requirements;
- HTTP/3 CONNECT support versus UDP relay/MASQUE behavior;
- client/server roles;
- chain behavior;
- error modes;
- platform limitations.

Store findings in `docs/adr/ADR_quic_h3_pproxy_parity.md` if not already present, or update the existing ADR.

## Workstream B: implementation feasibility if accepted

Evaluate Rust crates and integration impact:

- `quinn` or equivalent QUIC stack;
- rustls version compatibility with existing TLS crate;
- h3 library maturity;
- ALPN negotiation and certificate validation;
- stream mapping to existing relay abstractions;
- datagram/MASQUE support if UDP is desired;
- binary size and PyPI wheel impact;
- CI test complexity.

## Workstream C: security and operational model

Define:

- certificate verification defaults;
- custom root support;
- SNI behavior;
- connection migration policy;
- 0-RTT disabled/enabled policy;
- congestion-control defaults;
- UDP socket binding policy;
- rate limits and amplification controls;
- observability metrics.

## Workstream D: prototype/implementation path

If implementing, use staged delivery:

1. QUIC transport connector without pproxy parity claim.
2. H3 CONNECT client/server smoke tests.
3. Config schema behind feature flag.
4. Runtime supervisor integration.
5. pproxy translator support only after runtime stability.
6. Python diagnostics/reporting.
7. Differential/interop tests.

Do not expose as default release capability until all layers are complete.

## Workstream E: defer path

If deferring:

- keep manifest entries as `intentional_non_parity` or `unsupported` with `caveat_class = "deferred_by_adr"`;
- ensure generated report places QUIC/H3 under deferred design areas;
- add docs explaining that HTTP/1, SOCKS, Shadowsocks, and Trojan are the parity targets for the current release;
- add stable diagnostics for `quic-unsupported` and `h3-unsupported`;
- ensure `check_pproxy_args` reports unsupported before startup.

## Acceptance criteria

- ADR exists and states implement/defer/prototype decision.
- Manifest/report/docs agree.
- Parser/translator do not silently generate nonfunctional QUIC/H3 configs.
- If deferred, diagnostics are clear and stable.
- If implemented, runtime support has config, tests, docs, Python reporting, and interop evidence.

## Verification commands

```bash
cargo fmt --all -- --check
cargo test -p eggress-pproxy-compat quic
cargo test -p eggress-pproxy-compat h3
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

If implemented, add QUIC/H3 crate-specific tests and gated interop commands.

## Non-goals

- Do not add opportunistic QUIC support without certificate/security design.
- Do not call prototype support pproxy parity.
- Do not make QUIC/H3 a dependency of the mainline parity release unless explicitly accepted.
