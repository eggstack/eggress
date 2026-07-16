# Track B/C release closure: Python drop-in and certification pass

## Objective

Close the remaining release blockers after the Track B/C hard-closure implementation. This pass is intentionally narrow. It does not add new protocol breadth. It finishes the last incomplete surfaces required for a defensible modern pproxy-compatible release:

- expose native outbound TCP streams to Python through the Rust `OutboundConnector` rather than routing Python callers through a temporary local proxy;
- provide an intentional, testable top-level `pproxy` import and distribution path;
- make the AEAD cipher dependency policy deterministic across installations;
- execute and retain external differential and interoperability evidence tied to exact commits;
- run a complete cross-platform release validation matrix;
- reconcile every remaining `drop_in` capability with composition-specific evidence.

The pass is complete only when a supported pproxy Python program can install the compatibility distribution, `import pproxy`, establish a native outbound connection, use the documented lifecycle, and pass the pinned contract suite without hidden local listeners or undocumented optional behavior.

## Baseline

Current `main` includes:

- stream-native WS/WSS, raw/tunnel, and H2 upstream composition;
- H2 pool isolation by endpoint, TLS/auth context, and chain position;
- a Rust `OutboundConnector` with direct `connect_tcp()` and timeout support;
- a Python `OutboundConnector` wrapper that currently exposes construction and validation but not connected streams;
- a `ProxyConnection` compatibility facade that still uses a temporary local proxy for Python outbound I/O;
- working Python AEAD operations when `cryptography` is installed;
- an external Trojan interoperability harness;
- 145 manifest capabilities, including 101 currently classified `drop_in`;
- no top-level `pproxy` compatibility package.

## Release constraints

1. Do not promote a capability because an API symbol exists. Promotion requires equivalent behavior and lifecycle evidence.
2. Do not retain two independent Python outbound implementations as equal public paths. Select one canonical native path and deprecate or clearly classify the compatibility facade.
3. Do not make core compatibility behavior depend on an undeclared package.
4. External test scaffolding is not release evidence until it has executed and produced a retained result.
5. All reports must identify the Eggress commit, pinned pproxy version, Python version, target triple, and evidence artifact hash.
6. Preserve current honest classifications for plugin, wrapper, legacy cipher, SSH, H3/QUIC, SSR, and listener-side advanced transports unless implementation and evidence change.

# Workstream 1 — Native Python outbound stream API

## Goal

Expose the Rust `eggress_embed::OutboundConnector` as a real Python outbound connection primitive with native stream semantics and no temporary listening socket.

## Rust/PyO3 design

Add PyO3 types that own the connected Rust stream and its runtime resources. Recommended split:

- `PyOutboundConnector`: immutable compiled chain/routing state;
- `PyOutboundStream`: one connected TCP stream;
- optional `PyOutboundReader` and `PyOutboundWriter` only if split-half behavior is required by the pinned pproxy contract.

The connector must expose:

- construction from pproxy URI and TOML;
- `connect_tcp(host, port, timeout=None)`;
- asynchronous `aconnect_tcp(host, port, timeout=None)`;
- target validation before network I/O where possible;
- chain and selected-upstream introspection without exposing credentials;
- explicit close and wait-closed behavior.

The connected stream must expose the exact subset required by the C1 pproxy contract. At minimum characterize and implement:

- `read(n=-1)` or equivalent bounded read semantics;
- `readexactly(n)` if the reference exposes it;
- `write(data)` and `drain()` semantics;
- `write_eof()` or half-close behavior where supported;
- `close()` and `wait_closed()`;
- `is_closing()`/`closed` state;
- peer and local address access;
- metadata/`get_extra_info()` behavior;
- sync versus async method classification;
- cancellation during connect, read, write, drain, and close;
- timeout and partial-write behavior.

Use native Tokio/PyO3 awaitables. Do not implement async operations by blocking the Python event-loop thread. Blocking/synchronous wrappers must release the GIL.

## Runtime ownership

Define one runtime-ownership model and test it:

- connector can create multiple concurrent streams;
- streams retain required executor/runtime ownership after the connector object is dropped;
- dropping an unclosed stream produces `ResourceWarning` and best-effort cleanup;
- interpreter shutdown does not panic or deadlock;
- loop affinity follows the C5 rules;
- repeated `asyncio.run()` cycles do not leak native threads;
- cancellation always closes or transfers ownership deterministically.

