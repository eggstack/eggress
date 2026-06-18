# Phase 1 Corrective Closure Plan

## Purpose

This plan closes the remaining correctness, architecture, interoperability, and documentation gaps in eggress Phase 1 before Phase 2 begins.

The current repository already contains the intended Phase 1 foundation:

- typed proxy-chain URI parsing;
- mixed HTTP, SOCKS4, and SOCKS5 listeners;
- direct TCP connection support;
- HTTP CONNECT client/server support;
- SOCKS4/SOCKS4a client/server support;
- SOCKS5 CONNECT client/server support;
- HTTP/SOCKS multi-hop chain execution;
- bounded parsing and replay buffering;
- half-close-aware relaying;
- cross-platform CI configuration;
- internal integration tests.

The corrective pass must preserve those working components. It must not introduce UDP, TLS, health checking, advanced routing, persistent ordinary HTTP connections, or other Phase 2+ features.

## Primary defects to close

1. Ordinary HTTP forwarding opens a `DirectConnector` internally and bypasses the configured upstream chain.
2. Ordinary HTTP forwarding uses an empty stream as a sentinel to fit a tunnel-shaped return type.
3. Chunked request bodies are copied until TCP EOF rather than until the terminating chunk and trailers.
4. Requests without `Content-Length` or `Transfer-Encoding: chunked` may incorrectly be read until EOF.
5. HTTP CONNECT, SOCKS4, and SOCKS5 success replies are currently emitted before the outbound route is established.
6. Protocol orchestration is concentrated in the CLI binary instead of reusable library modules.
7. The tracing span is entered across async suspension with an RAII guard.
8. Existing “interoperability” tests mainly exercise eggress against itself.
9. README completion claims and checkboxes do not consistently follow the repository’s stated completion policy.
10. Runtime errors are not consistently translated into protocol-correct failure replies.

## Scope

### Included

- typed accepted-session state;
- deferred success and failure replies;
- common outbound route execution for tunnel and ordinary HTTP modes;
- correct bounded HTTP request-body framing;
- removal of the empty-stream sentinel;
- movement of protocol orchestration out of the CLI;
- structured connection outcomes and route-aware logging;
- external interoperability tests with Python `pproxy` and `curl`;
- README and Phase 1 status reconciliation;
- regression tests for all corrected behaviors.

### Excluded

- persistent ordinary HTTP client connections;
- request pipelining;
- HTTP response caching;
- TLS;
- UDP;
- SOCKS BIND;
- Shadowsocks;
- SSH;
- HTTP/2 or HTTP/3;
- QUIC;
- route policy beyond the existing chain/direct choice;
- health checks;
- system proxy management.

## Required implementation order

Execute the work in the order below. Do not begin external interoperability work until the session-state and reply-ordering changes pass internal tests.

---

# Workstream 1: Introduce an explicit accepted-session model

## Problem

`handle_connection` currently expects every inbound protocol to produce:

```rust
(TargetAddr, BoxStream)
```

That shape assumes every accepted request becomes a raw tunnel. Ordinary HTTP forwarding does not fit this model, so it completes the request internally and returns `tokio::io::empty()` as a sentinel.

This makes state transitions implicit and allows protocol code to connect directly instead of passing through routing.

## Target design

Add explicit orchestration types. The preferred placement is a new `eggress-server` crate because `eggress-core` should not depend on concrete HTTP protocol types.

Recommended structure:

```text
crates/eggress-server/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── accept.rs
    ├── execute.rs
    ├── reply.rs
    └── error.rs
```

Dependency direction:

```text
eggress-cli
    -> eggress-server
        -> eggress-core
        -> eggress-uri
        -> eggress-protocol-http
        -> eggress-protocol-socks
```

The CLI must not be imported by any library crate.

Suggested accepted-session types:

```rust
use eggress_core::{BoxStream, TargetAddr};
use eggress_protocol_http::ForwardRequest;

pub enum AcceptedSession {
    Tunnel(PendingTunnel),
    HttpForward(PendingHttpForward),
}

pub struct PendingTunnel {
    pub target: TargetAddr,
    pub client: BoxStream,
    pub protocol: TunnelProtocol,
    pub reply_context: ReplyContext,
}

pub struct PendingHttpForward {
    pub target: TargetAddr,
    pub client: BoxStream,
    pub request: ForwardRequest,
    pub body: RequestBodyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestBodyKind {
    None,
    ContentLength(u64),
    Chunked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelProtocol {
    HttpConnect,
    Socks4,
    Socks5,
}
```

For Phase 1, prefer concrete enums over a dynamic async reply trait. They are easier to inspect, avoid `async-trait`, and give exhaustive compiler checking.

Suggested reply context:

```rust
pub enum ReplyContext {
    Http,
    Socks4 {
        requested: eggress_core::TargetAddr,
    },
    Socks5 {
        requested: eggress_protocol_socks::socks5::server::SocksAddr,
    },
}
```

## Important ownership rule

Do not split a stream and then move reader and writer halves into unrelated owner objects unless they are immediately rejoined.

The simplest Phase 1 pattern is:

1. parse using `&mut BoxStream`;
2. retain the original `BoxStream`;
3. store protocol reply metadata, not a writer half;
4. after route connection, write success through the same stream;
5. pass the complete stream to relay.

Example:

```rust
pub async fn send_tunnel_success(
    pending: &mut PendingTunnel,
    bound: Option<std::net::SocketAddr>,
) -> Result<(), ProtocolError> {
    match (&pending.protocol, &pending.reply_context) {
        (TunnelProtocol::HttpConnect, ReplyContext::Http) => {
            pending.client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;
        }
        (TunnelProtocol::Socks4, ReplyContext::Socks4 { .. }) => {
            let addr = bound.unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
            eggress_protocol_socks::socks4::server::write_socks4_reply(
                &mut pending.client,
                eggress_protocol_socks::socks4::server::Socks4Status::Granted,
                addr,
            )
            .await?;
        }
        (TunnelProtocol::Socks5, ReplyContext::Socks5 { .. }) => {
            let addr = socks_addr_from_bound(bound);
            eggress_protocol_socks::socks5::server::send_connect_reply(
                &mut pending.client,
                0x00,
                &addr,
            )
            .await?;
        }
        _ => return Err(ProtocolError::InvalidReplyState),
    }
    Ok(())
}
```

## Acceptance criteria

- `handle_http_request` no longer returns `(TargetAddr, BoxStream)`.
- No handler returns an empty stream sentinel.
- Inbound protocol parsing does not open outbound sockets.
- Session variants clearly distinguish tunnel and ordinary HTTP forwarding.
- CLI does not contain protocol-specific parsing or body forwarding.
- Tests can assert that no success bytes are written before route establishment.

---

# Workstream 2: Move inbound orchestration out of the CLI

## Problem

`crates/eggress-cli/src/main.rs` currently contains:

- first-byte protocol detection;
- a prefixed stream implementation;
- HTTP request classification;
- ordinary HTTP body forwarding;
- SOCKS4 request parsing and replies;
- SOCKS5 negotiation and replies;
- direct/chain route opening;
- relay invocation.

This prevents an embeddable server API and makes the CLI responsible for wire protocol correctness.

## Target modules

### `accept.rs`

- run configured protocol detection;
- parse the inbound request;
- authenticate;
- return `AcceptedSession`;
- never open outbound connections;
- never send success replies.

### `execute.rs`

- obtain a route stream through direct connector or chain executor;
- send protocol success only after route success;
- send protocol failure on route failure;
- relay tunnels;
- run ordinary HTTP exchange over the selected route stream;
- produce a typed `SessionReport`.

### `reply.rs`

- protocol-specific success/failure mapping;
- HTTP status mapping;
- SOCKS4 status mapping;
- SOCKS5 reply-code mapping.

### `error.rs`

- normalize route and connect failures;
- identify timeout, DNS failure, refused, unreachable, authentication failure, and generic failure.

### `lib.rs`

Export a reusable entry point:

```rust
pub async fn serve_connection(
    client: BoxStream,
    config: ConnectionConfig,
) -> SessionReport;
```

Possible configuration:

```rust
pub struct ConnectionConfig {
    pub protocols: Vec<ProtocolId>,
    pub route: RouteConfig,
    pub handshake_timeout: Duration,
}
```

The CLI should be reduced to listener creation, task supervision, logging initialization, and calling `serve_connection`.

## Reuse the existing replay implementation

Do not keep a second `PrefixedStream` in the CLI if `egress-core` already has a replay stream.

Replace manual one-byte detection:

```rust
let mut first_byte = [0u8; 1];
stream.read_exact(&mut first_byte).await?;
```

with the existing bounded detection/dispatch abstraction.

The configured listener protocol order should control detector order. Do not hard-code all non-`0x04`/`0x05` traffic as HTTP without bounded HTTP detection.

## Acceptance criteria

- CLI `main.rs` primarily parses arguments, starts listeners, supervises tasks, and initializes logging.
- Protocol-specific parsing is absent from the CLI.
- Only one replay/prefix abstraction remains.
- Mixed listener behavior continues to pass all existing tests.
- A library integration test can call the server API without spawning the CLI binary.

---

# Workstream 3: Correct tunnel success and failure ordering

## Problem

Current SOCKS4 and SOCKS5 handlers send success immediately after parsing. HTTP CONNECT also completes its success handshake before the route is opened.

The correct sequence is:

```text
parse request
-> authenticate
-> select/open route
-> send success
-> relay
```

On route failure:

```text
parse request
-> authenticate
-> route open fails
-> send protocol-specific failure
-> close
```

## Implement a normalized open error

Suggested type:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SessionOpenError {
    #[error("connection timed out")]
    Timeout,

    #[error("connection refused")]
    Refused,

    #[error("network unreachable")]
    NetworkUnreachable,

    #[error("host unreachable")]
    HostUnreachable,

    #[error("DNS resolution failed")]
    Dns,

    #[error("upstream authentication failed")]
    UpstreamAuthentication,

    #[error("request rejected by policy")]
    PolicyDenied,

    #[error("route failed at hop {hop}")]
    Hop {
        hop: usize,
        #[source]
        source: Box<SessionOpenError>,
    },

    #[error("other connection error: {0}")]
    Other(String),
}
```

Add conversion helpers from `std::io::Error`, HTTP CONNECT errors, SOCKS client errors, and chain errors. Do not use error-string matching in protocol reply mapping.

## Reply mapping

### HTTP CONNECT

| Error | HTTP response |
|---|---|
| Timeout | `504 Gateway Timeout` |
| Policy denied | `403 Forbidden` |
| Refused/unreachable/DNS/upstream failure | `502 Bad Gateway` |

Example:

```rust
pub fn http_failure_status(error: &SessionOpenError) -> &'static [u8] {
    match error {
        SessionOpenError::Timeout => {
            b"HTTP/1.1 504 Gateway Timeout\r\nConnection: close\r\n\r\n"
        }
        SessionOpenError::PolicyDenied => {
            b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n"
        }
        _ => {
            b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n"
        }
    }
}
```

### SOCKS4

Use:

- `0x5a` only after outbound route success;
- `0x5b` for general rejection or route failure.

### SOCKS5

| Error | REP |
|---|---:|
| Generic | `0x01` |
| Policy denied | `0x02` |
| Network unreachable | `0x03` |
| Host unreachable or DNS | `0x04` |
| Refused | `0x05` |
| Timeout | `0x06` |
| Unsupported command | `0x07` |
| Unsupported address type | `0x08` |

## Test-first controlled connector

Add a connector that blocks on synchronization primitives:

```rust
struct ControlledConnector {
    started: Arc<Notify>,
    release: Arc<Notify>,
    result: Arc<Mutex<Option<Result<BoxStream, SessionOpenError>>>>,
}
```

Test sequence:

1. client sends a valid CONNECT/SOCKS request;
2. connector signals that route opening started;
3. assert no success reply is readable within a short timeout;
4. release connector with success;
5. assert success reply arrives;
6. repeat with failure and assert a protocol failure reply.

Example skeleton:

```rust
#[tokio::test]
async fn socks5_success_is_deferred_until_route_opens() {
    let (mut client, server) = tokio::io::duplex(4096);
    let controlled = ControlledConnector::pending_success();

    let task = tokio::spawn(serve_with_connector(
        Box::new(server),
        controlled.clone(),
    ));

    send_socks5_request(&mut client, "example.com", 443).await;
    controlled.wait_until_started().await;

    let early = tokio::time::timeout(
        Duration::from_millis(50),
        read_socks5_reply(&mut client),
    )
    .await;

    assert!(early.is_err(), "success reply was sent before route opened");

    controlled.succeed();
    let reply = read_socks5_reply(&mut client).await.unwrap();
    assert_eq!(reply.rep, 0x00);

    task.await.unwrap();
}
```

Do not rely on long sleeps. Use notifications or barriers, with only a short timeout to prove absence of an early reply.

## Acceptance criteria

- HTTP CONNECT success follows outbound connection success.
- SOCKS4 granted reply follows outbound connection success.
- SOCKS5 `REP=0` follows outbound connection success.
- Each protocol sends a useful failure response when route opening fails.
- Unit tests prove no early success is emitted.
- Chain hop failure is reflected as a final inbound protocol failure.

---

# Workstream 4: Route ordinary HTTP through the common chain executor

## Problem

The ordinary HTTP path directly creates `DirectConnector`, which ignores `-r` and any configured proxy chain.

## Target execution flow

```text
accept HTTP forward request
-> parse absolute-form target
-> choose direct route or configured chain
-> open route stream to target
-> write rewritten origin-form request
-> copy exactly one request body
-> read and forward exactly one response
-> close
```

Use the same route-opening function as tunnel protocols:

```rust
pub async fn open_route(
    route: &RouteConfig,
    target: &TargetAddr,
) -> Result<BoxStream, SessionOpenError> {
    match route {
        RouteConfig::Direct => DirectConnector
            .connect(target)
            .await
            .map_err(Into::into),
        RouteConfig::Chain(spec) => build_chain_executor()
            .execute(&spec.hops, target)
            .await
            .map_err(Into::into),
    }
}
```

Then both execution modes call it:

```rust
match accepted {
    AcceptedSession::Tunnel(mut pending) => {
        match open_route(&config.route, &pending.target).await {
            Ok(upstream) => {
                send_tunnel_success(&mut pending, upstream_local_addr).await?;
                relay(pending.client, upstream).await
            }
            Err(error) => {
                send_tunnel_failure(&mut pending, &error).await?;
                return SessionReport::open_failed(error);
            }
        }
    }

    AcceptedSession::HttpForward(mut pending) => {
        match open_route(&config.route, &pending.target).await {
            Ok(upstream) => execute_http_forward(pending, upstream).await,
            Err(error) => {
                send_http_forward_failure(&mut pending.client, &error).await?;
                return SessionReport::open_failed(error);
            }
        }
    }
}
```

## Required regression test

Build a fake or real upstream SOCKS5 proxy that records requested destinations.

Run eggress with:

```text
-r socks5://127.0.0.1:<port>
```

Send:

```http
GET http://example.test/resource HTTP/1.1
Host: example.test
Connection: close

