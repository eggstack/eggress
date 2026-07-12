# Eggress true pproxy parity roadmap

## Purpose

This roadmap defines the work required to move Eggress from strong common-path compatibility toward evidence-backed parity with the pinned Python `pproxy` reference implementation. It supersedes percentage-only parity claims. Capability counts remain useful, but release decisions must be based on user-visible behavior, protocol composition, Python API contracts, and differential evidence.

The roadmap is intentionally strict: parser support, URI translation, protocol-crate primitives, or documentation alone do not establish parity. A capability is only `drop_in` when the applicable parser, translator, configuration, compiler, runtime, CLI, Python, test, and documentation layers are complete.

## Current baseline

Eggress is already strong for common HTTP/SOCKS/TCP operation: mixed listeners, HTTP CONNECT, ordinary HTTP forwarding, SOCKS4/4a, SOCKS5, authentication, modern Shadowsocks AEAD, Trojan TCP, direct and selected proxied UDP, routing, schedulers, health checks, multi-hop TCP chains, reload, metrics, a pproxy-compatible binary, and Python service-management bindings.

The remaining parity gap is concentrated in high-value seams rather than the core relay loop:

- stale or over-broad parity classifications;
- incomplete reverse/backward configuration and runtime closure;
- protocol-crate-only WebSocket/raw/H2 features that are refused by the supervisor;
- incomplete low-level Python object compatibility;
- SSH, QUIC/H3, transparent-proxy, UDP-chain, legacy Shadowsocks, and SSR gaps;
- remaining CLI, rulefile, process, and connection-reuse differences;
- insufficient composition-level and oracle evidence.

## Frozen reference target

The primary compatibility oracle remains `pproxy==2.7.9` until deliberately changed. A second non-blocking lane may track the latest released pproxy version.

Every parity record must identify:

- reference version;
- supported platforms;
- roles: listener, upstream, chain hop, reverse endpoint, or Python-only helper;
- transport: TCP, UDP, Unix, TLS, WebSocket, H2, QUIC, SSH, or transparent interception;
- exactness tier;
- differential or interoperability scenario IDs;
- known divergence and security policy;
- release-blocking status.

## Definition of parity

A pproxy command, URI, configuration, or supported Python program should execute through Eggress unchanged and produce equivalent observable behavior, except for explicit platform limitations that also apply to pproxy.

Observable behavior includes:

- bind and connect semantics;
- wire protocol and framing;
- routing and scheduling decisions;
- authentication and failure behavior;
- stdout, stderr, and exit status where scripts consume them;
- cancellation, shutdown, half-close, timeout, and cleanup semantics;
- Python symbols, signatures, coroutine behavior, exceptions, and object lifecycle.

## Release terminology

- **Common-runtime parity:** mainstream HTTP/SOCKS/Shadowsocks/Trojan operation with corrected reverse mode and promoted existing transports.
- **Python drop-in parity:** low-level pproxy Python API contracts work, not only Eggress service helpers.
- **Modern pproxy parity:** all modern pproxy transports and operational surfaces, with legacy SSR/ciphers explicitly excluded.
- **Strict full parity:** all functional reference-version capabilities, including legacy surfaces, have compatible behavior or are proven nonfunctional upstream.

A release with intentional exclusions must not be called strict full parity.

# Track A — parity foundation

## A1: Baseline correction and source-of-truth repair

Reconcile the current manifest, generated report, README, roadmap, Python feature report, CLI diagnostics, and runtime behavior. Repair the reverse configuration regression and demote unsupported or protocol-crate-only claims. Ensure every recent correctness fix has regression evidence.

Exit gate: all published capability claims match executable behavior on current `main`.

## A2: Capability graph and composition matrix

Add a machine-readable graph of listener, upstream, transport, chain, reverse, TCP/UDP, authentication, platform, CLI, and Python dimensions. Generate documentation and validation from this graph. Unsupported combinations must fail during validation rather than at runtime.

Exit gate: every supported composition cell has an end-to-end scenario; no cell is inferred from another.

## A3: Differential oracle expansion

Convert the existing pproxy differential harness into a data-driven release oracle that captures command, environment, addresses, output, exit status, transcripts, lifecycle, errors, and cleanup. Require every release-blocking `drop_in` capability to cite a scenario or documented equivalent evidence.

Exit gate: parity reports are reproducible from pinned pproxy and current Eggress artifacts.

# Track B — complete existing partial runtime work

## B1: Reverse and backward proxy closure

Repair the config model, complete forward/reverse URI semantics, parallel control channels, reconnect/drain behavior, chain composition, Python exposure, security policy, and differential tests.

## B2: Trojan completion

Add external client/server interoperability, fallback routing, negative-path coverage, chain placement tests, complete Python exposure, and UDP only if the oracle confirms pproxy support.

## B3: WebSocket and raw runtime promotion

Promote existing protocol-crate implementations through config, compiler, supervisor, CLI, Python, chain, lifecycle, and hostile-peer tests.

## B4: HTTP/2 runtime promotion

Integrate H2 listener/upstream roles, ALPN, pooling, flow control, stream limits, GOAWAY/reset handling, chain behavior, CLI/Python exposure, and external interoperability.

# Track C — Python drop-in API

## C1: Public API contract freeze

