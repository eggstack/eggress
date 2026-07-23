# Milestone B — Python Source Compatibility

## Status

**REOPENED — final evidence and runtime closure in progress.**

## Parent roadmap

`plans/PPROXY_FULL_DROP_IN_ROADMAP.md`

## Objective

Make unchanged Python applications written against the public `pproxy==2.7.9` API run against the Eggress compatibility distribution for the protocol families already supported by the Eggress runtime.

Milestone B closes the fundamental namespace, signature, object-model, coroutine, return-type, stream, and lifecycle incompatibilities. It does not claim complete protocol-family parity; missing SSH, QUIC/H3, SSR, legacy cipher, and full composition work remains assigned to later milestones.

## Completion outcome

At milestone completion:

- the separately installed compatibility distribution exposes the complete public pproxy namespace required by the strict manifest;
- `pproxy.Connection`, `pproxy.Server`, `pproxy.Rule`, and `pproxy.DIRECT` reproduce the pinned oracle’s public contracts;
- `tcp_connect()` is awaitable and returns an asyncio-compatible `(reader, writer)` pair;
- the canonical upstream client and server examples run unchanged for supported protocol paths;
- source-compatible HTTP, HTTPS, SOCKS4/4a/5, direct, current Shadowsocks AEAD, Trojan, WebSocket/raw tunnel, and supported H2 paths use Rust runtime primitives beneath the adapter;
- the canonical `eggress` namespace retains its current native APIs and is not forced into pproxy semantics.

## Scope

### In scope

- package/module namespace parity for public API entry points;
- top-level aliases and constants;
- callable signatures and defaults;
- URI factory semantics;
- proxy-chain object structure;
- asyncio stream compatibility;
- coroutine and return contracts;
- public lifecycle methods used by upstream examples and common applications;
- direct and currently implemented protocol paths;
- exception and warning compatibility for this surface;
- compatibility-wheel packaging and isolation;
- differential source-compatibility tests.

### Out of scope

- functional parity for every pproxy internal helper;
- full direct use of every `pproxy.proto` method;
- complete cipher/plugin inventory;
- SSH;
- QUIC/H3;
- SSR;
- legacy Shadowsocks stream ciphers and OTA;
- complete multi-hop UDP;
- complete reverse and transparent behavior;
- all CLI process side effects.

Those remain in Milestones C through F.

## Current incompatibilities this milestone must remove

The current compatibility distribution has several structural mismatches that prevent a true drop-in claim:

1. Top-level pproxy aliases do not reproduce the upstream URI-factory object model.
2. Eggress exposes a distinct native `Server` class with listen/remote lists and explicit lifecycle methods rather than the upstream proxy-chain factory.
3. Compatibility `tcp_connect()` is synchronous in the current object and returns an Eggress `OutboundStream` rather than an awaitable `(StreamReader, StreamWriter)` pair.
4. The current connection surface lacks upstream-compatible UDP methods.
5. Several imported server symbols are placeholders or unconditional stubs.
6. Constants, defaults, and sentinel behavior differ from upstream.
7. The compatibility wheel can import successfully while applications fail when exercising methods.

Milestone B must correct the public source-level path without deleting or weakening the native Eggress API.

## Architectural design

### Separate native and compatibility layers

Preserve:

```text
python/eggress/                 canonical native API
python-pproxy-compat/pproxy/    strict pproxy adapter
```

The compatibility layer may import private adapter primitives from `eggress`, but it must not alias a native class directly when that class has a different public contract.

### Rust runtime boundary

Introduce or formalize a narrow compatibility transport boundary with operations conceptually equivalent to:

```text
open_tcp(chain, destination, options) -> native duplex handle
open_udp(chain, destination, options) -> native datagram association
start_listener(spec, handler/options) -> native listener handle
close(handle)
metadata(handle) -> peer/local/socket/tls information
```

The Python compatibility object model wraps these primitives. It must not reconstruct proxy protocols in Python when the Rust runtime already provides a correct implementation.

### Compatibility object identity