```

Assert:

- the SOCKS5 upstream receives target `example.test:80`;
- the origin receives the rewritten request through the SOCKS tunnel;
- the client receives the origin response;
- no direct connection to the origin was attempted outside the chain.

A strong way to prove absence of direct routing is to use a hostname resolvable only by the fake upstream.

## Acceptance criteria

- ordinary HTTP direct mode works;
- ordinary HTTP through one HTTP upstream works;
- ordinary HTTP through one SOCKS5 upstream works;
- ordinary HTTP through a two-hop HTTP/SOCKS chain works;
- route errors produce HTTP 502/504 rather than silent closure;
- no direct connector is instantiated inside the HTTP protocol crate or HTTP accept path.

---

# Workstream 5: Implement correct request-body framing

## Problem

The current body copier has two invalid behaviors:

1. chunked bodies are copied until socket EOF;
2. an unframed body may be copied until socket EOF.

For Phase 1, exactly one request per connection is sufficient, but the request body must terminate according to HTTP framing.

## Define explicit body kind

During request-head parsing:

```rust
pub enum RequestBodyKind {
    None,
    ContentLength(u64),
    Chunked,
}
```

Determine it with these rules:

1. Reject conflicting or invalid `Content-Length` values.
2. If valid `Transfer-Encoding` ends in `chunked`, use `Chunked`.
3. If both `Transfer-Encoding` and `Content-Length` are present, reject the request for Phase 1 closure.
4. If neither is present, use `None`.
5. Do not infer a request body from method name alone.

## Content-Length copier

```rust
pub async fn copy_exact_body<R, W>(
    reader: &mut R,
    writer: &mut W,
    mut remaining: u64,
) -> Result<u64, HttpError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut copied = 0;
    let mut buf = [0u8; 8192];

    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = reader.read(&mut buf[..want]).await?;
        if n == 0 {
            return Err(HttpError::UnexpectedEofInBody {
                remaining,
            });
        }

        writer.write_all(&buf[..n]).await?;
        remaining -= n as u64;
        copied += n as u64;
    }

    Ok(copied)
}
```

Do not silently accept EOF before the declared body length.

## Chunked copier

Implement a bounded line-and-exact-data state machine that forwards framing unchanged.

```rust
pub async fn copy_chunked_body<R, W>(
    reader: &mut R,
    writer: &mut W,
    limits: ChunkedLimits,
) -> Result<u64, HttpError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut total = 0u64;
    let mut trailer_bytes = 0usize;

    loop {
        let size_line = read_crlf_line(reader, limits.max_size_line).await?;
        writer.write_all(&size_line).await?;

        let without_crlf = &size_line[..size_line.len() - 2];
        let hex = without_crlf
            .split(|b| *b == b';')
            .next()
            .ok_or(HttpError::InvalidChunkSize)?;

        let size = parse_hex_u64(hex)?;
        if size > limits.max_chunk_size {
            return Err(HttpError::ChunkTooLarge(size));
        }

        if size == 0 {
            loop {
                let trailer = read_crlf_line(reader, limits.max_trailer_line).await?;
                trailer_bytes = trailer_bytes
                    .checked_add(trailer.len())
                    .ok_or(HttpError::BodyTooLarge)?;

                if trailer_bytes > limits.max_trailer_bytes {
                    return Err(HttpError::TrailersTooLarge);
                }

                writer.write_all(&trailer).await?;
                if trailer == b"\r\n" {
                    writer.flush().await?;
                    return Ok(total);
                }
            }
        }

        copy_exactly(reader, writer, size).await?;

        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).await?;
        if crlf != *b"\r\n" {
            return Err(HttpError::InvalidChunkTerminator);
        }
        writer.write_all(&crlf).await?;

        total = total
            .checked_add(size)
            .ok_or(HttpError::BodyTooLarge)?;
        if total > limits.max_decoded_body {
            return Err(HttpError::BodyTooLarge);
        }
    }
}
```

### Required limits

```rust
pub struct ChunkedLimits {
    pub max_size_line: usize,      // e.g. 1024
    pub max_trailer_line: usize,   // e.g. 8192
    pub max_trailer_bytes: usize,  // e.g. 32 KiB
    pub max_chunk_size: u64,
    pub max_decoded_body: u64,
}
```

Do not accumulate the entire request body in memory.

## Request-smuggling safeguards

Add tests for:

- duplicate equal `Content-Length`;
- duplicate conflicting `Content-Length`;
- `Transfer-Encoding: chunked` plus `Content-Length`;
- malformed hexadecimal chunk size;
- chunk extension;
- missing chunk CRLF;
- zero chunk with no trailers;
- zero chunk with trailers;
- oversized chunk-size line;
- premature EOF;
- bytes immediately following the terminal trailer block.

Because Phase 1 processes one request per connection, bytes after the terminal chunk may be rejected or ignored only after the response is completed and the connection is closed. Document the one-request policy.

## Response forwarding

Copying the response until upstream EOF is acceptable only if eggress forces `Connection: close` upstream and supports one exchange per connection.

During origin-form rewrite:

- remove incoming `Proxy-Connection`;
- remove connection-nominated hop-by-hop fields;
- emit `Connection: close`;
- ensure the upstream request does not rely on persistence.

Test that the origin receives `Connection: close`.

## Acceptance criteria

- no request body path reads until client EOF;
- chunked request terminates at zero chunk plus trailers;
- content-length short read returns an error;
- ambiguous framing is rejected;
- body data is streamed with bounded memory;
- ordinary HTTP POST works without client half-closing;
- a client can send the request and wait for the response without deadlock.

---

# Workstream 6: Correct HTTP header filtering

## Required filtering behavior

When forwarding an ordinary HTTP request, remove:

- `Proxy-Authorization`;
- `Proxy-Authenticate`;
- `Proxy-Connection`;
- `Connection`;
- `Keep-Alive`;
- `Upgrade`;
- every header named by tokens in the incoming `Connection` header.

Do not delete `Transfer-Encoding: chunked` when the chunked body is forwarded unchanged.

Recommended algorithm:

```rust
fn connection_tokens(headers: &[Header]) -> HashSet<HeaderName> {
    headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("connection"))
        .flat_map(|h| h.value.split(','))
        .filter_map(|token| HeaderName::from_bytes(token.trim().as_bytes()).ok())
        .collect()
}

