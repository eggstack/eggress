# Eggfetch Shared HTTP CONNECT Primitive — Upstream Prerequisite

## Status

**READY FOR IMPLEMENTATION — 2026-09-16**

## Target repository

This plan is stored in Eggress for handoff, but its executable work belongs first in:

```text
eggstack/eggfetch
```

Planning baseline:

```text
0f720b4fd9efea80d180ce5942fce5e3453d8003
```

At this baseline `eggfetch-core` is `0.1.5`, the Eggfetch workspace declares Rust `1.89`, and HTTP proxy CONNECT logic remains internal to the full client transport implementation.

Parent roadmap:

- `plans/EGGFETCH_CONNECT_CONSOLIDATION_ROADMAP.md`

Downstream follow-up:

- `plans/EGGFETCH_HTTP_CONNECT_INTEGRATION_AND_FOOTPRINT_CLOSURE.md`

## Objective

Extract the protocol-level HTTP/1.x CONNECT request/response machinery already maintained by Eggfetch into a small published workspace crate that can operate over any caller-supplied Tokio byte stream.

The new crate must be useful independently of Eggress. It should model the wire protocol, not a downstream product or an HTTP-client route.

A suitable package name is `eggfetch-http-connect` if available. If that crates.io name is unavailable or conflicts with current Eggfetch naming policy, select another narrow Eggfetch-owned name, record the choice in this plan's closure note, and preserve the same semantic boundary.

The target relationship is:

```text
eggfetch-http-connect
  owns: HTTP CONNECT request serialization + response-head parsing + tunnel read-ahead preservation
  knows: AsyncRead/AsyncWrite, HTTP header/status primitives, bounded parsing
  does not know: URL routing, DNS, TcpStream creation, TLS, Hyper client/pool, retries, redirects, cookies,
                 decompression, H2/H3, Eggress, ProxyChainSpec
```

## Why extraction is justified

`eggfetch-core` and `eggress-protocol-http` currently maintain independent versions of the same security-sensitive wire behavior. The overlap is not merely an HTTP convenience API; it includes parser limits, authentication serialization, authority formatting and preservation of bytes read after a successful CONNECT response.

The current Eggfetch implementation already contains behavior worth centralizing:

- correct bracketed IPv6 authority form;
- byte-preserving proxy response headers rather than whole-head UTF-8 conversion;
- bounded status/header parsing;
- explicit tests for truncated response heads;
- exact header-count boundaries;
- proxy credential handling and redaction discipline.

The extraction is successful only if Eggfetch itself consumes the new primitive. Creating a helper package that exists only for Eggress while Eggfetch retains its private copy would increase maintenance rather than reduce it.

---

# Workstream 0 — Reconfirm current Eggfetch transport ownership

Before editing:

1. Re-read the current versions of:
   - `crates/eggfetch-core/src/transport/connect.rs`;
   - `crates/eggfetch-core/src/transport/proxy.rs`;
   - `crates/eggfetch-core/src/proxy.rs`;
   - `crates/eggfetch-core/src/error.rs`;
   - `crates/eggfetch-core/src/headers.rs`;
   - `crates/eggfetch-core/Cargo.toml`;
   - workspace `Cargo.toml`;
   - current package/release scripts and dependency policy docs.
2. Search for all CONNECT request construction, authority formatting, proxy-response parsing and proxy-auth header construction.
3. Identify which helpers are shared by HTTP forward-proxy response handling versus CONNECT-only handling. Do not move unrelated general HTTP response machinery merely to maximize line deletion.
4. Confirm the new crate can remain small and free from Hyper client, TLS and socket dependencies.
5. Capture focused tests that currently define Eggfetch CONNECT semantics before refactoring.

If Eggfetch has already introduced an equivalent public stream-level primitive, stop and adapt this plan to consume it instead of creating a second one.

---

# Workstream 1 — Create a narrow published protocol crate

Add one workspace member under `crates/`, preferably:

```text
crates/eggfetch-http-connect/
```

The crate should be independently publishable. It should not depend on `eggfetch-core`.

## Dependency budget

Prefer a dependency floor no larger than what the protocol actually needs. Expected candidates are:

- `tokio` with only I/O features needed for `AsyncRead`/`AsyncWrite`/`BufReader`;
- `http` for `StatusCode`, `HeaderMap`, `HeaderName` and `HeaderValue`, if those types materially simplify a correct public contract;
- `base64` for Basic proxy authentication if auth serialization lives in the primitive;
- `thiserror` if a typed error materially improves the public API.