The `pproxy` namespace must expose pproxy-shaped Python objects with the expected classes, attributes, coroutine methods, and side effects. Internal native handles should remain private implementation details.

## Workstream B1 — Complete public namespace and alias parity

### Tasks

1. Use Milestone A’s generated namespace inventory to implement all public top-level and directly imported module symbols required for source compatibility.
2. Match top-level relationships equivalent to the oracle:
   - `Connection`;
   - `Server`;
   - `Rule`;
   - `DIRECT`.
3. Match:
   - `__all__`;
   - `__version__`;
   - module metadata;
   - symbol `__module__` and qualified names where observable;
   - import side effects;
   - constants and default values.
4. Ensure `eggress` alone does not install or inject the `pproxy` namespace.
5. Ensure `eggress-pproxy-compat` installs the namespace deterministically without runtime `sys.modules` alias hacks.
6. Add an explicit compatibility-package version policy tied to the exact required `eggress` runtime ABI/API.

### Required tests

- recursive namespace comparison;
- `from pproxy import *` comparison;
- top-level alias identity and callable-kind comparison;
- module metadata comparison;
- clean-environment import tests;
- coexistence tests with `eggress` imported before and after `pproxy`;
- uninstall/reinstall namespace tests.

### Acceptance criteria

- No required public symbol is missing.
- No top-level public symbol points directly to an incompatible native class.
- Alias identities and sentinel behavior match the oracle.
- Import order does not change behavior.

## Workstream B2 — Restore URI factory semantics

### Goal

Reproduce the upstream behavior in which `Connection` and `Server` construct proxy objects or chains from URI input.

### Tasks

1. Implement an upstream-compatible `proxy_by_uri()`.
2. Implement an upstream-compatible `proxies_by_uri()`.
3. Accept the same input shapes:
   - single URI string;
   - `__`-separated chain;
   - iterable/list forms where accepted;
   - jump/chain arguments;
   - local versus remote construction context.
4. Reproduce parsing defaults, omitted host/port behavior, credentials, modifiers, and error timing.
5. Construct compatibility proxy object classes rather than returning `eggress.pproxy_connection.ProxyConnection` directly.
6. Preserve chain topology and public `jump` relationships.
7. Match direct sentinel handling and explicit `direct://` behavior.
8. Match invalid scheme, malformed URI, invalid credentials, and invalid chain exception behavior.
9. Reuse `eggress-uri` and `eggress-pproxy-compat` translation primitives beneath the adapter where their behavior matches the oracle.
10. Keep translation diagnostics available internally without changing the exception surface expected by pproxy applications.

### Compatibility classes required in this milestone

At minimum, implement functional public shells for protocol families already supported by Eggress:

- `ProxyDirect`;
- `ProxySimple`;
- supported `ProxyH2` paths;
- supported backward/reverse object construction needed by public source tests.

Classes for later protocol families may exist as manifest-tracked gaps, but public constructors must not falsely claim functionality.

### Required tests

- single URI construction;
- multi-hop `__` construction;
- direct construction;
- credentials and modifiers;
- equivalent list and string forms;
- invalid URI corpus;
- public attributes after construction;
- chain order and `jump` identity;
- object repr and truthiness where used upstream.

### Acceptance criteria

- `pproxy.Connection(uri)` and `pproxy.Server(uri)` match the oracle’s type and object shape for supported paths.
- Chain construction does not require Eggress-specific configuration.
- Applications can inspect expected proxy attributes before starting network activity.

## Workstream B3 — Implement the compatibility proxy object model

### Tasks

Implement the fields and methods required by the strict manifest for public source compatibility, including as applicable:

- `bind`;
- `lbind`;
- `unix`;
- `alive`;
- `connections`;
- `udpmap`;
- `direct`;
- `rproto`;
- `auth`;
- `jump`;
- `destination`;
- `match_rule`;
- `connection_change`;
- `open_connection`;
- `prepare_connection`;
- `tcp_connect`;
- `udp_sendto`;
- `udp_open_connection`;
- `udp_prepare_connection`;
- `start_server`;
- close and wait helpers exposed by the oracle.

