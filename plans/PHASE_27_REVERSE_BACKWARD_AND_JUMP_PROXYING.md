# Phase 27 Plan: Reverse, Backward, and Jump Proxying Parity

## Purpose

Phase 27 addresses pproxy's reverse/backward proxying and jump semantics. This is distinct from normal client-initiated forward proxying. Reverse modes create long-lived control relationships, remote listener registration, and different lifecycle/failure behavior.

The goal is to capture pproxy behavior precisely, implement the minimum compatible reverse/backward surface that fits Eggress safely, and avoid inventing a different product under pproxy-compatible names.

## Scope

This phase covers:

- pproxy behavior capture for reverse, backward, jump, and backward-jump forms.
- Control-channel design for reverse proxying.
- Remote listener registration and lifecycle.
- Reconnect, heartbeat, drain, and shutdown semantics.
- Routing integration for reverse-accepted streams.
- CLI/URI translation for captured forms.
- Metrics/admin visibility for reverse sessions.
- Differential or local multi-process tests.

## Non-goals

Do not implement a general mesh, VPN, or daemon orchestration layer.

Do not add NAT traversal beyond what pproxy behavior requires.

Do not implement UDP reverse proxying unless pproxy evidence proves it exists and can be added safely.

Do not weaken authentication or access controls for reverse listeners.

## Vocabulary

The implementation must define terms explicitly after behavior capture. Until then, use provisional labels:

- **Forward proxy**: client connects to Eggress and asks Eggress to connect outward.
- **Reverse proxy control client**: process that establishes an outbound control channel to a remote acceptor.
- **Reverse acceptor/listener**: side that exposes a listener and dispatches accepted streams over a control channel.
- **Jump chain**: pproxy chain syntax that composes multiple proxy hops.
- **Backward jump**: pproxy-specific reverse form if confirmed by behavior capture.

Do not finalize naming until pproxy behavior is documented.

## Work items

### 27.1 Capture pproxy reverse/backward behavior

Use the pproxy oracle with multi-process fixtures.

Capture:

- URI syntax and CLI forms for reverse, backward, jump, and backward-jump;
- which side opens the TCP control connection;
- which side binds the public listener;
- how target destination is encoded;
- whether protocols are multiplexed over one control channel or use one control channel per connection;
- whether authentication is required or optional;
- reconnect behavior;
- heartbeat/keepalive behavior;
- behavior when either side restarts;
- behavior when listener bind fails;
- behavior when target connect fails;
- close/half-close behavior;
- log messages and exit codes;
- whether UDP is supported;
- how chaining interacts with reverse mode.

Document in:

```text
docs/protocols/REVERSE_PROXYING.md
docs/PPROXY_PARITY_SPEC.md
```

Add initial manifest entries as unimplemented/captured.

### 27.2 Decide protocol model and security envelope

Write an ADR before implementing.

Suggested path:

```text
docs/adr/ADR_reverse_backward_proxying.md
```

Decision points:

- custom Eggress reverse control protocol vs pproxy wire-compatible reverse protocol;
- whether reverse mode is pproxy-compatible only or also Eggress-native;
- authentication requirement;
- TLS requirement/default;
- multiplexing strategy;
- reconnect strategy;
- remote listener authorization model;
- exposed config fields;
- compatibility limitations.

Security defaults should be stricter than plain unauthenticated reverse control unless pproxy-compatible mode explicitly requires otherwise. If a permissive pproxy mode is implemented, gate it as compatibility mode with warnings.

### 27.3 Control channel design

Design the reverse control channel as a small state machine.

Possible states:

```text
Disconnected
Connecting
Authenticating
RegisteringListener
Ready
Draining
Reconnecting
Closed
```

Required control messages, subject to pproxy capture:

- hello/version;
- authenticate;
- register listener;
- listener registered/failed;
- open stream;
- stream opened/failed;
- stream data;
- stream close/reset;
- heartbeat/ping;
- drain/shutdown;
- error.

Requirements:

- bounded frame sizes;
- bounded concurrent streams;
- per-stream flow control or backpressure;
- cancellation-safe stream cleanup;
- redacted logs;
- routeable target identity;
- metrics for each control state.

### 27.4 Reverse acceptor/server implementation

Implement the side that accepts control connections and exposes remote listeners, if this matches pproxy behavior.

Requirements:

- authenticate control clients;
- authorize requested listener bind address;
- bind listener only after successful authorization;
- accept client connections on registered listener;
- dispatch accepted streams over control channel;
- handle control channel loss by closing or draining listener according to config;
- prevent arbitrary listener bind unless explicitly allowed;
- enforce max listeners per control client;
- enforce max streams per listener;
- expose listener state in admin API.

Tests:

- register listener successfully;
- reject unauthorized listener;
- reject bind conflict;
- accept stream and relay echo;
- control channel closes while listener active;
- graceful drain.

