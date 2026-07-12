# Phase C4 — Python protocol, cipher, and plugin object compatibility

## Objective

Reproduce the public protocol, cipher, registry, factory, and plugin object model exposed by pinned pproxy. These objects must be usable wherever pproxy accepts them, preserve names and aliases, map cleanly onto Eggress runtime capabilities, and provide an explicit, bounded bridge for Python-defined extension points where strict compatibility requires callbacks.

## Dependencies

- C1 contract inventory.
- C2 `Connection` and C3 `Server` object adapters.
- A2 composition matrix.
- Track B runtime promotion for the protocols to be exposed.

## Principles

1. Object compatibility is more than accepting strings.
2. Supported built-in protocols and ciphers should execute in Rust.
3. Python callbacks should be invoked only at documented extension boundaries.
4. No unbounded packet/frame callback queue is acceptable.
5. Unsupported legacy objects must fail explicitly rather than map to a different algorithm.

## Workstream 1: registry and factory inventory

From C1, identify:

- protocol registries;
- cipher registries;
- aliases and normalized names;
- factory functions;
- singleton versus per-use objects;
- constructors and defaults;
- attribute and repr behavior;
- composition operators or wrappers;
- plugin registration hooks;
- callback signatures;
- lookup failure behavior.

Generate table-driven contract tests from the inventory.

## Workstream 2: protocol object model

Implement compatibility wrappers for supported protocol families:

- direct;
- HTTP/HTTPS;
- SOCKS4/4a/5;
- Shadowsocks;
- Trojan;
- promoted WS/WSS/raw/tunnel/H2;
- Unix and platform-specific protocols;
- future SSH/QUIC/H3 only after runtime availability.

Each object must carry enough typed data to resolve an A2 composition cell. Preserve constructor validation, aliases, target/address behavior, credentials, wrappers, and serialization/repr semantics.

Avoid storing secrets in repr or pickled state.

## Workstream 3: cipher objects

Implement exact names, aliases, key/password handling, constructor behavior, and error types for supported ciphers.

Requirements:

- modern AEAD ciphers use existing Rust implementations;
- key derivation matches pproxy/reference test vectors;
- invalid key/password lengths fail compatibly;
- object equality/hash behavior is characterized and reproduced if public;
- no key material in repr/logs/exceptions;
- legacy algorithms remain explicit unsupported blockers until Track F implements them;
- no silent alias from an unsupported cipher to a modern cipher.

## Workstream 4: wrapper and composition objects

Characterize objects representing TLS, inbound/reverse modifiers, chains, plugins, or transport wrappers.

Implement normalized composition with:

- deterministic ordering;
- validation against A2;
- preservation of pproxy-visible attributes;
- round-trip conversion where the API exposes strings/URIs;
- stable diagnostics for impossible combinations.

## Workstream 5: Python-defined plugin bridge

Only implement callback bridging for extension points confirmed by C1 and oracle probes.

Design requirements:

- bounded queue between Rust tasks and Python loop;
- explicit concurrency and ordering;
- callback timeouts;
- cancellation propagation;
- no Rust mutex held while calling Python;
- GIL acquired only for callback execution and conversion;
- callback exceptions converted into compatible protocol/session failures;
- backpressure or deterministic rejection under overload;
- prevention of recursive/reentrant misuse;
- safe interpreter-shutdown behavior.

Where a callback would route every byte through Python, document and benchmark the cost. Preserve a fast path for all built-ins.

## Workstream 6: import paths and identity

Ensure expected imports and aliases work. If pproxy relies on identity relationships between registry values, factories, or constants, reproduce them where feasible. Test module reload, multiple imports, and object lifetime.

## Workstream 7: serialization and introspection

Characterize and implement only contracted behavior for:

- repr/str;
- equality/hash;
- copy/deepcopy;
- pickling;
- name/method/password/protocol attributes;
- URI conversion;
- type annotations.

Security takes precedence over reproducing secret-bearing repr; any divergence requires a stable compatibility note.

## Workstream 8: tests

Add:

- full registry/alias enumeration tests;
- signature and constructor tests;
- protocol object → runtime composition tests;
- cipher known-answer and interop tests;
- invalid name/key/password tests;
- wrapper/chain normalization tests;
- repr/redaction tests;
- callback success, exception, timeout, cancellation, overload, ordering, and reentrancy tests;
- GIL contention tests;
- interpreter shutdown and leak tests;
- examples copied or adapted from pproxy documentation and run against both packages.

## Acceptance criteria

- Every C1 protocol/cipher/factory symbol is implemented or explicitly classified.
- Supported objects can be passed to C2/C3 wherever pproxy accepts them.
- Registry names and aliases match the pinned contract.
- Modern cipher behavior has known-answer and interoperability evidence.
- Unsupported legacy ciphers never silently degrade.
- Python plugin callbacks are bounded, cancellation-safe, exception-safe, and do not block native built-in paths.
- Runtime, stubs, docs, matrix, and manifest agree.

## Out of scope

- implementing legacy ciphers/SSR, handled in Track F;
- adding missing runtime transports;
- unrestricted arbitrary Python packet-processing APIs not present in pproxy;
- generalized serialization beyond the reference contract.

## Recommended commit sequence

1. registries and object shells;
2. built-in protocol adapters;
3. modern cipher objects/vectors;
4. wrappers and composition;
5. plugin bridge design and implementation;
6. introspection/redaction;
7. contract/interoperability/GIL/leak tests;
8. stubs/docs/manifest promotion.