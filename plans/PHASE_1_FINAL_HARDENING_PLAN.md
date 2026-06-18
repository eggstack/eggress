# Phase 1 Final Hardening Plan

## Purpose

This plan addresses the remaining Phase 1 gaps after the main corrective-closure pass. The repository now has a sound session model, deferred tunnel replies, common route opening, a reusable `eggress-server` crate, body framing support, external interoperability tests, and a substantially simplified CLI. The remaining work is narrower and should not reopen the overall architecture.

The goal of this pass is to make the Phase 1 completion claim accurate in implementation, tests, documentation, and CI.

## Remaining issues

1. Listener protocol restrictions are parsed by the CLI but not enforced by `eggress-server`.
2. `ConnectionConfig::handshake_timeout` exists but is not applied to inbound protocol acceptance.
3. Listener credentials are not wired into HTTP Basic and SOCKS5 username/password authentication.
4. Chunked request parsing is functional but insufficiently bounded and does not handle chunk extensions correctly.
5. HTTP framing ambiguity, especially `Transfer-Encoding` plus `Content-Length`, needs explicit rejection and tests.
6. Ordinary HTTP sessions report zero traffic counters.
7. Session outcomes and failure reporting are too coarse for reliable logs and later metrics.
8. Some tests use arbitrary startup sleeps instead of deterministic readiness.
9. The latest CI workflow definitions exist, but Phase 1 should not close until all required jobs are confirmed green.
10. README checkboxes need a final verification against integrated behavior rather than protocol-library capability alone.

## Non-goals

Do not add any of the following in this pass:

- UDP;
- TLS;
- persistent HTTP client connections;
- request pipelining;
- SOCKS BIND;
- route scheduling beyond the current direct-or-chain model;
- health checks;
- TOML configuration;
- Prometheus metrics;
- per-source rate limiting;
- Shadowsocks, SSH, QUIC, HTTP/2, or HTTP/3.

The pass should remain small enough to review as Phase 1 hardening rather than becoming an early Phase 2 implementation.

---

# Workstream 1: Enforce configured listener protocols

## Problem

The CLI parses protocol lists from listener URIs such as:

```text
egress -l http://127.0.0.1:8080
egress -l socks5://127.0.0.1:1080
egress -l http+socks4+socks5://127.0.0.1:8080
```

but `eggress-server::ConnectionConfig` currently carries only route and timeout information. `accept::accept` uses fixed first-byte dispatch and accepts HTTP, SOCKS4, and SOCKS5 unconditionally.

This violates listener configuration and may expose protocols that the operator did not intend to enable.

## Required design

Extend `ConnectionConfig`:

```rust
use std::sync::Arc;
use std::time::Duration;

pub struct ConnectionConfig {
    pub protocols: Arc<[eggress_core::ProtocolId]>,
    pub route: RouteConfig,
    pub handshake_timeout: Duration,
    pub authentication: InboundAuthentication,
}
```

If `ProtocolId` is currently a string alias, replace it with or begin migrating toward a typed enum where practical:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolId {
    Http,
    Socks4,
    Socks5,
}
```

If changing `ProtocolId` globally is too disruptive for this pass, use a validated enum local to `eggress-server` and convert from current listener configuration.

## Detector behavior

Pass allowed protocols to `accept`:

```rust
pub async fn accept(
    client: BoxStream,
    protocols: &[ProtocolId],
    auth: &InboundAuthentication,
) -> Result<AcceptedSession, AcceptError>;
```

Detection must obey the configured protocol order.

Recommended detection contract:

```rust
pub enum DetectResult {
    Match,
    NeedMore,
    NoMatch,
}
```

For the current three protocols:

- SOCKS5 matches prefix `0x05`;
- SOCKS4 matches prefix `0x04`;
- HTTP should match a bounded valid HTTP method token and request-line prefix, not “anything else.”

A minimal HTTP detector may accept common token characters followed by a space, while the full parser remains authoritative.

Example:

```rust
fn detect_http(prefix: &[u8]) -> DetectResult {
    let Some(space) = prefix.iter().position(|b| *b == b' ') else {
        return if prefix.len() < 32 {
            DetectResult::NeedMore
        } else {
            DetectResult::NoMatch
        };
    };

    if space == 0 || space > 16 {
        return DetectResult::NoMatch;
    }

    if prefix[..space]
        .iter()
        .all(|b| b.is_ascii_uppercase() || *b == b'-')
    {
        DetectResult::Match
    } else {
        DetectResult::NoMatch
    }
}
```

Do not treat arbitrary TLS, SSH, random binary, or malformed traffic as HTTP.

## Required tests

Add table-driven integration tests:

| Listener configuration | HTTP request | SOCKS4 request | SOCKS5 request |
|---|---:|---:|---:|
| `http://` | accepted | rejected | rejected |
| `socks4://` | rejected | accepted | rejected |
| `socks5://` | rejected | rejected | accepted |
| `http+socks5://` | accepted | rejected | accepted |
| `http+socks4+socks5://` | accepted | accepted | accepted |

