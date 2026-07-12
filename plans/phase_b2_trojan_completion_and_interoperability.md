# Phase B2 — Trojan completion and interoperability

## Objective

Move Trojan support from internally implemented and partially tested to release-grade pproxy parity. The end state must provide reliable client and server roles, real interoperability with independent implementations, exact URI/config/CLI/Python exposure, negative-path correctness, chain integration, fallback behavior where pproxy supports it, and evidence sufficient to promote only the cells that truly behave compatibly.

## Dependencies

- A1 parity classifications and source-of-truth repair.
- A2 composition matrix IDs and validation.
- A3 oracle scenarios and reporting.
- B1 should be complete enough that reverse-specific Trojan composition can be explicitly classified, even if unsupported.

## Current state

Eggress already has Trojan TCP client and server primitives, rustls integration, password verification, request framing, and synthetic happy-path tests. The current parity classification was correctly demoted because the repository lacks enough independent interoperability evidence and does not yet close fallback, chain, and Python surfaces.

## Workstream 1: reference characterization

Use pinned pproxy and at least one independent Trojan implementation to characterize:

- accepted URI forms and default ports;
- password-only userinfo and percent encoding;
- client and server roles;
- IPv4, IPv6, and domain target framing;
- TLS SNI, ALPN, CA, certificate, and insecure-mode behavior;
- authentication rejection behavior;
- malformed request handling;
- fallback behavior and trigger conditions;
- connection close and half-close behavior;
- whether UDP is implemented and functional in the reference target;
- chaining before and after a Trojan hop;
- startup, stdout/stderr, and exit behavior.

Record all observations as A3 scenarios or external interoperability fixtures.

## Workstream 2: client correctness

1. Audit request encoding and maximum lengths.
2. Verify domain length enforcement and address-type encoding.
3. Ensure TLS verification is secure by default.
4. Support custom CA roots, SNI override only when explicitly configured, and explicit insecure compatibility mode.
5. Bound handshake and response reads.
6. Preserve half-close and relay error semantics.
7. Ensure connection failure categories map to stable diagnostics and Python exceptions.
8. Add direct, chained, and TLS-error tests.

## Workstream 3: server correctness

1. Validate SHA-224 password verification behavior against external clients.
2. Bound credential and request parsing.
3. Reject malformed address types, invalid domain lengths, and unsupported commands.
4. Delay success until target connection succeeds where the wire protocol permits.
5. Propagate refusal and timeout behavior consistently.
6. Support IPv4, IPv6, and domains.
7. Integrate connection limits, routing, observability, graceful shutdown, and reload semantics.
8. Add hostile-client and resource-exhaustion tests.

## Workstream 4: fallback routing

Characterize and implement pproxy-compatible fallback if present.

Requirements:

- explicit fallback target or route;
- well-defined trigger: authentication failure, non-Trojan TLS traffic, malformed request, or other characterized condition;
- no credential oracle or timing leak beyond unavoidable protocol behavior;
- bounded pre-read/replay buffer;
- route through normal Eggress routing abstractions where possible;
- clear distinction between Trojan fallback and generic listener protocol sniffing;
- metrics for fallback attempts, successes, failures, and reasons.

Do not add fallback behavior based on assumption. If pinned pproxy does not support it, classify it as Eggress-native rather than parity.

## Workstream 5: runtime/config/CLI integration

1. Normalize `trojan://` URI parsing and default ports.
2. Ensure listener and upstream configuration expose all supported TLS and auth options.
3. Add A2 matrix cells for listener, upstream, terminal hop, and intermediate chain positions.
4. Validate impossible compositions before startup.
5. Ensure `pproxy check`, `translate`, and drop-in binary behavior agree with the matrix.
6. Preserve secret redaction in generated TOML, logs, diagnostics, and debug output.
7. Add examples for client, server, chained, custom CA, and insecure compatibility modes.

## Workstream 6: Python exposure

Expose Trojan through the current Python service API and future Track C compatibility layer:

- construct from URI, args, TOML, and typed config;
- client/server role selection;
- TLS options;
- start/astart and close/wait_closed;
- status, bound address, metrics, and last error;
- stable exceptions for TLS, authentication, malformed protocol, target connect, and configuration failures;
- type stubs and documentation.

No network operation may hold the GIL.

## Workstream 7: interoperability suite

Required pairs:

- Eggress client → Eggress server;
- Eggress client → independent Trojan server;
- independent Trojan client → Eggress server;
- pproxy client/server pair where functional;
- Eggress ↔ pproxy cross-pair if wire-compatible.

Test:

- IPv4, IPv6, domain;
- valid and invalid auth;
- trusted and untrusted certificates;
- SNI mismatch;
- custom CA;
- fragmented handshakes;
- large and boundary-sized domains;
- target refusal and timeout;
- half-close;
- concurrent connections;
- chain positions;
- fallback if supported.

External implementation versions must be pinned in CI or container fixtures.

## Workstream 8: UDP decision

Use the oracle to determine whether pproxy Trojan UDP is implemented and usable.

- If functional: write a separate datagram design using Track E abstractions and add explicit cells.
- If nonfunctional or absent: document the finding and retain TCP-only parity.
- Do not claim unsupported UDP based on TCP capability.

## Diagnostics

Define stable codes for:

- invalid Trojan URI;
- missing password;
- TLS verification failure;
- SNI mismatch;
- unsupported address type;
- malformed request;
- authentication failure;
- target refusal/timeout;
- fallback unavailable/failure;
- unsupported chain position;
- unsupported Trojan UDP.

## Testing requirements

Run:

- protocol unit/property tests;
- runtime integration tests;
- external interoperability matrix;
- A3 differential scenarios;
- Python lifecycle tests;
- fuzz smoke for request parser;
- concurrency and soak tests;
- full workspace fmt/clippy/test and parity validators.

## Acceptance criteria

- Trojan client and server interoperate with at least one independent implementation.
- Supported pproxy Trojan forms are accepted by the drop-in CLI and Python service layer.
- TLS verification is secure by default and compatibility relaxations are explicit.
- Authentication, malformed input, timeout, refusal, half-close, and shutdown behavior are tested.
- Fallback behavior is implemented only if characterized and is covered by negative-path tests.
- Every promoted A2 matrix cell has integration and differential/interoperability evidence.
- Manifest tiers are promoted only after all applicable layers pass.
- Documentation no longer relies on synthetic-only evidence.

## Out of scope

- generic QUIC/H3 transport;
- SSH;
- multi-hop UDP;
- full low-level Python `Connection`/`Server` compatibility;
- reverse-Trojan compositions unless separately characterized.

## Recommended commit sequence

1. oracle characterization;
2. client/server correctness fixes;
3. runtime/config/CLI integration;
4. external interoperability fixtures;
5. fallback implementation if justified;
6. Python exposure;
7. UDP decision record;
8. docs, manifest, and report regeneration.