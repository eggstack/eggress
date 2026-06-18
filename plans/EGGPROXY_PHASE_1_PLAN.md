# eggproxy Phase 1 Development Plan

## Phase title

**Core TCP proxy foundation**

## Phase objective

Build the minimum architecture that can support the entire eggproxy roadmap without later replacing the core connection model.

At phase completion, eggproxy must be a useful standalone TCP proxy supporting:

- a `pproxy`-like CLI;
- typed URI parsing;
- mixed HTTP, SOCKS4, and SOCKS5 listeners;
- direct TCP connections;
- HTTP and SOCKS upstreams;
- multi-hop HTTP/SOCKS chains;
- basic authentication;
- routing through the first matching live upstream;
- correct bidirectional relay semantics;
- interoperability with Python `pproxy` and standard clients.

Phase 1 deliberately excludes UDP, TLS, encryption protocols, SSH, QUIC, transparent proxying, reverse tunnels, and administrative features beyond basic logs and counters.

## Required final commands

The following command forms must work by the end of the phase:

```text
eggproxy
eggproxy -l http://:8080
eggproxy -l socks4://:1080
eggproxy -l socks5://:1080
eggproxy -l http+socks4+socks5://:8080
eggproxy -l http+socks5://user:pass@:8080
eggproxy -r http://proxy.example:8080
eggproxy -r socks5://proxy.example:1080
eggproxy -r socks5://user:pass@proxy.example:1080
eggproxy -r socks5://hop1:1080__http://hop2:8080
```

The exact credential placement must follow the compatibility grammar established in Phase 0. The examples above are behavioral targets rather than a substitute for the formal grammar.

## Phase principles

1. Do not special-case chain combinations.
2. Preserve domain names until local resolution is required.
3. Keep inbound parsing independent of outbound connection establishment.
4. Keep HTTP, SOCKS, routing, and relay logic in separate modules.
5. Prefer pure Rust dependencies.
6. Do not introduce TLS or UDP abstractions prematurely, but leave explicit extension points.
7. Avoid a single large `core` crate with every implementation inside it.
8. Every parser must be bounded and fragmentation-safe.
9. Do not check README boxes without interoperability tests.
10. Use a compatibility test harness from the first milestone.

## Proposed initial workspace

```text
eggproxy/
├── Cargo.toml
├── README.md
├── rust-toolchain.toml
├── deny.toml
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── security.yml
├── docs/
│   ├── ROADMAP.md
│   ├── PHASE_1_PLAN.md
│   ├── URI_GRAMMAR.md
│   └── ARCHITECTURE.md
├── crates/
│   ├── eggproxy-cli/
│   ├── eggproxy-core/
│   ├── eggproxy-uri/
│   ├── eggproxy-routing/
│   ├── eggproxy-protocol-http/
│   ├── eggproxy-protocol-socks/
│   └── eggproxy-testkit/
└── tests/
    ├── interoperability/
    └── fixtures/
```

The protocol crates may initially be modules if workspace overhead becomes counterproductive, but public boundaries should match this structure.

## Dependency set

Initial dependencies should be intentionally small.

### Required

```text
tokio
tokio-util
clap
thiserror
tracing
tracing-subscriber
bytes
http
httparse
regex
serde
toml
socket2
pin-project-lite
```

### Test and development

```text
proptest
assert_cmd
predicates
tempfile
tokio-test
insta
```

### Dependency notes

- Use `httparse` only for bounded HTTP/1 parsing; it is pure Rust.
- Do not add Hyper until ordinary HTTP forwarding requires it. CONNECT handling does not require a complete HTTP stack.
- Do not add OpenSSL.
- Do not add a C HTTP parser.
- Do not add a native SOCKS library.
- Implement the small SOCKS4 and SOCKS5 handshakes locally so server behavior, sniffing, and chain operation remain under project control.
- Use the operating-system resolver initially through Tokio.
- Avoid `async-trait` where associated futures or boxed futures remain manageable; use it only if it materially improves maintainability.
- Keep `unsafe_code = "forbid"` for all phase-one crates unless a narrowly reviewed exception becomes necessary.

