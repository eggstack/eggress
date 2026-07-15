# Track B/C hard closure and external certification pass

## Objective

Finish the remaining hard deliverables after the Track B/C corrective verification pass. This phase moves beyond classification cleanup and targets the actual missing behavior required for a defensible modern pproxy compatibility claim:

- advanced transports must compose through prior-hop streams rather than bypassing the chain;
- the Python connection layer must expose a real outbound connection primitive rather than primarily standing up a temporary local proxy;
- users must have an intentional, testable top-level `pproxy` import/distribution path;
- plugin and wrapper objects must either participate in the live data path or remain explicitly structural;
- cipher objects promoted as compatible must perform their advertised operations;
- external interoperability and differential evidence must be executed, retained, and tied to exact commits.

This phase is complete only when implementation, composition matrix, Python contracts, packaging, external evidence, and release documentation agree.

## Baseline

Current `main` already includes:

- Trojan TCP listener/upstream support, hardening, differential tests, and third-party test scaffolding;
- WS/WSS, raw/tunnel, and H2 upstream runtime handlers;
- explicit evidence that WS/raw/H2 currently open independent endpoint connections instead of consuming a prior-hop stream;
- corrected composition classifications for upstream-only and non-intermediate use;
- H2 pooling, flow control, GOAWAY/RST handling, metrics, and fault tests;
- Python `Connection`, `ProxyConnection`, `Server`, protocol, cipher, wrapper, plugin, and asyncio compatibility surfaces;
- contract extraction, provenance classification, behavioral probes, and extensive lifecycle tests;
- a 145-capability manifest and composition matrix;
- improved documentation of construction-only, metadata-only, and unsupported behavior.

The remaining gaps are implementation gaps rather than primarily reporting gaps.

# Workstream 1 — stream-native advanced transport composition

## Goal

Refactor WS/WSS, raw/tunnel, and H2 handlers so they can operate over the `BoxStream` produced by a preceding chain hop. Preserve direct endpoint dialing only for first-hop operation.

## Required architecture

Introduce or standardize two handler modes:

1. `connect_endpoint(endpoint, context) -> BoxStream`
2. `wrap_stream(prior_stream, endpoint_metadata, context) -> BoxStream`

A protocol must explicitly declare whether it supports:

- first upstream hop;
- intermediate hop over an existing stream;
- terminal hop;
- listener role;
- TLS wrapping over an existing stream;
- multiplexing over an existing transport.

The chain executor must reject a composition at compile time when the selected handler cannot consume the prior stream.

## WS/WSS tasks

1. Split TCP dialing from WebSocket client handshake.
2. Add `websocket_handshake_over_stream()` using the supplied stream.
3. Preserve Host, path, headers, SNI, authentication, subprotocol, and masking behavior.
4. Ensure WSS wraps the prior stream in TLS rather than opening a second TCP connection.
5. Support fragmented frames, ping/pong, close handshake, half-close approximation, and bounded frame/message sizes.
6. Add chain tests:
   - SOCKS5 → WS;
   - HTTP CONNECT → WS;
   - Shadowsocks → WSS;
   - WS as first hop;
   - invalid WS after non-stream transport.
7. Add packet/transcript assertions proving the preceding proxy sees the connection to the WS endpoint.

## raw/tunnel tasks

1. Define raw as a transformation over the incoming stream where a prior hop exists.
2. Prevent `RawHopHandler` from independently dialing when a prior stream is supplied.
3. Preserve fixed-target and URI semantics.
4. Add direct, first-hop, and intermediate-hop tests.
5. Verify cancellation and half-close propagation.

## H2 tasks

1. Split H2 endpoint dialing from client handshake/session creation.
2. Add an H2 connection constructor over an arbitrary async stream.
3. Include prior-hop identity in pool keys, or disable cross-chain pooling when stream provenance cannot be represented safely.
4. Never reuse an H2 connection across differing:
   - chain topology;
   - endpoint;
   - TLS roots or verification mode;
   - SNI;
   - authentication;
   - policy profile.
5. Define whether pooling is allowed only for first-hop H2 or also for stable prior-hop chains.
6. Add tests:
   - SOCKS5 → H2;
   - HTTP CONNECT → H2;
   - TLS prior hop → H2;
   - pool isolation by chain identity;
   - GOAWAY/RST recovery through a prior hop;
   - cancellation while waiting for stream capacity.