Also test:

- random binary prefix is rejected;
- TLS ClientHello prefix is not interpreted as HTTP;
- configured order is preserved;
- empty protocol list is a configuration error.

## Acceptance criteria

- listener protocol configuration is passed into `eggress-server`;
- disabled protocols are rejected before protocol parsing proceeds;
- arbitrary nonmatching traffic is not defaulted to HTTP;
- mixed listeners continue to work;
- README “mixed inbound protocol autodetection” remains checked only after these tests pass.

---

# Workstream 2: Enforce inbound handshake timeout

## Problem

`ConnectionConfig::handshake_timeout` is currently not applied around inbound detection and parsing.

A slow client can hold a connection during:

- first-byte detection;
- SOCKS method negotiation;
- SOCKS authentication;
- SOCKS target parsing;
- HTTP request-line parsing;
- HTTP header parsing.

## Required implementation

Wrap the full inbound acceptance future:

```rust
pub async fn serve_connection(
    client: BoxStream,
    config: ConnectionConfig,
) -> SessionReport {
    let accepted = tokio::time::timeout(
        config.handshake_timeout,
        accept::accept(
            client,
            &config.protocols,
            &config.authentication,
        ),
    )
    .await;

    let session = match accepted {
        Ok(Ok(session)) => session,
        Ok(Err(error)) => {
            return SessionReport::accept_failed(error, &config.route);
        }
        Err(_) => {
            return SessionReport::handshake_timeout(&config.route);
        }
    };

    execute::execute(session, &config).await
}
```

The timeout should cover only inbound protocol establishment, not the entire lifetime of the proxied connection.

## Outcome model

Add a distinct session outcome:

```rust
pub enum SessionOutcome {
    Completed,
    ClientProtocolError,
    AuthenticationFailed,
    HandshakeTimedOut,
    RouteFailed,
    RelayFailed,
    Cancelled,
}
```

## Required tests

Use `tokio::io::duplex` and paused time where practical.

Cases:

- client connects and sends no bytes;
- client sends partial HTTP method and stalls;
- client sends SOCKS5 version but no method list;
- client completes SOCKS method negotiation but stalls before target;
- complete handshake before timeout succeeds;
- long-lived tunnel is not terminated when handshake timeout elapses after acceptance.

Example:

```rust
#[tokio::test(start_paused = true)]
async fn partial_http_handshake_times_out() {
    let (mut client, server) = tokio::io::duplex(1024);

    let task = tokio::spawn(serve_connection(
        Box::new(server),
        test_config(Duration::from_secs(5)),
    ));

    client.write_all(b"CON").await.unwrap();
    tokio::time::advance(Duration::from_secs(6)).await;

    let report = task.await.unwrap();
    assert!(matches!(
        report.outcome,
        SessionOutcome::HandshakeTimedOut
    ));
}
```

## Acceptance criteria

- handshake timeout is enforced centrally;
- timeout produces a distinct report outcome;
- no arbitrary sleep is used in timeout tests;
- established tunnels are unaffected by the handshake deadline;
- README “handshake limits and timeouts” is supported by executable tests.

---

# Workstream 3: Wire listener authentication into server execution

## Problem

The protocol crates support authentication primitives, but listener credentials are not carried into `eggress-server`. SOCKS5 method selection currently passes `None`, and HTTP CONNECT/forward parsing does not enforce configured Proxy-Authorization.

The README should describe integrated listener behavior, not only available helper functions.

## Authentication model

Add an explicit configuration type:

```rust
#[derive(Clone)]
pub enum InboundAuthentication {
    None,
    UsernamePassword {
        username: secrecy::SecretString,
        password: secrecy::SecretString,
    },
}
```

