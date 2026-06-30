# Phase 26 Plan: Advanced Transports — H2, H3, QUIC, WebSocket, and Raw Tunnels

## Purpose

Phase 26 addresses the advanced transport surface from Python `pproxy`: HTTP/2 CONNECT, HTTP/3 CONNECT, QUIC-style proxy transports, WebSocket tunnels, raw fixed-target tunnels, and related TLS/ALPN behavior.

This is a high-risk compatibility phase because these transports can look similar at the product level while having materially different framing, stream, flow-control, and connection lifecycle semantics. The phase must begin with behavior capture and adapter design, not direct feature sprawl.

## Scope

This phase covers:

- pproxy behavior capture for `h2`, `h3`, `quic`, `ws`, `wss`, `raw`, `tunnel`, and related URI forms.
- Transport abstraction cleanup where needed to support stream-oriented and multiplexed upstreams.
- HTTP/2 CONNECT server/client support if pproxy behavior is clear and compatible with current stack.
- WebSocket tunnel support over TCP streams.
- Raw fixed-target TCP tunnel support.
- HTTP/3/QUIC investigation and either implementation plan or explicit deferral.
- TLS/ALPN integration for advanced transports.
- Differential/interop tests where practical.

## Non-goals

Do not implement MASQUE or CONNECT-UDP unless it is needed for a specific pproxy-compatible behavior already captured.

Do not attempt to support every QUIC/H3 variant in one pass. Start with the exact pproxy behavior.

Do not replace the existing HTTP/1/SOCKS/TCP path. Advanced transports should plug into existing route and relay machinery through adapters.

Do not claim H2/H3/QUIC compatibility without real client/server interop evidence.

## Architectural principle

Separate three layers:

1. **Application proxy protocol**: HTTP CONNECT, SOCKS5, raw tunnel, etc.
2. **Transport wrapper**: TLS, WebSocket, HTTP/2 stream, QUIC stream, Unix socket, TCP.
3. **Routing/chain semantics**: direct, upstream, group scheduling, fallback, rejection.

Advanced transports should be represented as stream adapters or datagram adapters, not as special cases inside HTTP/SOCKS protocol code.

## Work items

### 26.1 Capture pproxy advanced transport behavior

Use real pproxy and black-box probes to document accepted syntax and behavior.

Capture:

- URI schemes and aliases for HTTP/2, HTTP/3, QUIC, WebSocket, WSS, raw, tunnel, and echo modes;
- whether each scheme is listener, upstream, or both;
- whether schemes are transport wrappers or application protocols;
- TLS requirements and ALPN values;
- authentication behavior;
- path/query handling for WebSocket URLs;
- WebSocket subprotocol behavior;
- HTTP/2 stream behavior and multiplexing;
- HTTP/3/QUIC certificate behavior;
- close/reset behavior;
- error diagnostics for malformed frames;
- CLI translation behavior;
- Python API exposure, if any.

Document findings in:

```text
docs/protocols/ADVANCED_TRANSPORTS.md
docs/protocols/WEBSOCKET_TUNNELS.md
docs/protocols/H2_H3_QUIC.md
docs/protocols/RAW_TUNNELS.md
docs/PPROXY_PARITY_SPEC.md
```

Add manifest entries for every captured surface before implementation.

### 26.2 Define stream adapter traits

If current stream handling is not already abstract enough, add a small adapter layer.

Suggested shape:

```rust
pub trait ProxyStream: AsyncRead + AsyncWrite + Send + Unpin {}

pub trait StreamTransportConnector {
    async fn connect_stream(&self, target: TargetAddr, ctx: TransportContext) -> Result<BoxedProxyStream, TransportError>;
}

pub trait StreamTransportAcceptor {
    async fn accept_stream(&self) -> Result<AcceptedStream, TransportError>;
}
```

Requirements:

- avoid leaking H2/QUIC/WebSocket types into protocol code;
- support close/half-close semantics where transport permits;
- expose peer/local address where available;
- expose stream metadata for observability;
- preserve backpressure;
- support cancellation and graceful shutdown.

