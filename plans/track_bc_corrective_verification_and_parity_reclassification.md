# Track B/C corrective verification and parity reclassification pass

## Objective

Close the correctness and classification gaps revealed after implementation of Tracks B and C. This pass does not primarily add new protocol breadth. It verifies that promoted capabilities behave correctly in their claimed roles, repairs architectural shortcuts that bypass chain semantics, aligns Python objects with actual pproxy contracts, establishes an explicit import/package strategy, and prevents unsupported or structurally emulated behavior from being reported as drop-in parity.

The pass is complete only when every promoted B/C capability has composition-specific runtime evidence and every Python compatibility claim is backed by executable pproxy contract tests.

## Baseline

Current `main` includes:

- Trojan listener and upstream runtime paths;
- WebSocket/WSS and raw/tunnel upstream handlers;
- HTTP/2 CONNECT upstream support with pooling and fault handling;
- a pproxy API extractor and behavioral-probe corpus;
- Python `Connection`, `Server`, protocol, cipher, wrapper, plugin, and asyncio compatibility layers;
- extensive Python lifecycle and stress tests;
- composition-matrix and parity-manifest integration.

The main residual risks are:

1. Advanced transport handlers may open independent connections instead of consuming the prior-hop stream supplied by the chain executor.
2. Parity cells may be classified too broadly across listener, first-hop, intermediate-hop, and terminal-hop roles.
3. Trojan fallback and independent interoperability are incomplete or insufficiently evidenced.
4. The Python `Connection` object may model an embedded service rather than pproxy's low-level connection abstraction.
5. The C1 classifier marks too much of the pproxy surface as `internal_observed` and therefore non-blocking.
6. Protocol/cipher/plugin objects are structurally compatible in places where behavioral methods still raise `UnsupportedFeatureError` or are not connected to the live Rust data path.
7. No top-level `pproxy` namespace/package is currently provided, preventing literal import-level drop-in behavior.
8. Gated differential tests and commit-message test claims are not yet sufficient release evidence.

# Workstream 1 — Advanced transport chain semantics audit

## Goal

Prove or repair the stream-composition behavior of WebSocket, WSS, raw, tunnel, and H2 handlers.

## Required work

1. Trace the chain executor contract and document what each `HopHandler` receives and must return.
2. For each advanced transport handler, identify whether it:
   - wraps the supplied prior-hop stream;
   - ignores it and opens a fresh endpoint connection;
   - can only operate as the first upstream;
   - can operate as an intermediate hop;
   - can operate as the terminal transport.
3. Add instrumentation or test-only connectors that fail if a handler bypasses the supplied stream.
4. Repair handlers that are intended to support intermediate-hop composition:
   - WebSocket upgrade must run over the supplied stream when a prior hop exists;
   - WSS must apply TLS over the supplied stream, not redial directly;
   - H2 handshake/CONNECT must run over the supplied stream when chained;
   - raw/tunnel must preserve fixed-target semantics without bypassing prior hops.
5. If a protocol cannot safely consume an arbitrary prior-hop stream, restrict its matrix role to `first_upstream` or another accurate role.
6. Ensure pooling does not cross incompatible prior-hop contexts. H2 pool keys must include the complete effective transport route or pooling must be disabled when a prior hop is present.
7. Add explicit diagnostics for unsupported chain positions.

## Required tests

- direct → WS → target;
- SOCKS5 → WS → target;
- HTTP CONNECT → WSS → target;
- direct → raw/tunnel → target;
- SOCKS5 → raw/tunnel → target if supported;
- direct → H2 → target;
- SOCKS5 → H2 → target;
- HTTP CONNECT → H2 → target;
- three-hop representative chains;
- negative tests proving unsupported chain positions fail before socket creation;
- test connector proving no direct endpoint dial occurs when a prior-hop stream is supplied;
- pool isolation tests for H2 across different upstream routes and credentials.

## Acceptance criteria