## Core data model

The first milestone must define stable internal concepts.

### Target address

```rust
pub enum TargetHost {
    Ip(std::net::IpAddr),
    Domain(String),
}

pub struct TargetAddr {
    pub host: TargetHost,
    pub port: u16,
}
```

Domain validation should reject empty names and enforce a configurable maximum length. It should not force IDNA conversion during protocol parsing.

### Inbound identity

```rust
pub enum ClientIdentity {
    Anonymous,
    Username(String),
    Opaque(String),
}
```

Secrets must never be stored in this type.

### Session metadata

```rust
pub struct SessionContext {
    pub listener_id: ListenerId,
    pub peer_addr: Option<std::net::SocketAddr>,
    pub local_addr: Option<std::net::SocketAddr>,
    pub inbound_protocol: ProtocolId,
    pub identity: ClientIdentity,
    pub target: TargetAddr,
}
```

### Route result

```rust
pub enum RouteAction {
    Direct,
    Upstream(UpstreamId),
    Reject(RejectReason),
}
```

Phase 1 needs direct and upstream actions. Reject should exist immediately so unsupported or blocked behavior can fail cleanly.

### Boxed stream

Define one project-local trait alias pattern:

```rust
pub trait AsyncStream:
    tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin
{
}

impl<T> AsyncStream for T
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin
{
}

pub type BoxStream = Box<dyn AsyncStream>;
```

Use boxing at protocol and transport boundaries. Avoid propagating complex nested generic stream types through routing code.

## Core traits

### Inbound protocol handler

Conceptually:

```rust
pub trait InboundProtocol {
    fn id(&self) -> ProtocolId;
    fn detect(&self, prefix: &[u8]) -> DetectResult;
    async fn accept(
        &self,
        stream: &mut ReplayStream<BoxStream>,
        context: &AcceptContext,
    ) -> Result<AcceptedSession, ProtocolError>;
}
```

The exact async trait representation may vary, but the separation should remain.

`AcceptedSession` should contain:

- target;
- identity;
- success-response state;
- optional initial payload;
- protocol-specific responder.

Do not send a success response until the outbound route is connected unless the protocol requires earlier acknowledgement.

### Outbound connector

Conceptually:

```rust
pub trait Connector {
    async fn connect(
        &self,
        target: &TargetAddr,
        context: &ConnectContext,
    ) -> Result<BoxStream, ConnectError>;
}
```

A proxy hop connector must also be able to negotiate over an existing stream. This can be modeled as a separate trait:

```rust
pub trait StreamProxyHandshake {
    async fn connect_over(
        &self,
        stream: BoxStream,
        target: &TargetAddr,
        context: &ConnectContext,
    ) -> Result<BoxStream, ConnectError>;
}
```

### Listener

The listener subsystem should yield accepted byte streams and connection metadata without knowing the application proxy protocol.

### Router

Phase 1 router behavior:

1. Evaluate upstreams in configured order.
2. Choose the first live upstream whose rule matches.
3. Fall back to direct where compatibility requires it.
4. Return a route plan, not an already connected stream.

### Relay

The relay accepts two streams and returns directional byte counts and a normalized termination reason.

## Replay stream and protocol detection

A replayable stream is mandatory for mixed listeners.

### Requirements

- bounded sniff buffer;
- no direct mutation of Tokio internals;
- preserves every byte read during detection;
- supports additional reads when a detector reports “need more data”;
- deterministic protocol priority;
- configurable handshake timeout;
- distinguishes malformed matching traffic from non-matching traffic.

### Detection result

```rust
pub enum DetectResult {
    Match { confidence: u8 },
    NeedMore { minimum: usize },
    NoMatch,
}
```

Malformed input should be reported during parsing after selection, not converted into a match for another protocol.

### Initial detection order

For the default mixed listener:

1. SOCKS5;
2. SOCKS4;
3. HTTP.

The compatibility fixture suite must verify whether Python `pproxy` uses configured order or a fixed order and adjust the implementation accordingly.