Do not add:

- `hyper` or `hyper-util`;
- `hyper-rustls`, `rustls` or `tokio-rustls`;
- `url` solely for CONNECT authority formatting;
- DNS/resolver crates;
- socket crates;
- retry/backoff crates;
- serde;
- Eggress dependencies.

If an implementation needs those heavier dependencies, the boundary has drifted and should be redesigned.

## MSRV

Set an explicit `rust-version` consistent with current project policy and the downstream Eggress constraint at implementation time. Do not accidentally inherit an MSRV that makes Eggress unusable if Eggress has not yet moved to that toolchain.

This crate is simple enough that there should be no technical need to use newer language features merely for CONNECT parsing.

---

# Workstream 2 — Define the public wire-level contract

The exact syntax may vary, but the API must support the following semantics.

## 2.1 Target representation

Expose a target representation based on logical host + port rather than a full URL, for example:

```rust
pub struct ConnectTarget {
    host: String,
    port: u16,
}
```

Requirements:

- format domains and IPv4 as `host:port`;
- format IPv6 literals as `[addr]:port`;
- reject empty/invalid hosts that could create malformed request lines;
- never accept CR, LF or other request-line injection bytes;
- do not carry origin path/query/body data.

Do not require callers to hand-format authority strings if that would reintroduce IPv6/injection bugs in each consumer.

## 2.2 Limits

Expose explicit response-head limits, for example:

```rust
pub struct ConnectLimits {
    pub max_status_line: usize,
    pub max_header_line: usize,
    pub max_headers_bytes: usize,
    pub max_header_count: usize,
}
```

Defaults may follow Eggfetch's current CONNECT parser, but both Eggfetch and Eggress must be able to provide their own existing values during migration.

`max_header_count` must mean actual header fields. It must not count the status line or terminal empty line.

All limits must be checked before unbounded allocation. Tests must cover exact-limit acceptance and one-past-limit rejection.

## 2.3 Authentication and proxy-only headers

Support CONNECT-leg Basic proxy authentication without exposing secrets through derived `Debug`/`Display` output.

The shared layer must:

- reject CR/LF/control-character injection in username/password inputs before writing;
- serialize `Proxy-Authorization: Basic ...` correctly;
- allow Eggfetch's existing proxy-only headers to be written on the CONNECT leg;
- reject or structurally prevent raw header-name/value injection;
- avoid duplicate `Proxy-Authorization` when a configured auth object already owns it;
- never include credentials in automatically generated errors.

A small redacting auth type is preferred to accepting a raw prebuilt header string if that keeps secret handling centralized. However, do not force Eggfetch's higher-level `ProxyAuth` type into the new crate; adapt at the `eggfetch-core` boundary.

## 2.4 Request serialization

Provide a reusable serializer/writer that emits at least:

```text
CONNECT host:port HTTP/1.1\r\n
Host: host:port\r\n
[Proxy-Authorization ...]\r\n
[proxy-only headers ...]\r\n
\r\n
```

Requirements:

- authority and `Host` must agree;
- IPv6 must be bracketed;
- request size must be bounded;
- duplicate/forbidden proxy headers must have deterministic handling;
- serialization must be byte-exact and independently unit-testable.

Do not make this function perform DNS, TCP connect or TLS.

## 2.5 Response parser

Provide a parser that reads one HTTP/1.x response head from a buffered asynchronous stream.

Requirements:

- bounded status line;
- bounded individual header line where configured;
- bounded aggregate head bytes;
- bounded header count;
- clean rejection of premature EOF/truncated status/header sections;
- status code parsed without requiring all header values to be UTF-8;
- header names validated according to HTTP token rules;
- header values retained as bytes / `HeaderValue` semantics;
- optional reason phrase may be retained if already useful to Eggfetch, but must not force lossy conversion;
- no body read is required to classify a successful tunnel;
- bytes already prefetched beyond `\r\n\r\n` must remain available to the caller.

The parser should return protocol metadata separately from policy classification. Do not bake Eggfetch's exact-200 rule or Eggress's any-2xx rule into a single universal success decision.

A conceptual return shape is:

```rust
pub struct ConnectResponse {
    status: StatusCode,
    headers: HeaderMap,
    // optional reason phrase if retained
}
```

The stream/tunnel must remain caller-owned after parsing.

## 2.6 Read-ahead-preserving stream ownership