Generate an executable inventory of pproxy exports, signatures, defaults, coroutine status, attributes, exceptions, and representations.

## C2: `Connection` compatibility

Implement pproxy's low-level TCP/UDP connection abstraction over Rust-owned state, including cancellation, timeouts, close/wait semantics, addresses, and resource cleanup.

## C3: `Server` compatibility

Implement listener lifecycle, handlers/callbacks, bound addresses, start/close/wait behavior, error propagation, and multi-listener semantics.

## C4: Protocol, cipher, and plugin objects

Reproduce public registries, factories, aliases, constructors, and extension points. Preserve a fast Rust path; only cross the GIL where the pproxy contract requires Python callbacks.

## C5: Asyncio semantic compatibility

Complete loop affinity, native awaitables, cancellation propagation, deterministic cleanup, async context management, and supported Python-version behavior.

# Track D — missing transports and platform facilities

## D1: SSH upstream

Implement host-key verification, known-hosts, password/key/agent authentication, encrypted keys, `direct-tcpip`, keepalive, reconnect, pooling, chaining, CLI/Python exposure, and OpenSSH interoperability.

## D2: QUIC and HTTP/3

First characterize pproxy's actual wire behavior. Then implement endpoint lifecycle, ALPN/certificates, streams/datagrams, limits, loss/timeout behavior, listener/upstream roles, and interoperability if the pinned target is functional.

## D3: Transparent proxy completion

Add Linux IPv6 destination recovery, TPROXY, transparent bind, applicable UDP interception, macOS PF destination recovery, privilege diagnostics, rollback, and real platform integration tests.

## D4: Unix and platform-specific closure

Complete Unix URI forms, permissions, cleanup, abstract namespaces where relevant, and accurate Windows diagnostics.

# Track E — UDP parity

## E1: SOCKS5 UDP exactness

Characterize pproxy framing and separate standards-compliant native mode from pproxy compatibility mode. Add bidirectional pproxy interoperability.

## E2: Multi-hop UDP

Introduce a general datagram-hop abstraction and implement supported combinations with bounded association/flow state, reply-source correctness, and compile-time rejection of impossible chains.

## E3: Transport-carried UDP

Implement Trojan, QUIC, or other UDP carriers only where confirmed by the oracle.

# Track F — legacy Shadowsocks and SSR

## F1: Legacy cipher compatibility

Add reference-version cipher names and aliases behind explicit compatibility/legacy policy, with test vectors and no implicit downgrade.

## F2: OTA

Implement only after wire characterization and bidirectional interoperability evidence.

## F3: SSR

If strict full parity remains the target, isolate SSR protocols/obfuscation in an optional crate with bounded parsers and external interoperability. Permanent exclusion requires the release to be named modern parity, not strict parity.

# Track G — CLI and process behavior

## G1: Remaining utilities

Close or exactly classify daemonization, `--get`, `--reuse`, logging, verbosity, alive checks, system-proxy mutation, probe behavior, signals, and exit behavior.

## G2: Rulefile and regex closure

Parse the complete pproxy rule grammar, preserve rule order/actions, use `regex` plus `fancy_regex` where needed, bound expensive matching, and provide line-numbered diagnostics.

## G3: Reuse and pooling

Characterize pproxy `--reuse`; implement safe pool keys, idle validation, limits, TLS/H2/SSH interactions, reload behavior, and oracle comparisons.

# Track H — security-preserving compatibility

## H1: Policy profiles

Provide `secure`, `pproxy-compatible`, and `legacy` profiles. Every relaxation must be explicit in diagnostics, JSON reports, logs, metrics, and Python reports.

## H2: Remaining hardening

Complete per-source limits, auth throttling, loop detection, private-network and DNS policy, bounded queues, practical secret zeroization, fuzzing, and mixed-protocol soak/resource tests.

# Track I — certification and release

## I1: Packaging completeness

Validate wheels, binaries, containers, crates, Homebrew where maintained, SBOM, signing, provenance, reproducibility, and clean-environment smoke tests.

## I2: External compatibility corpus

Maintain real pproxy commands, configs, rulefiles, Python programs, UDP topologies, reverse modes, and failure cases. Execute each against both implementations.

## I3: Performance and resource gates

Measure connection latency, throughput, concurrency, memory, descriptors, tasks, UDP state, Python callback overhead, multiplexing, and shutdown. Prevent semantic parity from becoming operationally unusable.

## I4: Final certification

Strict release requirements:

- no unclassified or unsupported functional reference capabilities;
- no protocol-crate-only capability presented as runtime parity;
- complete Python API contract coverage;
- composition matrix coverage;
- differential or independent interoperability evidence for every release blocker;
- green Linux, macOS, Windows, Python, packaging, fuzz-smoke, and security workflows;
- generated report tied to exact Eggress commit and pproxy version;
- no known high-severity correctness or security defect.

## Recommended sequence

1. A1, A2, A3.
2. B1, then B2-B4.
3. C1-C5.
4. E1-E3.
5. D1 and G1-G3.
6. D2-D4.
7. F1-F3 if strict legacy parity remains required.
8. H1-H2 and I1-I4.

The first implementation batch is A1-A3 plus B1 because it repairs the current truth model, creates the evidence machinery needed for all later work, and closes the most immediate runtime inconsistency.