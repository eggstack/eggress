# Eggress Roadmap

This document is the canonical roadmap. The active planning control surface is
`plans/registry.md`; the two MUST agree on active work. New milestones follow
the `plans/003-planning-process.md` lifecycle (subsystem roadmap → bounded
implementation plan → registration here and in `plans/registry.md` →
implementation → closure record). Canonical direction also lives in
`plans/000-long-term-specification.md` through `plans/002-long-term-roadmap.md`;
pre-transition phase records are provenance under `plans/archive/phase-records/`.

## Current Status

All core milestones are complete. The Rust-native CLI and runtime are
production-ready with broad pproxy 2.7.9 behavioral compatibility. See the
active [compatibility matrix](parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md)
and [capability manifest](parity/pproxy_capability_manifest.toml).

Known boundaries:
- Legacy Shadowsocks stream ciphers remain intentional non-parity (opt-in via `legacy-crypto` feature).
- QUIC/HTTP/3 is optional and feature-gated (`quic` feature).
- SSH upstream compatibility is optional and feature-gated (`ssh` feature); SSH listeners remain unsupported.
- Bounded pproxy SSR TCP framing/plugin path is feature-gated compatibility work.
- macOS PF original-destination recovery and four unavailable legacy cipher names are intentional exclusions.

See `docs/parity/README.md` and `crates/eggress-pproxy-compat/src/tier.rs`
for the tier taxonomy (`docs/PPROXY_PARITY_SPEC.md` is historical provenance only).

### 1.0.10 qualification — complete, prepared, unpublished

The 1.0.10 runtime/TLS implementation, package dry-run, and CI are
qualified. The final evidence corrective has landed: the two-valid-policy
H2 isolation regression now counts successful server-side H2 handshakes
after `h2::server::handshake` (two TLS accepts plus exactly two H2
handshakes), with mutation sensitivity proven by the shared-registry
control. No active 1.0.10 blocker remains.

Recently completed:

1. [H2 Physical Session Evidence Corrective](../plans/archive/phase-records/H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md) — **IMPLEMENTED AND QUALIFIED**. Physical H2 isolation proven by handshake counts; supported 28-crate dry-run green; Rust CI `36032622797` success on `c5826b3`.
2. [1.0.10 Closure and Evidence Reconciliation Pass](../plans/archive/phase-records/ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md) — **IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED**.
3. [H2 TLS Override ALPN Preservation and 1.0.10 Qualification Corrective](../plans/archive/phase-records/H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md) — **IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED**.
4. [Pooled Transport Policy Identity and 1.0.10 Roll-Forward Corrective](../plans/archive/phase-records/POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md) — **IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED**.

The workspace remains 1.0.10, prepared but unpublished. No v1.0.10 tag or
publication is authorized by these plans; publication/tagging remains a
separate maintainer-authorized release action.

## Completed Milestones

### Phase 1: Core TCP proxy foundation

- [x] 1.1: Repository and compatibility skeleton
- [x] 1.2: URI grammar and validation
- [x] 1.3: Core stream and relay
- [x] 1.4: Replay stream and protocol dispatch
- [x] 1.5: SOCKS4/SOCKS4a
- [x] 1.6: SOCKS5 CONNECT
- [x] 1.7: HTTP CONNECT
- [x] 1.8: Ordinary HTTP forwarding
- [x] 1.9: Chain executor
- [x] 1.10: CLI integration
- [x] 1.11: Corrective closure — session model, deferred replies, body framing, header filtering, external interop

### Phase 2: Routing, health, and operations — complete

- [x] 2.1: Routing rule engine (matchers, first-match-wins, route explanation)
- [x] 2.2: Upstream groups and schedulers (first-available, round-robin, random, least-connections)
- [x] 2.3: Server routing integration (RouteService trait, protocol-correct rejects, connect timeout)
- [x] 2.4: Health management (state machine, TCP probes, hysteresis, eligibility)
- [x] 2.5: TOML configuration (validation, secret sources, CLI compatibility)
- [x] 2.6: Metrics and JSON logging (Prometheus registry, bounded cardinality)
- [x] 2.7: Admin API, PAC, and static content (health, status, metrics, PAC serving)
- [x] 2.8: Reload and graceful shutdown (ArcSwap, SIGHUP, drain timeout)
- [x] 2.9: Route explanation and upstream test command (human/JSON output, reachability testing)
- [x] 2.10: Phase closure (README, ARCHITECTURE, AGENTS.md updates)
- [x] 2.11–2.20: Corrective integration and end-to-end tests