fn should_drop_request_header(
    name: &HeaderName,
    nominated: &HashSet<HeaderName>,
) -> bool {
    nominated.contains(name)
        || matches!(
            name.as_str(),
            "connection"
                | "proxy-connection"
                | "proxy-authorization"
                | "proxy-authenticate"
                | "keep-alive"
                | "upgrade"
        )
}
```

Handle `TE` conservatively. Since eggress does not transform trailers, remove unsupported `TE` values while retaining `Transfer-Encoding: chunked` when required for body framing.

## Required tests

- `Proxy-Authorization` never reaches origin;
- a custom header named in `Connection` is removed;
- `Proxy-Connection` is removed;
- `Transfer-Encoding: chunked` remains when the body remains chunked;
- `Connection: close` is emitted upstream;
- `Host` is preserved or regenerated correctly;
- absolute-form URI becomes origin-form path and query.

## README rule

Do not check “Hop-by-hop header filtering” until these cases pass.

---

# Workstream 7: Improve structured logging and async span handling

## Replace span guard across await

Current pattern:

```rust
let _guard = span.enter();
handle_connection(...).await;
```

Replace with `tracing::Instrument`:

```rust
use tracing::Instrument;

let span = tracing::info_span!(
    "connection",
    connection_id = conn_id,
    peer = %peer,
    listener = %listener,
);

async move {
    let started = Instant::now();
    let report = serve_connection(conn.stream, config).await;

    tracing::info!(
        protocol = ?report.protocol,
        target = ?report.target,
        route = ?report.route,
        outcome = ?report.outcome,
        bytes_upstream = report.bytes_upstream,
        bytes_downstream = report.bytes_downstream,
        duration_ms = started.elapsed().as_millis() as u64,
        "connection completed",
    );
}
.instrument(span)
.await;
```

## Session report

```rust
pub struct SessionReport {
    pub protocol: Option<ProtocolId>,
    pub target: Option<TargetAddr>,
    pub route: RouteSummary,
    pub outcome: SessionOutcome,
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
}