7. Add explicit listener-role tests or retain upstream-only classification.

## Acceptance criteria

- WS/raw/H2 intermediate-hop cells only return to `drop_in` after end-to-end prior-stream tests pass.
- Tests prove no independent endpoint TCP connection is created for intermediate-hop operation.
- First-hop pooling remains bounded and correct.
- Chain compiler errors are stable and composition-specific.
- Metrics distinguish first-hop versus wrapped-stream sessions.

# Workstream 2 — native outbound connection primitive

## Goal

Replace the temporary-local-listener implementation path as the canonical Python connection backend with a direct Rust outbound chain API.

## Rust API

Add a reusable outbound connector to `eggress-embed` or a dedicated crate:

```text
OutboundConnector::from_pproxy_uri(...)
OutboundConnector::from_config(...)
OutboundConnector::connect_tcp(host, port)
OutboundConnector::associate_udp(...)
```

The connector must:

- compile routing/upstream state without binding a listener;
- execute the same chain engine used by the runtime;
- return an owned async stream or datagram association;
- expose local/peer addresses and selected route/upstream metadata;
- support timeouts and cancellation;
- avoid global singleton state;
- release all tasks, descriptors, and pool leases on close/drop.

## Python API

Rebase `ProxyConnection` and the pproxy-compatible `Connection` adapter on the native outbound connector.

Required behavior:

- `tcp_connect(host, port)` returns a usable reader/writer or documented stream object matching pinned pproxy behavior;
- repeated connections from one connector are supported where pproxy permits;
- connect cancellation terminates Rust work promptly;
- errors map to stable pproxy-compatible exceptions;
- `peername`, `sockname`, `closed`, and extra metadata are accurate;
- sync and async entry points share one semantic backend;
- no visible local listener is opened;
- no nested Tokio runtime is created;
- GIL is not held during network I/O.

## UDP

Characterize pinned pproxy `Connection` UDP behavior before implementation. Add only the methods and semantics confirmed by the contract corpus.

At minimum model:

- association lifecycle;
- destination addressing;
- send/receive return values;
- timeout and cancellation;
- close behavior;
- unsupported-chain diagnostics.

## Tests

- direct TCP echo;
- HTTP, SOCKS5, Shadowsocks, Trojan, WS, raw, and H2 upstreams where supported;
- representative multi-hop chains;
- DNS failure, refusal, timeout, auth, TLS, and policy errors;
- cancellation during DNS, connect, handshake, and read;
- 1,000 create/connect/close cycles;
- concurrent connectors and repeated connections;
- descriptor/task/thread leak checks;
- exact pproxy behavioral probes for return types and exceptions.

## Acceptance criteria

- Python outbound connections no longer require a temporary bound listener.
- `Connection` contract tests exercise real stream I/O, not only service lifecycle.
- All connection resources are deterministic under cancellation and GC.
- The original service-oriented `Connection` wrapper is renamed or clearly separated if retained.

# Workstream 3 — top-level `pproxy` namespace and distribution strategy

## Goal

Allow existing supported applications to use `import pproxy` unchanged after intentionally installing the Eggress compatibility distribution.

## Decision

Choose and document one of these approaches:

### Preferred: separate compatibility distribution

Publish a small package with distribution name such as `eggress-pproxy-compat` that installs the `pproxy` Python package and depends on the matching `eggress` wheel.

Advantages:

- avoids silently hijacking the namespace in the primary Eggress package;
- makes replacement deliberate;
- permits strict version coupling;
- provides clear uninstall and rollback behavior.

### Alternative: explicit extra

An install extra may provide the namespace only if packaging tools can guarantee deterministic ownership and conflict diagnostics.

Do not install a top-level `pproxy` package silently from the default `eggress` wheel.

## Package requirements

The compatibility package must:

- export the classified public symbols at the expected module paths;
- provide `pproxy`, `pproxy.proto`, `pproxy.cipher`, and other supported submodules;
- expose the correct version/compatibility metadata;
- fail installation or emit a clear conflict when the original pproxy distribution is installed;
- pin an exact compatible Eggress version range;
- contain `py.typed` and stubs where appropriate;
- avoid duplicate native binaries;
- support clean uninstall without damaging Eggress.

## Test matrix