### Phase 3: UDP foundation — complete

- [x] 3.1–3.9: SOCKS5 UDP ASSOCIATE, datagram codec, association registry, direct forwarding, metrics, security, shutdown, corrective closure

### Phase 4: UDP upstream relay — complete

- [x] 4.1–4.8: Capability model, SOCKS5 upstream client, flow model, relay integration, metrics, codec rename, synthetic tests, integration tests

### Phase 5: Upstream protocol parity — complete

- [x] 5.1–5.9: Capability matrix, HTTP/SOCKS4/SOCKS5 polish, Shadowsocks TCP/UDP foundation, Trojan TCP, URI/config integration, chain executor integration, protocol documentation

### Phase 7: pproxy parity specification — complete

- [x] 7.1–7.6: Parity spec, tier taxonomy, expanded matrix, differential harness primitives, probe tests, intentional non-parity documentation

### Phase 8: pproxy-compatible CLI and URI translation — complete

- [x] 8.1–8.7: `eggress-pproxy-compat` crate, CLI subcommands, flag translation, credential redaction, migration guide, integration tests

### Phase 13: Rust embed API stabilization — complete

- [x] 13.1–13.7: `eggress-embed` crate, config constructors, async/blocking start/handle API, metrics/reload, integration tests, documentation

### Phase 14: Python bindings — complete

- [x] 14.1–14.7: PyO3 native module, Python classes, exception hierarchy, GIL release, context manager, tests, documentation

### Phase 15: PyPI/wheel release pipeline — complete

- [x] 15.1–15.4: Wheel build infrastructure, testing, PyPI docs, supply chain checks

### Phase 16: Python pproxy library helpers — complete

- [x] 16.1–16.4: Translation helpers, convenience APIs, async lifecycle, compat/redaction/concurrency tests

### Phase 17: True pproxy parity release candidate audit — complete

- [x] 17.1–17.8: Final parity matrix audit, runtime/package release audit, differential/interop evidence audit, security/redaction audit, packaging audit, documentation consistency, release candidate document

### Phase 18: pproxy oracle and evidence harness — complete

- [x] Oracle process runner for real pproxy differential testing

### Phase 19: HTTP/SOCKS baseline closure — complete

- [x] Persistent HTTP forwarding, expanded differential tests for HTTP CONNECT, SOCKS4/4a, SOCKS5

### Phases 20–24: Standalone UDP, evidence cleanup, hardening — complete

- [x] Standalone UDP relay, manifest validation, evidence taxonomy, shadowsocks standardization

### Phases 25–28: Hardening and advanced transports — complete

- [x] Transparent proxy, Unix domain sockets, reverse proxy supervisor integration, H2/WS/Raw runtime and bounded compatibility listener roles, QUIC/H3 rejection, CLI native-equivalent closure

### Phase 29–32: Python API parity and hardening — complete

- [x] Python API parity inventory (114 entries), Python hardening (GIL release, tier normalization, evidence reclassification), Python packaging (py.typed, version metadata, capability introspection)

### Phase 36: Final parity release audit — complete

- [x] Frozen targets, manifest completeness audit, manifest corrections, docs consistency audit, final parity report (historical Phase 51 snapshot)

### Phase 37: Parity capability manifest and validator — complete

- [x] `docs/parity/pproxy_capability_manifest.toml` — per-capability inventory with status, evidence, and diagnostics (counts live in the manifest itself)
- [x] `docs/parity/README.md` — tiers, layers, evidence, validation rules
- [x] Parity report — generated from the manifest via `scripts/validate_pproxy_parity_manifest.py --write-report` (generated artifact, not a committed claim)
- [x] `scripts/validate_pproxy_parity_manifest.py` — manifest validator with strict mode

### Phase 38: pproxy CLI native-equivalent closure — complete

- [x] `--ssl` generates TLS TOML config (Phase 42: applies to all compatible listeners)
- [x] `-b` generates `[[rules]] reject` entries
- [x] `--rulefile` translates pproxy rulefiles to `[[rules]]` with diagnostics
- [x] `-a N` generates `[health] interval = "Ns"`
- [x] `--pac` generates `[admin.pac] enabled = true`
- [x] `--test` translates and runs `eggress upstream test`, then exits
- [x] Compatibility `--sys` applies the selected bound listener through the existing system-proxy backend and restores prior settings; native inspection remains read-only.
- [x] `--log`, `--get` emit structured diagnostics

