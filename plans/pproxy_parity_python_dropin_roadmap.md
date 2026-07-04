# pproxy parity and Python drop-in roadmap

## Purpose

This roadmap defines the remaining work needed to make eggress a credible pproxy replacement in two separate senses:

1. pproxy-compatible CLI/runtime behavior for supported pproxy command lines and URI forms.
2. Drop-in Python-library usability, where Python users can embed the Rust networking engine without rewriting their control flow around a different service model.

The current repo is already strong on the main proxy core: TCP listeners, mixed HTTP/SOCKS detection, HTTP CONNECT and ordinary HTTP forward proxying, SOCKS4/4a/SOCKS5 CONNECT, SOCKS5 UDP ASSOCIATE, direct routing, ordered upstreams, schedulers, health checking, multi-hop TCP chains, modern Shadowsocks AEAD, standalone pproxy-style UDP, admin/metrics, and PyO3 service primitives. This roadmap is not a rewrite plan. It is a closure plan for the remaining compatibility seams.

## Non-negotiable distinction

Do not mark a feature as pproxy parity merely because one layer can parse or partially represent it. A parity claim requires all applicable layers to be complete:

- URI parser behavior
- pproxy compatibility translator behavior
- native TOML/config compiler behavior
- runtime supervisor behavior
- CLI diagnostics and exit behavior
- Python binding exposure when relevant
- unit/integration/differential test evidence
- documentation and migration note

Protocol-crate-only implementations, such as current WebSocket/raw/H2 protocol pieces that are refused by the runtime/config compiler, must be classified as protocol-internal support, not runtime parity.

## Compatibility tiers

All future plans should use these terms consistently.

- `drop_in`: Existing pproxy usage should work unchanged through `eggress pproxy run -- ...` or through the Python compatibility shim.
- `compatible_with_warning`: Behavior is functionally usable but differs in a way that must be surfaced by diagnostics, docs, or JSON compatibility output.
- `native_equivalent`: Eggress supports the capability through native TOML/admin/Python APIs, but not through pproxy's exact CLI/API spelling.
- `intentional_non_parity`: The project deliberately rejects the pproxy feature, usually for security, maintenance, or legacy-protocol reasons.
- `unsupported`: Not implemented and not yet a deliberate product decision.

## Current high-level capability picture

### Strong or mostly complete

- HTTP CONNECT server/client
- ordinary HTTP forward proxying
- SOCKS4/4a CONNECT
- SOCKS5 CONNECT
- SOCKS5 authentication
- SOCKS5 UDP ASSOCIATE
- direct TCP and direct UDP
- modern Shadowsocks AEAD TCP/UDP
- HTTP/SOCKS TCP upstream chains
- three-or-more-hop TCP chains through native chain representation
- routing rules, schedulers, health checking, fallback, route explanation
- standalone pproxy UDP translation for `-ul`/`-ur`
- pproxy `translate`, `check`, and `run` subcommands
- PyO3 service primitives: config, service, handle, status, metrics, reload, shutdown

### Partial or mismatched

- pproxy CLI flags whose native equivalent exists but is not translated, especially `--ssl`, `-b`, `--rulefile`, `--pac`, `--test`, `--sys`, and `--log`
- pproxy URI grammar around raw `__` jump chains, repeated modifiers, and reverse/backward composition
- reverse/backward proxying beyond basic control-channel/session support
- Trojan: upstream/client exists, but server, fallback, and real interoperability tests are incomplete
- transparent proxying: Linux REDIRECT path exists, but IPv6 original-destination, TPROXY, transparent bind, macOS PF recovery, and PF tests remain incomplete
- Python API: useful eggress embedding exists, but not a pproxy-shaped drop-in module
- runtime availability for protocol-crate-only WebSocket/raw/H2 features

### Missing or intentionally excluded

- SSH upstream transport
- QUIC and HTTP/3 parity
- SSR and legacy ShadowsocksR behavior
- SOCKS4 BIND and SOCKS5 BIND
- UDP through Trojan
- UDP through multi-hop proxy chains
- release-grade packaging/signing/SBOM/reproducible artifact story

## Roadmap phases

### Phase 37: mechanical parity manifest and validator

Create a machine-readable parity manifest and CI validator. The manifest becomes the source of truth for README tables, CLI inventory, Python compatibility reports, and release go/no-go checks. It must prevent synthetic or parser-only evidence from being counted as runtime parity.

Primary deliverables:

- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/README.md`
- generated or maintained `docs/parity/PPROXY_PARITY_REPORT.md`
- validator script, preferably Rust if it fits the workspace, otherwise a small deterministic Python script under `scripts/`
- CI integration for manifest validation

### Phase 38: pproxy CLI native-equivalent closure

Translate pproxy CLI flags that already have native eggress equivalents. The goal is to reduce false unsupported/unknown diagnostics and make common pproxy commands run without hand-written TOML.

Priority flags:

- `--ssl cert[,key]`
- `-b PATTERN`
- `--rulefile` / `-rulefile`
- `--pac`
- `--test`
- `--sys`
- `--log`
- `-v` / verbosity mapping improvements
- `-a` alive interval mapping into health config where safe

Explicit decision item:

- `--daemon` and `--reuse` may remain intentional non-parity, but the reason must be recorded in the manifest and surfaced through structured diagnostics.

### Phase 39: pproxy URI grammar and chain semantics

Close the biggest drop-in parser mismatch: raw pproxy URI and modifier grammar. In particular, `__` jump-chain syntax should be accepted and translated into the native chain representation when all hops are supported.

Priority grammar surfaces:

- `__` chain separator
- `+ssl`, `+tls`, `+in`, repeated `+in`
- `bind://`, `listen://`, `backward://`, `rebind://`
- password-only userinfo for Trojan
- Shadowsocks method/password variants
- default port inference
- IPv6 bracket handling
- percent-encoded credentials
- path-style Unix socket forms
- rule query parameters

### Phase 40: Python pproxy-compatible API shim

Build a Python compatibility layer on top of the existing PyO3 service primitives. This is the key phase for drop-in Python use.

Target user-facing shape:

- `translate_pproxy_args(...)`
- `translate_pproxy_uri(...)`
- `check_pproxy_args(...)`
- `start_pproxy(...)`
- `serve(...)`
- `ProxyService` or `PPProxyService`
- context-manager lifecycle
- optional async lifecycle wrappers if practical
- stable exceptions mirroring pproxy-relevant failure classes
- typed status, metrics, reload, bound-address, and shutdown helpers

### Phase 41: differential parity harness expansion

Make pproxy itself the compatibility oracle for drop-in claims. The current project already has many unit and integration tests, but the release-quality parity claim needs a reusable harness that runs pproxy and eggress side by side where feasible.

Priority comparisons:

- HTTP CONNECT success/failure
- ordinary HTTP forward proxy requests
- SOCKS4/4a CONNECT
- SOCKS5 CONNECT and auth failure modes
- SOCKS5 UDP ASSOCIATE
- standalone UDP `-ul`/`-ur`
- scheduler behavior under multiple upstreams
- route/rule behavior for block and rulefile translations
- TLS listener behavior from `--ssl`
- CLI stdout/stderr/exit-code compatibility where scripts depend on it

## Later phases after 37-41

### Phase 42: Trojan server/fallback/interoperability completion

Finish Trojan server mode, fallback routing, real interop tests, and Python exposure.

### Phase 43: SOCKS BIND and UDP edge semantics

Implement or formally classify SOCKS4/SOCKS5 BIND, UDP through Trojan, UDP through multi-hop chains, and unsupported UDP chain diagnostics.

### Phase 44: runtime promotion or demotion of WebSocket/raw/H2

Either promote protocol-crate-only features into runtime/config/CLI/Python support or demote them in docs and manifest so they are not treated as pproxy parity.

### Phase 45: SSH compatibility decision and implementation plan

SSH upstream support is the largest remaining true-parity question. If full pproxy parity is non-negotiable, implement SSH transport, auth, host-key verification, direct-tcpip, keepalives, reconnect, and chaining. If not, classify SSH as intentional non-parity.

### Phase 46: QUIC/H3 compatibility decision and implementation plan

QUIC/H3 should only proceed after a design record. It is high risk and should not block mainline HTTP/SOCKS/Shadowsocks/Trojan-client parity unless the release definition requires exact pproxy protocol surface coverage.

### Phase 47: packaging, wheels, release artifacts

Finish PyPI wheels, binaries, container image, SBOM, artifact signing, reproducibility checks, and crates.io packaging decisions.

### Phase 48: security/robustness release gate

Complete per-source limits, auth failure rate limiting, loop detection, private-network/DNS policies, rebinding-aware routing, secret handling, fuzz corpus, soak tests, resource-exhaustion tests, and disclosure process.

### Phase 49: final parity certification

Generate the final parity report from the manifest, run all differential tests against pinned pproxy, verify package installs, and publish release notes with exact supported/unsupported surfaces.

## Release definitions

### Mainline pproxy-compatible release

This can ship once phases 37-41 plus packaging are complete, provided docs clearly state exclusions. This release should be honest: compatible for HTTP/SOCKS/Shadowsocks/Trojan-client TCP, SOCKS5/Shadowsocks UDP, standalone UDP, route/rule translation, and Python embedded service use.

### True full-parity release

This requires all mainline work plus SSH, SOCKS BIND, Trojan server/fallback, protocol-crate/runtime alignment, and explicit QUIC/H3/SSR decisions. If SSR remains intentionally excluded, do not call the result strict full parity; call it modern pproxy parity or pproxy-compatible minus legacy SSR.

## Global acceptance criteria

- No README capability claim may exceed the manifest tier.
- No parser-only implementation may count as runtime parity.
- No Python helper may be documented as drop-in unless it is imported and exercised by tests.
- Every unsupported pproxy feature must have a stable diagnostic code.
- Every intentional non-parity decision must have a documented rationale.
- Every `drop_in` feature must have either differential pproxy evidence or an explicit reason why differential testing is impossible.
- Common pproxy users must be able to start with `eggress pproxy check --json -- <args>` and get a useful compatibility report before running the service.