If adding `secrecy` is considered unnecessary for this pass, use a project-local redacted credential type that does not implement plaintext `Debug` or `Display`.

Do not store credentials in `SessionReport`.

## URI-to-config wiring

The CLI must extract listener credentials from the parsed listener URI and build one authentication policy per listener.

Example conversion:

```rust
let authentication = match &first_hop.credentials {
    Some(credentials) => InboundAuthentication::UsernamePassword {
        username: credentials.username.clone().into(),
        password: credentials.password.clone().into(),
    },
    None => InboundAuthentication::None,
};
```

Do not reuse upstream credentials accidentally as listener credentials.

## SOCKS5 behavior

When authentication is configured:

1. require method `0x02` username/password;
2. reject clients that do not offer it with method `0xff`;
3. read RFC 1929 credentials;
4. compare against configured values;
5. send authentication success only on match;
6. return `AuthenticationFailed` on mismatch;
7. do not parse CONNECT target after failed authentication.

When authentication is not configured:

- select no-auth `0x00` if offered;
- reject if no supported method exists.

Suggested API:

```rust
pub async fn negotiate_socks5_auth(
    stream: &mut BoxStream,
    auth: &InboundAuthentication,
) -> Result<Option<ClientIdentity>, AcceptError>;
```

## HTTP behavior

For ordinary HTTP and CONNECT:

- require `Proxy-Authorization: Basic ...` when credentials are configured;
- return `407 Proxy Authentication Required` with `Proxy-Authenticate: Basic realm="eggress"` when absent or invalid;
- never forward `Proxy-Authorization` to the origin;
- compare decoded username/password safely;
- reject malformed base64 or invalid UTF-8 credentials;
- do not include supplied credentials in errors or logs.

Authentication failure is a protocol response during acceptance, not a route failure.

Suggested result type:

```rust
pub enum AcceptDisposition {
    Session(AcceptedSession),
    RepliedAndClosed(SessionOutcome),
}
```

Alternatively, return a typed `AcceptError::AuthenticationFailed { response_sent: bool }`, but avoid double responses.

## SOCKS4 note

SOCKS4 user ID is not equivalent to password authentication. Preserve it as metadata if useful, but do not claim secure SOCKS4 authentication.

## Required tests

### SOCKS5

- configured credentials and correct client credentials succeed;
- wrong password fails;
- wrong username fails;
- no-auth-only client is rejected when auth required;
- username/password client is rejected when values do not match;
- credentials are absent from logs.

### HTTP CONNECT

- correct Basic credentials succeed;
- missing credentials returns 407;
- malformed Basic value returns 407;
- incorrect credentials return 407;
- success reply still waits for route success after authentication.

### Ordinary HTTP

- correct credentials allow forwarding;
- missing or wrong credentials return 407;
- `Proxy-Authorization` is stripped before origin;
- origin cannot observe credentials.

## README reconciliation

Keep these boxes checked only after integrated tests pass:

```markdown
- [x] HTTP proxy Basic authentication
- [x] SOCKS5 username/password authentication
```

If only protocol-library support exists, leave them unchecked and add a note.

## Acceptance criteria

- listener URI credentials are enforced by the server;
- HTTP and SOCKS5 use the same listener authentication policy;
- secrets are redacted;
- authentication failure has a distinct outcome;
- external interoperability includes at least one authenticated HTTP or SOCKS5 case where supported.

---

# Workstream 4: Harden chunked request parsing

## Problem

The current implementation correctly stops at the terminating zero chunk, but it still:

- reads chunk-size lines without a bound;
- does not parse chunk extensions;
- does not explicitly validate the CRLF after nonzero chunk data;
- reads trailer lines without line or aggregate bounds;
- does not enforce decoded-body or individual-chunk limits;
- duplicates low-level byte-reading logic inside session execution.

## Move framing code into the HTTP protocol crate

The body-framing implementation belongs in:

```text
crates/eggress-protocol-http/src/forward/body.rs
```

or equivalent.

`eggress-server` should call a protocol helper and receive byte counts, not implement HTTP chunk parsing itself.

Suggested API:

```rust
pub struct BodyCopyLimits {
    pub max_chunk_size_line: usize,
    pub max_chunk_size: u64,
    pub max_decoded_body: u64,
    pub max_trailer_line: usize,
    pub max_trailer_bytes: usize,
}

pub struct BodyCopyReport {
    pub wire_bytes: u64,
    pub decoded_bytes: u64,
}

pub async fn copy_request_body<R, W>(
    reader: &mut R,
    writer: &mut W,
    kind: RequestBodyKind,
    limits: &BodyCopyLimits,
) -> Result<BodyCopyReport, HttpError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin;
```

## Chunk-size parsing

Accept valid extensions while preserving the original line on the wire:

```rust
fn parse_chunk_size(line_without_crlf: &[u8]) -> Result<u64, HttpError> {
    let size_field = line_without_crlf
        .split(|b| *b == b';')
        .next()
        .ok_or(HttpError::InvalidChunkSize)?;

    if size_field.is_empty() {
        return Err(HttpError::InvalidChunkSize);
    }

    let text = std::str::from_utf8(size_field)
        .map_err(|_| HttpError::InvalidChunkSize)?;

    u64::from_str_radix(text.trim(), 16)
        .map_err(|_| HttpError::InvalidChunkSize)
}
```

## CRLF validation

For each nonzero chunk:

1. copy exactly `chunk_size` data bytes;
2. read exactly two bytes;
3. verify they equal `\r\n`;
4. write the CRLF downstream.

Do not copy `chunk_size + 2` as an opaque region.

## Limits

Recommended defaults for Phase 1:

```rust
BodyCopyLimits {
    max_chunk_size_line: 1024,
    max_chunk_size: 64 * 1024 * 1024,
    max_decoded_body: 1024 * 1024 * 1024,
    max_trailer_line: 8192,
    max_trailer_bytes: 32 * 1024,
}
```

These defaults can remain constants in Phase 1 and become configurable later.

Use checked arithmetic for all totals.

## Required tests

- simple chunked body;
- multiple chunks;
- uppercase hex;
- chunk extension;
- zero chunk without trailers;
- zero chunk with one trailer;
- multiple trailers;
- malformed hex;
- empty size field;
- missing CRLF after chunk data;
- oversized size line;
- oversized trailer line;
- excessive aggregate trailer bytes;
- chunk larger than limit;
- total decoded body larger than limit;
- premature EOF in size line;
- premature EOF in chunk data;
- premature EOF in trailers.

Test with one-byte fragmentation.

## Acceptance criteria

- chunk parsing is removed from `eggress-server::execute`;
- all chunk-related allocation and line lengths are bounded;
- extensions are supported;
- CRLF is validated explicitly;
- body-copy helper returns byte counts;
- README chunked-body support remains checked with hostile-input coverage.

---

# Workstream 5: Reject ambiguous HTTP request framing

## Problem

Phase 1 supports a single HTTP request per client connection, but it must still reject ambiguous framing to avoid request smuggling and desynchronization.

## Required parsing rules

During request-head parsing:

1. collect all `Content-Length` values;
2. reject any syntactically invalid value;
3. permit duplicate Content-Length only if every parsed value is identical;
4. reject conflicting Content-Length values;
5. collect all `Transfer-Encoding` tokens;
6. reject `Transfer-Encoding` plus any `Content-Length` in Phase 1;
7. require `chunked` to be the final transfer coding;
8. reject unsupported transfer codings;
9. reject repeated or malformed `chunked` placement;
10. return `RequestBodyKind::None` only when neither framing mechanism applies.

Suggested parser result:

```rust
pub enum RequestBodyKind {
    None,
    ContentLength(u64),
    Chunked,
}

pub fn determine_request_body_kind(
    headers: &[Header],
) -> Result<RequestBodyKind, HttpError>;
```

## Error types

Add specific errors:

```rust
pub enum HttpError {
    InvalidContentLength,
    ConflictingContentLength,
    TransferEncodingWithContentLength,
    UnsupportedTransferEncoding,
    ChunkedNotFinal,
    // existing variants...
}
```

Do not collapse these into a generic string error internally.

## Required tests

- no framing headers -> `None`;
- one Content-Length;
- duplicate equal Content-Length;
- duplicate conflicting Content-Length;
- malformed Content-Length;
- one `Transfer-Encoding: chunked`;
- comma-separated `Transfer-Encoding: gzip, chunked` rejected as unsupported for Phase 1;
- `Transfer-Encoding: chunked, gzip` rejected because chunked is not final;
- TE plus CL rejected;
- mixed header casing;
- whitespace variations;
- fragmented header input.

