# Real pproxy Parity Roadmap

This roadmap defines the long-term incremental path from the current Eggress release-candidate boundary to real pproxy parity. It intentionally uses a stricter definition of parity than the existing release-candidate documents.

The two end goals are:

1. Feature parity with Python `pproxy` for the full practical protocol and CLI surface.
2. A drop-in Python replacement whose public API shape can satisfy existing `pproxy` library consumers while offloading networking and protocol execution to Rust.

This is a bird's-eye roadmap, not an implementation checklist. Each phase should receive its own detailed handoff plan before execution.

## 0. Definitions

### Real feature parity

A feature is not considered truly pproxy-compatible until all of the following are true:

- Eggress implements the same user-visible behavior for the relevant pproxy version.
- The behavior is exercised through the same or equivalent CLI syntax where pproxy exposes CLI syntax.
- The behavior is exercised through the Python compatibility API where pproxy exposes a Python library surface.
- Differential tests run against a real Python `pproxy` installation, not only against synthetic fixtures.
- Interoperability tests run against standard third-party implementations where the protocol has an external ecosystem, such as `shadowsocks-rust`, `trojan-go`, OpenSSH, curl, browser clients, and OS transparent-proxy tooling.
- Documentation labels the feature as compatible only after implementation, tests, interop evidence, and migration notes are complete.

### Drop-in Python replacement

A Python compatibility layer is drop-in only when a Python user can replace imports and common object construction patterns with minimal or no code changes. The goal is not only to expose convenient Eggress-native Python bindings; the goal is to reproduce pproxy's public module layout, constructor shape, lifecycle semantics, URI parsing behavior, flag parsing behavior, exceptions, and async behavior closely enough that existing `pproxy` users can migrate without rewriting their applications.

Where exact compatibility is impossible or unsafe, the compatibility layer must fail with pproxy-shaped exceptions and clear diagnostics rather than exposing Rust-native errors directly.

## 1. Current capability boundary

Eggress currently has a strong Rust proxy core for common HTTP/SOCKS/TCP cases and a useful Python embedding surface. It should be treated as a credible foundation, not as real pproxy parity.

Current strengths:

- HTTP CONNECT listener and upstream support.
- SOCKS4/SOCKS4a CONNECT listener and upstream support.
- SOCKS5 CONNECT listener and upstream support.
- SOCKS5 UDP ASSOCIATE support.
- Mixed-protocol listeners.
- TCP multi-hop chains for supported protocols.
- TLS listener/upstream wrapping.
- TOML configuration, validation, reload, admin API, PAC/static serving, metrics, and route explanation.
- Rust embedding via `eggress-embed`.
- Python bindings and pproxy-oriented helper functions.

Current non-parity areas that must be treated as real gaps:

- Shadowsocks TCP is not wire-compatible with standard Shadowsocks AEAD framing.
- Shadowsocks inbound server support is absent.
- Shadowsocks UDP server support is absent.
- Legacy Shadowsocks stream ciphers, OTA, and SSR are absent.
- Standalone pproxy-style UDP `-ul` / `-ur` behavior is absent.
- UDP multi-hop chains are absent.
- Trojan inbound server support and fallback behavior are absent.
- SSH upstream transport is absent.
- Redirect/PF/transparent proxy support is absent.
- Unix-domain socket listeners are absent.
- SOCKS4/SOCKS5 BIND commands are absent.
- Persistent ordinary HTTP forwarding is incomplete.
- HTTP/2 CONNECT, HTTP/3 CONNECT, QUIC, WebSocket tunnel, raw tunnel, and reverse/backward proxying are absent.
- System proxy configuration is absent.
- The Python package is Eggress-native with pproxy helpers, not a pproxy-shaped public API clone.
- Differential and interop tests are still gated and not part of normal release evidence.

## 2. Workstream structure

The roadmap is organized into five parallel workstreams that should advance in a controlled order.

### Workstream A: Evidence, differential testing, and compatibility discipline