pub enum SessionOutcome {
    Completed,
    ClientProtocolError,
    AuthenticationFailed,
    RouteFailed,
    RelayFailed,
    Cancelled,
}
```

## Secret-redaction checks

Add a formatted log-capture test that verifies:

- URI password does not appear;
- HTTP `Proxy-Authorization` value does not appear;
- SOCKS password does not appear;
- safe user identity appears only if intentionally supported.

## Acceptance criteria

- no span guard is held across `.await`;
- completion logs include protocol, target, route, outcome, byte counts, and duration;
- credentials are absent from logs;
- README logging and secret-redaction boxes reflect actual coverage.

---

# Workstream 8: Add genuine external interoperability tests

## Problem

Existing tests are valuable internal end-to-end tests but do not prove compatibility with independent implementations.

## Test categories

Separate tests by name and directory:

```text
tests/
├── integration/
│   ├── http.rs
│   ├── socks4.rs
│   ├── socks5.rs
│   └── chains.rs
└── interoperability/
    ├── curl.rs
    ├── pproxy.rs
    └── README.md
```

Internal eggress-client to eggress-server tests belong under `integration`.

## Python `pproxy` harness

The harness should:

1. detect a usable Python interpreter;
2. use a controlled environment in CI;
3. install a pinned `pproxy` version;
4. start it on a dynamically allocated port;
5. wait for readiness by polling with a deadline;
6. terminate it after the test;
7. capture stdout/stderr for failures.

Do not install an unpinned latest version during every test invocation.

Recommended CI setup:

```yaml
- uses: actions/setup-python@v5
  with:
    python-version: "3.12"

- run: python -m pip install "pproxy==<PINNED_VERSION>"

- run: cargo test --test interoperability_pproxy
```

Record the exact target version in `tests/interoperability/README.md`.

## Required Python cross-tests

### eggress server, independent client

- independent client through eggress HTTP CONNECT;
- independent client through eggress SOCKS4a;
- independent client through eggress SOCKS5;
- authenticated SOCKS5 where the reference supports it.

### eggress client, Python server

- eggress HTTP CONNECT client through Python `pproxy` HTTP server;
- eggress SOCKS4a client through Python `pproxy` SOCKS4 server;
- eggress SOCKS5 client through Python `pproxy` SOCKS5 server;
- one two-hop chain containing Python `pproxy`.

If Python `pproxy` exposes no convenient direct client mode for a case, use curl or a small independent client through the Python proxy. The essential condition is that one side is not eggress.

## curl tests

Run when curl is available:

```text
curl --proxy http://127.0.0.1:<port> http://origin/
curl --proxy http://127.0.0.1:<port> https://tls-origin/
curl --socks4a 127.0.0.1:<port> http://origin/
curl --socks5-hostname 127.0.0.1:<port> http://origin/
```

For HTTPS CONNECT testing, use a local TLS origin with a test certificate and `--insecure`, not a public internet target.

Tests must not depend on external internet availability.

## CI strategy

External interoperability may initially run on Ubuntu only. Internal tests continue across Ubuntu, macOS, and Windows.

Make skip behavior explicit:

- local developer machines may skip if Python or curl is absent;
- CI must install dependencies and must not skip;
- skipped local interoperability tests must print a clear reason.

## Acceptance criteria

- internal tests are no longer mislabeled as interoperability;
- Python `pproxy` is pinned;
- HTTP, SOCKS4a, and SOCKS5 are tested in at least one cross-implementation direction each;
- both eggress server and eggress client roles receive external coverage;
- curl covers ordinary HTTP and CONNECT;
- no test requires public internet access.

---

# Workstream 9: Reconcile README and Phase 1 status

## Required status changes during implementation

At the start of this corrective pass, change the top status from:

```text
Phase 1 complete
```

to:

```text
Phase 1 corrective closure in progress
```

Do not wait until the end if implementation is split across multiple commits.

## Checkbox policy

Follow the existing README policy literally:

- `[x]` only when implementation, tests, documentation, and applicable interoperability evidence are complete;
- partial features remain `[ ]` and include a note.

Correct the current contradictory form:

```text
- [x] Ordinary HTTP forward-proxy server — partial: one request per connection
```

Preferred wording:

```text
- [x] Single-exchange ordinary HTTP forward-proxy server
- [ ] Persistent HTTP forwarding
```

This distinguishes a deliberately complete limited mode from an incomplete umbrella item.

Recommended Phase 1 HTTP checklist after closure:

```markdown
### HTTP/1