Implementation rules:

1. Public state must live on the compatibility object or be exposed through compatible properties.
2. Rust handles must not appear as public return values where the oracle returns Python stream or proxy objects.
3. Connection counters, alive state, and chain state must change at compatible operation boundaries.
4. Methods must fail at the same stage as the oracle: constructor, call, first await, first read, or background task.
5. Later-protocol methods may dispatch to a typed unsupported gap only when the oracle itself is unavailable under the same dependency profile; otherwise they remain milestone blockers.

### Acceptance criteria

- Object attribute inventories match the oracle for supported object types.
- Connection state transitions match paired observations.
- No public method required by upstream examples remains a structural stub.
- Native Eggress handles are fully encapsulated.

## Workstream B4 — Asyncio stream adapter

### Goal

Return true asyncio-compatible reader/writer objects over Rust-owned I/O.

### Design requirements

The adapter must support at least:

#### Reader behavior

- `read()`;
- `readexactly()`;
- `readuntil()`;
- `readline()`;
- async iteration where supported;
- `at_eof()`;
- cancellation;
- EOF and exception propagation.

#### Writer behavior

- `write()`;
- `writelines()`;
- `drain()`;
- `can_write_eof()`;
- `write_eof()`;
- `close()`;
- `wait_closed()`;
- `is_closing()`;
- `get_extra_info()`.

#### pproxy helper behavior

Reproduce the pproxy helper methods or import-time monkey patches used by the oracle, including:

- `read_w`;
- `read_n`;
- `read_until`;
- `rollback`.

### Implementation options

Preferred order:

1. A custom `asyncio.Transport` and `Protocol` bridge feeding real `StreamReader`/`StreamWriter` objects.
2. A socketpair/pipe bridge only if it preserves cancellation, half-close, backpressure, and resource bounds.
3. A documented compatible wrapper only if oracle introspection proves exact asyncio concrete types are not required.

Do not simply create look-alike objects with matching method names if third-party asyncio utilities reject them.

### Backpressure and concurrency

Define bounded buffering between Rust and Python:

- configurable high/low water marks;
- Rust-to-Python read pause/resume;
- Python-to-Rust write queue;
- `drain()` tied to actual queue pressure;
- no unbounded background task spawning;
- one deterministic shutdown path.

### Metadata

`get_extra_info()` must match available oracle keys, including where applicable:

- `socket`;
- `sockname`;
- `peername`;
- `ssl_object`;
- `cipher`;
- `peercert`.

For multiplexed or virtual transports with no direct socket, match the oracle behavior for that transport rather than inventing a socket.

### Required tests

- exact coroutine function introspection;
- tuple return shape;
- real asyncio reader/writer integration;
- small and large reads/writes;
- fragmented reads;
- backpressure and `drain()`;
- half-close;
- remote EOF;
- local close;
- repeated close;
- cancellation during connect/read/write/drain;
- task and descriptor leak loops;
- `get_extra_info()`;
- rollback helpers;
- event-loop debug mode;
- multiple event loops in separate threads/processes where supported.

### Acceptance criteria

- `await conn.tcp_connect(...)` returns `(reader, writer)`.
- The returned objects satisfy upstream tests and common asyncio utilities.
- No Eggress-specific stream type leaks through the public compatibility API.
- Cancellation and close leave no persistent task or descriptor leak.

## Workstream B5 — Exact `tcp_connect()` contract

### Tasks

1. Match the oracle signature exactly.
2. Match coroutine status.
3. Match destination parsing and default parameters.
4. Match chain traversal.
5. Match authentication and TLS setup ordering.
6. Match exception stage and category.
7. Match connection counter updates.
8. Return the asyncio reader/writer adapter.
9. Ensure direct and currently supported proxy families use the same public contract.
10. Preserve the native synchronous and asynchronous Eggress methods under `eggress` without exposing them through the compatibility object.

### Required differential scenarios