This workstream prevents accidental self-certification. It must start first and continue throughout all later protocol phases.

### Workstream B: Core protocol parity

This workstream fills the missing proxy protocols and repairs partial implementations.

### Workstream C: CLI, URI, flag, and config parity

This workstream makes Eggress behave like pproxy at the process boundary.

### Workstream D: Python drop-in API parity

This workstream builds a compatibility package/module that mirrors pproxy's Python-facing object model.

### Workstream E: packaging, platform, and operational release hardening

This workstream makes parity reproducible across Linux, macOS, Windows, CI, PyPI, and release artifacts.

## 3. Phase roadmap

## Phase 18: Parity evidence harness and pproxy oracle

Goal: establish a mandatory, automated compatibility harness before implementing more long-tail protocols.

Scope:

- Pin the target pproxy version initially to `pproxy==2.7.9`.
- Add a compatibility manifest that enumerates every protocol, flag, URI scheme, Python API symbol, and behavior under test.
- Build an oracle runner that can launch real pproxy processes with controlled local echo servers, UDP echo servers, TLS fixtures, upstream proxies, and chained topologies.
- Move current gated differential tests into a first-class CI job where external dependencies are installed automatically.
- Add fixture capture for pproxy wire behavior, handshake error behavior, stdout/stderr shape, exit codes, and signal handling.
- Add compatibility labels: `unimplemented`, `implemented-synthetic`, `implemented-differential`, `implemented-interop`, `compatible`, `intentional-non-parity`.
- Make `compatible` unavailable unless the differential or interop job has passed for the feature.

Acceptance criteria:

- CI can install pproxy, run oracle cases, and compare Eggress behavior.
- Every existing compatibility claim is backed by a manifest entry.
- Current gaps remain documented, but no gap is hidden behind an ambiguous `supported` label.
- Test output produces a machine-readable parity report.

## Phase 19: HTTP/SOCKS baseline closure

Goal: finish the conventional proxy baseline before moving into anti-censorship and transport protocols.

Scope:

- Implement persistent ordinary HTTP forward-proxy sessions.
- Support request pipelining where pproxy behavior requires it, or document exact pproxy divergence if pproxy's behavior is non-standard.
- Harden absolute-form to origin-form rewriting under keep-alive reuse.
- Add full HTTP proxy authentication differential tests, including missing auth, malformed auth, wrong password, multiple headers, and connection reuse after failure.
- Add SOCKS4/SOCKS4a pproxy differential tests.
- Add SOCKS5 edge-case differential tests for auth negotiation, IPv4, IPv6, domain targets, malformed commands, unsupported commands, and close behavior.
- Implement or intentionally classify SOCKS4 BIND and SOCKS5 BIND after direct pproxy behavior capture.
- Add curl/browser smoke tests for HTTP and SOCKS listeners.

Acceptance criteria:

- HTTP CONNECT, ordinary HTTP forward proxying, SOCKS4/4a CONNECT, and SOCKS5 CONNECT are all `compatible` in the parity manifest.
- Persistent HTTP forwarding is no longer marked partial.
- Unsupported SOCKS commands produce pproxy-matching errors or explicitly documented pproxy-compatible rejection behavior.

## Phase 20: Standalone UDP and UDP chain parity

Goal: reproduce pproxy's UDP behavior, not just provide SOCKS5 UDP ASSOCIATE.

Scope:

- Implement pproxy-style standalone UDP listen mode equivalent to `-ul`.
- Implement pproxy-style UDP remote/upstream behavior equivalent to `-ur`.
- Preserve SOCKS5 UDP ASSOCIATE as a standards-compliant Eggress-native mode, but separate it clearly from pproxy-compatible standalone UDP mode.
- Add UDP route selection semantics for direct, SOCKS5 upstream, Shadowsocks upstream, and chained upstreams.
- Implement UDP multi-hop chain support where pproxy supports it.
- Define connectionless association lifecycle, NAT-style flow tracking, idle reaping, error propagation, and metrics.
- Add amplification controls and client pinning without breaking pproxy-compatible behavior.
- Add differential tests using real pproxy UDP listeners/remotes and local UDP echo fixtures.