In isolated environments:

1. install original pproxy and run the compatibility corpus;
2. uninstall original pproxy;
3. install Eggress compatibility package;
4. run the same programs unchanged;
5. verify `import pproxy` resolves to the intended package;
6. test conflict scenarios and upgrade/downgrade;
7. test Linux, macOS, Windows, supported Python versions, wheel-only installation, and no source tree present.

## Acceptance criteria

- Supported `import pproxy` programs run unchanged.
- Package ownership and conflicts are deterministic.
- Distribution/version coupling is explicit.
- Documentation never imply that the base `eggress` wheel alone owns the namespace unless that is the chosen final policy.

# Workstream 4 — live-path plugin and wrapper integration

## Goal

Either make supported plugin/wrapper objects affect real runtime traffic or permanently classify them as structural compatibility only.

## Plugin execution contract

Characterize pinned pproxy plugin hooks:

- hook names;
- invocation order;
- arguments and return values;
- sync versus async callbacks;
- mutation/rejection semantics;
- error propagation;
- per-connection versus global state;
- reentrancy;
- cancellation and shutdown.

## Rust/Python bridge

For hooks promoted to behavioral parity:

- insert the callback at the real connection/stream lifecycle point;
- maintain bounded callback queues and concurrency;
- enforce timeout and cancellation;
- preserve context variables where relevant;
- avoid holding the GIL outside callback execution;
- prevent recursive reentry into the same connection;
- define fail-open/fail-closed policy explicitly;
- expose callback metrics without high-cardinality labels.

Add a generic stream decorator only where required. Avoid copying every payload through Python by default. Prefer connection-level hooks or bounded chunk callbacks matching pproxy behavior.

## Wrapper execution

Map wrapper objects to runtime config/chain construction:

- `TLS(inner, ...)` must produce an executable TLS-wrapped composition;
- `Chain([...])` must compile through the A2 composition graph;
- plugin wrappers must insert the selected callback;
- unsupported wrappers fail before network activity;
- serialized/copy objects preserve runtime-relevant fields.

## Tests

- plugin modifies or rejects a real live connection;
- callback exception, timeout, overload, cancellation, and shutdown;
- wrapper-generated TLS connection;
- wrapper-generated chain involving a prior-hop advanced transport;
- context and thread/loop identity;
- no callback after close;
- bounded memory under slow callbacks.

## Acceptance criteria

- Any plugin/wrapper marked behaviorally compatible is exercised on the live data path.
- Construction-only objects remain clearly classified and documented.
- Python callback integration does not become an unbounded throughput or memory hazard.

# Workstream 5 — cipher behavioral closure

## Goal

Implement direct cipher methods for every cipher class classified above structural compatibility, or demote it.

## Tasks

1. Re-characterize pproxy cipher constructors, key derivation, nonce state, packet/stream semantics, copy/pickle behavior, and failure modes.
2. Expose Rust crypto operations through bounded PyO3 methods for supported AEAD ciphers.
3. Implement:
   - encrypt/decrypt;
   - encrypt-and-digest/decrypt-and-verify where present;
   - packet cipher operations;
   - deterministic test-vector APIs only when reference semantics permit.
4. Maintain nonce/state ownership per object.
5. Do not expose key material in repr, exceptions, logs, or metrics.
6. Legacy cipher stubs remain warnings/unsupported until Track F.
7. Add known-answer vectors against pproxy/OpenSSL/reference implementations.
8. Add mutation, truncation, bad-tag, wrong-key, nonce exhaustion, and concurrency tests.

## Acceptance criteria

- Supported AEAD cipher objects perform the methods advertised by the Python contract.
- Stub classes cannot be counted as behavioral drop-in.
- Cipher state and copy/pickle behavior match the characterized contract or carry explicit divergence IDs.

# Workstream 6 — Trojan external certification closure

## Goal

Turn the Trojan interop scaffold into executed, retained, release-blocking evidence.

## Tasks

1. Use the shared testkit process guard and artifact model.
2. Replace panic-based unavailable handling with a clear prerequisite result outside release CI; release CI must fail if the required implementation is absent.
3. Eliminate `mem::forget` temporary-file handling; keep files owned by a guard and clean them on drop.
4. Run against at least one maintained external implementation, preferably two where practical.
5. Cover:
   - Eggress client → external server;
   - external client → Eggress server;
   - correct and wrong password;
   - SNI and custom CA;
   - IPv4/domain/IPv6 targets;
   - large transfer and half-close;
   - fragmented handshake;
   - abrupt peer shutdown;
   - concurrency;
   - fallback behavior if supported by pproxy.