### Phase 39: pproxy URI grammar and chain semantics — complete

- [x] `__` chain separator
- [x] Modifiers (`+ssl`, `+tls`, `+in`)
- [x] `backward://`, `bind://`, `listen://` schemes
- [x] Default port inference (`default_port_for_scheme()`)
- [x] `parse_endpoint` relaxed for bare hosts

### Phase 40: Python pproxy-shaped migration API — historical complete

This phase delivered the bundled helper API, not strict pproxy drop-in parity.
Current acceptance is governed by the practical parity roadmap and manifest.

- [x] `PPProxyService` class — `from_args`, `from_uri`, `from_toml`, `from_file`, `start`, context manager
- [x] `CompatibilityReport` dataclass — tier, ok, warnings, unsupported, diagnostics, features, toml, parsed_uris, raw_args
- [x] `FeatureInfo` dataclass — feature_id, tier, supported
- [x] `check_pproxy_args()` returns `CompatibilityReport`
- [x] Updated `start_pproxy` — multiple input modes (args, local/remote, config, config_path)
- [x] `PPProxyHandle` type alias for `EggressHandle`
- [x] `.pyi` type stubs for all public modules
- [x] Credential redaction in repr and TOML output
- [x] Comprehensive test suite (296 lines)

### Phase 41: Differential parity harness — complete

- [x] Reusable harness in `eggress-testkit::differential` (455 lines)
- [x] Primary differential suite — 27 scenarios against pproxy 2.7.9 (2938+ lines)
- [x] Extended differential suite — 11 scenarios using reusable harness (1254 lines)
- [x] Python differential tests — 3 structural tests (126 lines)
- [x] Two-gate strategy: `EGRESS_REQUIRE_EXTERNAL_INTEROP` and `EGRESS_RUN_PPROXY_DIFFERENTIAL`
- [x] Parity manifest updated with differential evidence entries

### Phase 42: pproxy parity corrective consistency pass — complete

- [x] `CompatibilityReport.tier` uses the five-tier manifest vocabulary
- [x] `PPProxyService.from_args` preserves the full pproxy argument vector through `translate_pproxy_args`
- [x] Manifest validator gains Rule 12 (stale "not recognized"/"unknown-flag" wording) and Rule 13 (`config = not_applicable` justification)
- [x] Manifest stale wording fixed for `cli.alive`, `cli.ssl_listener`, `cli.block`, `cli.rulefile`, `cli.reuse`, `cli.get`, `cli.pac`, `cli.test`, `cli.sys`
- [x] `--ssl` applies TLS to all compatible listeners (matches pproxy, which loads the cert chain into every ssl context); new unit test
- [x] Parity report is now generated from the manifest (`--write-report`) and CI verifies consistency (`--check-report`)
- [x] Stale tier/notes fields harmonized in `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`, `README.md`, `AGENTS.md`, `.skills/testing/skill.md`, `docs/parity/README.md`

## Remaining Work

Post-milestone work covers advanced transport hardening, Python async API
refinements, and release automation. These are ongoing improvements rather than
blocking milestones.

The API-boundary/interoperability implementation phases have landed and the
narrow post-implementation corrective closure is complete (native write
contract restored behind a private async submit path, connection exception
identity converged, preview hop count corrected, startup-forwarding and Rust
API evidence added). Details remain in
[`plans/archive/phase-records/API_BOUNDARY_CORRECTIVE_CLOSURE.md`](../plans/archive/phase-records/API_BOUNDARY_CORRECTIVE_CLOSURE.md)
under the parent
[`plans/archive/phase-records/API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md`](../plans/archive/phase-records/API_BOUNDARY_AND_INTEROP_MAINTENANCE_ROADMAP.md).
Existing API surface and capability remain fixed constraints.
The final evidence-state polish pass (acceptance-checkbox reconciliation plus a deterministic native-write semantic proof) is complete in [`plans/archive/phase-records/API_BOUNDARY_EVIDENCE_STATE_POLISH.md`](../plans/archive/phase-records/API_BOUNDARY_EVIDENCE_STATE_POLISH.md); it did not reopen runtime/API scope.

### Completed maintenance — Eggfetch 0.2 HTTP CONNECT consolidation