Acceptance criteria:

- `-ul` and `-ur` compatibility paths exist.
- UDP direct relay, UDP through SOCKS5, UDP through Shadowsocks, and pproxy-compatible UDP chains are tested against real pproxy.
- SOCKS5 UDP ASSOCIATE remains available but is no longer used as a substitute for pproxy standalone UDP parity.

## Phase 21: Shadowsocks standardization and inbound server parity

Goal: replace the current partial Shadowsocks implementation with real wire-compatible Shadowsocks support.

Scope:

- Replace non-standard Shadowsocks TCP AEAD framing with SIP003-compatible stream framing.
- Add interop tests against `shadowsocks-rust` for TCP and UDP.
- Implement Shadowsocks inbound listener/server mode for TCP.
- Implement Shadowsocks inbound UDP server mode.
- Support AEAD methods required for pproxy parity: `aes-128-gcm`, `aes-256-gcm`, and `chacha20-ietf-poly1305`.
- Inventory pproxy's legacy stream cipher behavior and decide whether to implement behind a legacy feature gate.
- If legacy stream ciphers are implemented, isolate them behind explicit insecure-compat flags, warnings, and tests.
- Implement pproxy-compatible Shadowsocks URI parsing, method/password handling, and error behavior.
- Add differential tests against real pproxy and interop tests against standard Shadowsocks implementations.

Acceptance criteria:

- Shadowsocks TCP upstream is no longer experimental.
- Shadowsocks TCP and UDP are wire-compatible with standard implementations.
- Shadowsocks inbound listener support exists for pproxy-compatible configurations.
- Legacy cipher support is either implemented behind explicit compatibility gates or documented as a conscious final non-parity item with security rationale.

## Phase 22: ShadowsocksR and legacy Shadowsocks compatibility

Goal: address pproxy's long-tail Shadowsocks/SSR compatibility surface separately from modern Shadowsocks.

Scope:

- Capture pproxy behavior for SSR schemes, obfs parameters, protocol parameters, and URI parsing.
- Add SSR URI grammar support without enabling SSR runtime behavior by default.
- Implement the minimum SSR protocols and obfs modes needed to match pproxy's documented behavior.
- Add `plain`, `origin`, `http_simple`, `tls1.2_ticket_auth`, `verify_simple`, and `verify_deflate` support if confirmed in the target pproxy version.
- Keep SSR under an explicit feature gate due to protocol age and maintenance burden.
- Add pproxy differential tests and fixture-driven codec tests.
- Document security limitations and compatibility warnings.

Acceptance criteria:

- SSR behavior is either implemented to pproxy parity under a compatibility feature or explicitly excluded from the final parity definition with a documented security/maintenance decision.
- The user-facing compatibility layer gives pproxy-shaped diagnostics for unsupported SSR combinations.

## Phase 23: Trojan full parity

Goal: move Trojan from upstream-client support to pproxy-compatible client/server behavior.

Scope:

- Implement Trojan inbound listener/server mode.
- Implement Trojan TCP request parsing, authentication, domain/IP targets, and close behavior.
- Implement Trojan fallback routing behavior if present in pproxy's behavior surface.
- Add real TLS server configuration, certificate loading, reload behavior, and SNI handling.
- Add pproxy differential tests for Trojan listener/upstream behavior.
- Add interoperability tests against a standard Trojan implementation such as `trojan-go`, where practical.
- Ensure secrets and authentication failures are redacted consistently.

Acceptance criteria:

- Trojan client/upstream and server/listener are both supported.
- Trojan behavior is tested against real pproxy and at least one external implementation where possible.

## Phase 24: SSH upstream transport parity

Goal: implement pproxy-compatible SSH remote transport for `ssh://` upstream chains.

Scope:

- Choose and integrate a Rust SSH implementation, likely `russh`, only if it satisfies required interop semantics.
- Implement password authentication.
- Implement public-key authentication.
- Implement encrypted private-key loading if pproxy supports it.
- Implement host-key verification policy, known-hosts integration, and explicit insecure mode.
- Implement SSH agent support if required for pproxy API/CLI compatibility.
- Implement `direct-tcpip` channel forwarding.
- Implement SSH keepalives, reconnect behavior, and timeout semantics to match pproxy where observable.
- Support SSH through prior proxy hops if pproxy permits it.
- Add OpenSSH-server integration tests.
- Add pproxy differential tests for SSH upstream URI forms and failure modes.

Acceptance criteria:

- `ssh://` upstream chains work for TCP targets.
- Authentication forms match pproxy's practical behavior.
- Host-key behavior is explicit, secure by default, and compatible through opt-in settings where pproxy is permissive.

## Phase 25: Transparent proxy, redir, PF, and Unix sockets

Goal: implement the OS-facing local listener modes that pproxy supports.

Scope:

- Implement Linux `redir://` support using `SO_ORIGINAL_DST` for IPv4.
- Add IPv6 original-destination support where available.
- Implement Linux REDIRECT workflow documentation and tests in privileged CI or containerized integration environments.
- Evaluate TPROXY support separately from REDIRECT if needed for parity or operational value.
- Implement macOS PF original-destination recovery if pproxy-compatible behavior can be reproduced.
- Add Unix-domain socket listener support for `unix://`.
- Add startup capability checks and precise diagnostics for missing privileges or unsupported platforms.
- Add platform-specific integration tests and skip logic.

Acceptance criteria:

- Linux transparent TCP proxying works in documented iptables/nftables REDIRECT workflows.
- macOS PF behavior is either supported or explicitly documented as unsupported with evidence.
- Unix socket listeners work for local clients and are covered by tests.

## Phase 26: Advanced transports — HTTP/2, HTTP/3, QUIC, WebSocket, and raw tunnels

Goal: address pproxy's advanced transport and tunnel matrix.

Scope:

- Capture exact pproxy behavior for HTTP/2 CONNECT, HTTP/3 CONNECT, QUIC transport, WebSocket tunnels, and raw tunnels.
- Implement HTTP/2 CONNECT server and client with stream reset, GOAWAY, flow-control, and ALPN handling.
- Implement HTTP/3 CONNECT server and client if pproxy behavior is confirmed and stable enough to replicate.
- Implement QUIC client/server transport using `quinn` or another mature Rust QUIC stack.
- Implement WebSocket binary stream adapter, ping/pong, close semantics, and TLS-wrapped WSS mode.
- Implement raw fixed-target TCP and UDP forwarding where pproxy exposes it.
- Add interop tests with curl, h2/h3 clients, and pproxy.
- Define route validation so unsupported transport/protocol combinations fail at startup.

Acceptance criteria:

- Each advanced transport has a pproxy behavior capture before implementation.
- Compatible features are tested with real pproxy and standard clients.
- Unsupported combinations produce deterministic startup errors rather than runtime surprises.

## Phase 27: Reverse/backward proxying and jump semantics

Goal: reproduce pproxy's jump and backward-jump capability if retained in the final parity target.

Scope:

- Capture pproxy's reverse endpoint registration protocol, authentication, stream multiplexing, reconnect behavior, heartbeats, and routing semantics.
- Design a Rust reverse control channel that can preserve pproxy-compatible behavior while fitting Eggress's runtime model.
- Implement reverse listener registration and remote accept loops.
- Implement reconnect, re-registration, graceful drain, and heartbeat timeouts.
- Implement reverse TCP first; evaluate reverse UDP separately.
- Add pproxy differential tests using two local processes and NAT-like fixture boundaries.
- Add security review for exposed control-channel authentication and replay resistance.

Acceptance criteria:

- pproxy backward/jump examples can be reproduced with Eggress compatibility mode.
- Reverse mode has explicit authentication and lifecycle tests.

## Phase 28: CLI flag, URI, and process behavior drop-in closure