The public API must make it difficult to accidentally discard buffered bytes.

Acceptable designs include:

- taking and returning a `tokio::io::BufReader<S>`;
- returning a small `ConnectTunnel<S>` wrapper that contains the buffered reader and implements `AsyncRead + AsyncWrite`;
- another equally explicit ownership transfer.

Do not parse by reading arbitrary chunks into a temporary vector and then return only the inner stream unless every post-head byte is explicitly retained and replayed first.

## 2.7 Timeout ownership

Do **not** put global retry/timeout policy in this crate.

Eggfetch currently distinguishes proxy connect, proxy TLS, CONNECT write, CONNECT read and destination TLS budgets. Eggress applies its own outer handshake/connect timing. The shared primitive must not erase those distinctions.

Prefer lower-level async operations that can be wrapped externally, such as:

```text
write_connect_request(...).await
read_connect_response(...).await
```

A convenience `establish_connect()` may be provided for callers that do not require phase-specific wrapping, but Eggfetch must be able to preserve its current per-phase timeout semantics without duplicating the parser/serializer.

---

# Workstream 3 — Error model

Create a narrow protocol error type with no Eggfetch-client or Eggress variants.

Expected categories include:

- I/O failure;
- invalid target;
- invalid credential/header input;
- malformed status line;
- malformed header;
- unsupported/malformed HTTP version where relevant;
- status line too large;
- header line too large;
- total head too large;
- too many headers;
- unexpected EOF.

Do not encode application-specific mappings such as `AuthRequired`, `BadGateway`, retryability or route failure in this crate. Consumers classify `ConnectResponse.status()` themselves.

`Debug`/`Display` must remain bounded and must not print secret-bearing auth/header values.

---

# Workstream 4 — Convert Eggfetch to consume the primitive

After the crate is independently tested, modify `eggfetch-core` so its applicable CONNECT path delegates to it.

At minimum inspect and reduce duplication in:

- `transport/connect.rs`;
- `transport/proxy.rs`;
- proxy-auth/request-header construction helpers.

Preserve current Eggfetch behavior:

- proxy TCP/TLS connection ownership remains in `eggfetch-core`;
- request deadline and phase timeout ownership remains in `eggfetch-core`;
- destination TLS remains in `eggfetch-core`;
- HTTP request/response behavior inside the established tunnel remains unchanged;
- route-cache identity and reusable connector policy remain unchanged;
- proxy rejection-body sanitization remains Eggfetch-owned if it depends on Eggfetch response/error policy;
- current status acceptance semantics remain unchanged (currently exact success semantics must be confirmed from source and tests before refactor).

The extraction must not force a public `eggfetch-core` API change. Existing `Client`, `ClientBuilder`, `Proxy`, `Dialer`, request and response APIs should compile unchanged.

Delete the old duplicate helpers only after the shared primitive is in use and tests prove parity.

---

# Workstream 5 — Security and correctness test matrix

Add focused tests in the new crate, then retain/extend consumer integration tests.

## Target/request tests

- domain authority;
- IPv4 authority;
- IPv6 authority with brackets;
- explicit non-default port;
- empty host rejected;
- CR injection in host rejected;
- LF injection in host rejected;
- credential control characters rejected;
- Basic auth exact encoding;
- proxy-only headers serialized once;
- caller cannot smuggle a second `Proxy-Authorization` when auth ownership forbids it;
- request-size limit exact boundary and one-past boundary.

## Response parser tests

- `HTTP/1.1 200 Connection Established`;
- another syntactically valid 2xx response returned as status without universal policy classification;
- 403, 407, 502, 504 and arbitrary status returned intact;
- malformed status code rejected;
- missing status rejected;
- unsupported/malformed version behavior is deterministic;
- bare/truncated status line rejected;
- truncated header section rejected;
- exact maximum header count accepted;
- maximum + 1 rejected;
- exact total byte limit accepted where semantically complete;
- oversized status/header line rejected;
- non-UTF-8/obs-text header values preserved;
- duplicate headers preserved according to `HeaderMap` semantics;
- bytes sent in the same packet after `\r\n\r\n` are readable from the returned tunnel before later socket bytes.

## Secret handling tests

- `Debug` for auth-containing configuration does not reveal username/password where policy considers them secret;
- parse/write errors do not include Base64 credentials;
- invalid header errors do not echo unbounded attacker-controlled values;
- no trace/log helper in the shared crate prints raw secret headers.

