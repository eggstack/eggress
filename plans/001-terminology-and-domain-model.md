# Eggress Canonical Terminology and Domain Model

Status: normative companion to `plans/000-long-term-specification.md`

This document defines the language Eggress implementation plans, protocol types, config schemas, architecture documents, tests, CLI labels, and operator documentation MUST use. When current code or older docs use a term differently, the compatibility mapping here describes the migration target.

## 1. Naming rules

1. A durable concept MUST have a typed representation rather than an ad-hoc string where the codebase already provides one (e.g. `RouteService`, `SelectedRoute`, tier enums).
2. A URI is a locator and compatibility input; it MUST NOT be treated as the validated runtime config. Translation (`eggress-pproxy-compat`) and compilation (TOML → snapshot) are distinct stages.
3. A listener, a session, a route decision, an upstream chain, and a relay flow are distinct objects.
4. Terms MUST NOT be used as interchangeable shorthand when they cross listener/routing/chain/relay boundaries.
5. Compatibility projections MAY remain during migration but MUST be labeled as such (e.g. generated-report artifacts vs manifest claims).

## 2. Top-level relationships

```text
Deployment
|-- Listeners (protocol + optional TLS + bind topology; NOT hot-reloaded)
|-- CompiledRuntimeSnapshot (router + shared upstreams + health plan + PAC)
|   |-- RoutingRules (first-match-wins + route explanation)
|   |-- UpstreamGroups (first-available | round-robin | random | least-connections)
|   `-- HealthState (probes, hysteresis, eligibility)
|-- ServerSessions (accept -> route -> open_route -> deferred reply -> relay -> SessionReport)
|-- OutboundChains (listener-free; HopHandlers consume prior BoxStream; lease Pending -> Active)
|-- UdpAssociations / Flows
|-- ReverseSupervisor
|-- Admin/Metrics/PAC
`-- CompatTranslator (pproxy args/URIs/rulefiles -> TOML + CompatibilityReport)
```

The principal runtime relationship is:

```text
Client
  -> Listener
  -> AcceptedSession (ReplayStream + ProtocolDispatcher + auth)
  -> RouteRequest
  -> SelectedRoute::Direct | Upstream{chain}
  -> relay() (64 KiB buffers, bounded post-half-close drain)
  -> SessionReport -> SessionMetrics (recorded exactly once)
```

## 3. Term definitions

- **Listener**: a bound accept surface (TCP/TLS/Unix/transparent variants). Topology is fixed at startup; only routing/upstream/group/health state reloads.
- **Session**: one accepted client connection and its lifecycle through route, open, relay, and report.
- **Route / Rule**: first-match-wins matcher set mapping a `RouteRequest` to a decision, with explanation.
- **Upstream / Group / Scheduler**: a candidate egress target; groups combine upstreams under a scheduler; health gates eligibility.
- **Chain / Hop**: an ordered list of upstream hops executed by the chain executor; each `HopHandler` consumes the prior stream and returns an upgraded one.
- **BoxStream**: the boxed `AsyncRead + AsyncWrite` byte-stream boundary type. Generic stream types MUST NOT cross protocol/transport boundaries.
- **Snapshot**: the validated, atomically swapped `CompiledRuntimeSnapshot` all hot paths read.
- **Relay**: the bidirectional byte-copy data plane (compatibility facade: 64 KiB buffers, one-second bounded post-half-close drain).
- **OutboundConnector**: the listener-free chain execution facade (over `eggress-outbound`, the single implementation authority), including the SSH session cache when `ssh` is enabled.
- **Tier (diagnostic)**: per-warning five-level vocabulary (`drop_in`, `native_equivalent`, `compatible_with_warning`, `intentional_non_parity`, `unsupported`) owned by `crates/eggress-pproxy-compat/src/tier.rs`.
- **Status (claim)**: per-capability manifest labels (`matched`, `supported_difference`, `platform_limited`, `intentional_non_parity`, `gap`). `gap` means an intended target is unimplemented.
- **Native vs compat surfaces**: the native runtime/config and the pproxy compatibility translator are separate surfaces. A matching name or successful import does not establish parity.
- **Closure**: the evidence record proving a milestone is complete. A commit message alone is not closure.

## 4. Compatibility mappings

- Older flat `plans/*.md` phase/closure records are now historical provenance under `plans/archive/phase-records/`; they MUST NOT be treated as active handoffs.
- `docs/ROADMAP.md` remains the canonical roadmap; `plans/registry.md` is the active planning control surface. The two MUST agree on active work.
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` is the user-facing compat summary; `docs/parity/pproxy_capability_manifest.toml` is the machine-readable authority; the generated parity report is derived, never leading.
- `docs/PPROXY_PARITY_SPEC.md` is historical provenance; `docs/parity/README.md` plus `crates/eggress-pproxy-compat/src/tier.rs` own current tier semantics.
- `EGGRESS_ROADMAP.md` (root) is the original roadmap retained for provenance.