Goal: make the executable a real command-line replacement for pproxy where possible.

Scope:

- Complete support for pproxy's CLI flags, aliases, defaults, and diagnostics.
- Match pproxy exit codes for common success/failure cases.
- Match logging verbosity, quiet mode, debug mode, and stderr/stdout placement where practical.
- Implement `--daemon` only if needed for true drop-in replacement, or provide a compatibility shim that intentionally mimics pproxy's visible behavior while delegating process management.
- Implement `--ssl`, `--sys`, `--reuse`, `--rulefile`, `-b`, `-a`, `-s`, and other remaining flags according to captured behavior.
- Translate pproxy rulefiles losslessly or add a native evaluator for pproxy rulefile syntax.
- Support pproxy URI quirks, default ports, missing hosts, empty binds, credential encoding, IPv6 bracket behavior, and multi-hop separators exactly.
- Add golden tests for help text, parser behavior, warnings, generated config, and failure messages.

Acceptance criteria:

- A suite of real pproxy command lines from the pproxy README and examples can run under Eggress compatibility mode without modification.
- Parser and diagnostics differences are either eliminated or documented as intentional final non-parity.

## Phase 29: Python API shape discovery and compatibility shim

Goal: map pproxy's Python library surface before attempting a drop-in Python replacement.

Scope:

- Inventory pproxy's public modules, classes, functions, constructors, attributes, exceptions, coroutine behavior, and examples.
- Build an API snapshot test suite that imports pproxy and records module-level symbols, signatures where inspectable, repr behavior, exception types, and lifecycle behavior.
- Identify which APIs are stable public API versus incidental internals used by real-world examples.
- Search public GitHub usage examples of pproxy as a library and classify common import patterns.
- Decide package strategy:
  - `eggress` remains Eggress-native API.
  - `eggress.pproxy` exposes compatibility helpers.
  - Optional `pproxy` compatibility distribution or import shim exposes pproxy-shaped names backed by Rust.
- Define compatibility modes: strict pproxy mode, Eggress-extended mode, and insecure legacy compatibility mode.

Acceptance criteria:

- A complete Python API compatibility inventory exists.
- API compatibility tests can run against both real pproxy and the Eggress shim.
- The package/module strategy is settled before implementation.

## Phase 30: Python Server API compatibility

Goal: reproduce pproxy's core Python server construction and lifecycle semantics.

Scope:

- Implement pproxy-shaped server objects, constructors, and lifecycle methods.
- Match pproxy's async event-loop integration closely enough for existing users.
- Support programmatic listener/remotes construction from pproxy-style URIs.
- Support start, stop, close, wait, task cancellation, and context manager behavior as pproxy users expect.
- Translate Rust runtime handles into pproxy-shaped Python objects.
- Preserve pproxy-style exception categories and messages where users are likely to depend on them.
- Add tests that run the same Python snippets against pproxy and Eggress compatibility mode.

Acceptance criteria:

- Common pproxy library examples run with the Eggress-backed compatibility API.
- The compatibility API does not leak Eggress-native object shapes unless explicitly requested.

## Phase 31: Python Connection, Rule, Plugin, and Utility API compatibility

Goal: cover the secondary Python APIs that real pproxy library users may import.

Scope:

- Implement pproxy-compatible URI parsing utilities.
- Implement pproxy-compatible rule parsing/evaluation utilities where exposed.
- Implement pproxy-compatible protocol registration or plugin hooks if pproxy exposes them as public API.
- Implement pproxy-compatible logging configuration utilities.
- Implement pproxy-compatible cipher/method utilities for Shadowsocks and SSR if exposed.
- Add import-compatibility tests for examples and real-world usage snippets.
- Provide clear deprecation or unsupported diagnostics for internals that cannot be safely replicated.

Acceptance criteria:

- Public utility imports used in real pproxy examples resolve successfully.
- Unsupported internals fail predictably and are documented.

## Phase 32: Python packaging as a drop-in replacement

Goal: make installation and import behavior compatible enough for real migration.

Scope:

- Decide whether to publish only `eggress` or also a separate compatibility package that provides a `pproxy` module backed by Eggress.
- If publishing a `pproxy`-named compatibility package is not acceptable, provide a documented import shim and migration helper.
- Ensure wheels include native Rust modules, type hints, license files, and platform tags.
- Add Linux, macOS, and Windows wheels for the Python compatibility layer.
- Add tests for clean virtualenv installation and import behavior.
- Add compatibility metadata that exposes target pproxy version and compatibility level.
- Add migration documentation for library users, not just CLI users.

Acceptance criteria:

- A Python user can install the package in a clean environment and run pproxy-compatible examples using the chosen import strategy.
- Wheel tests cover all supported OS targets.

## Phase 33: System proxy integration and platform convenience parity

Goal: match pproxy's platform-facing convenience features.

Scope:

- Implement macOS system proxy configuration and restoration if pproxy supports it on the target platform.
- Implement Windows system proxy configuration and restoration.
- Implement Linux desktop environment proxy integration only if pproxy exposes comparable behavior.
- Add crash-safe restoration logic.
- Add dry-run mode and visible diagnostics.
- Add tests using platform mocks where real OS mutation is unsafe in CI.

Acceptance criteria:

- System proxy commands are supported or explicitly documented as final non-parity with rationale.
- Failure paths restore previous state where possible.

## Phase 34: Performance, soak, and adverse network validation

Goal: prove that Rust parity does not merely pass functional tests but behaves well under real network pressure.

Scope:

- Add pproxy-vs-Eggress benchmarks for connection setup, steady-state TCP relay, UDP relay, HTTP CONNECT, SOCKS5 CONNECT, Shadowsocks, and multi-hop chains.
- Add long-running soak tests for mixed TCP and UDP workloads.
- Add adverse network tests: half-close, reset, slowloris-style handshakes, fragmented SOCKS/HTTP handshakes, DNS failures, upstream stalls, TLS failures, and idle timeout races.
- Add resource exhaustion tests for associations, connections, parser buffers, rule matching, and auth failures.
- Add flamegraph/perf capture scripts.
- Track memory, fd count, task count, and scheduler state over time.

Acceptance criteria:

- Eggress meets or exceeds pproxy performance in the common cases, or differences are documented.
- No unbounded resource growth appears in soak tests.
- Release notes include performance evidence, not only correctness evidence.

## Phase 35: Security review and legacy compatibility containment

Goal: support pproxy parity without letting insecure legacy modes contaminate safe default operation.

Scope:

- Split default mode, strict pproxy compatibility mode, and insecure legacy compatibility mode.
- Gate stream ciphers, OTA, SSR, permissive TLS, permissive SSH host-key behavior, transparent proxying, and system proxy mutation behind explicit flags/config.
- Add security warnings that are visible but not noisy.
- Add rate limits for auth failures and handshake abuse.
- Add per-source limits and private-network egress policies.
- Add DNS policy and DNS rebinding-aware routing.
- Add proxy-loop detection.
- Add secret zeroization where practical.
- Complete unsafe-code audit and keep `unsafe_code = "forbid"` unless a platform feature requires carefully isolated unsafe code.

Acceptance criteria:

- Full compatibility can be enabled deliberately, but safe defaults remain safe.
- Security-sensitive compatibility toggles have tests and documentation.
- The compatibility layer never silently enables insecure legacy behavior.

## Phase 36: Final parity release audit

Goal: produce a defensible 1.0-style parity claim.

Scope:

- Freeze the target pproxy version for the parity release.
- Run the full parity manifest in CI and locally.
- Run all protocol interop suites.
- Run all Python API compatibility examples.
- Run package installation tests across OS targets.
- Run security, audit, deny, SBOM, and release signing checks.
- Update README, parity matrix, migration guide, Python bindings guide, and release notes.
- Publish a feature-by-feature compatibility table with evidence links.

Acceptance criteria:

