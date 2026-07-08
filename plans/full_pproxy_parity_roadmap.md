# Full pproxy Parity Roadmap

## Goal

The goal of this line of work is literal pproxy parity, including a Python library drop-in replacement path, while preserving egress as a modern Rust-native proxy framework. The target is not merely "common pproxy-compatible behavior". It is a staged path where existing pproxy CLI invocations and common `import pproxy` Python programs either run unchanged or receive precise, manifest-backed diagnostics for the remaining gaps.

The compatibility strategy should distinguish three surfaces:

1. Native egress: secure, typed, Rust-first behavior with modern defaults.
2. pproxy compatibility mode: behavior shaped to match pproxy, including awkward or legacy semantics where feasible.
3. legacy parity features: SSR, OTA, legacy ciphers, SSH, QUIC/H3, daemon/sys mutation, and other surfaces that should be feature-gated or packaged as explicit extras.

## Current Assessment

egress has credible parity for common HTTP/SOCKS TCP proxying, direct upstreams, basic chains, TLS wrapping, standard Shadowsocks AEAD, standalone UDP, SOCKS5 UDP ASSOCIATE, routing, health-aware upstream selection, PAC/admin operations, and an embeddable Rust/Python service surface.

The remaining work is not just protocol implementation. The critical gap is consistency: the repo currently has multiple parity inventories and documentation surfaces with different vocabulary and some conflicting statuses. The first release-blocking task is to make one canonical manifest drive README claims, compatibility reports, CLI `pproxy check`, Python `supported_features()`, and differential evidence.

## Regex Compatibility Decision

pproxy rule semantics are Python-regex based. Rust `regex` cannot support the full Python `re`/PCRE-like feature set. For this line of work, the compatibility path should adopt a dual-backend rule evaluator:

- retain Rust `regex` for native egress fast-path matching where its grammar is sufficient;
- add `fancy_regex` for pproxy compatibility mode to support look-around and backtracking-oriented constructs that occur in Python-style rule files;
- detect unsupported constructs deterministically and report them as an upstream semantic gap rather than pretending byte-for-byte Python `re` parity;
- document remaining divergences in the manifest and migration docs;
- preserve native egress rules as a safer, faster, typed rule language.

This follows the same practical posture as eggsact-style compatibility: expand the supported feature set with `fancy_regex`, make the gap explicit, and treat residual mismatch as an upstream/runtime semantic mismatch rather than a silent behavior change.

## Tracks

### Track A: Honest Compatibility Closure

Track A turns the current repo into a defensible common-pproxy replacement. It does not try to implement every long-tail protocol. It eliminates contradictory claims, hardens oracle testing, makes CLI compatibility real, strengthens URI/rule semantics, expands HTTP/SOCKS exactness, and resolves immediate status conflicts such as Trojan server and protocol-crate-only surfaces.

Detailed handoff files:

- `plans/track_a_00_canonical_parity_contract.md`
- `plans/track_a_01_pproxy_oracle_harness.md`
- `plans/track_a_02_cli_drop_in_compatibility.md`
- `plans/track_a_03_uri_rule_regex_compatibility.md`
- `plans/track_a_04_http_socks_exactness.md`
- `plans/track_a_05_status_consistency_and_trojan_closure.md`

Track A exit criteria:

- one canonical parity manifest exists and all docs/tooling derive from or validate against it;
- `pproxy check --json`, README status tables, compatibility docs, and Python feature introspection agree;
- pproxy CLI alias/default behavior is implemented or has precise blockers;
- rulefile compatibility uses a `fancy_regex` compatibility backend with documented residual gaps;
- HTTP/SOCKS common path is backed by expanded differential evidence;
- contradictory Trojan/runtime support claims are eliminated.

### Track B: Python Drop-in Replacement

Track B focuses on `import pproxy` compatibility backed by Rust networking. It should ship a native `eggress` API and a compatibility `pproxy` module without conflating the two.

Core deliverables:

- Python package exposes native `eggress` and compatibility `pproxy` module surfaces.
- Console script `pproxy` starts egress compatibility mode.
- `pproxy.Connection(proxy_uri)` supports async `tcp_connect(host, port)` and `udp_sendto(host, port, data, callback)`.
- `pproxy.Server(uri)` supports `start_server(args)` and returns a closeable/waitable handler.
- Async lifecycle does not block the Python event loop or hold the GIL across long-running Rust network operations.
- `.pyi` stubs and `py.typed` describe both native and compatibility APIs.
- README pproxy Python API examples run unchanged for supported protocols.

Track B exit criteria:

- common pproxy client/server examples pass under the egress-backed `pproxy` module;
- pytest-asyncio coverage includes repeated start/stop, cancellation, concurrent clients, TCP, UDP, and chained URI examples;
- package extras make legacy/long-tail protocols explicit.

### Track C: Long-tail Protocol and Legacy Parity

Track C implements or feature-gates the long-tail pproxy surface.

Core work areas:

- Shadowsocks legacy ciphers, base64 cipher strings, and OTA marker compatibility;
- SSR parser/server/client/plugin support behind legacy feature flags;
- SSH upstream transport behind an optional feature;
- runtime/CLI/Python wiring for WebSocket, raw tunnel, tunnel target forms, and echo;
- H2 runtime wiring and a separate QUIC/H3 decision/implementation path;
- Linux TPROXY, Linux IPv6 original destination, transparent bind, and macOS PF;
- full UDP chain semantics where pproxy supports them;
- pproxy-compatible `--daemon`, `--sys`, `--get`, `--test`, `--log`, and verbosity behavior.

Track C exit criteria:

- remaining intentional non-parity items are either implemented behind explicit features or formally excluded from the project definition of parity;
- `cargo xtask parity-certify` can generate a release-grade parity report from actual test output.

## Certification Model

Add `cargo xtask parity-certify` after Track A establishes the manifest. The command should run:

- manifest schema validation;
- README/docs/Python/CLI consistency checks;
- Rust unit/integration tests;
- Python package smoke tests;
- non-privileged pproxy differential tests;
- gated differential tests when `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` and dependencies are present;
- generated `docs/parity/PARITY_REPORT.md` update/check.

No feature should be marked `drop_in` unless it has differential evidence or an explicit, manifest-recorded reason differential testing is impossible.

## Release Language Until Full Closure

Use precise wording before Track C completes:

"egress is a Rust-native pproxy-compatible proxy framework with strong drop-in coverage for common HTTP/SOCKS/TCP and selected UDP workflows, plus explicit diagnostics for long-tail pproxy features not yet implemented."

Avoid unqualified "full pproxy parity" or "final parity certification" until the certification command and manifest support that claim.
