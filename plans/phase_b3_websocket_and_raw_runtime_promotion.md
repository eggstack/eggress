# Phase B3 — WebSocket and raw runtime promotion

## Objective

Promote the existing WebSocket, WSS, raw, and tunnel protocol-crate implementations into fully deployable Eggress runtime capabilities. Promotion requires end-to-end support through URI parsing, pproxy translation, native config, validation, compilation, supervisor lifecycle, chaining, Python exposure, observability, hostile-peer handling, and differential or independent interoperability evidence.

## Dependencies

- A1 corrected classifications.
- A2 composition matrix must be available for explicit role and chain cells.
- A3 oracle must support scenario IDs and protocol-specific observations.
- B1/B2 patterns for lifecycle, diagnostics, and external interop should be reused.

## Current state

The protocol crates already contain WebSocket client/server, WSS, byte-stream adaptation, ping/pong, close mapping, fixed-target tunnel behavior, raw tunnel client/server, and some chain primitives. The config compiler and runtime supervisor currently refuse these URI schemes, so none of them qualify as runtime parity.

## Workstream 1: characterize pproxy behavior

Determine for pinned pproxy:

- URI schemes and aliases: `ws`, `wss`, `raw`, `tunnel`;
- listener and upstream roles;
- fixed-target versus dynamically targeted forms;
- path, host header, origin, subprotocol, and query behavior;
- TLS certificate/SNI/CA behavior for WSS;
- binary versus text message handling;
- fragmentation and maximum-message behavior;
- ping/pong and idle timeout behavior;
- close-code and half-close mapping;
- authentication placement;
- chain placement and prior-hop support;
- reverse and UDP applicability;
- stdout/stderr and startup failure behavior.

Record wire transcripts and exact supported composition cells before enabling the runtime.

## Workstream 2: typed configuration

Add or complete typed config for WebSocket and raw transports:

- role and scheme;
- bind or upstream endpoint;
- fixed target where required;
- WebSocket path;
- host/origin/subprotocol headers if supported;
- TLS server and client settings;
- handshake, idle, close, and relay timeouts;
- maximum HTTP header, frame, message, and replay-buffer sizes;
- ping interval and timeout;
- authentication and secret sources;
- chain linkage.

Validation must reject ambiguous target modes, unsupported text-message mode, invalid paths, contradictory TLS settings, unsupported UDP, and invalid chain positions before startup.

## Workstream 3: runtime supervisor integration

1. Add listener construction for WS/WSS/raw/tunnel cells.
2. Add upstream connector construction for supported cells.
3. Integrate with normal service startup, bound-address reporting, cancellation, graceful drain, reload policy, and task tracking.
4. Ensure partial startup cleans up listeners and tasks.
5. Preserve existing routing and chain abstractions rather than creating a parallel supervisor.
6. Add explicit capability checks generated from A2.
7. Remove runtime refusal only for cells that pass all gates.

## Workstream 4: WebSocket stream adapter hardening

Audit the byte-stream adapter for:

- binary-only enforcement;
- fragmented message reassembly with strict bounds;
- no unbounded buffering across frames;
- correct handling of interleaved control frames;
- ping/pong deadlines;
- close handshake and abnormal close;
- mapping stream EOF and half-close to WebSocket close behavior;
- backpressure between frames and stream consumers;
- cancellation safety;
- HTTP upgrade parser bounds;
- header injection prevention;
- masking correctness;
- protocol error close codes.

Use property tests and fuzz smoke for frame and upgrade parsing.

## Workstream 5: raw/tunnel semantics

Define raw/tunnel behavior precisely:

- whether `tunnel://` is an alias or distinct mode;
- listener target acquisition;
- fixed target configuration;
- dynamic target framing if any;
- authentication and TLS wrapping;
- half-close and error behavior;
- chain participation;
- UDP exclusion or implementation.

Do not silently reinterpret raw tunnels as ordinary fixed forwarding if pproxy uses different framing.

## Workstream 6: chaining

Add explicit A2 cells and tests for supported combinations, including representative:

- HTTP/SOCKS5 → WS/WSS → target;
- WS/WSS → HTTP/SOCKS5 where meaningful;
- raw/tunnel as terminal or intermediate hop;
- TLS wrappers around supported hops;
- three-hop chains.

Validate every edge before opening sockets. Unsupported reverse or UDP placement must fail with stable diagnostics.

## Workstream 7: CLI and URI compatibility

1. Normalize paths, percent encoding, credentials, IPv6, default ports, and `__` chains.
2. Ensure `translate`, `check`, and `run` produce consistent config.
3. Provide detailed JSON diagnostics for unsupported headers, target modes, wrappers, or chain positions.
4. Ensure the drop-in binary can launch supported WS/WSS/raw/tunnel services directly.
5. Add help and migration examples.

## Workstream 8: Python exposure

Expose supported cells through:

- service constructors from URI/args/TOML;
- typed config classes where present;
- start/astart, close/wait_closed;
- bound addresses and status;
- WebSocket/raw metrics;
- stable configuration, handshake, TLS, protocol, target, and shutdown exceptions;
- type stubs and docs.

Track C low-level objects should later be able to select these protocol objects without a second implementation.

## Workstream 9: observability

Add bounded-cardinality metrics for:

- handshake accepted/rejected;
- active tunnels;
- frames/messages/bytes;
- ping timeout;
- protocol close/error reason groups;
- upstream connect failures;
- message-size rejection;
- raw tunnel sessions and bytes.

Do not expose paths, credentials, or arbitrary hosts as labels.

## Workstream 10: interoperability and fault testing

Required tests:

- Eggress client/server pairs;
- pproxy pair and cross-pairs where wire-compatible;
- independent WebSocket implementation interoperability;
- WSS trusted/untrusted/custom CA/SNI cases;
- fragmented binary messages;
- text-frame rejection;
- oversized headers/frames/messages;
- malformed masking/opcodes/control frames;
- ping/pong timeout;
- normal and abnormal close;
- target refusal and timeout;
- upstream disconnect;
- half-close;
- concurrent sessions;
- chain combinations;
- cancellation, drain, reload, and soak.

## Diagnostics

Define stable codes for invalid URI/path, unsupported role, unsupported text mode, handshake failure, TLS failure, frame/message too large, protocol violation, ping timeout, unsupported chain cell, unsupported UDP, target failure, and runtime promotion unavailable.

## Acceptance criteria

- Supported WS/WSS/raw/tunnel URIs are accepted by config and supervisor and launch real services.
- Listener/upstream/chain roles match characterized pproxy behavior.
- WebSocket parsing and buffering are bounded and fuzz-smoked.
- WSS uses secure TLS defaults with explicit compatibility relaxations.
- Supported cells pass end-to-end and differential/interoperability tests.
- CLI, Python, A2 matrix, manifest, and docs agree.
- Runtime refusal remains for every unproven cell.
- No protocol-crate-only capability is promoted solely because it compiles.

## Out of scope

- H2 promotion, handled in B4;
- QUIC/H3;
- reverse WebSocket/raw unless characterized and separately approved;
- UDP tunnels unless pproxy behavior is confirmed and Track E abstractions exist;
- generic application-level WebSocket proxying beyond byte tunnels.

## Recommended commit sequence

1. oracle characterization and design record;
2. config/model/validation;
3. listener and upstream supervisor integration;
4. adapter hardening;
5. chain integration;
6. CLI/Python exposure;
7. interop/fuzz/soak tests;
8. matrix, manifest, and documentation promotion.