- No feature is marked compatible without automated evidence.
- All intentional non-parity items are explicit, justified, and stable.
- The Python compatibility layer has a documented target pproxy version and a tested public API surface.
- Release artifacts are signed, reproducible where practical, and accompanied by SBOMs.

## 4. Suggested sequencing

The phases should not all be treated as equal priority. The practical order should be:

1. Phase 18 first, because the project needs a real pproxy oracle before more parity claims.
2. Phase 19 second, because HTTP/SOCKS baseline closure is the highest-value common path.
3. Phase 20 third, because UDP workflow parity is a visible CLI replacement gap.
4. Phase 21 fourth, because current Shadowsocks TCP is explicitly non-standard and should be corrected before layering SSR or broader claims.
5. Phase 29 in parallel with Phases 19-21, because Python API discovery can proceed before all protocol work is finished.
6. Phase 30 after Phase 29, because implementation should follow API capture.
7. Phases 22-28 according to demand: SSR, Trojan server, SSH, transparent proxying, advanced transports, reverse/jump, and CLI closure.
8. Phases 32-36 after the compatibility surface stabilizes.

A reasonable milestone structure is:

- Milestone A: pproxy-compatible HTTP/SOCKS/TCP core with mandatory differential CI.
- Milestone B: pproxy-compatible UDP and standard Shadowsocks.
- Milestone C: pproxy-shaped Python API for common server usage.
- Milestone D: long-tail protocol parity for Trojan, SSH, transparent proxying, SSR, and advanced transports.
- Milestone E: final packaging, platform integration, security containment, and parity release audit.

## 5. Evidence policy for future work

Each phase must update the following artifacts when applicable:

- `docs/PARITY_MATRIX.md`
- `docs/PPROXY_PARITY_SPEC.md`
- `docs/PPROXY_MIGRATION.md`
- `docs/PYTHON_BINDINGS.md`
- `docs/TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md` or a successor release document
- README capability checkboxes
- Rust unit/integration tests
- Python compatibility tests
- pproxy differential tests
- external protocol interop tests

Do not mark any pproxy feature complete on documentation alone. A completion PR must contain implementation, tests, documentation, and evidence classification changes together.

## 6. Product positioning during the roadmap

Until Phase 36 is complete, the project should avoid saying "true pproxy parity" without qualifiers. Preferred wording:

- "pproxy-compatible for common HTTP/SOCKS/TCP workflows"
- "Rust-native pproxy-style proxy framework"
- "Python bindings with pproxy compatibility helpers"
- "long-term target: full pproxy feature and Python API parity"

Avoid wording such as:

- "drop-in replacement for pproxy"
- "full pproxy parity"
- "Shadowsocks compatible" unless standard interop tests pass
- "release candidate for true parity" unless the full parity manifest is active in CI

## 7. Key architectural decisions to make explicitly

Several roadmap items require conscious decisions before implementation:

- Whether insecure Shadowsocks stream ciphers and OTA are implemented at all, and under what feature gates.
- Whether SSR is part of final parity or a documented non-parity item.
- Whether a `pproxy`-named Python package/import shim will be published, or whether compatibility remains under `eggress.pproxy`.
- Whether `--daemon` is implemented literally or treated as a compatibility shim.
- Whether transparent proxying should require privileged integration tests before being marked compatible.
- Whether HTTP/3/QUIC behavior in pproxy is stable enough to reproduce or should be classified separately.
- Whether pproxy internals imported by third-party code count as API compatibility targets.

These decisions should be recorded in ADRs before the relevant phases begin.

## 8. Final target state

The desired endpoint is a Rust implementation that can be used in three ways:

1. `eggress` as a modern Rust-native proxy CLI and service.
2. `eggress` as an embeddable Rust/Python networking runtime.
3. A pproxy-compatible Python/API/CLI mode that can run existing pproxy workflows with minimal changes.

At that point, Eggress can credibly claim real pproxy parity only with an attached compatibility report that names the exact pproxy version, lists all supported features, lists all intentional divergences, and links each compatibility claim to tests or interop evidence.