Property/fuzz testing is optional if Eggfetch already has a suitable bounded parser fuzz harness. Do not add a large new fuzz/CI subsystem solely for this extraction.

---

# Workstream 6 — Packaging, documentation and release

## Package metadata

The new crate must include:

- description that identifies it as a generic async HTTP CONNECT wire primitive;
- repository/homepage/license consistent with Eggfetch;
- rustdoc example using an in-memory or Tokio stream without Eggress;
- explicit statement that it does not dial sockets or perform TLS.

Do not market it as an Eggress adapter.

## Eggfetch docs

Update only the durable architecture/dependency documentation necessary to show ownership:

```text
eggfetch-core transport/connect -> eggfetch-http-connect wire primitive
```

Do not expand the root README with internal extraction history unless project conventions require it.

## Publication

Before Eggress adopts it:

1. run normal Eggfetch checks;
2. run focused package tests with the minimum supported toolchain;
3. run `cargo package` / `cargo publish --dry-run` for the new crate;
4. ensure packaged `eggfetch-core` resolves the dependency from crates.io metadata rather than relying only on a workspace path;
5. publish the new crate dependency-first;
6. publish the next Eggfetch patch release if `eggfetch-core` must change its published dependency graph;
7. verify clean downstream resolution from a temporary project.

Do not make Eggress depend permanently on an unpublished git SHA.

---

# Required validation

Use current repository scripts if command names have changed. At minimum the equivalent of:

```sh
cargo fmt --all -- --check
cargo clippy -p eggfetch-http-connect --all-targets -- -D warnings
cargo test -p eggfetch-http-connect
cargo check -p eggfetch-core --no-default-features --features http1
cargo check -p eggfetch-core --no-default-features --features http1,tls-rustls
cargo test -p eggfetch-core
./scripts/check.sh
```

If proxy behavior is excluded from some minimal feature combinations, also run the project's existing proxy/extended qualification command that actually exercises CONNECT.

Run the workspace's Rust 1.89 MSRV gate unless the shared crate intentionally supports an earlier MSRV; in that case separately prove the crate at its declared MSRV.

---

# Acceptance criteria

- [ ] One small published Eggfetch-owned crate owns stream-level H1 CONNECT wire mechanics.
- [ ] The crate has no dependency on `eggfetch-core`.
- [ ] The crate has no Hyper client/pool, TLS, DNS or socket-creation dependency.
- [ ] The public target type formats IPv6 authority correctly.
- [ ] Request construction structurally prevents CR/LF injection.
- [ ] Basic proxy auth is supported without secret leakage.
- [ ] Response parsing does not require the entire head/header values to be UTF-8.
- [ ] Status/header/aggregate/count limits are explicit and tested at boundaries.
- [ ] Header count means actual header fields.
- [ ] Truncated status/header sections fail closed.
- [ ] Post-head read-ahead bytes are preserved.
- [ ] The shared parser returns status without imposing Eggfetch/Eggress-specific success policy.
- [ ] Eggfetch preserves its existing phase timeout/deadline semantics.
- [ ] Eggfetch preserves its route-cache/pooling/TLS behavior.
- [ ] `eggfetch-core` public API remains source-compatible.
- [ ] Eggfetch's existing CONNECT/proxy regression suite passes after the old duplicate parser/serializer is removed or reduced to adapters.
- [ ] Package dry-run and clean registry-style downstream resolution succeed.
- [ ] A published version is available for Eggress to consume.
- [ ] Closure evidence records exact Eggfetch commit and crate version.

## Non-goals

- no H2 CONNECT implementation;
- no H3/QUIC CONNECT implementation;
- no SOCKS extraction;
- no generalized proxy-chain framework;
- no Eggress types or feature names;
- no custom-dialer redesign;
- no TCP/TLS connector API;
- no retry/redirect policy;
- no body streaming abstraction;
- no attempt to replace all of `transport/proxy.rs`;
- no breaking `eggfetch-core` API cleanup bundled with this work;
- no claim that the new package reduces Eggfetch's own binary size unless measured separately.

## Handoff to Eggress

The downstream Eggress plan may begin only when the shared package's API, version and registry availability are stable enough for a normal Cargo dependency. Record the published package/version and the Eggfetch commit that passed validation, then execute `EGGFETCH_HTTP_CONNECT_INTEGRATION_AND_FOOTPRINT_CLOSURE.md` from a refreshed Eggress `main` baseline.