- Every advanced transport cell identifies exact chain positions.
- Handlers either consume prior-hop streams correctly or are demoted to roles they actually support.
- H2 pooling cannot bypass routing, credentials, or prior-hop isolation.
- Composition-specific tests exist for every promoted cell.

# Workstream 2 — Listener/upstream role reclassification

## Goal

Split broad protocol parity claims into exact runtime roles.

## Required work

1. Extend composition IDs where needed to distinguish:
   - listener;
   - first upstream;
   - intermediate hop;
   - terminal fixed-target hop;
   - reverse transport;
   - Python-only construction object.
2. Audit WS, WSS, raw, tunnel, H2, and Trojan manifest entries against these roles.
3. Add listener-side runtime tests only for roles actually implemented.
4. Demote unsupported listener or intermediate-hop cells instead of inferring them from upstream support.
5. Update README, ADRs, Python metadata, and generated parity reports.
6. Add validator rules preventing protocol-wide `drop_in` claims when only one role is supported.

## Acceptance criteria

- No protocol-level entry masks role-specific gaps.
- Generated reports show role-specific counts and tiers.
- README wording uses "upstream", "listener", or "chain hop" precisely.

# Workstream 3 — Trojan closure verification

## Goal

Complete or accurately classify the remaining Trojan gaps.

## Required work

1. Verify client and server interoperability against an independent Trojan implementation in addition to pproxy.
2. Add fallback routing if the pinned pproxy target exposes functional fallback behavior.
3. Characterize and test:
   - valid and invalid password hashes;
   - domain, IPv4, and IPv6 targets;
   - SNI and certificate verification;
   - custom CA and explicit insecure mode;
   - fragmented handshake input;
   - oversized/malformed request frames;
   - server fallback behavior;
   - listener and upstream chain positions.
4. Confirm whether Trojan UDP exists in the pinned pproxy release. If absent or nonfunctional, record evidence. If functional, leave it unsupported with a release blocker or schedule a dedicated UDP phase.
5. Add metrics and diagnostics assertions for auth failure, parse failure, target refusal, TLS failure, and fallback.
6. Ensure constant-time authentication is preserved.

## Acceptance criteria

- Bidirectional independent interoperability passes.
- Fallback is implemented or explicitly classified with oracle evidence.
- No Trojan capability is promoted solely from synthetic echo tests.

# Workstream 4 — Python `Connection` semantic correction

## Goal

Determine whether the current `Connection` object matches pproxy's actual low-level semantics and repair it where necessary.

## Required work

1. Use the C1 extractor and behavioral probes to document the exact pproxy `Connection` constructor, attributes, methods, coroutine status, and state transitions.
2. Add real pproxy programs exercising:
   - `tcp_connect`;
   - UDP send/receive methods;
   - caller-specified destination per operation;
   - stream/reader/writer access;
   - close and wait behavior;
   - address inspection;
   - timeout and cancellation;
   - reuse of one object across operations if supported.
3. Compare the current Eggress object model. If it starts a managed proxy service rather than representing one outbound proxied connection, split the APIs:
   - retain the current managed object under an Eggress-native name;
   - implement a true pproxy-compatible `Connection` facade backed by Rust connection primitives.
4. Add Rust embed primitives needed for one-shot or reusable outbound connections without starting a listener service.
5. Implement TCP connection return values and I/O semantics compatible with pproxy.
6. Implement UDP only where the pproxy contract and Eggress transport support allow it; otherwise raise stable composition-specific errors.
7. Preserve loop affinity, GIL release, cancellation, and leak accounting from C5.

## Acceptance criteria

- Representative pproxy `Connection` programs run unchanged.
- `Connection` no longer substitutes service lifecycle for low-level connection semantics.
- TCP and supported UDP behavior are proven against pproxy.
- Existing Eggress-native service wrappers remain available under unambiguous names.

# Workstream 5 — C1 classification re-audit