### Buffer limits

Initial defaults:

- sniff buffer: 8 KiB;
- HTTP request head: 32 KiB;
- SOCKS username: 255 bytes;
- SOCKS password: 255 bytes;
- domain: 255 bytes;
- handshake timeout: 10 seconds.

These values should be constants with future configuration hooks.

## URI parser implementation

The URI parser is its own deliverable, not incidental CLI code.

### Phase-one grammar scope

Support:

- listener and upstream URI lists;
- protocol lists joined with `+`;
- proxy hops joined with `__`;
- host and port;
- bracketed IPv6;
- Unix socket representation if included in target grammar;
- credentials;
- local bind modifier if present in the frozen grammar;
- query rule reference;
- URI normalization;
- source-span-aware parse errors.

### Typed AST

```rust
pub struct ProxyChainSpec {
    pub hops: Vec<ProxyHopSpec>,
}

pub struct ProxyHopSpec {
    pub protocols: Vec<ProtocolSpec>,
    pub endpoint: EndpointSpec,
    pub credentials: Option<CredentialSpec>,
    pub rule: Option<RuleSpec>,
    pub local_bind: Option<BindSpec>,
}
```

### Parser requirements

- no secret values in `Debug`;
- round-trip tests where normalization is defined;
- exact errors for unknown schemes;
- reject impossible phase-one combinations during validation;
- preserve unsupported but syntactically valid schemes as explicit errors rather than generic malformed URIs;
- support multiple `-l` and `-r` occurrences if reference behavior allows them.

### CLI integration

`clap` should collect raw values. The eggproxy parser should produce the typed configuration and all validation errors before listeners are started.

## HTTP CONNECT implementation

### Server behavior

Implement:

- CONNECT request parsing;
- authority-form target parsing;
- IPv4, IPv6, and domain targets;
- optional Proxy-Authorization Basic authentication;
- `200 Connection Established`;
- appropriate 400, 403, 407, 502, and 504 outcomes;
- preservation of bytes arriving immediately after the HTTP header;
- header size and line count limits;
- case-insensitive header names;
- connection-close behavior.

### Client behavior

Implement:

- outbound TCP connection to proxy;
- CONNECT request;
- Host header;
- optional Proxy-Authorization;
- response parser;
- acceptance of valid 2xx responses;
- bounded response head;
- preservation of bytes after the response head;
- useful mapping of 407 and non-2xx statuses.

### Ordinary HTTP forwarding

Phase 1 should support the minimum useful non-CONNECT behavior:

- absolute-form request target;
- target extraction;
- removal of Proxy-Authorization before forwarding;
- conversion to origin-form;
- one request/response exchange per connection initially;
- request body forwarding for content-length and chunked framing;
- response forwarding;
- explicit documentation of keep-alive limitations.

If this creates excessive phase-one complexity, ordinary forwarding may be marked partial while CONNECT is completed. The README must reflect the distinction.

## SOCKS4 and SOCKS4a implementation

### Server

Implement:

- version 4;
- CONNECT command;
- user ID;
- IPv4 target;
- SOCKS4a domain target;
- success and rejection replies;
- bounded NUL-terminated fields;
- unsupported command rejection.

### Client

Implement:

- SOCKS4 CONNECT;
- SOCKS4a when the target is a domain;
- configurable user ID;
- exact reply validation;
- no premature local DNS for SOCKS4a.

### BIND

SOCKS4 BIND is outside phase 1 unless it is trivial after CONNECT. Keep protocol enums extensible.

## SOCKS5 implementation

### Server

Implement:

- method negotiation;
- no-auth method;
- username/password method;
- CONNECT;
- IPv4, IPv6, and domain targets;
- reply codes;
- target validation;
- authentication failure handling;
- command rejection for BIND and UDP ASSOCIATE during phase 1.

### Client

Implement:

- no-auth negotiation;
- username/password negotiation;
- CONNECT;
- IPv4, IPv6, and domain targets;
- remote DNS preservation;
- response address parsing;
- exact reply-code mapping.

