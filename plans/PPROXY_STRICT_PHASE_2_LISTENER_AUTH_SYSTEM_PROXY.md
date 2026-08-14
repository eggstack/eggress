# pproxy Strict Phase 2 — Listener Roles, Auth Reuse, and System Proxy

## Objective

Close the highest-value remaining gaps that can be implemented primarily by wiring existing Eggress machinery rather than creating new transport stacks.

This phase has three independent work packages. They may be implemented as separate commits, but all must meet their own acceptance criteria before the phase is marked complete.

## Work package A — H2 and WS/WSS listener parity

### Current boundary

Eggress already contains H2 dependencies/bridges and a dedicated `eggress-protocol-websocket` crate, but practical compatibility documents currently classify H2 and WS/WSS as upstream-only. pproxy 2.7.9 exposes server/listener roles for both.

### Likely files

- `crates/eggress-protocol-websocket/src/lib.rs`
- `crates/eggress-protocol-http/src/*`
- `crates/eggress-server/src/*`
- `crates/eggress-runtime/src/*`
- `crates/eggress-config/src/*`
- `crates/eggress-pproxy-compat/src/uri.rs`
- `crates/eggress-pproxy-compat/src/translate.rs`
- Python protocol/server adapters and focused tests

### H2 requirements

Implement compatibility listener behavior equivalent to pproxy's H2 CONNECT server:

- accept H2 client connection;
- expose each inbound stream as an independent proxy request;
- parse `:method`, `:authority`, `:path`, and proxy authorization as pproxy does;
- return status headers for successful CONNECT;
- relay DATA frames with flow-control accounting;
- handle stream end/reset without terminating unrelated streams;
- close connection cleanly on GOAWAY/transport loss;
- apply TLS/ALPN only where the URI/configuration requests it.

Do not reproduce pproxy's internal use of private `h2` objects if standard `h2` crate behavior gives the same observable result.

### WS/WSS requirements

Add listener/server mode to the existing WebSocket transport:

- HTTP Upgrade handshake;
- binary data stream adaptation;
- configured fixed target semantics matching pproxy `ws{target}://` behavior;
- authentication handling where pproxy applies it;
- correct close and peer-disconnect behavior;
- WSS as WebSocket over the existing TLS transport.

Preserve standards-compliant ping/pong/close handling even if pproxy's hand-written framing is simpler.

### Evidence

For H2 and WS/WSS, require both:

- pproxy 2.7.9 client -> Eggress listener;
- Eggress client -> pproxy 2.7.9 listener.

Use local echo/fixed-target fixtures so the test does not depend on the public internet.

## Work package B — `--auth` per-source-IP reuse

### Exact behavior

pproxy 2.7.9 keeps authentication state in a class-level table keyed by remote source IP. After a successful auth, subsequent connections from the same IP may reuse that identity until `authtime` expires.

Eggress already has a Python `AuthTable` compatibility object, but the executable compatibility path currently rejects `--auth`.

### Implementation

Add a compatibility-only cache with:

- key: normalized peer IP only, not source port;
- value: authenticated identity plus last-auth timestamp;
- configurable timeout from `--auth`;
- monotonic time for expiry internally;
- bounded memory with lazy expiry or a small periodic cleanup;
- no persistence across process restart.

The cache should be owned by the compatibility listener/runtime state and consulted by HTTP, SOCKS4, SOCKS5, and WS auth paths where pproxy does so.

Do not apply this weak IP-based reuse to native Eggress authentication unless explicitly configured through the compatibility mode.

### Parser correction

Remove any artificial maximum that pproxy 2.7.9 does not impose. Match argparse integer behavior for negative/invalid values according to the oracle rather than inventing validation.

### Tests

Cover:

- first request requires credentials;
- second request from same source IP within timeout succeeds under pproxy-equivalent rules;
- different source IP does not inherit auth;
- entry expires;
- failed credentials do not populate the cache;
- HTTP and SOCKS behavior match oracle expectations.

## Work package C — `--sys` using `eggress-system-proxy`

### Current reusable machinery

Use the existing system-proxy crate. Do not create a second OS-mutation implementation in the pproxy compatibility crate.

Relevant code:

- `crates/eggress-system-proxy/src/apply.rs`
- `crates/eggress-system-proxy/src/backends/*`
- `crates/eggress-system-proxy/src/inspection.rs`
- `crates/eggress-system-proxy/src/command_runner.rs`
- compatibility CLI startup/shutdown orchestration

### Compatibility behavior

For pproxy mode:

1. inspect configured compatibility listeners after bind/startup succeeds;
2. select the same class of listener pproxy selects: prefer usable local SOCKS5 where appropriate, otherwise HTTP according to platform behavior;
3. capture prior settings;
4. apply localhost plus the actual bound port;
5. keep rollback state in memory;
6. restore previous state on normal exit, Ctrl-C, SIGTERM where supported, or startup failure after apply;
7. never leave the system proxy enabled if the service never reached the run loop.

macOS and Windows are the strict 2.7.9 parity targets for `--sys`. Native Linux support may remain an Eggress extension but must not be confused with pproxy parity.

### Safety requirements

- no shell-string command execution;
- no credentials in logs;
- rollback is idempotent;
- failure to apply requested `--sys` is fatal before normal service operation;
- native `eggress system-proxy` commands keep their existing semantics.

### Platform evidence

Unit tests must use `MockCommandRunner`.

Run a real macOS apply/rollback smoke when a suitable environment is available. Windows behavior may be validated by CI or a dedicated environment if no local Windows host exists; do not build a large matrix solely for this phase.

## Non-goals

- H3/QUIC; Phase 8.
- SSH; Phase 7.
- Native-mode IP auth reuse.
- Replacing the existing system-proxy crate.
- Adding a background system service.

## Acceptance criteria

1. `h2://` compatibility listeners accept pproxy clients and proxy a real local TCP target.
2. `ws://` and `wss://` compatibility listeners are operational, not parser-only, and interoperate bidirectionally with pproxy 2.7.9.
3. Existing upstream/client H2 and WS/WSS behavior remains intact.
4. `--auth N` reproduces same-IP authentication reuse and expiry without leaking auth across IPs.
5. Compatibility `--auth` no longer fails the execution gate when the value is valid under pproxy semantics.
6. `--sys` applies an appropriate local listener on supported platforms using the existing system-proxy backend.
7. `--sys` restores prior settings on every tested shutdown/error path.
8. Native Eggress mode does not inherit compatibility-only weak auth or automatic system-proxy mutation.