- direct TCP;
- HTTP CONNECT;
- HTTPS proxy;
- SOCKS4;
- SOCKS4a domain destination;
- SOCKS5 no-auth;
- SOCKS5 username/password;
- supported Shadowsocks AEAD;
- Trojan;
- supported H2 path;
- one-hop and multi-hop chains;
- DNS failure;
- refused connection;
- auth failure;
- TLS failure;
- timeout;
- cancellation.

### Acceptance criteria

- Signature and callable kind match exactly.
- Return shape and stream behavior match.
- Bidirectional external interoperability passes for supported paths.
- All mismatches are represented by strict-manifest gaps rather than normalized away.

## Workstream B6 — Minimum viable UDP source contract

### Goal

Restore the public `udp_sendto()` and related construction contract for UDP paths already supported by the Eggress runtime.

### Tasks

1. Match the oracle method signature and coroutine/callback behavior.
2. Implement direct UDP.
3. Implement currently supported SOCKS5 UDP association behavior.
4. Implement currently supported Shadowsocks UDP behavior.
5. Expose compatible association state and callbacks.
6. Match timeout, close, and eviction behavior within this supported subset.
7. Record full multi-hop and later-protocol UDP as explicit Milestone D gaps.

### Acceptance criteria

- Upstream UDP API examples for supported paths run unchanged.
- Response callbacks and source addresses match the oracle’s public contract.
- Unsupported compositions fail explicitly and remain visible in the strict report.
- No absent `udp_sendto` attribute remains on supported proxy objects.

## Workstream B7 — Public server lifecycle compatibility

### Goal

Support the upstream `server = pproxy.Server(uri)` and `await server.start_server(args)` pattern for protocol families already implemented by Eggress.

### Tasks

1. Match `start_server()` signature and callable kind.
2. Translate the compatibility proxy object into an Eggress listener/runtime handle.
3. Return the object type and lifecycle contract expected by the oracle.
4. Match startup error timing.
5. Match bound-address visibility.
6. Match close and wait behavior.
7. Match task ownership and event-loop interaction.
8. Support direct, HTTP/HTTPS, SOCKS4/4a/5, Unix listener where supported, and current compatible listener modes.
9. Keep `eggress.Server` separate and unchanged.

### Required tests

- unchanged upstream server example;
- ephemeral port listener;
- explicit host/port;
- authentication;
- multiple concurrent clients;
- startup bind failure;
- close before first client;
- close with active clients;
- cancellation of startup;
- event-loop shutdown;
- listener object attributes and repr.

### Acceptance criteria

- The canonical upstream server example runs unchanged for supported listeners.
- Returned listener lifecycle matches paired observations.
- No user must construct Eggress config objects to use the compatibility API.

## Workstream B8 — Rules, `DIRECT`, and public scheduling behavior

### Tasks

1. Replace the current lightweight `Rule` record with behavior matching upstream `compile_rule`.
2. Implement rule callable semantics and matching against hostname and port.
3. Match missing-file, malformed-pattern, and invalid-action failures.
4. Reproduce `DIRECT` as the expected object/sentinel, not merely a string or unrelated custom sentinel.
5. Match public scheduler selection for lists of remotes used by source-level applications.
6. Preserve richer native Eggress routing features behind the native API.

### Acceptance criteria

- `pproxy.Rule(...)` returns an oracle-compatible callable/object.
- `DIRECT` identity and behavior match.
- Basic rule-driven direct/proxy selection passes paired tests.
- Top-level and `pproxy.server` references agree on the same sentinel.

## Workstream B9 — Exceptions, warnings, and redaction

### Tasks

1. Inventory exception classes and operation stages for all Milestone B scenarios.
2. Translate Rust errors at the compatibility boundary.
3. Preserve Python exception chaining only where the oracle exposes it.
4. Match warnings for insecure or deprecated behavior within the supported subset.
5. Preserve Eggress credential redaction without changing pproxy-visible exception categories or output shape.
6. Prevent native diagnostic structures from leaking into public exception reprs unless requested through Eggress-native debugging APIs.