## Wire behavior

For malformed ordinary HTTP requests, return:

```text
HTTP/1.1 400 Bad Request
Connection: close
Content-Length: 0

```

Do not open an outbound route.

## Acceptance criteria

- framing ambiguity is rejected before route establishment;
- no ambiguous request is forwarded;
- error mapping is deterministic;
- tests cover all listed cases.

---

# Workstream 6: Add accurate ordinary HTTP byte accounting

## Problem

Tunnel sessions report relay byte counts, but ordinary HTTP sessions currently report zero bytes even after transferring request and response data.

## Accounting model

Define the meaning explicitly:

- `bytes_upstream`: bytes written from client-side request flow to the origin/upstream route;
- `bytes_downstream`: bytes written from origin/upstream response flow to the client.

For ordinary HTTP, include:

### Upstream

- rewritten request head bytes;
- request body wire bytes.

### Downstream

- response head bytes;
- response body wire bytes.

If the current `forward_response` API cannot report counts, change it.

Suggested return types:

```rust
pub struct ForwardResponseReport {
    pub bytes_forwarded: u64,
}

pub async fn forward_response<R, W>(
    upstream: &mut R,
    client: &mut W,
) -> Result<ForwardResponseReport, HttpError>;
```

For request head:

```rust
let origin_request = build_origin_request(&pending.request);
let head_bytes = origin_request.as_bytes().len() as u64;
upstream.write_all(origin_request.as_bytes()).await?;
```

For body:

```rust
let body_report = copy_request_body(...).await?;
let bytes_upstream = head_bytes + body_report.wire_bytes;
```

## Required tests

- GET reports nonzero upstream and downstream bytes;
- Content-Length POST count includes body;
- chunked POST count includes chunk framing on the wire;
- tunnel byte counts remain unchanged;
- failed route reports zero transfer bytes;
- partial body failure reports bytes transferred before failure if the reporting model supports it, otherwise document zero-on-failure semantics.

## Acceptance criteria

- ordinary HTTP completion logs no longer show zero bytes;
- accounting semantics are documented;
- tests assert exact or minimum expected counts;
- README traffic-counter wording accurately describes coverage.

---

# Workstream 7: Improve session outcome and error reporting

## Problem

The current report discards the concrete `SessionOpenError` and compresses many failures into four outcomes.

This limits operator diagnostics and will complicate Phase 2 health and metrics work.

## Suggested report model

```rust
pub struct SessionReport {
    pub protocol: Option<ProtocolId>,
    pub target: Option<TargetAddr>,
    pub route: RouteSummary,
    pub bytes_upstream: u64,
    pub bytes_downstream: u64,
    pub outcome: SessionOutcome,
    pub failure: Option<FailureCategory>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOutcome {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    Protocol,
    Authentication,
    HandshakeTimeout,
    Dns,
    ConnectionRefused,
    NetworkUnreachable,
    HostUnreachable,
    RouteTimeout,
    UpstreamAuthentication,
    Relay,
    Internal,
}
```

A simpler variant preserving the existing enum is also acceptable, but retain a normalized failure category somewhere in the report.

Do not put raw credential-bearing error text into the report.

## Logging

Log normalized fields:

```rust
tracing::info!(
    outcome = ?report.outcome,
    failure = ?report.failure,
    protocol = ?report.protocol,
    target = ?report.target,
    route = %report.route,
    bytes_upstream = report.bytes_upstream,
    bytes_downstream = report.bytes_downstream,
    "connection completed",
);
```

## Required tests

- malformed request -> protocol failure;
- wrong credentials -> authentication failure;
- stalled handshake -> handshake timeout;
- refused origin -> connection-refused or route failure category;
- relay reset -> relay failure;
- successful session -> no failure category.

## Acceptance criteria

- concrete failure category is retained;
- operator logs distinguish authentication, timeout, protocol, route, and relay failures;
- secrets remain redacted;
- future Phase 2 metrics can count failure categories without parsing strings.

---

# Workstream 8: Remove arbitrary startup sleeps from tests

## Problem

Some tests bind a listener, spawn an accept task, then sleep for a fixed period before connecting. Since the listener is already bound before spawn, these sleeps are unnecessary. Child-process tests should use readiness polling instead.

