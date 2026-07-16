# Eggress Roadmap

This document references the main roadmap in [EGGRESS_ROADMAP.md](../EGGRESS_ROADMAP.md).

## Current Phase

Phase 42: pproxy parity corrective consistency pass (complete)

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

- [x] Transparent proxy, Unix domain sockets, reverse proxy supervisor integration, H2/WS/Raw protocol-crate only, QUIC/H3 rejection, CLI native-equivalent closure

### Phase 29–32: Python API parity and hardening — complete

- [x] Python API parity inventory (114 entries), Python hardening (GIL release, tier normalization, evidence reclassification), Python packaging (py.typed, version metadata, capability introspection)

### Phase 36: Final parity release audit — complete

- [x] Frozen targets, manifest completeness audit, manifest corrections, docs consistency audit, final parity report (historical Phase 51 snapshot)

### Phase 37: Parity capability manifest and validator — complete

- [x] `docs/parity/pproxy_capability_manifest.toml` — 148 capabilities, 5 categories, 7 layers (updated by Track B/C closure)
- [x] `docs/parity/README.md` — tiers, layers, evidence, 13 validation rules
- [x] `docs/parity/PPROXY_PARITY_REPORT.md` — auto-generated from the manifest
- [x] `scripts/validate_pproxy_parity_manifest.py` — Python validator, 13 rules, strict mode

### Phase 38: pproxy CLI native-equivalent closure — complete

- [x] `--ssl` generates TLS TOML config (Phase 42: applies to all compatible listeners)
- [x] `-b` generates `[[rules]] reject` entries
- [x] `--rulefile` translates pproxy rulefiles to `[[rules]]` with diagnostics
- [x] `-a N` generates `[health] interval = "Ns"`
- [x] `--pac` generates `[admin.pac] enabled = true`
- [x] `--test` translates and runs `eggress upstream test`, then exits
- [x] `--sys` auto-invokes `eggress system-proxy inspect` before starting
- [x] `--log`, `--get`, `--reuse` emit structured diagnostics

### Phase 39: pproxy URI grammar and chain semantics — complete

- [x] `__` chain separator
- [x] Modifiers (`+ssl`, `+tls`, `+in`)
- [x] `backward://`, `bind://`, `listen://` schemes
- [x] Default port inference (`default_port_for_scheme()`)
- [x] `parse_endpoint` relaxed for bare hosts

### Phase 40: Python pproxy drop-in API — complete

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
- [x] Stale tier/notes fields harmonized in `docs/PARITY_MATRIX.md`, `README.md`, `AGENTS.md`, `.skills/testing/skill.md`, `docs/parity/README.md`

## Remaining Work

Phases 43+ are defined in `plans/pproxy_parity_python_dropin_roadmap.md` and cover advanced transport hardening, Python async API, CI integration, and release automation. These are post-release scope.

## Next Phase

Phase 43: (post-release scope — see roadmap)