### Acceptance criteria

- Differential tests match exception class and failure stage.
- Secret values remain redacted in logs and retained evidence.
- Redaction does not make signatures or public error categories incompatible.

## Workstream B10 — Packaging and installation behavior

### Tasks

1. Keep `eggress-pproxy-compat` as a separate distribution.
2. Pin a compatible Eggress version or ABI range deliberately.
3. Validate wheels in clean virtual environments.
4. Test installation when upstream pproxy is absent.
5. Test replacement/upgrade from upstream pproxy.
6. Test uninstall behavior and namespace cleanup.
7. Test that installing `eggress` alone does not satisfy `import pproxy`.
8. Record supported Python versions based on both Eggress wheel availability and oracle comparison coverage.

### Acceptance criteria

- Clean installation produces one deterministic `pproxy` namespace.
- Package metadata does not claim full drop-in compatibility before Milestone F.
- Wheel smoke tests execute real `Connection` and `Server` behavior, not imports alone.

## Sequencing

### Stage 1 — Namespace and object skeleton

Complete B1, then B2 and the non-network portions of B3.

### Stage 2 — Stream architecture

Complete B4 before finalizing `tcp_connect()` or `start_server()`. The stream adapter is the main architectural dependency.

### Stage 3 — Client API

Complete B5 and supported-path portions of B6.

### Stage 4 — Server API and public utilities

Complete B7 and B8.

### Stage 5 — Error, packaging, and evidence closure

Complete B9 and B10, then run the full strict differential subset.

## Required CI gates

### Fast candidate gate

- compatibility namespace tests;
- object construction tests;
- asyncio adapter unit tests;
- packaging import tests;
- candidate-only client/server integration.

### Oracle differential gate

- namespace and signature inventory;
- top-level alias behavior;
- unchanged upstream client example;
- unchanged upstream server example;
- direct/HTTP/SOCKS client scenarios;
- direct/HTTP/SOCKS server scenarios;
- exception-stage scenarios.

### External interoperability gate

- pproxy client to Eggress compatibility server;
- Eggress compatibility client to pproxy server;
- packet/transcript comparison for supported protocols;
- no skipped required scenarios.

## Milestone acceptance criteria

Milestone B is complete only when all of the following are true:

1. The public pproxy namespace required by the strict manifest is present.
2. Top-level aliases and sentinel behavior match the oracle.
3. `Connection` and `Server` reproduce URI-factory semantics.
4. Supported proxy-chain objects expose compatible public attributes.
5. `tcp_connect()` is a coroutine with the exact signature.
6. `tcp_connect()` returns an asyncio-compatible `(reader, writer)` pair.
7. Supported `udp_sendto()` paths have the expected public contract.
8. The upstream client example passes unchanged.
9. The upstream server example passes unchanged.
10. Public rules and `DIRECT` behavior match.
11. Supported-path exception classes and failure stages match.
12. Bidirectional external interoperability passes for the milestone’s protocol subset.
13. Clean compatibility-wheel installation and replacement tests pass.
14. The canonical `eggress` API remains backward compatible.
15. Every remaining source-level mismatch is explicitly assigned to Milestone C or later in the strict manifest.

## Non-goals and claim boundary

Milestone B authorizes language such as:

> Python source-compatibility preview for common pproxy HTTP/SOCKS/direct deployments.

It does not authorize:

- full Python internal API parity;
- full protocol parity;
- full CLI drop-in parity;
- unqualified drop-in replacement claims.

## Handoff notes

Start by implementing the stream adapter as an isolated prototype with direct TCP only. Validate it against asyncio behavior and leak tests before wiring proxy protocols. A flawed bridge will contaminate every later protocol and lifecycle result.

Do not modify the canonical `eggress.pproxy_connection.ProxyConnection` contract to look like pproxy. Add dedicated compatibility objects under the compatibility distribution or a clearly private adapter module.

Keep all temporary unsupported paths visible in the strict manifest. Do not preserve today’s unconditional success stubs or import-only placeholders as an expedient milestone closure.