### Authentication

Use a configured credential set. Compare byte strings without logging them. Authentication source-IP caching is not part of secure phase-one behavior.

## Multi-hop chain executor

This is the highest-priority architectural component.

### Route plan

A chain should compile into:

```text
Transport to hop 1
→ protocol handshake requesting hop 2
→ protocol handshake requesting hop 3
→ final protocol handshake requesting destination
```

### Requirements

- arbitrary HTTP CONNECT and SOCKS4/5 order;
- target of each handshake is the next hop, except the final handshake;
- domain names remain unresolved where supported;
- local DNS only where required;
- per-hop timeout;
- error identifies failing hop without exposing credentials;
- connection counters cover in-progress attempts;
- chain validation rejects impossible schemes before runtime;
- test chains of length one, two, and three.

### Required phase-one combinations

- HTTP → destination;
- SOCKS4a → destination;
- SOCKS5 → destination;
- HTTP → SOCKS5 → destination;
- SOCKS5 → HTTP → destination;
- SOCKS5 → SOCKS5 → destination;
- HTTP → HTTP → destination.

## Relay implementation

### Semantics

The relay must:

- copy both directions concurrently;
- preserve half-close;
- call shutdown on the opposite write half after EOF;
- allow the remaining direction to drain;
- terminate both tasks on fatal error or cancellation;
- expose byte counts;
- avoid unbounded buffering;
- support idle timeout as a future extension;
- avoid treating normal EOF as an error.

### Initial implementation

A project-local relay using `tokio::io::copy` on split halves is acceptable. Do not optimize with Linux `splice` in phase 1.

### Tests

- client half-closes request body and receives response;
- server half-closes and client drains remaining data;
- simultaneous close;
- abrupt reset;
- cancellation;
- large transfer;
- slow reader;
- slow writer.

## Listener and server orchestration

### Listener configuration

Each listener has:

- stable listener ID;
- endpoint;
- ordered inbound protocol set;
- authentication configuration;
- handshake timeout;
- connection limit;
- optional route group.

### Connection supervision

Use a `JoinSet` or equivalent supervisor with:

- cancellation token;
- per-connection task;
- semaphore for max concurrency;
- clean shutdown;
- listener failure propagation;
- structured connection span.

### Default command behavior

Running `eggproxy` with no arguments should start a loopback-safe or reference-compatible mixed HTTP/SOCKS listener. The exact bind address must follow the frozen compatibility decision. If reference behavior binds publicly, eggproxy may require an explicit compatibility profile to do so; the deviation must be documented.

## Routing and scheduling

Phase 1 implements only enough routing to exercise architecture.

### First-available scheduler

- evaluate upstreams in configuration order;
- skip disabled or known-dead upstreams;
- apply matching rule;
- select first candidate;
- otherwise direct fallback if configured.

### Rule scope

Implement regex rules only if they are required for reference-compatible `-r` behavior. Rich routing belongs to phase 2.

### Connection counters

Track:

- active inbound connections;
- active outbound connections per upstream;
- total successful sessions;
- failed handshakes;
- failed outbound connects;
- bytes each direction.

Use atomics and avoid a global mutex in the relay loop.

## Error model

Create typed errors by layer:

```text
CliError
ConfigError
UriError
AcceptError
ProtocolError
AuthError
RouteError
ConnectError
RelayError
ShutdownError
```

Errors should carry:

- stable category;
- safe operator message;
- source where useful;
- optional retryability;
- no credential values.

Protocol responses should be derived from typed errors rather than string matching.

## Logging

Use `tracing`.

### Required fields

- listener ID;
- connection ID;
- peer address;
- inbound protocol;
- target;
- selected route;
- upstream ID;
- chain hop index;
- duration;
- bytes upstream;
- bytes downstream;
- normalized outcome.

### Secret policy

Never log:

- passwords;
- complete authorization headers;
- private keys;
- proxy URIs containing credentials.

Provide a redacted display implementation for URI AST types.

## Testkit

The testkit should be created early rather than after protocol code.