## Goal

Make the frozen Python contract conservative enough for a real drop-in claim.

## Required work

1. Re-review all 105 extracted symbols, especially the 87 classified as `internal_observed`.
2. Promote to release-blocking any symbol that is:
   - documented upstream;
   - imported in upstream examples;
   - returned by public constructors;
   - referenced by plugins or registries;
   - exposed through `__all__`;
   - used by a representative third-party pproxy integration.
3. Add provenance fields to classification records:
   - documented;
   - example-used;
   - return-type reachable;
   - plugin-reachable;
   - registry-reachable;
   - private-name-only.
4. Require a written rationale for every non-blocking public or reachable symbol.
5. Add CI rules preventing mass reclassification to `internal_observed` without explicit evidence.
6. Expand behavioral probes for constructor defaults, object identity, mutation behavior, repr/str, equality, hashing, pickling, and exception timing.

## Acceptance criteria

- No public/reachable symbol is non-blocking without rationale.
- Classification changes are reviewed as API changes.
- The contract report clearly distinguishes private implementation details from de facto public surface.

# Workstream 6 — Top-level `pproxy` namespace and packaging strategy

## Goal

Enable literal import-level compatibility without creating unsafe package conflicts.

## Required decision

Choose and document one supported strategy:

1. a separately published compatibility distribution that installs the `pproxy` namespace and depends on Eggress;
2. an optional wheel/extra that provides the namespace;
3. a deliberate replacement distribution with conflict detection and migration tooling.

Do not silently inject `sys.modules` aliases as the primary strategy.

## Required work

1. Produce an ADR covering package ownership, PyPI naming, coexistence, uninstall behavior, import precedence, and security implications.
2. Implement the chosen namespace package with:
   - `import pproxy`;
   - expected submodules;
   - version metadata;
   - exported classes/functions/constants;
   - type stubs;
   - console entry point if required.
3. Add clean-environment install tests:
   - Eggress only;
   - compatibility package only;
   - existing Python pproxy installed first;
   - Eggress compatibility package installed first;
   - uninstall and reinstall sequences.
4. Fail clearly on unsupported coexistence rather than shadowing unpredictably.
5. Add package provenance and supply-chain metadata.

## Acceptance criteria

- A supported installation path makes unchanged `import pproxy` programs run.
- Namespace behavior is deterministic and documented.
- Packaging tests cover conflicts, upgrades, and uninstalls.

# Workstream 7 — Protocol, cipher, wrapper, and plugin behavioral audit

## Goal

Separate structural compatibility from executable compatibility.

## Required work

1. Inventory every public method on protocol, cipher, wrapper, and plugin objects.
2. Classify each method as:
   - fully functional;
   - delegated to Rust runtime;
   - construction-only;
   - unsupported with warning;
   - unsupported release blocker.
3. For methods such as `encrypt_and_digest` and `decrypt_and_verify`, either:
   - implement behavior compatible with pproxy;
   - expose a real Rust-backed operation;
   - or demote the capability and ensure the method fails with an explicitly documented incompatibility.
4. Determine whether plugin callbacks are actually inserted into the live stream/datagram path. If not, classify them as standalone callback utilities rather than pproxy plugin parity.
5. Add live-path tests showing callback invocation on real proxied traffic, including mutation, rejection, timeout, cancellation, and backpressure.
6. Verify wrapper objects alter runtime configuration rather than only metadata.
7. Ensure unsupported SSR/H3/SSH and legacy cipher objects do not inflate drop-in counts merely because constructors exist.

## Acceptance criteria

- Every public object method has behavioral tests or an explicit incompatibility classification.
- Plugin and wrapper parity requires live runtime execution.
- Stub cipher/protocol classes are not counted as drop-in.

# Workstream 8 — Differential evidence and release gating

## Goal

Convert gated tests and commit claims into reproducible release evidence.

## Required work