Do not overgeneralize. Add only the adapter surface required by this phase.

### 26.3 HTTP/2 CONNECT behavior and implementation

Implement HTTP/2 CONNECT if pproxy behavior and crate support make it practical.

Potential dependencies:

- existing `hyper` stack if HTTP/2 support is already available;
- `h2` crate for lower-level stream control;
- rustls ALPN integration.

Server/listener requirements:

- accept TLS with ALPN `h2` or cleartext h2c only if pproxy supports it;
- parse CONNECT requests;
- validate `:authority` target;
- authenticate if configured;
- open route/upstream/direct target;
- bridge HTTP/2 stream data to TCP stream;
- handle RST_STREAM, GOAWAY, window updates, remote close;
- record per-stream metrics.

Client/upstream requirements:

- connect to HTTP/2 proxy;
- negotiate ALPN;
- issue CONNECT request;
- map response status to route errors;
- reuse H2 connection for multiple streams only if planned and safe;
- support per-stream cancellation.

Testing:

- synthetic H2 CONNECT direct echo;
- H2 upstream chain from SOCKS5 inbound;
- auth success/failure if pproxy supports it;
- RST_STREAM behavior;
- large payload flow-control test;
- gated pproxy differential if pproxy H2 is available.

### 26.4 WebSocket tunnel support

Implement WebSocket stream tunnel support as an explicit transport wrapper.

Server/listener requirements:

- accept HTTP Upgrade to WebSocket;
- validate path/host according to config and captured pproxy behavior;
- authenticate if pproxy-compatible behavior exists;
- treat binary frames as stream bytes;
- define behavior for text frames;
- map close frames to stream shutdown;
- implement ping/pong handling;
- enforce frame/message size limits;
- support WSS via existing TLS listener.

Client/upstream requirements:

- open WebSocket or WSS connection to upstream;
- send binary frames for stream data;
- handle fragmentation;
- handle close/ping/pong;
- preserve backpressure;
- support proxy chains where WebSocket is one hop.

Tests:

- WebSocket echo tunnel;
- WSS echo tunnel with local certs;
- fragmented frames;
- text frame rejection/handling;
- close frame behavior;
- chain through WebSocket upstream;
- pproxy differential if pproxy WebSocket behavior is available.

### 26.5 Raw fixed-target tunnel support

Implement raw fixed-target TCP tunnel support if pproxy exposes it.

Behavior model:

- listener accepts a TCP stream;
- no HTTP/SOCKS negotiation;
- target is fixed by listener config or URI;
- route engine may still enforce policy before connecting;
- bytes relay directly.

Requirements:

- explicit listener mode: no autodetection;
- fixed target validation at startup;
- metrics distinct from HTTP/SOCKS;
- no accidental open proxy behavior;
- clear diagnostics when target omitted.

Tests:

- raw TCP listener to echo target;
- half-close behavior;
- rejected target policy;
- route explanation;
- CLI URI translation.

### 26.6 QUIC and HTTP/3 investigation

Capture pproxy behavior first. Then decide whether to implement now.

Investigation questions:

- Does pproxy's `quic` mode use QUIC streams, datagrams, or custom framing?
- Does `h3` mean HTTP/3 CONNECT or an HTTP-over-QUIC wrapper?
- What TLS/cert/ALPN behavior is expected?
- Are UDP datagrams part of the behavior?
- Does pproxy support multiplexing?
- Is behavior stable enough to reproduce?

Potential implementation dependencies:

- `quinn` for QUIC;
- `h3`/`h3-quinn` if HTTP/3 CONNECT is needed;
- rustls for TLS.

Decision outputs:

- implement minimal QUIC stream transport;
- implement H3 CONNECT;
- defer with explicit non-parity rationale;
- split QUIC/H3 into a later dedicated phase.

Add ADR:

```text
docs/adr/ADR_quic_h3_pproxy_parity.md
```