- [x] HTTP CONNECT server
- [x] HTTP CONNECT client
- [x] Single-exchange ordinary HTTP forward-proxy server
- [x] Absolute-form to origin-form rewriting
- [x] HTTP proxy Basic authentication
- [ ] Persistent HTTP forwarding
- [x] Hop-by-hop request-header filtering
- [x] HTTP upstream chaining
- [x] Content-Length request bodies
- [x] Chunked request bodies
- [x] Deferred CONNECT success reply
```

Recommended operations updates after verified implementation:

```markdown
- [x] Human-readable structured logs
- [ ] JSON logs
- [x] Secret redaction for URIs, authentication, and runtime logs
- [x] Traffic counters for TCP relay sessions
```

Recommended security update after confirming the workflow passes:

```markdown
- [x] Dependency audit in CI
```

## Completion status

At final closure:

```text
Status: Phase 1 complete — externally interoperable core TCP proxy with mixed HTTP/SOCKS listeners, ordinary HTTP forwarding, and HTTP/SOCKS chaining.
```

## Acceptance criteria

- no checked item contains the word “partial”;
- README status matches actual test evidence;
- external tests are described;
- the Phase 1 plan references this corrective closure artifact;
- future-phase features remain unchecked.

---

# Workstream 10: Strengthen CI and quality gates

## Required commands

All must pass:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

## CI jobs

Maintain:

- check on Ubuntu, macOS, Windows;
- tests on Ubuntu, macOS, Windows;
- formatting;
- clippy;
- cargo-deny;
- cargo-audit.

Add:

- pinned Python `pproxy` interoperability on Ubuntu;
- curl interoperability on Ubuntu.

## Confirm actual workflow status

The repository must not rely only on workflow YAML existence. Verify the latest closure commit has successful required checks.

## Useful constraints

- use dynamic ports everywhere;
- retain child process logs on failure;
- avoid arbitrary readiness sleeps;
- poll ports with a deadline;
- isolate process-based tests to prevent leaked children.

---

# Detailed execution sequence for a smaller model

Use the following commit-sized sequence. Each step should leave the workspace compiling and tests passing, except the first intentionally failing test commit if the workflow permits it.

## Step 1: Add reply-order tests

Add controlled connector tests for:

- HTTP CONNECT;
- SOCKS4;
- SOCKS5.

Expected result before implementation: tests demonstrate that success currently arrives too early.

## Step 2: Add accepted-session types

Add:

- `AcceptedSession`;
- `PendingTunnel`;
- `PendingHttpForward`;
- `RequestBodyKind`;
- protocol and reply-context enums.

Refactor inbound parsing to return these types.

Expected result:

- existing direct tests still pass;
- no empty-stream sentinel remains;
- ordinary HTTP execution is represented as its own variant.

## Step 3: Defer tunnel success replies

Refactor HTTP CONNECT, SOCKS4, and SOCKS5 parsing:

- parse only;
- retain enough state to reply later;
- remove immediate success writes.

Update execution:

- open route;
- on success, write success;
- on failure, write mapped failure;
- relay or close.

Run the reply-order tests.

## Step 4: Extract server orchestration from CLI

Create `eggress-server` or equivalent reusable library module.

Move:

- protocol dispatch;
- accepted-session execution;
- route opening;
- reply mapping;
- session reporting.

Reduce CLI to listener startup and supervision.

Add a library API integration test.

## Step 5: Route ordinary HTTP through common route opening

Delete all direct-connector creation from ordinary HTTP code.

Use the common `open_route` function.

Add:

- ordinary HTTP through SOCKS5 test;
- ordinary HTTP through HTTP CONNECT test;
- two-hop ordinary HTTP chain test.

## Step 6: Implement body framing

Add:

- body-kind parsing;
- exact Content-Length copier;
- bounded chunked copier;
- framing ambiguity rejection.

Add unit tests before integration tests.

Then add a POST test where the client does not half-close before awaiting the response.

## Step 7: Complete header filtering

Implement `Connection` token filtering and proxy credential removal.

Ensure `Transfer-Encoding: chunked` remains when forwarding chunked framing unchanged.

Add origin-observation tests.

## Step 8: Improve logging

Add `SessionReport`.

Use `.instrument(span)`.

Capture and test redaction.

Update relevant README boxes only after tests pass.

## Step 9: Reclassify tests and add external interoperability

Move self-interoperability tests to integration naming.

Add pinned Python `pproxy` harness and curl tests.

Run external tests on Ubuntu CI.

## Step 10: Reconcile documentation and close Phase 1

Update:

- README status;
- capability checkboxes;
- `docs/PHASE_1_PLAN.md` with a closure note;
- architecture documentation for the new session types;
- interoperability README.

Run all quality gates and verify CI.

---

# Specific regression matrix

The closure is incomplete unless all applicable rows pass.

| Case | Direct | HTTP upstream | SOCKS5 upstream | Two-hop chain |
|---|---:|---:|---:|---:|
| HTTP CONNECT | yes | yes | yes | yes |
| Ordinary HTTP GET | yes | yes | yes | yes |
| Ordinary HTTP POST Content-Length | yes | yes | yes | yes |
| Ordinary HTTP POST chunked | yes | yes | yes | yes |
| SOCKS4a CONNECT | yes | yes | yes | yes |
| SOCKS5 CONNECT IPv4 | yes | yes | yes | yes |
| SOCKS5 CONNECT IPv6 | yes | yes | yes | where CI supports IPv6 |
| SOCKS5 CONNECT domain | yes | yes | yes | yes |
| Route refused | protocol failure reply | protocol failure reply | protocol failure reply | protocol failure reply |
| Route timeout | protocol failure reply | protocol failure reply | protocol failure reply | protocol failure reply |

Use table-driven tests where practical.

---

# Required negative tests

## HTTP

- malformed request line;
- oversized request head;
- invalid authority;
- conflicting Content-Length;
- TE+CL ambiguity;
- malformed chunk size;
- oversized chunk-size line;
- missing chunk CRLF;
- premature EOF in fixed body;
- premature EOF in chunked body;
- upstream refused;
- upstream timeout;
- Proxy-Authorization redaction.

## SOCKS4

- unsupported command;
- malformed NUL-terminated user field;
- oversized field;
- route refused after valid request;
- no granted reply before route success.

## SOCKS5

- unsupported auth method;
- authentication failure;
- unsupported command;
- unsupported address type;
- malformed domain length;
- route refused;
- route timeout;
- no `REP=0` before route success.

## Chains

- failure at hop 0;
- failure at hop 1;
- failure at final destination;
- hop timeout;
- credentials absent from returned display and log errors.

---

# Architecture constraints

The corrective pass must preserve these invariants:

1. `eggress-core` does not depend on CLI.
2. Protocol crates do not depend on CLI.
3. CLI does not implement protocol wire parsing.
4. Domain targets remain unresolved until a connector requires resolution.
5. All outbound paths use one route-opening abstraction.
6. Success replies are emitted only after route success.
7. Relay receives two genuinely connected streams; no sentinel streams.
8. Request bodies are streamed with bounded memory.
9. No C dependency or OpenSSL is introduced.
10. No unsafe Rust is required for this closure.
11. Existing chain executor behavior remains reusable for future transports.
12. Errors are typed; reply mapping does not depend on string matching.

---

# Suggested code review checklist

## Session model

- [ ] Is each state explicit?
- [ ] Can a completed request be confused with a tunnel?
- [ ] Is there any dummy or sentinel stream?
- [ ] Can protocol parsing accidentally open a route?

## Reply ordering

- [ ] Is success sent after the final route opens?
- [ ] Is failure sent when opening fails?
- [ ] Does cancellation avoid sending success?
- [ ] Are protocol reply codes correct?

## HTTP framing

- [ ] Is a no-length request treated as no body?
- [ ] Is Content-Length copied exactly?
- [ ] Is chunked framing terminated by zero chunk and trailers?
- [ ] Are TE+CL conflicts rejected?
- [ ] Are all size limits enforced before allocation?

## Routing

- [ ] Does ordinary HTTP use `-r`?
- [ ] Do all modes use the same route opener?
- [ ] Are domain names preserved through SOCKS5 and HTTP hops?
- [ ] Does a failed hop identify its index safely?

## Logging

- [ ] Is span instrumentation async-safe?
- [ ] Are credentials absent?
- [ ] Are outcome and byte counts present?
- [ ] Is the route summarized without secrets?

## Tests

- [ ] Are self-tests called integration tests?
- [ ] Is Python `pproxy` pinned?
- [ ] Are curl tests local-only?
- [ ] Do tests avoid arbitrary sleeps?
- [ ] Does CI fail rather than skip external interoperability?

---

# Definition of done

Phase 1 corrective closure is complete only when:

1. ordinary HTTP forwarding uses the configured route or chain;
2. no empty-stream sentinel remains;
3. HTTP request-body framing is correct for no-body, Content-Length, and chunked requests;
4. HTTP CONNECT, SOCKS4, and SOCKS5 success replies are deferred until route success;
5. route failures produce protocol-appropriate failure responses;
6. protocol orchestration is available through a reusable library boundary rather than residing in CLI;
7. async tracing spans are instrumented safely;
8. external interoperability tests with pinned Python `pproxy` and curl pass in CI;
9. README checkboxes and status accurately reflect verified functionality;
10. all workspace, lint, security, and cross-platform CI checks pass;
11. no new native dependency, OpenSSL dependency, or unsafe code is introduced;
12. the repository is ready to begin Phase 2 without carrying known Phase 1 session-model or HTTP-framing debt.