1. Run the A3 core and extended oracle tiers against pinned `pproxy==2.7.9` in CI or release workflows.
2. Persist machine-readable result artifacts tied to:
   - Eggress commit;
   - pproxy version;
   - Python version;
   - platform;
   - scenario schema version.
3. Require passing scenario IDs for each promoted B/C drop-in cell.
4. Add release validation that fails if:
   - evidence is missing;
   - evidence references an older implementation commit outside an allowed ancestry policy;
   - a scenario is skipped on all supported platforms;
   - a manifest cell is broader than the scenario topology.
5. Add independent interoperability jobs for Trojan and advanced transports where pproxy is not a sufficient oracle.
6. Publish normalized reports as workflow artifacts.

## Acceptance criteria

- Release parity reports are generated from actual result artifacts.
- Gated tests are mandatory in release workflows.
- No commit-message assertion substitutes for CI evidence.

# Workstream 9 — Security and resource review

## Goal

Ensure the new transports and Python bridges do not introduce bypasses or unbounded state.

## Required work

1. Review direct-dial behavior for routing and private-network policy bypass.
2. Verify H2 pool keys isolate credentials, routes, TLS policy, SNI, and prior-hop context.
3. Verify WS/H2 header/path parsing is bounded.
4. Verify plugin callback queues and task ownership remain bounded under cancellation and interpreter shutdown.
5. Verify Python wrappers do not retain secret material in repr, pickle, traceback, or generated config.
6. Add fuzz targets for advanced transport handshakes/config normalization where absent.
7. Add FD/task/thread leak tests for repeated advanced transport and Python low-level connection cycles.

## Acceptance criteria

- No advanced transport bypasses routing/security policy.
- All pools, queues, and callback concurrency are bounded.
- Repeated stress tests show no resource growth beyond explicit tolerances.

# Workstream 10 — Documentation and parity report cleanup

## Required work

1. Update README status language after reclassification.
2. Regenerate capability and composition reports.
3. Update ADRs to distinguish role-specific support.
4. Document Python import strategy and compatibility package installation.
5. Clearly label construction-only protocol/cipher objects.
6. Add migration examples for both Eggress-native and literal pproxy-compatible usage.
7. Mark all completed acceptance criteria in the relevant B/C phase plans only after tests pass.

# Required validation

Run at minimum:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python -m pytest python/tests tests/compat
python parity contract extraction/validation
composition matrix validation
manifest/report consistency checks
core and extended pproxy differential tiers
Trojan independent interoperability
advanced transport chain-composition tests
wheel/install/import conflict matrix
Windows and macOS supported subsets
```

Release evidence must include exact command lines and artifact references.

# Exit criteria

This corrective pass is complete when:

- WS/WSS/raw/tunnel/H2 chain roles are proven or accurately restricted;
- role-specific composition cells replace broad protocol-wide claims;
- Trojan fallback/interoperability is complete or explicitly demoted;
- Python `Connection` matches actual pproxy low-level behavior;
- C1 classification no longer hides reachable API behind `internal_observed`;
- a supported top-level `pproxy` installation strategy exists;
- protocol/cipher/plugin/wrapper methods are behaviorally functional or accurately classified;
- every promoted B/C drop-in cell references current differential/interoperability evidence;
- all security, leak, and bounded-resource tests pass;
- generated reports and documentation match the corrected implementation.

# Recommended commit sequence

1. Composition-role audit and immediate parity demotions.
2. Advanced transport prior-hop stream fixes.
3. H2 pool isolation and chain tests.
4. Trojan fallback/interoperability closure.
5. C1 classification re-audit.
6. True low-level Python `Connection` correction.
7. Namespace/package ADR and implementation.
8. Protocol/cipher/plugin live-path corrections.
9. Mandatory oracle/release evidence integration.
10. Security, resource, documentation, and final parity regeneration.

Do not defer reclassification until the end. Demote unsupported claims first, then promote individual cells only as their tests and evidence pass.