## `ProxyConnection` migration

Choose one of these outcomes:

1. reimplement `ProxyConnection` as a thin facade over native `OutboundConnector`/`PyOutboundStream`; or
2. retain it as a documented legacy compatibility fallback with a warning and a separate non-drop-in classification.

Preferred outcome: preserve the public name but replace its temporary-listener implementation internally. Add a test asserting no listening socket is created during `tcp_connect()`.

## Tests

Add:

- direct TCP echo through each supported first-hop protocol;
- multi-hop HTTP/SOCKS/WS/raw/H2 chains;
- DNS, IPv4, and IPv6 targets;
- refused, timed-out, TLS, authentication, and policy failures;
- read/write/half-close lifecycle;
- cancellation at every await point;
- cross-loop rejection;
- concurrent streams from one connector;
- connector drop before stream close;
- stream drop without close;
- file-descriptor, task, thread, and port-leak tests;
- proof that no temporary listener is created;
- pinned pproxy behavioral comparisons for representative `Connection` programs.

## Acceptance criteria

- Python callers can establish and use a native outbound TCP stream through a configured chain.
- No temporary local proxy listener is used.
- Sync and async methods match the pinned contract classifications.
- Cancellation, half-close, errors, and cleanup match documented pproxy behavior or carry explicit compatibility diagnostics.
- `ProxyConnection`, if retained, delegates to the native connector.
- Native stream tests pass across the supported Python matrix.

# Workstream 2 — Top-level `pproxy` compatibility distribution

## Goal

Allow supported existing applications to replace the Python dependency and continue using `import pproxy` without source changes.

## Packaging decision

Implement a separate compatibility distribution rather than silently placing an undeclared top-level package in every `eggress` wheel.

Recommended model:

- canonical engine package: `eggress`;
- compatibility distribution project: a deliberately named package such as `eggress-pproxy-compat`;
- installed import namespace: `pproxy`;
- dependency: exact compatible range of `eggress`;
- source package under a clearly separated directory, for example `python-pproxy-compat/pproxy/`;
- release process publishes both artifacts from the same tag and verifies version alignment.

Before implementation, verify package-index ownership and naming constraints. If publishing the `pproxy` distribution name is not legally or operationally possible, the distribution may have another project name while still intentionally providing the `pproxy` import namespace. Document the conflict and installation command.

## Namespace contents

Use the C1 contract file to generate or validate exports. The shim should re-export only classified public or required registry-reachable symbols.

At minimum cover:

- `Server`;
- `Connection` and native outbound connection types where reference-visible;
- protocol classes and registries;
- cipher classes and registries;
- helper functions and constants;
- exception classes;
- coroutine classification;
- module paths used by supported programs.

Support imports such as:

- `import pproxy`;
- `from pproxy import Server`;
- documented submodule imports;
- registry and factory imports used by the contract corpus.

Do not use broad `sys.modules` mutation from the `eggress` package as the primary mechanism. Provide real package modules and `.pyi` stubs.

## Coexistence policy

Define behavior when upstream `pproxy` is already installed:

- installation should fail with an actionable dependency conflict rather than overwrite unpredictably;
- runtime should expose `__eggress_compat__`, Eggress version, and reference compatibility version;
- diagnostics must make it clear which implementation is imported;
- uninstalling the compatibility distribution must not damage the canonical `eggress` package.

## Tests

Build wheels in clean environments and verify:

- install canonical `eggress` only: `import pproxy` fails as documented;
- install compatibility distribution: `import pproxy` succeeds;
- import identity and submodule paths;
- exact `__all__`, signatures, async classification, exceptions, and registries;
- representative unchanged pproxy programs;
- type checking against provided stubs;
- conflict behavior with upstream pproxy installed;
- uninstall/reinstall and version mismatch behavior;
- Linux, macOS, and Windows wheels;
- supported Python versions.

## Acceptance criteria

- A clean environment can install the compatibility distribution and run supported `import pproxy` programs unchanged.
- Package and engine versions are checked for compatibility.
- No runtime `sys.path` or opportunistic module aliasing is required.
- Wheel and sdist metadata are correct and reproducible.
- The manifest has separate capability IDs for `eggress` importability and top-level `pproxy` import compatibility.

# Workstream 3 — Deterministic cipher dependency policy

## Goal

Ensure supported AEAD methods behave consistently across all installations that claim Python cipher compatibility.

## Decision

Choose and implement one policy:

### Preferred policy: compatibility extra/distribution dependency

- the top-level pproxy compatibility distribution depends on `cryptography` within a tested version range;
- canonical `eggress` may keep it optional through an extra such as `eggress[cipher-api]`;
- installation metadata clearly identifies the requirement;
- capability reporting states whether functional Python cipher operations are available.

### Alternative policy: native Rust-backed PyO3 cipher operations

Move AEAD operations into the Rust extension and remove the Python dependency. This is preferable long-term if the existing Rust cipher implementations can expose safe stateless/stateful APIs without duplicating protocol-specific nonce behavior.

Do not leave the current undeclared-optional state.

## Semantic verification

For each supported cipher:

- known-answer vectors;
- cross-check against pinned pproxy where it implements the same method;
- cross-check against `cryptography` or another independent implementation;
- nonce increment and exhaustion behavior;
- tag failure behavior;
- empty plaintext and large payloads;
- copy/pickle state rules;
- thread/task concurrency restrictions;
- key redaction and best-effort zeroization.

Legacy cipher classes must remain explicitly unsupported unless Track F implements them. Construction-only stubs cannot be classified as functional drop-in ciphers.

## Acceptance criteria

- Functional AEAD behavior is deterministic for every installation profile that advertises it.
- Dependency metadata matches runtime behavior.
- Capability reporting distinguishes available, unavailable, legacy-stub, and Rust-runtime-only cipher surfaces.
- Supported methods pass independent vectors and pproxy comparisons.

# Workstream 4 — External evidence execution and artifact retention

## Goal

Turn gated test code into release evidence.

## Required external suites

Execute and retain results for:

- pinned `pproxy==2.7.9` differential core suite;
- extended CLI, HTTP, SOCKS, chain, TLS, UDP, reverse, Trojan, WS/raw/H2 scenarios where meaningful;
- `trojan-go` or another independent Trojan implementation;
- independent WebSocket server/proxy behavior;
- independent H2 CONNECT implementation or a protocol-level fixture not using Eggress code on both sides;
- standard Shadowsocks implementation for TCP and UDP;
- Python contract and unchanged-program corpus;
- wheel installation/import tests.

Where pproxy cannot serve as a meaningful oracle, use an independent implementation and label the evidence type accurately.

## Evidence bundle

Add a release-evidence generator that emits a deterministic directory or archive containing:

- `metadata.json`: Eggress SHA, dirty-state flag, reference versions, OS, architecture, Python, Rust, dependency locks;
- scenario results with normalized observations;
- raw redacted logs and transcripts for failures;
- manifest and composition-matrix hashes;
- wheel hashes;
- command inventory;
- skipped scenarios and exact reasons;
- summary report;
- SHA-256 manifest for all files.

Artifacts must never contain passwords, private keys, tokens, or unredacted credential-bearing URIs.

CI should upload the evidence bundle for release-candidate tags and optionally sign or attest it using the repository's release provenance mechanism.

## Gating policy

- ordinary PR CI may keep expensive external jobs optional or scheduled;
- release-candidate CI must require every release-blocking external suite;
- unavailable dependencies must fail the release job, not silently skip;
- skips are permitted only for explicitly non-applicable platform cells;
- manifest promotion requires a scenario/result ID that exists in the evidence bundle.

## Acceptance criteria

- One complete evidence bundle exists for the candidate commit.
- All release-blocking scenarios pass.
- No required scenario is skipped because a binary or dependency is absent.
- Every `drop_in` capability references accepted evidence.
- Artifact hashes and metadata make the results reproducible.

# Workstream 5 — Full cross-platform release validation

## Goal

Run the actual supported release matrix rather than a selective local subset.

## Rust matrix

Require:

- formatting;
- clippy with warnings denied;
- workspace build/check for all maintained feature sets;
- workspace tests;
- doctests;
- minimum supported Rust version if declared;
- Linux x86_64 and aarch64 compile/test as maintained;
- macOS x86_64 and arm64;
- Windows x86_64;
- platform-specific transparent/Unix compile gates;
- fuzz-target compilation and bounded smoke execution;
- sanitizer or Miri subsets where maintained and practical.

Use `--test-threads=1` only for suites with documented resource or global-state constraints. Do not globally serialize tests without evidence.

## Python matrix

Require clean wheel installation and tests for every supported Python version on:

- Linux x86_64;
- macOS arm64 and x86_64 where maintained;
- Windows x86_64.