The narrow dependency/conformance migration from
[`plans/archive/phase-records/EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](../plans/archive/phase-records/EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md)
has landed with response-parser Outcome B:

- Workspace MSRV is now 1.89 (`rust-toolchain.toml` pins 1.89.0) and
  direct Base64 usage is aligned to 0.23 (byte-identical auth behavior).
- `eggfetch-http-connect 0.2.0` (crates.io; no `eggfetch-core`, no
  git/path) owns outbound HTTP/1 CONNECT authority rendering
  (`ConnectTarget`) and request framing (`encode_connect_request()`);
  Eggress keeps its validation adapter, `validate_credentials()`,
  `HttpConnectLimits`, status-to-`HttpError` policy, and redaction, with
  no public-surface changes and no Eggfetch types leaked.
- Response parsing intentionally remains local: the public
  total-head limit accounting and shape-tolerant header counting cannot
  be reproduced exactly by the upstream 0.2.0 response limits.
- Full workspace suite, Clippy, `cargo fmt --check`, fuzz compile,
  exact-floor `cargo +1.89.0 check`, `cargo deny check`, and `cargo audit`
  pass; same-toolchain release binaries are within 0.1% of baseline and
  the CONNECT benchmark shows no material change.

Compatibility claims remain governed by the active [compatibility
matrix](parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md) and [capability
manifest](parity/pproxy_capability_manifest.toml); this pass changed no
claims.

### Completed maintenance — Eggfetch 0.2 closure

Post-implementation review found three narrow residuals: the shared request
serializer had been given a new 64 KiB request-head cap that did not exist in
the pre-migration Eggress contract; the Rust-1.85-era
`RUSTSEC-2026-0009`/`time` exception was removable under MSRV 1.89; and
`plans/README.md` retained stale active-parent bookkeeping.

The bounded corrective has landed as **IMPLEMENTED** in
[`plans/archive/phase-records/EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`](../plans/archive/phase-records/EGGFETCH_0_2_CORRECTIVE_CLOSURE.md).
It preserved the implemented Outcome B ownership split without reopening
protocol/API/capability scope. The Eggfetch runtime/dependency line is closed.

### Completed maintenance — Eggfetch 0.2 evidence/documentation cleanup

The final documentation-only consistency pass has landed as **IMPLEMENTED** in
[`plans/archive/phase-records/EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md`](../plans/archive/phase-records/EGGFETCH_0_2_EVIDENCE_DOCUMENTATION_CLEANUP.md).

It reconciled the implemented corrective plan's acceptance checkboxes with real
repository/CI/command evidence and removed stale live `RUSTSEC-2025-0134`
cargo-audit ignores now that `rustls-pemfile` is absent from the lockfile. The
separate `RUSTSEC-2023-0071` RSA exception remains outside this cleanup.
Runtime, dependency, API, capability, parity, and compatibility behavior were
fixed constraints. The Eggfetch runtime/dependency line remains closed.

## Next Phase

The bounded distribution/release-maintenance campaign in
[`plans/archive/phase-records/DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md`](../plans/archive/phase-records/DISTRIBUTION_AND_RELEASE_MAINTENANCE_ROADMAP.md)
is implemented and closed:

1. [`plans/archive/phase-records/PYPI_WHEEL_MATRIX_EXPANSION.md`](../plans/archive/phase-records/PYPI_WHEEL_MATRIX_EXPANSION.md) — **IMPLEMENTED**. Tier A ten-family `cp39-abi3` matrix plus sdist, matrix-driven validation, native ARM smokes, musl/ARMv7 execution smokes, ordinary CPython 3.9–3.15 qualification (3.15 RC-qualified). Tier B deferred per sequencing; physical SBC qualification documented as a pending one-time procedure.
2. [`plans/archive/phase-records/MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md`](../plans/archive/phase-records/MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md) — **IMPLEMENTED**. Graph-derived resumable local helper (`scripts/publish-crates.py`); crates.io publication stays local/manual. Native workspace publication is nightly-only on stable 1.89, hence Outcome B.

This campaign was release-engineering only. Existing Rust/Python/CLI/configuration/protocol behavior and pproxy compatibility claims remain fixed constraints. Free-threaded Python and automated crates.io publication remain explicitly out of scope.

Compatibility claims remain governed by the active [compatibility matrix](parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md)
and [capability manifest](parity/pproxy_capability_manifest.toml).
