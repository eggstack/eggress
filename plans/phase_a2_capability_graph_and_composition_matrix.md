# Phase A2 — Capability graph and composition matrix

## Objective

Replace flat or over-broad parity reasoning with a machine-readable composition model that captures how protocols, transports, roles, directions, platforms, CLI forms, and Python APIs combine. The result must prevent Eggress from claiming parity for a protocol merely because one isolated implementation exists.

## Dependency

Phase A1 must be complete. The manifest and documentation must already reflect current executable behavior before this phase adds a richer model.

## Design principles

1. The existing capability manifest remains the canonical inventory, but gains references to composition nodes and edges or is paired with a generated companion file.
2. Do not create two independently maintained sources of truth.
3. Composition validity must be queryable by config validation, CLI compatibility checks, Python compatibility reports, tests, and documentation generation.
4. Unsupported compositions must fail before service startup with a stable diagnostic code.
5. Support in one role must never imply support in another role.

## Required dimensions

Model at least the following dimensions:

- protocol: direct, HTTP, HTTPS, SOCKS4, SOCKS4a, SOCKS5, Shadowsocks, Trojan, SSH, WS, WSS, raw, tunnel, H2, QUIC, H3, Unix, redir/PF;
- role: listener, upstream, intermediate chain hop, terminal target, reverse server, reverse client, transparent interception endpoint;
- transport wrapper: plain TCP, TLS, WebSocket, H2 stream, QUIC stream/datagram, SSH channel, Unix socket;
- traffic kind: TCP stream, UDP datagram;
- direction: forward, reverse/backward;
- addressing: IPv4, IPv6, domain, Unix path;
- authentication: none, username/password, protocol password/key, certificate, SSH key/agent;
- platform: Linux, macOS, Windows, Unix-only;
- entry surface: native TOML, Eggress CLI, pproxy-compatible CLI, Rust embed API, Python service API, Python pproxy API;
- evidence: unit, integration, differential, independent interoperability, platform integration;
- exactness: drop-in, warning, native equivalent, intentional non-parity, unsupported.

## Workstream 1: schema design

Design a deterministic schema under `docs/parity/` or extend the existing TOML manifest. A recommended shape is:

- capability records remain human-meaningful feature claims;
- composition nodes represent protocols/transports/roles;
- edges represent legal transitions or wrappers;
- cells reference capability IDs and evidence IDs;
- constraints express platform, traffic type, auth, and chain-position restrictions.

The schema must support questions such as:

- Can a SOCKS5 listener route TCP through HTTP then Shadowsocks?
- Can a SOCKS5 UDP association traverse a multi-hop chain?
- Can WSS be used as an upstream through a prior SOCKS5 hop?
- Is Trojan available as listener, upstream, and intermediate hop?
- Does the pproxy CLI expose the same cell as native TOML?
- Is the cell available from the Python compatibility API?

Document schema invariants and examples in `docs/parity/README.md`.

## Workstream 2: validator and generator

Extend the parity validator or add a small deterministic generator that:

1. rejects unknown node, edge, capability, evidence, platform, and diagnostic references;
2. rejects a supported cell when an applicable runtime/config layer is incomplete;
3. rejects a drop-in cell without differential or justified equivalent evidence;
4. rejects contradictory classifications for the same normalized cell;
5. verifies every CLI/Python-exposed composition maps to an executable native composition;
6. verifies runtime-refused protocols cannot appear in supported cells;
7. verifies UDP cells identify framing and association semantics;
8. verifies platform-specific cells have platform tests or explicit environment gates;
9. generates a human-readable matrix and summary statistics;
10. supports a strict `--check` mode for CI.

Generated files must include a header stating that manual edits are invalid.

## Workstream 3: seed the current matrix

Populate the graph from current behavior, beginning with:

### TCP listeners

- HTTP/HTTPS;
- SOCKS4/4a;
- SOCKS5;
- mixed HTTP/SOCKS listeners;
- Shadowsocks;
- Trojan;
- Unix where applicable;
- transparent Linux redirection.

### TCP upstreams and chains

- direct;
- HTTP CONNECT;
- SOCKS4/4a;
- SOCKS5;
- Shadowsocks;
- Trojan;
- three-or-more-hop chains currently supported.

### UDP

- direct;
- SOCKS5 server/client association;
- one-hop SOCKS5 upstream;
- Shadowsocks UDP;
- standalone pproxy UDP;
- explicit unsupported cells for HTTP, Trojan, multi-hop, MASQUE/CONNECT-UDP, and transparent UDP.

### Protocol-crate-only nodes

Represent WS/WSS, raw/tunnel, and H2 as implemented primitives with runtime/config edges absent or refused. This allows the graph to show investment without falsely promoting them.

### Reverse

Represent only cells proven by Phase A1. Do not infer chain, TLS, parallel-channel, Python, or UDP support.

## Workstream 4: integrate validation into configuration

Introduce a reusable capability query in an appropriate crate so configuration compilation can validate a normalized service topology against the same conceptual matrix.

Requirements:

- no runtime dependency on documentation parsing;
- generated Rust data or a shared typed source is acceptable;
- validation errors must identify the unsupported edge/cell, not merely the protocol name;
- diagnostics should suggest a supported alternative where clear;
- pproxy `check --json` should expose normalized composition and failed constraints;
- Python `CompatibilityReport` should include composition-level findings.

Avoid putting test-only or docs-only data in the runtime binary if a compact generated representation suffices.

## Workstream 5: tests

Add table-driven tests for:

- all currently supported listener→upstream combinations;
- all supported chain pairs and representative 3+ hop chains;
- TLS wrapper legality;
- UDP restrictions;
- reverse restrictions;
- platform restrictions;
- CLI translator to matrix agreement;
- Python feature report to matrix agreement;
- runtime-refused protocol handling;
- unknown and contradictory schema entries;
- generated document freshness.

Add negative tests showing invalid topologies fail during validation rather than after sockets are opened.

## Deliverables

- machine-readable capability graph/composition schema;
- typed model or generated runtime representation;
- strict validator/generator;
- generated composition matrix documentation;
- config compiler integration;
- enhanced `pproxy check --json` composition output;
- Python compatibility-report integration;
- table-driven positive and negative tests;
- CI freshness and consistency checks.

## Acceptance criteria

- Every supported composition is represented explicitly.
- Every matrix cell identifies its role, traffic type, platform, entry surface, tier, and evidence.
- WS/raw/H2 protocol primitives cannot be mistaken for runtime support.
- Unsupported compositions fail before listener startup.
- CLI, TOML, runtime, Python, and documentation use the same normalized composition vocabulary.
- At least one end-to-end test exists for every supported cell category; high-value cells have direct scenarios.
- Generated matrices are reproducible and CI rejects stale output.
- The flat 139-capability summary remains available but no longer serves as the sole parity metric.

## Out of scope

- implementing missing transports or matrix cells;
- promoting WS/raw/H2;
- changing protocol wire behavior;
- building the expanded pproxy process oracle, which belongs to A3;
- assigning final business weights to capabilities beyond adding schema support for weighting.

## Handoff notes

Implement the schema and validator before integrating runtime validation. Keep normalization logic centralized. Use stable IDs suitable for test references and generated reports. Avoid a giant hard-coded match statement duplicated across crates; prefer generated typed data or one shared capability module.