Run:

- full Python suite;
- C1 contract suite;
- C2/C3/C4/C5 semantic suites;
- top-level `pproxy` package suite;
- type-check/stub validation;
- native outbound stream tests;
- cipher tests with required dependency installed;
- import/uninstall/reinstall tests;
- wheel smoke tests without repository source on `PYTHONPATH`.

## Resource and soak gates

Add bounded release gates for:

- repeated native outbound connections;
- concurrent streams through each advanced transport;
- H2 pool retirement and isolation;
- cancellation storms;
- plugin queue shutdown even though plugin behavior remains structural;
- reverse reconnect cycles;
- file-descriptor and task counts;
- no persistent test listeners or child processes.

## Acceptance criteria

- All declared platform/Python combinations are green.
- Results come from installed artifacts where applicable.
- No test depends on repository-relative imports accidentally shadowing wheel contents.
- Resource deltas remain within documented bounds.
- Current head has visible, successful required checks.

# Workstream 6 — Final parity audit and release nomenclature

## Goal

Ensure the manifest, matrix, Python contract, packaging, and release language describe exactly what shipped.

## Audit procedure

For every current `drop_in` capability:

1. identify exact role and composition cell;
2. verify implementation path;
3. verify CLI/TOML/Python entry surfaces that are claimed;
4. verify differential or independent evidence;
5. verify platform scope;
6. verify packaging availability;
7. verify no warning or missing optional dependency changes behavior;
8. verify documentation and examples use the supported path.

Demote capabilities that have only internal integration evidence when the tier definition requires external or differential evidence. Do not preserve the 101 count as a target.

## Release naming

Use explicit release language:

- **modern pproxy-compatible runtime** if modern protocols and common Python programs are supported but legacy SSR/ciphers, SSH, QUIC/H3, or plugin live-path behavior remain excluded;
- **Python import drop-in for the certified subset** only after the compatibility distribution passes unchanged-program tests;
- never use **strict full parity** until all functional pinned-reference capabilities are implemented or proven nonfunctional upstream.

## Documentation updates

Update and regenerate:

- README status block;
- parity manifest/report;
- composition matrix;
- Python compatibility report;
- package installation guide;
- namespace strategy ADR;
- migration guide;
- release notes;
- final certification report;
- known divergences and unsupported surfaces.

Archive or mark superseded completion documents that contain broader claims.

## Acceptance criteria

- Counts, tiers, evidence references, package behavior, and documentation agree.
- Top-level import compatibility is separately classified and evidenced.
- Native Python outbound behavior is separately classified and evidenced.
- Plugin/wrapper structural compatibility is not described as live-path parity.
- Legacy and deferred protocols remain explicit exclusions.
- Release nomenclature matches the achieved scope.

# Recommended implementation order

1. Native PyO3 outbound stream and `ProxyConnection` migration.
2. Top-level compatibility distribution and clean-environment tests.
3. Cipher dependency policy and semantic vectors.
4. External evidence bundle tooling.
5. Execute external and cross-platform matrices.
6. Final parity audit, demotions/promotions, and release documentation.

# Expected files and areas

Likely areas include:

- `crates/eggress-embed/src/outbound.rs`;
- `crates/eggress-python/src/lib.rs`;
- `python/eggress/outbound.py` and stubs;
- `python/eggress/pproxy_connection.py`;
- new compatibility-distribution package directory;
- Python packaging metadata and release workflows;
- `python/eggress/cipher.py` or Rust cipher bindings;
- external interoperability workflows and testkit;
- evidence-generation scripts;
- parity manifest, composition matrix, reports, ADRs, and migration documentation.

Follow the repository's actual layout and avoid duplicating existing testkit/process-supervisor infrastructure.

# Definition of done

This pass is done when all of the following are true:

- Python outbound TCP uses the native Rust connector and returns a usable stream object;
- no temporary listener is created for the canonical Python connection path;
- a clean environment can install the compatibility distribution and run `import pproxy` programs unchanged for the certified subset;
- cipher API behavior is deterministic from declared dependencies;
- external pproxy and third-party interoperability suites have executed successfully;
- a commit-bound, redacted evidence bundle is retained;
- full supported Rust/Python/platform CI is green on the candidate commit;
- every remaining `drop_in` capability has composition-specific accepted evidence;
- documentation calls the release modern/certified-subset parity unless strict full parity is actually achieved.