### 26.7 TLS and ALPN integration

Advanced transports require precise TLS behavior.

Tasks:

- add ALPN config for H2/H3/WSS where needed;
- validate certificate verification behavior;
- preserve secure defaults;
- keep insecure compatibility mode explicit;
- add SNI tests;
- document pproxy divergence if pproxy is more permissive.

Acceptance:

- advanced transport TLS does not bypass existing rustls policy;
- ALPN values are visible in config/docs;
- insecure behavior requires explicit config.

### 26.8 URI/CLI compatibility

Extend `eggress-pproxy-compat` for captured schemes.

Tasks:

- parse H2/H3/QUIC/WebSocket/raw schemes;
- classify each as supported, partial, unsupported, or intentional non-parity;
- generate TOML for supported schemes;
- produce structured diagnostics for unsupported variants;
- update Python translation helpers if applicable.

Acceptance:

- every captured pproxy advanced scheme has a manifest entry and CLI behavior;
- unsupported forms fail deterministically and redacted.

### 26.9 Metrics and admin visibility

Add metrics for advanced transports.

Suggested metrics:

- H2 streams opened/closed/reset;
- H2 flow-control stalls;
- WebSocket sessions opened/closed;
- WebSocket frame decode errors;
- raw tunnel sessions;
- QUIC connections/streams if implemented;
- ALPN negotiation failures;
- advanced transport auth failures.

Admin/status should include listener/upstream transport kind and feature-gate status.

### 26.10 Testing strategy

Ungated tests:

- parser/URI translation tests;
- raw tunnel synthetic tests;
- WebSocket local synthetic tests;
- H2 local synthetic tests if implementation exists;
- config validation;
- manifest validation.

Gated tests:

- pproxy differential for any pproxy-supported advanced scheme;
- curl or h2 client interop for H2 where possible;
- WebSocket interop using standard client/server tools;
- QUIC/H3 interop only if implementation lands.

Suggested gates:

```text
EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1
EGRESS_REQUIRE_H3_INTEROP=1
```

### 26.11 Documentation updates

Update:

- `docs/PARITY_MATRIX.md`;
- `docs/COMPATIBILITY_EVIDENCE.md`;
- `docs/PPROXY_PARITY_SPEC.md`;
- `docs/PPROXY_MIGRATION.md`;
- `docs/CONFIG_REFERENCE.md`;
- `docs/METRICS.md`;
- `docs/SECURITY_REVIEW.md`;
- README capability table;
- manifest.

Docs must avoid lumping H2, H3, QUIC, WebSocket, and raw together as a single compatible feature. Each has separate evidence.

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
cargo test -p eggress-runtime raw_tunnel
cargo test -p eggress-runtime websocket
cargo test -p eggress-runtime h2_connect
cargo test -p eggress-pproxy-compat advanced_transport
```

Gated:

```bash
EGRESS_REQUIRE_ADVANCED_TRANSPORT_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1 advanced_transport
```

## Acceptance criteria

Phase 26 is complete when:

- pproxy advanced transport behavior is captured and documented.
- Every advanced scheme has manifest classification.
- At least raw tunnel and WebSocket support are implemented or explicitly deferred with rationale.
- H2 CONNECT is implemented or has a documented deferral based on behavior capture.
- H3/QUIC has an ADR and either implementation or explicit split/defer decision.
- CLI/URI translation reflects actual support.
- Metrics and admin status identify advanced transport sessions.
- Docs and manifest agree with tests and evidence.

## Remaining expected gaps after this phase

- Reverse/backward proxying if not already handled.
- System proxy configuration.
- True pproxy-shaped Python API drop-in replacement.
- Any advanced transport intentionally deferred by ADR.

## Handoff notes

The core risk is collapsing all advanced transport names into one generic “supported” label. Keep each transport independently modeled, tested, and documented. H2 stream semantics, WebSocket frame semantics, raw TCP streams, and QUIC streams are not interchangeable.