### 27.5 Reverse control client implementation

Implement the side that dials out to a reverse acceptor and services stream-open requests.

Requirements:

- establish outbound control connection;
- authenticate;
- request listener registration;
- service incoming stream-open messages;
- route each stream through normal route engine or fixed target according to pproxy behavior;
- reconnect with backoff;
- drain on shutdown;
- expose status and errors.

Tests:

- connects/registers successfully;
- wrong auth fails;
- reconnect after acceptor restart;
- route rejection propagates to acceptor;
- target connection failure maps to stream-open failure;
- graceful shutdown drains streams.

### 27.6 Stream multiplexing and relay

If pproxy multiplexes streams, add a bounded stream-multiplexing layer. If pproxy uses one connection per accepted stream, implement that simpler model.

Requirements:

- preserve TCP byte ordering per stream;
- prevent one stream from starving all others;
- handle half-close or map to close semantics explicitly;
- enforce max frame size;
- enforce max concurrent streams;
- ensure stream IDs cannot collide or be reused unsafely;
- expose per-stream metrics.

If multiplexing is implemented, add property tests for stream ID allocation, close ordering, and frame parsing.

### 27.7 Routing and policy integration

Reverse-accepted streams must still flow through policy.

Route request should include:

- original client address if known;
- reverse listener ID;
- control client identity;
- requested target;
- transport kind;
- protocol ID.

Policy requirements:

- support listener-specific allowlists;
- deny private-network targets unless allowed;
- avoid proxy loops;
- preserve route explanation;
- account for active leases and scheduler state.

### 27.8 CLI/URI compatibility

Extend pproxy compatibility parsing.

Tasks:

- parse captured reverse/backward/jump URI forms;
- translate supported forms to TOML;
- reject unsupported forms with structured diagnostics;
- include warnings for security-sensitive compatibility defaults;
- update Python translation helpers if applicable;
- add manifest entries and tests.

Acceptance:

- `eggress pproxy check` correctly classifies reverse forms;
- `eggress pproxy translate` produces runnable configs for supported forms;
- unsupported forms provide precise next-step diagnostics.

### 27.9 Admin, metrics, and observability

Add visibility for reverse mode.

Metrics:

- control connections active;
- control reconnects;
- listener registrations success/failure;
- reverse streams opened/closed/failed;
- stream bytes in/out;
- heartbeat failures;
- auth failures;
- drain duration.

Admin/status:

- control client state;
- registered listeners;
- active streams;
- last error;
- reconnect backoff;
- peer identity;
- security mode.

### 27.10 Testing strategy

Ungated tests:

- parser/URI translation;
- control protocol frame encode/decode;
- mocked control connection state machine;
- local reverse server/client echo over loopback;
- auth failure;
- bind conflict;
- reconnect using local test processes;
- route rejection.

Gated tests:

- pproxy reverse/backward differential if pproxy behavior can be driven deterministically;
- long-running reconnect smoke;
- adverse network half-close/reset tests.

Suggested gate:

```text
EGRESS_REQUIRE_REVERSE_INTEROP=1
```

### 27.11 Documentation updates

Update:

- `docs/protocols/REVERSE_PROXYING.md`;
- `docs/PPROXY_PARITY_SPEC.md`;
- `docs/PARITY_MATRIX.md`;
- `docs/COMPATIBILITY_EVIDENCE.md`;
- `docs/CONFIG_REFERENCE.md`;
- `docs/OPERATIONS.md`;
- `docs/SECURITY_REVIEW.md`;
- README;
- manifest.

Docs must include sequence diagrams for both sides of the connection.

## Validation commands

Baseline:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-testkit manifest
```

Feature-specific examples:

```bash
cargo test -p eggress-runtime reverse
cargo test -p eggress-server reverse
cargo test -p eggress-pproxy-compat reverse
```

Gated:

```bash
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1 reverse
```

## Acceptance criteria

Phase 27 is complete when:

- pproxy reverse/backward/jump behavior is captured.
- An ADR defines the reverse protocol/security model.
- Supported reverse mode can relay at least TCP echo through local multi-process tests.
- CLI/URI compatibility classifies all captured reverse forms.
- Metrics/admin expose reverse state.
- Manifest and docs agree with evidence.
- Unsupported reverse variants are explicitly classified and diagnostically rejected.

## Remaining expected gaps after this phase

- System proxy configuration.
- True pproxy-shaped Python API drop-in replacement.
- Any advanced transport deferred from Phase 26.
- Any reverse UDP or multiplexing variant intentionally deferred.

## Handoff notes

Reverse proxying is a lifecycle problem more than a relay problem. Most defects will appear around reconnects, listener cleanup, stale streams, and over-permissive listener registration. Keep the first implementation narrow and test lifecycle edges aggressively.