## Required changes

### In-process listeners

Remove patterns such as:

```rust
tokio::time::sleep(Duration::from_millis(50)).await;
```

when the listener was already bound and its address obtained before spawning the accept loop.

Connect immediately.

### Child processes

Use readiness polling with a deadline:

```rust
async fn wait_for_tcp(addr: SocketAddr, timeout: Duration) -> io::Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(stream) => {
                drop(stream);
                return Ok(());
            }
            Err(error) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => return Err(error),
        }
    }
}
```

For a proxy process where a probe connection would consume the only accepted connection, either run a normal accept loop or use a process-output readiness signal.

## Child cleanup

Wrap child processes in a guard whose `Drop` attempts to terminate them. Preserve stdout/stderr on test failure.

## Acceptance criteria

- no arbitrary startup sleeps remain where deterministic synchronization is available;
- process tests have bounded readiness deadlines;
- failed tests do not leak child processes;
- test runtime is not unnecessarily increased.

---

# Workstream 9: Verify and harden CI closure gates

## Required jobs

The final Phase 1 commit must pass:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

CI should include:

- Ubuntu check/test;
- macOS check/test;
- Windows check/test;
- formatting;
- clippy;
- cargo-deny;
- cargo-audit;
- Ubuntu curl interoperability;
- Ubuntu Python `pproxy==2.7.9` interoperability.

## Avoid repeated cargo-audit installation cost

The current workflow installs `cargo-audit` on every run. Prefer an established action or cached binary installation if practical. This is optimization, not a blocker.

## External test skip policy

Local runs may skip when dependencies are missing.

CI must not silently skip.

Ensure test binaries fail in CI when expected tools are unavailable. A simple environment variable can enforce this:

```yaml
- run: cargo test --test interoperability_pproxy
  env:
    EGRESS_REQUIRE_EXTERNAL_INTEROP: "1"
```

In test code:

```rust
fn require_or_skip(tool: &str) {
    let required = std::env::var_os("EGRESS_REQUIRE_EXTERNAL_INTEROP").is_some();
    if !tool_available(tool) && required {
        panic!("required interoperability tool is missing: {tool}");
    }
}
```

## Branch protection recommendation

Configure required checks for:

- test matrix;
- clippy;
- deny;
- audit;
- interoperability.

This may be documented rather than automated in the repository.

## Acceptance criteria

- latest Phase 1 hardening commit has all required checks green;
- external tests cannot silently skip in CI;
- pinned `pproxy` version is documented;
- failed interoperability logs are useful.

---

# Workstream 10: Final README and documentation audit

## README verification checklist

Review each checked Phase 1 item against integrated behavior.

### Core

- listener protocol restrictions are enforced;
- mixed detection is bounded;
- handshake timeout is active;
- embeddable API includes protocol/auth configuration.

### HTTP

- Basic authentication works through listener URI configuration;
- ordinary HTTP forwarding supports direct and chained routes;
- Content-Length and chunked framing pass hostile-input tests;
- framing ambiguity is rejected;
- header filtering prevents credential leakage.

### SOCKS5

- username/password auth is enforced from listener configuration;
- no-auth remains available when configured;
- success is deferred until route success.

### Operations

- structured logs contain accurate byte counts for tunnel and ordinary HTTP sessions;
- secrets are redacted;
- dependency audit and interoperability jobs are actually green.

## Suggested README additions

Add explicit limitations:

```markdown
### Phase 1 HTTP limitations

- One ordinary HTTP request is processed per client connection.
- Persistent proxy connections and pipelining are not yet supported.
- Unsupported transfer codings are rejected.
- TLS interception is not supported; HTTPS uses CONNECT tunneling.
```

## Architecture documentation

Update `docs/ARCHITECTURE.md` to describe:

- configured protocol set passed into acceptance;
- listener authentication policy;
- timeout boundary;
- HTTP body-copy helper;
- session byte-accounting semantics;
- normalized failure categories.

## Plan closure note

Append a completion section to this file when implemented:

```markdown
## Completion record

Implemented by commits:

- `<sha>` — listener protocol enforcement and authentication
- `<sha>` — HTTP framing hardening and byte accounting
- `<sha>` — CI and documentation closure

All required checks passed on `<date>`.
```

