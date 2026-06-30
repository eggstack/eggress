# ADR: HTTP/3/QUIC Deferral

| Field | Value |
|-------|-------|
| Status | Accepted (Deferred) |
| Date | Phase 26 |
| Decision makers | Eggress maintainers |
| Related | `docs/protocols/H2_H3_QUIC.md`, `docs/protocols/ADVANCED_TRANSPORTS.md`, `docs/PPROXY_PARITY_SPEC.md` |

## Context

pproxy mentions HTTP/3 over QUIC as a transport option. QUIC provides
multiplexed streams over UDP with built-in TLS 1.3, offering benefits for
lossy networks (mobile, satellite) and reduced connection establishment
latency.

However, pproxy's H3/QUIC behavior in version 2.7.9 is:

- **Experimental**: The feature is not documented in pproxy's README or
  primary documentation. It exists in the codebase but is not presented as
  a stable interface.
- **Unstable**: No guaranteed wire-format compatibility across pproxy
  versions. The `aioquic` Python library (pproxy's QUIC backend) has
  limited deployment.
- **No interop evidence**: No documented cases of pproxy's H3/QUIC
  interoperating with standard QUIC implementations (e.g., quinn, lsquic,
  nghttp3).

eggress must decide whether to implement H3/QUIC for pproxy parity or defer
implementation.

## Decision

**HTTP/3/QUIC implementation is DEFERRED.**

eggress will:

1. Not implement H3/QUIC in Phase 26.
2. Document the deferral rationale in this ADR.
3. Track H3/QUIC as a "deferred" transport in the parity matrix.
4. Re-evaluate when the conditions below are met.

## Rationale

### pproxy H3/QUIC Is Experimental

pproxy's H3/QUIC support is not a first-class feature:

- The `h3://` scheme is recognized but behavior is not documented.
- No differential testing is possible because pproxy's H3 behavior is
  not stable enough to serve as an oracle.
- The `aioquic` library is a reference implementation but not widely
  deployed in production proxy scenarios.

Implementing against an unstable reference creates a maintenance burden:
the eggress implementation would need to track pproxy's undocumented
behavior changes without a specification.

### Significant Dependency Weight

The QUIC dependency stack is substantial:

| Crate | Purpose | Dependency tree |
|-------|---------|----------------|
| `quinn` | QUIC transport | rustls, ring, webpki-roots |
| `h3` | HTTP/3 protocol | Minimal |
| `h3-quinn` | h3 + quinn integration | Depends on both |

`quinn` pulls in `rustls` and `ring`, which are already dependencies of
eggress (via `eggress-transport-tls`). However, `quinn` adds QUIC-specific
code (UDP socket management, connection migration, 0-RTT) that is
independent of the TLS layer.

The maintenance cost of adding `quinn` (UDP socket lifecycle, connection
migration, 0-RTT, congestion control) outweighs the benefit for a transport
with uncertain pproxy interop.

### No Clear Use Case for Proxy Deployments

QUIC's primary benefits are:

- **0-RTT connection establishment**: Marginal for proxy deployments where
  connections are long-lived and reused.
- **Loss recovery**: TCP's loss recovery is adequate for stable proxy links.
- **Connection migration**: Not applicable — proxy connections are
  typically not migrated between network interfaces.

For typical proxy deployments (datacenter-to-datacenter, client-to-proxy on
LAN), TCP with HTTP/2 provides equivalent or better performance without the
QUIC complexity.

### No RFC-Based Interop Target

Unlike HTTP/2 (RFC 7540, widely implemented), HTTP/3 (RFC 9114) QUIC
interoperability in the proxy context is not well-established:

- No standard QUIC-based proxy protocol exists.
- CONNECT-UDP (RFC 9298) and MASQUE (RFC 9298) are proxy-specific QUIC
  extensions but are not implemented by pproxy.
- pproxy's `h3://` does not appear to implement CONNECT-UDP or MASQUE.

Without a clear interop target, implementing H3/QUIC would be
underspecified.

## Consequences

### Positive

- **Reduced dependency surface**: No `quinn` dependency to maintain, test,
  or audit.
- **Focus on stable transports**: Phase 26 resources allocated to WebSocket,
  H2 CONNECT, and Raw tunnels — all with clear pproxy interop.
- **Clear deferral rationale**: The ADR documents why H3/QUIC is deferred,
  preventing repeated debate.
- **Easy future extension**: The transport wrapper architecture makes it
  straightforward to add H3/QUIC later when conditions are met.

### Negative

- **pproxy parity gap**: Users who rely on pproxy's `h3://` scheme cannot
  use eggress for that transport.
- **No QUIC benefit**: Deployments that would benefit from QUIC (lossy
  networks, 0-RTT) cannot use eggress for that path.

### Neutral

- **Parity matrix tracking**: H3/QUIC is listed as "deferred" in the
  transport summary table, not "unsupported" or "intentional non-parity".
  This reflects the contingent nature of the decision.

## Conditions for Re-Evaluation

H3/QUIC implementation will be reconsidered when **all** of the following
conditions are met:

1. **pproxy H3/QUIC stabilizes**: pproxy publishes documented, stable H3/QUIC
   behavior with a clear wire-format specification.
2. **Interop evidence exists**: Demonstrated interoperability between pproxy's
   H3/QUIC and at least one other implementation (e.g., quinn, lsquic).
3. **Differential testing possible**: The eggress test harness can run H3/QUIC
   differential tests against pproxy's H3/QUIC.
4. **User demand**: Multiple users request H3/QUIC support with concrete use
   cases.
5. **CONNECT-UDP/MASQUE standardization**: If RFC 9298 (CONNECT-UDP) or MASQUE
   gains traction in the proxy ecosystem, H3/QUIC becomes more relevant.

When re-evaluation is triggered, the implementation would:

- Use `quinn` + `h3` + `h3-quinn` crates.
- Implement H3 CONNECT for TCP tunneling.
- Optionally implement CONNECT-UDP for UDP tunneling.
- Add differential tests against pproxy's `h3://` behavior.

## Alternatives Considered

### 1. Full Implementation Now

Implement H3/QUIC in Phase 26 using `quinn` + `h3` + `h3-quinn`.

**Rejected because**: pproxy's H3 behavior is experimental and unstable.
No interop evidence exists. The dependency weight is significant for
uncertain benefit. Phase 26 resources are better allocated to stable
transports (WebSocket, H2 CONNECT, Raw).

### 2. Stub Implementation

Add a `quinn` dependency but only implement a stub that rejects H3/QUIC
with a clear diagnostic.

**Rejected because**: Adding `quinn` as a dependency without using it adds
compilation overhead and maintenance surface for no benefit. The diagnostic
is better served by rejecting `h3://` schemes at URI parse time.

### 3. Feature-Gated Implementation

Implement H3/QUIC behind a Cargo feature flag (e.g., `quic-support`).

**Rejected because**: The feature would still require full implementation
effort. A feature gate does not reduce the maintenance burden — it only
makes the code optional to compile. The dependency concerns remain.

## References

- `docs/protocols/H2_H3_QUIC.md` — H3/QUIC investigation findings
- `docs/protocols/ADVANCED_TRANSPORTS.md` — Transport summary with deferred H3/QUIC
- `docs/PPROXY_PARITY_SPEC.md` — Intentional non-parity decisions
- `docs/PARITY_MATRIX.md` — Feature parity tracking
- [RFC 9114 — HTTP/3](https://www.rfc-editor.org/rfc/rfc9114)
- [RFC 9298 — CONNECT-UDP](https://www.rfc-editor.org/rfc/rfc9298)
- [quinn crate](https://crates.io/crates/quinn)
- [h3 crate](https://crates.io/crates/h3)
