# Full pproxy 2.7.9 Drop-In Compatibility Roadmap

## Status

Proposed implementation roadmap.

## Target

Make Eggress a true, full drop-in replacement for `pproxy==2.7.9` across:

1. the `pproxy` command-line executable;
2. the `pproxy` Python package and module namespace;
3. observable protocol and transport behavior;
4. process, lifecycle, failure, and platform behavior relied on by existing deployments.

The target is deliberately frozen to `pproxy==2.7.9`. A future pproxy release must be treated as a separate compatibility profile rather than silently changing this contract.

## Why this roadmap exists

Eggress currently provides a strong Rust-native proxy runtime and a broad modern pproxy compatibility subset. It does not yet satisfy strict drop-in replacement requirements.

The present compatibility surface includes several categories that are useful for migration but are not equivalent to unchanged pproxy application behavior:

- native equivalents with different object models;
- compatibility-with-warning entries;
- intentional non-parity;
- unsupported features;
- Python symbols that import but are structural placeholders;
- Python methods with different sync/async and return contracts;
- modernized protocol behavior that differs from the pinned oracle;
- release evidence that emphasizes capability coverage rather than full behavioral identity.

This roadmap replaces capability-count reasoning with an oracle-backed behavioral contract.

## Definition of drop-in

A capability may be called `drop_in` only when every applicable dimension matches the pinned oracle:

- import path and symbol location;
- exported names and aliases;
- callable signature and defaults;
- synchronous versus coroutine behavior;
- return type and returned object protocol;
- public and commonly used attributes;
- exception type, failure timing, and warning behavior;
- lifecycle, cancellation, and event-loop behavior;
- network byte behavior;
- CLI parsing, output stream, exit status, and signals;
- optional dependency behavior;
- platform-specific behavior;
- side effects such as monkey-patching, daemonization, PAC serving, logging, and system-proxy mutation.

Equivalent outcomes through a different public contract are not drop-in.

## Product boundary

The project must maintain two distinct public personalities.

### Canonical Eggress API

The `eggress` namespace remains modern, typed, explicit, secure, and Rust-native. It may retain:

- native stream abstractions;
- structured configuration;
- metrics and administration APIs;
- bounded resources;
- stronger lifecycle management;
- safe defaults;
- deliberate rejection of obsolete behavior;
- implementation-specific extensions.

### Strict pproxy compatibility API

The separately installed `pproxy` namespace must reproduce pproxy 2.7.9 behavior, including legacy or awkward contracts. It must not silently substitute Eggress-native signatures, return types, or lifecycle models.

Compatibility adapters should terminate at a narrow internal boundary and delegate to shared Rust runtime primitives beneath that boundary.

## Roadmap milestones

### Milestone A — Honest Contract

Create a frozen compatibility oracle, strict manifest, paired observation harness, and governance rules that prevent unsupported or equivalent-only behavior from being called drop-in.

This milestone is the prerequisite for all later implementation work.

Detailed plan: `plans/MILESTONE_A_HONEST_CONTRACT.md`

### Milestone B — Python Source Compatibility

Reproduce the pproxy package namespace, aliases, signatures, object model, coroutine contracts, reader/writer behavior, and core server helpers needed for unchanged Python applications to run.

This milestone targets common HTTP, SOCKS, direct, and currently supported modern-protocol paths. It does not claim complete protocol-family parity.

Detailed plan: `plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md`

### Milestone C — Functional Internal API

Replace structural Python stubs with functional `pproxy.server`, `pproxy.proto`, cipher, and plugin behavior. Third-party code that imports pproxy internals must work, not merely import.

Detailed plan: `plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md`

### Milestone D — Protocol Completeness

Implement all protocol and transport families accepted by pproxy 2.7.9, including the current deliberate gaps:

- SSH;
- QUIC and HTTP/3;
- full HTTP/2 compatibility roles;
- ShadowsocksR;
- legacy Shadowsocks stream ciphers and OTA behavior;
- all listener/upstream role asymmetries;
- complete reverse/backward compositions;
- full multi-hop UDP;
- platform-specific transparent proxy behavior.

Milestone D must close every protocol-level `unsupported`, `intentional_non_parity`, and `compatible_with_warning` record in the strict manifest.

### Milestone E — Executable Drop-In

Make the `pproxy` executable interchangeable in existing scripts and services.

Required scope includes exact or observationally equivalent behavior for:

- all flags and aliases;
- parser defaults;
- repeated options;
- help and version output;
- daemonization;
- `--get` static serving;
- `--reuse`;
- rulefiles;
- PAC serving;
- system proxy mutation and restoration;
- uvloop selection;
- log destinations;
- startup output;
- signal handling;
- exit status;
- event-loop shutdown.

Packaging and optional-dependency behavior also become release-gated in this milestone.

### Milestone F — Full Certification

Generate a reproducible, machine-readable certification bundle proving that all strict-manifest entries pass against the pinned oracle on the supported platform and Python matrix.

Only Milestone F permits the unqualified statement:

> Eggress is a full drop-in replacement for pproxy 2.7.9.

## Implementation phases

The milestones above group the following technical phases.

### Phase 0 — Reset the compatibility contract

Deliverables:

- frozen oracle provenance and hashes;
- strict compatibility manifest;
- known-upstream-defects registry;
- compatibility vocabulary and documentation policy;
- manifest validator that forbids unsupported tiers in a full certification profile.

### Phase 1 — Authoritative oracle and conformance harness

Deliverables:

- isolated oracle and candidate environments;
- paired subprocess probes;
- normalized JSON observations;
- upstream examples and tests as immutable fixtures;
- API, CLI, protocol, process, optional-dependency, and platform suites;
- retained differential artifacts in CI.

### Phase 2 — Exact package and module namespace

Deliverables:

- complete `pproxy` module tree;
- matching exports, aliases, constants, and module metadata;
- matching import side effects;
- no public compatibility placeholder that only raises unsupported errors.

### Phase 3 — pproxy object model

Deliverables:

- upstream-compatible `Connection`, `Server`, `Rule`, and `DIRECT`;
- functional proxy-chain object types;
- matching chain construction and public attributes;
- matching URI factory behavior.

### Phase 4 — asyncio reader/writer compatibility

Deliverables:

- `asyncio.StreamReader` and `StreamWriter` compatible adapters over Rust-owned I/O;
- pproxy stream helper methods and rollback behavior;
- coroutine-compatible `tcp_connect()` returning `(reader, writer)`;
- cancellation, EOF, half-close, and descriptor-leak tests.

### Phase 5 — functional `pproxy.server`

Deliverables:

- operational auth table, schedulers, stream and datagram handlers;
- real cipher preparation and health checks;
- functional rule compilation and URI factories;
- matching startup and shutdown helpers.

### Phase 6 — functional protocol object parity

Deliverables:

- operational `guess`, `accept`, `connect`, UDP, channel, address, and TLS helpers;
- protocol auto-detection;
- malformed and fragmented handshake parity;
- direct submodule use without the high-level Eggress service API.

### Phase 7 — full cipher and plugin compatibility

Deliverables:

- complete cipher inventory and aliases;
- EVP-compatible key derivation, IVs, nonces, frames, and packet behavior;
- functional plugin lookup, ordering, encoding, decoding, and UDP hooks;
- bidirectional known-answer and external interoperability tests.

### Phase 8 — missing protocol and transport families

Deliverables:

- SSH;
- QUIC/H3;
- full H2 roles;
- SSR;
- legacy Shadowsocks;
- complete listener/upstream roles;
- dependency-compatible failure behavior.

### Phase 9 — UDP, reverse, and transparent semantics

Deliverables:

- exact pproxy UDP framing in compatibility mode;
- complete multi-hop UDP;
- reverse/backward chains and reuse;
- platform-specific transparent destination recovery;
- association identity, timeout, callback, and eviction parity.

### Phase 10 — CLI and process-model parity

Deliverables:

- all flags operational;
- exact parser and repeated-argument behavior;
- daemon, PAC, system proxy, static serving, logging, signals, and exit codes;
- compatibility process tests in disposable environments.

### Phase 11 — failure, timing, and resource semantics

Deliverables:

- differential negative-path corpus;
- failure-stage compatibility;
- bounded task, thread, memory, and descriptor behavior;
- cancellation and shutdown parity;
- explicit approvals for reproduced or intentionally fixed upstream defects.

### Phase 12 — platform, packaging, and dependencies

Deliverables:

- supported wheel matrix;
- source distribution and editable install tests;
- clean installation with each optional dependency profile;
- lazy dependency loading;
- coexistence, replacement, upgrade, and uninstall tests.

### Phase 13 — strict differential certification

Deliverables:

- zero unresolved differential failures;
- all strict-manifest entries marked `drop_in`;
- retained machine-readable evidence;
- visible required CI;
- consistent release decision and release notes.

## Parallel execution tracks

| Track | Scope | Governing milestone |
|---|---|---|
| A | oracle, manifest, introspection, differential harness | A |
| B | namespace, signatures, object model, asyncio adapters | B |
| C | server helpers, protocol objects, handlers | C |
| D | ciphers, plugins, Shadowsocks, SSR | C and D |
| E | SSH, H2, QUIC/H3, reverse, UDP, transparent proxying | D |
| F | CLI, process behavior, packaging, platforms, release evidence | E and F |

Track A owns acceptance evidence for every other track. An implementation track cannot close its own compatibility item using candidate-only tests.

## Critical path

1. Freeze the oracle and build strict observations.
2. Define the complete public and de facto public symbol inventory.
3. Implement asyncio reader/writer adapters.
4. Restore the upstream URI-factory and proxy-chain object model.
5. Replace server and protocol stubs with functional behavior.
6. Complete cipher and plugin compatibility.
7. Implement missing transport families and composition semantics.
8. Complete CLI and process behavior.
9. Certify the full matrix.

The asyncio adapter and object model are the principal architectural bottlenecks. Protocol and CLI work that assumes the current Eggress-native Python contracts will need rework if performed before those contracts are stabilized.

## Compatibility manifest requirements

The strict manifest must inventory at least:

- every package and module;
- every exported and commonly imported symbol;
- signatures and callable kinds;
- constants and aliases;
- object attributes and protocols;
- exceptions and warning behavior;
- CLI options and defaults;
- protocols by role;
- transports and modifiers;
- ciphers and plugins;
- composition pairs and chains;
- platform-specific behavior;
- optional dependencies;
- process lifecycle;
- negative paths.

Each record must identify:

- oracle probe;
- candidate probe;
- differential comparator;
- implementation owner;
- current status;
- blocking dependencies;
- required test identifiers;
- supported platforms;
- evidence artifact location;
- approved upstream-defect exception, if any.

## Release language policy

Before Milestone F, public language must remain qualified, for example:

- “modern pproxy compatibility subset”;
- “pproxy-shaped migration API”;
- “compatible with common HTTP/SOCKS deployments”;
- “not a strict full-parity replacement.”

The following claims are prohibited before Milestone F:

- “full pproxy parity”;
- “drop-in replacement” without qualification;
- “unchanged pproxy applications are supported”;
- manifest percentages presented as application compatibility probability.

## Testing policy

Every compatibility feature requires all applicable test layers:

1. candidate unit tests;
2. candidate integration tests;
3. oracle observation tests;
4. paired differential tests;
5. unchanged upstream example or test coverage where available;
6. external client/server interoperability for wire features;
7. negative-path coverage;
8. resource and lifecycle coverage;
9. platform coverage where relevant.

Skipped external interoperability tests cannot count as passing certification evidence.

## Non-negotiable guardrails

- Do not substitute the canonical Eggress API when the pproxy signature or return type differs.
- Do not count import success as behavioral parity.
- Do not call standardized modern behavior drop-in when the oracle behaves differently.
- Do not certify from documentation-only evidence.
- Do not certify from candidate-only tests.
- Do not omit insecure or legacy capabilities while claiming full compatibility.
- Do not let strict compatibility defaults leak into the canonical `eggress` namespace.
- Do not broaden the oracle target during implementation.
- Do not silently fix an upstream bug in strict mode without an explicit, tested compatibility policy.

## Full release gate

A full drop-in release requires all of the following:

- unchanged imports pass;
- unchanged upstream examples pass;
- callable signatures match;
- sync/async contracts match;
- return objects are compatible;
- exception and warning behavior match;
- submodule internals are functional;
- all pproxy protocols are implemented;
- all pproxy ciphers are implemented;
- all pproxy plugins are implemented;
- all CLI flags are operational;
- all valid compositions are tested;
- bidirectional external interoperability passes;
- platform and packaging matrices pass;
- no unsupported, intentional-non-parity, native-equivalent, or warning-tier entries remain in the strict profile;
- zero unresolved differential failures remain;
- release evidence and release documents agree.

## Completion definition

This roadmap is complete only when Milestone F is closed and the generated certification bundle proves full `pproxy==2.7.9` compatibility. Individual milestones may produce useful preview releases, but they do not independently authorize an unqualified drop-in claim.