### Components

- TCP echo server;
- half-close test server;
- HTTP origin server;
- malformed protocol peer;
- slow reader/writer;
- temporary port allocator;
- child-process supervisor;
- Python `pproxy` launcher;
- curl launcher where available;
- deterministic timeout helpers;
- packet fragmentation wrapper.

### Fragmentation harness

Every handshake test should be runnable with:

- one-byte writes;
- randomized fragments;
- combined handshake and payload;
- delayed fragments.

## Differential interoperability suite

### Python reference direction

For each protocol:

1. eggproxy client → Python `pproxy` server;
2. Python `pproxy` client → eggproxy server.

### External clients

At minimum:

- curl through HTTP CONNECT;
- curl through SOCKS4a;
- curl through SOCKS5h;
- a Rust or Python socket client for authentication variants.

### Assertions

- expected target reached;
- payload integrity;
- remote DNS behavior;
- correct authentication rejection;
- correct HTTP/SOCKS response code;
- no task leak after shutdown.

## CI requirements

### Platforms

- Ubuntu stable;
- macOS stable;
- Windows stable where phase-one code is intended to compile.

### Toolchains

- stable Rust;
- minimum supported Rust version once chosen;
- nightly only for optional sanitizers or fuzzing.

### Checks

```text
cargo fmt --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

`cargo audit` and `cargo deny` may run in a dedicated security workflow.

### Optional early quality checks

- `cargo machete`;
- documentation link checks;
- code coverage;
- Miri for parser-only modules;
- sanitizer jobs for testkit and protocol parsers.

## Milestone sequence

### Milestone 1.1: Repository and compatibility skeleton

Deliver:

- workspace;
- CI;
- root README;
- roadmap and phase plan;
- CLI shell;
- typed error crate/module;
- logging initialization;
- compatibility fixture directory;
- Python test harness skeleton.

Acceptance:

- `eggproxy --help` works;
- all workspace checks pass;
- README checklist exists;
- no network behavior yet.

### Milestone 1.2: URI grammar and validation

Deliver:

- URI AST;
- parser;
- redacted display;
- chain parsing;
- protocol-list parsing;
- IPv6 handling;
- credential parsing;
- fixture corpus;
- explain/validate internal command hooks.

Acceptance:

- parser handles all phase-one examples;
- invalid input produces source-aware diagnostics;
- secrets do not appear in snapshots;
- property tests cover parse stability.

### Milestone 1.3: Core stream and relay

Deliver:

- `BoxStream`;
- target types;
- session context;
- cancellation;
- listener abstraction;
- direct TCP connector;
- half-close-aware relay;
- byte counters;
- echo integration tests.

Acceptance:

- direct fixed-target proxy test passes;
- transfer and shutdown tests pass;
- no protocol handling yet.

### Milestone 1.4: Replay stream and protocol dispatch

Deliver:

- replay buffer;
- detector interface;
- ordered dispatch;
- sniff and handshake limits;
- malformed versus no-match distinction;
- mixed-listener test harness.

Acceptance:

- fragmented detector tests pass;
- bytes are not lost or duplicated;
- unknown input closes deterministically.

### Milestone 1.5: SOCKS4/SOCKS4a

Deliver:

- inbound server;
- outbound client;
- CONNECT;
- user ID;
- domain preservation;
- response mapping;
- mixed-dispatch registration.

Acceptance:

- direct SOCKS4 and SOCKS4a tests pass;
- curl SOCKS4a test passes;
- Python cross-tests pass.

### Milestone 1.6: SOCKS5 CONNECT

Deliver:

- method negotiation;
- username/password auth;
- inbound server;
- outbound client;
- target encoding;
- response mapping;
- unsupported command handling.

Acceptance:

- direct SOCKS5 and SOCKS5h tests pass;
- auth tests pass;
- fragmented handshakes pass;
- Python cross-tests pass.

### Milestone 1.7: HTTP CONNECT

Deliver:

- inbound parser;
- outbound client;
- Basic proxy auth;
- response mapping;
- immediate post-header payload handling;
- mixed-dispatch registration.

Acceptance:

- curl HTTPS through proxy passes;
- authenticated and unauthenticated tests pass;
- Python cross-tests pass.

### Milestone 1.8: Basic ordinary HTTP forwarding

Deliver:

- absolute target parsing;
- origin-form rewrite;
- hop-by-hop header filtering;
- request and response streaming;
- documented keep-alive scope.

Acceptance:

- curl plain HTTP through proxy passes;
- request body test passes;
- Proxy-Authorization is not leaked upstream.

### Milestone 1.9: Chain executor

Deliver:

- typed route plan;
- stream-over-stream handshakes;
- hop-indexed errors;
- one-, two-, and three-hop tests;
- direct fallback.

Acceptance:

- all required HTTP/SOCKS chain permutations pass;
- domain names are preserved through remote-DNS-capable hops;
- failures identify the correct hop.

### Milestone 1.10: CLI integration and phase closure

Deliver:

- `-l`;
- `-r`;
- multiple listeners/upstreams if in scope;
- compatibility defaults;
- shutdown handling;
- basic statistics;
- polished logs;
- README updates;
- phase-one interoperability report.

Acceptance:

- required final commands work;
- all phase-one README boxes are checked;
- full CI passes on supported platforms;
- no known high-severity defects;
- phase-one architecture review completed.

## README checklist update requirements

Every milestone must update the root README.

Example:

```markdown
### HTTP