6. Retain redacted logs and machine-readable result artifacts.
7. Tie manifest evidence IDs to the exact passing workflow and Eggress commit.

## Acceptance criteria

- External Trojan interoperability is mandatory in release certification.
- Test resources are cleaned deterministically.
- Failure diagnostics identify which implementation and direction failed.

# Workstream 7 — external certification pipeline

## Goal

Create one release workflow that produces a signed or checksummed compatibility evidence bundle for the exact commit.

## Required tiers

1. Rust workspace format, check, clippy, and tests.
2. Python full suite across supported versions.
3. API contract extraction and behavioral probes against pinned pproxy.
4. Core and extended differential oracle suites.
5. Advanced transport composition tests.
6. Trojan external interoperability.
7. Packaging and top-level namespace tests.
8. Linux/macOS/Windows platform jobs.
9. Wheel smoke tests in clean environments.
10. Fuzz smoke and resource-leak stress tests.

## Evidence bundle

Produce:

- Eggress commit and build metadata;
- pproxy version and environment;
- capability manifest and composition matrix digest;
- per-scenario machine-readable outcomes;
- external implementation versions;
- Python contract/classification digest;
- package install/import results;
- skipped test list with release policy;
- redacted failure artifacts;
- checksums.

The parity report generator must reject release-blocking `drop_in` entries when the evidence bundle lacks the required passing scenario.

## Acceptance criteria

- Release claims can be reproduced from the bundle.
- No gated release-required test is silently skipped.
- Manifest evidence references resolve to exact scenario results.
- CI status is visible and required on `main` or release tags.

# Workstream 8 — parity and documentation cleanup

After implementation:

1. Re-run the 145-capability audit.
2. Split any remaining broad IDs by role or chain position.
3. Promote only cells with direct evidence.
4. Update README status without using a single aggregate percentage as the primary claim.
5. Update migration examples to distinguish:
   - exact drop-in;
   - compatible with warning;
   - Eggress-native alternative;
   - unsupported.
6. Remove or supersede stale release-candidate documents.
7. Add explicit instructions for the top-level compatibility package.
8. Document performance costs of Python callbacks and advanced-transport wrapping.
9. Ensure all generated reports are fresh and CI-verified.

# Sequencing

Recommended implementation order:

1. Native outbound connector foundation.
2. WS/raw stream wrapping.
3. H2 stream wrapping and pool-provenance correction.
4. Python `Connection` rebasing and contract probes.
5. Top-level `pproxy` compatibility distribution.
6. Live plugin/wrapper integration.
7. Cipher behavioral closure.
8. Trojan external certification cleanup and execution.
9. Full external evidence workflow.
10. Final manifest reclassification and documentation pass.

# Required verification commands

At minimum:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python -m pytest python/tests tests/compat
python scripts/validate_pproxy_parity_manifest.py --check
python scripts/validate_pproxy_parity_manifest.py --check-matrix
```

Release verification must additionally execute all environment-gated differential, external interoperability, packaging, platform, and stress suites.

# Final acceptance criteria

- WS/raw/H2 can consume a prior-hop stream for every promoted intermediate-hop cell.
- The Python connection API uses a native outbound connector and exposes real stream semantics.
- A deliberate compatibility distribution supports `import pproxy` unchanged.
- Plugins/wrappers marked compatible affect the live runtime path.
- Supported cipher objects implement advertised operations.
- Trojan external interoperability has retained passing evidence.
- Release CI emits a commit-bound evidence bundle.
- No release-required scenario is skipped.
- Manifest, composition matrix, Python contract, package behavior, and documentation agree.
- Remaining unsupported features are explicit and do not count toward strict parity.

## Out of scope

- SSH, QUIC/H3, transparent-proxy expansion, legacy Shadowsocks, SSR, and broad UDP chaining except where required to complete the characterized Python connection contract;
- adding new protocols unrelated to the current Track B/C closure;
- performance optimization beyond preventing unacceptable regressions and bounding callback/pool/resource behavior.