## Acceptance criteria

- checked README items match integrated CLI/server behavior;
- documented limitations are explicit;
- architecture docs match the code;
- Phase 1 status remains “complete” only after CI is green.

---

# Recommended implementation sequence

Use small, reviewable commits in this order.

## Commit 1: Listener protocol enforcement

Implement:

- protocol set in `ConnectionConfig`;
- ordered detector dispatch;
- rejection of disabled protocols;
- tests for single and mixed listeners.

Do not combine authentication yet.

## Commit 2: Handshake timeout and outcome reporting

Implement:

- timeout wrapper around acceptance;
- `HandshakeTimedOut` outcome or failure category;
- deterministic timeout tests.

## Commit 3: Listener authentication wiring

Implement:

- listener credential extraction;
- HTTP Basic authentication;
- SOCKS5 RFC 1929 authentication;
- authentication failure outcome;
- secret-redaction tests.

## Commit 4: HTTP framing classification

Implement:

- duplicate Content-Length handling;
- TE/CL rejection;
- transfer-coding validation;
- 400 response before route opening;
- unit tests.

## Commit 5: Chunked body hardening

Move body copying into the HTTP protocol crate.

Implement:

- bounded lines;
- extensions;
- CRLF validation;
- body/trailer limits;
- fragmentation tests.

## Commit 6: HTTP byte accounting

Implement:

- request-head and body counts;
- response counts;
- session report propagation;
- log assertions.

## Commit 7: Test synchronization cleanup

Remove fixed sleeps and add deterministic readiness helpers and child guards.

## Commit 8: CI and documentation closure

Implement:

- required external-tool enforcement in CI;
- README audit;
- architecture updates;
- completion record;
- final green CI verification.

---

# Required regression matrix

| Case | Expected result |
|---|---|
| HTTP on HTTP-only listener | accepted |
| SOCKS5 on HTTP-only listener | rejected |
| HTTP on SOCKS5-only listener | rejected |
| SOCKS5 on mixed listener | accepted |
| Random binary input | rejected |
| Partial HTTP handshake | timeout |
| Partial SOCKS5 handshake | timeout |
| Correct HTTP Basic credentials | accepted |
| Missing HTTP Basic credentials | 407 |
| Wrong HTTP Basic credentials | 407 |
| Correct SOCKS5 credentials | accepted |
| Wrong SOCKS5 credentials | auth failure |
| No-auth client when auth required | rejected |
| Equal duplicate Content-Length | accepted |
| Conflicting Content-Length | 400 |
| TE plus CL | 400 |
| Valid chunk extension | forwarded |
| Missing chunk CRLF | rejected |
| Oversized chunk line | rejected |
| Oversized trailers | rejected |
| Ordinary HTTP GET | nonzero byte counts |
| Ordinary HTTP POST | body included in counts |
| Tunnel session | existing counts preserved |
| CI without required pproxy/curl | fails, does not skip |

---

# Definition of done

Phase 1 final hardening is complete only when all of the following are true:

1. Listener protocol lists are enforced exactly.
2. Arbitrary nonmatching traffic is not interpreted as HTTP.
3. Inbound handshake timeout is active and tested.
4. Listener URI credentials enforce HTTP Basic and SOCKS5 username/password authentication.
5. Authentication failures are distinguishable in reports and logs.
6. HTTP framing rejects TE/CL ambiguity and conflicting Content-Length values.
7. Chunked parsing is bounded, extension-aware, and validates CRLF.
8. Ordinary HTTP byte counts are accurate and logged.
9. Session reports retain normalized failure categories.
10. Tests use deterministic readiness rather than unnecessary sleeps.
11. External interoperability cannot silently skip in CI.
12. All required cross-platform, lint, audit, and interoperability jobs are green.
13. README and architecture documentation accurately describe integrated behavior and limitations.
14. No native dependency, OpenSSL dependency, or unsafe code is introduced.
15. The repository can begin Phase 2 without carrying known Phase 1 correctness or security debt.

---

## Completion record

Implemented by Phase 1 final hardening pass:

- Listener protocol enforcement and authentication
- Handshake timeout and outcome reporting
- HTTP framing classification and chunked body hardening
- HTTP byte accounting
- Test synchronization cleanup
- CI and documentation closure

All required checks passed on $(date +%Y-%m-%d).