- [x] HTTP CONNECT server
- [x] HTTP CONNECT client
- [ ] Ordinary HTTP forwarding — partial: one request per connection
- [x] Basic proxy authentication
- [ ] Persistent forward-proxy connections
```

Do not check off `HTTP proxy` as a single umbrella capability while ordinary forwarding remains partial.

## Review gates

### Architecture gate after Milestone 1.3

Confirm that:

- streams can later be wrapped by TLS;
- connectors can operate over prior-hop streams;
- target names are not prematurely resolved;
- UDP can be introduced without changing TCP session types;
- protocol implementations do not depend on CLI types.

### Protocol gate after Milestone 1.7

Review:

- parser bounds;
- authentication handling;
- response-code accuracy;
- fragmented reads;
- immediate payload preservation;
- error-to-wire mapping.

### Phase closure gate

Review:

- chain semantics;
- cancellation;
- half-close correctness;
- resource cleanup;
- logging redaction;
- CI portability;
- differential results.

## Security requirements

Phase 1 must include:

- listener connection semaphore;
- handshake timeout;
- bounded protocol headers;
- bounded credentials;
- bounded replay buffer;
- no credentials in logs;
- rejection of NUL and invalid target forms where applicable;
- no unsafe Rust in core crates;
- explicit public-bind warning;
- no open proxy surprise in secure-default mode;
- graceful task cancellation;
- dependency audit.

## Performance requirements

Phase 1 is correctness-first, but the following budgets should be monitored:

- no per-chunk heap allocation in steady-state relay where avoidable;
- no central mutex per transferred chunk;
- one task pair or equivalent per active TCP tunnel;
- bounded per-connection handshake memory;
- connection setup overhead measured separately from relay throughput;
- no extra payload copies in replay after the initial sniff buffer is consumed.

Benchmarks should include:

- direct relay throughput;
- HTTP CONNECT setup latency;
- SOCKS5 setup latency;
- two-hop setup latency;
- memory per 1,000 idle tunnels;
- CPU during large bidirectional transfer.

These are tracking metrics, not hard release thresholds in phase 1.

## Phase-one definition of done

Phase 1 is complete when eggproxy is a reliable TCP proxy supporting mixed HTTP/SOCKS listeners and direct or chained HTTP/SOCKS upstreams, with nearly identical common CLI usage to Python `pproxy`, explicit capability tracking in the README, cross-implementation interoperability tests, bounded hostile-input handling, and an architecture that can accept later UDP, TLS, encrypted protocols, multiplexed transports, reverse tunnels, and transparent proxying without replacement of the core model.
