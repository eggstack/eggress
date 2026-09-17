# Eggfetch HTTP CONNECT Integration and Footprint Closure

## Status

**READY FOR IMPLEMENTATION — 2026-09-16**

## Target repository

```text
eggstack/eggress
```

Planning baseline:

```text
c09f46b11e90848f29e50c9feb68c5e025f470e5
```

At this baseline Eggress `1.0.7` is released and the repository's prior closure work is complete.

Parent roadmap:

- `plans/EGGFETCH_CONNECT_CONSOLIDATION_ROADMAP.md`

Required upstream prerequisite:

- `plans/EGGFETCH_SHARED_HTTP_CONNECT_PRIMITIVE_PREREQUISITE.md`

This plan must not begin its final dependency/closure phase until the generic Eggfetch-owned stream-level HTTP CONNECT package described by the prerequisite is published and resolvable through the normal Cargo registry.

## Objective

Replace Eggress's duplicated outbound HTTP/1.x CONNECT request serializer, response-head parser, buffering wrapper and authority formatting with the published generic Eggfetch CONNECT primitive while preserving Eggress's existing proxy-chain architecture and public API.

This is a maintenance-consolidation pass. It is not a migration of Eggress onto `eggfetch-core`, not a replacement of the chain executor, and not an HTTP-client redesign.

The intended post-change path is:

```text
HttpHopHandler
    -> eggress_protocol_http::http_connect(...)       existing public boundary
        -> adapt Eggress TargetAddr/limits/auth
        -> eggfetch-http-connect stream primitive
        -> classify returned status with Eggress policy
        -> return BoxStream
```

The generic primitive owns wire mechanics. Eggress continues to own routing, protocol selection, status policy, errors and stream-chain semantics.

---

# Current ownership that must be preserved

At the planning baseline:

- `crates/eggress-server/src/execute/hops.rs` uses `HttpHopHandler` for `ProtocolSpec::Http` and delegates the handshake to `eggress_protocol_http::http_connect`.
- `crates/eggress-protocol-http/src/connect/mod.rs` publicly exposes the `client` module and re-exports `http_connect`, `validate_credentials` and `HttpConnectLimits`.
- `crates/eggress-protocol-http/src/connect/client.rs` currently owns outbound H1 CONNECT serialization/parsing and exposes `parse_status_code` through the public `connect::client` module.
- `crates/eggress-protocol-http/src/h2_connect.rs` separately owns HTTP/2 CONNECT client/server stream handling and pooling.
- `crates/eggress-protocol-http/src/connect/server.rs` separately owns inbound H1 CONNECT request parsing/authentication.

Only the outbound H1 client mechanics are in scope.

Do not merge inbound and outbound CONNECT parsing merely because they share tokens such as `CONNECT`, `Host` or Basic auth. Their trust direction, error responses and lifetime semantics are different.

---

# Workstream 0 — Refresh baseline and capture evidence before editing

Before changing manifests or source:

1. Fetch current Eggress `main` and confirm the outbound path still reaches `eggress_protocol_http::http_connect`.
2. Read the current published API of the Eggfetch shared CONNECT crate. Do not code against a planning sketch if the upstream implementation chose different names.
3. Confirm the shared crate is published on crates.io and record its exact selected version.
4. Confirm the shared crate's declared MSRV is compatible with Eggress's current declared MSRV. If not, stop until an independently approved Eggress MSRV migration has landed.
5. Capture the current public signatures and behavior of:
   - `http_connect`;
   - `validate_credentials`;
   - `HttpConnectLimits`;
   - `connect::client::parse_status_code`.
6. Capture existing targeted tests for 2xx success, 403/407 mapping, malformed status, Basic auth, timeout wrapping and buffered post-CONNECT bytes.
7. Capture dependency/artifact baselines before adding the new package.

Recommended baseline commands, adjusted to current repository scripts/profile names if needed:

```sh
cargo tree -p eggress-protocol-http
cargo tree -p eggress-cli -d
cargo build -p eggress-cli --release
```

Also build the repository's existing size-optimized CLI profile if one remains supported. Record target triple, rustc/cargo versions, linker if materially relevant, profile, raw file size and stripped file size.

Do not use a dirty tree as the only baseline measurement.

---

# Workstream 1 — Add the shared dependency at the narrowest layer

Add the published shared CONNECT crate to `egress-protocol-http`, not to `egress-server`, `egress-runtime`, the CLI or the workspace root as a conceptual owner.

Use the workspace dependency table if that is the repository convention, but ownership remains:

```text
eggress-protocol-http -> eggfetch HTTP CONNECT primitive
```

Do **not** depend on `eggfetch-core`.

Do **not** enable unrelated Eggfetch HTTP client, TLS, proxy, JSON, compression, cookie, H2 or H3 features.

If the shared primitive offers optional features, select only those required for the stream-level H1 CONNECT contract. Prefer `default-features = false` when the published package's defaults would otherwise pull unrelated capabilities.

After manifest changes run `cargo tree -p eggress-protocol-http -e features` and verify the dependency graph does not contain `eggfetch-core`, Hyper client/pool, Rustls or resolver/socket dependencies introduced solely through this integration.

## Dependency-reduction expectations

Do not assume this change will delete obvious Eggress dependencies.

At the planning baseline H2 CONNECT still uses `base64`, `bytes`, `h2`, Tokio and HTTP types, so those dependencies remain necessary even if the H1 client stops using them directly. The primary expected reduction is locally maintained H1 wire code, not package count.

Remove a direct dependency only when `cargo tree` and source search prove no remaining Eggress code needs it.

---

# Workstream 2 — Preserve the Eggress public compatibility facade

The following surfaces must remain source-compatible unless a separate semver/API plan explicitly authorizes change:

```rust
eggress_protocol_http::http_connect(
    stream: BoxStream,
    target: &TargetAddr,
    auth: Option<(&str, &str)>,
    limits: &HttpConnectLimits,
) -> Result<BoxStream, HttpError>
```

```rust
eggress_protocol_http::validate_credentials(value: &str) -> Result<(), HttpError>
```

```rust
pub struct HttpConnectLimits {
    pub max_status_line: usize,
    pub max_headers_bytes: usize,
    pub max_header_count: usize,
}
```

and the currently public module-level fuzz/helper API:

```rust
eggress_protocol_http::connect::client::parse_status_code(
    response: &str,
    limits: &HttpConnectLimits,
) -> Result<u16, HttpError>
```

`parse_status_code` does not need to remain the production parser internally. It may become a small compatibility/fuzz helper implemented using equivalent validation logic. Do not remove it accidentally merely because the shared parser works on bytes or typed response metadata.

`HttpConnectLimits::default()` must remain behaviorally compatible unless this plan explicitly identifies and tests a correctness fix. Baseline values are:

```text
max_status_line   = 1024
max_headers_bytes = 32768
max_header_count  = 100
```

If the upstream primitive also supports a per-header-line limit that Eggress did not previously expose, map it conservatively so it does not create an undocumented tighter limit. Do not add a new public field to `HttpConnectLimits` casually because struct-literal construction by downstream Rust callers is source-sensitive.

---

# Workstream 3 — Implement a thin Eggress adapter

Refactor `crates/eggress-protocol-http/src/connect/client.rs` so `http_connect` becomes primarily an adapter and Eggress status classifier.

## 3.1 Target conversion

Convert `TargetAddr` into the shared crate's logical target type without preformatting authority strings when possible.

Required behavior:

- domain -> `domain:port`;
- IPv4 -> `addr:port`;
- IPv6 -> `[addr]:port`.

The IPv6 case is an intentional correctness repair relative to the current outbound client, which formats IP literals uniformly and therefore emits ambiguous unbracketed IPv6 authority text.

Add a wire-level regression test that observes the request line and `Host` value, not merely the target object's formatted display.

## 3.2 Credential validation/auth conversion

Keep `validate_credentials` available with its established Eggress behavior: reject control bytes below space and DEL while allowing ordinary printable input.

If the shared primitive offers a stricter or differently typed validation function, adapt carefully rather than silently changing what the Eggress public helper accepts.

For `http_connect`, credentials must be validated before any CONNECT request bytes are written. Conversion into the shared auth type must not cause credentials or Base64 output to appear in `Debug`, `Display`, errors or tracing.

## 3.3 Limits conversion

Translate `HttpConnectLimits` into the shared parser's explicit limits.

Preserve the Eggress total/status defaults.

Correct one documented-contract bug while migrating: `max_header_count` is documented as header lines excluding the status line. The production parser must count actual header fields, not CRLF framing boundaries or the terminating empty line.

Regression tests must prove:

- exactly 100 headers are accepted with default count limits when all other limits permit the head;
- 101 headers are rejected;
- status line and terminal CRLF are not counted as fields.

Treat this as a correctness fix, not a compatibility target for the old erroneous counting behavior.

## 3.4 Write/read phases and cancellation

Eggress currently relies on its callers/outer executor for timing; the H1 helper itself does not own a full retry/deadline system.

Use the shared primitive in a way that remains cancellation-safe under existing outer `tokio::time::timeout` usage. Do not add hidden retries or its own unrelated timeout defaults.

If the shared crate exposes separate `write_connect_request` and `read_connect_response` calls, using both is acceptable. If it exposes an `establish` convenience, use it only if it preserves the same cancellation/read-ahead semantics and does not impose Eggfetch policy.

## 3.5 Stream ownership/read-ahead

Return a `BoxStream` whose first reads expose any bytes already buffered beyond the CONNECT response terminator before reading later bytes from the underlying transport.

This is a hard invariant for chained protocols and clients that pipeline tunnel bytes with the CONNECT response.

The old local `BufferedStream` should be deleted if the shared tunnel wrapper fully owns this responsibility. Do not keep a second redundant buffering layer unless required by a documented adapter mismatch.

---

# Workstream 4 — Preserve Eggress-specific status policy and errors

The shared primitive must return protocol status without deciding application success. Eggress must keep its current policy:

```text
200..=299 -> success
407       -> HttpError::AuthRequired
403       -> HttpError::AuthFailed
502       -> HttpError::BadGateway
504       -> HttpError::GatewayTimeout
other     -> HttpError::UnexpectedStatus(code)
```

Do not inherit Eggfetch's own CONNECT success/retry classification if it differs.

Map shared protocol/parser errors into existing `HttpError` variants with enough fidelity for current callers/tests. Avoid adding a public Eggfetch error type to Eggress's API.

Expected mapping principles:

- I/O -> existing Eggress I/O path;
- aggregate/status/header size failures -> `HeaderTooLarge` or the closest existing bounded parse error consistent with current API;
- header-count overflow -> `TooManyHeaders`;
- malformed/truncated response -> `MalformedResponse`;
- invalid local credentials -> `InvalidCredentials`.

If the shared primitive distinguishes additional safe categories, preserve detail in bounded error text only where doing so cannot leak proxy-controlled or secret values.

Do not add a large new Eggress error taxonomy solely to mirror the upstream crate.

---

# Workstream 5 — Delete only superseded H1 client mechanics

After the adapter is proven, remove local code that is now genuinely owned upstream, expected to include equivalents of:

- manual CONNECT request `String` construction;
- local Basic auth encoding in the H1 client path;
- H1 response-head byte loop;
- whole-head UTF-8 conversion;
- local H1 response header counting;
- `BufferedStream` if the shared returned tunnel handles read-ahead;
- production use of the old string-based status parser.

Retain:

- public wrappers required for source compatibility;
- `HttpConnectLimits` as the Eggress-facing configuration type;
- Eggress `HttpError` status classification;
- inbound `connect/server.rs`;
- H2 CONNECT implementation and pool;
- HTTP forward/httponly code;
- test fixtures still useful for Eggress integration behavior.

Do not chase maximum line deletion into unrelated forwarding/server code.

---

# Workstream 6 — Required correctness regression matrix

Add/retain local Eggress tests proving behavior at the actual public boundary.

## Request wire tests

- domain authority and Host agree;
- IPv4 authority and Host agree;
- IPv6 is bracketed in request line and Host;
- non-default ports preserved;
- Basic auth succeeds with ordinary credentials;
- credential control bytes fail before writing request bytes;
- no credential value appears in error/debug output.

## Response/status tests

- 200 succeeds;
- at least one non-200 2xx succeeds, preserving Eggress any-2xx policy;
- 403 -> `AuthFailed`;
- 407 -> `AuthRequired`;
- 502 -> `BadGateway`;
- 504 -> `GatewayTimeout`;
- arbitrary non-2xx -> `UnexpectedStatus`;
- malformed status rejected;
- truncated response head rejected;
- overlong status rejected with Eggress configured limit;
- total response-head limit enforced;
- exactly max header count accepted;
- max + 1 rejected;
- non-UTF-8/obs-text header value does not cause the entire response to be rejected solely for UTF-8;
- bytes after `\r\n\r\n` remain readable from returned `BoxStream`.

## Chaining tests

At least one integration test must prove a normal HTTP hop remains usable inside the existing chain executor, not merely by calling the helper directly.

Where current fixtures permit, retain coverage for HTTP -> another stream protocol or HTTP -> target relay. The objective is to catch ownership/buffering mismatches that unit tests cannot.

## Compatibility helper tests

Keep tests for `parse_status_code` and `validate_credentials` so public helper behavior does not disappear unnoticed during refactor.

---

# Workstream 7 — Keep H2 and other protocols isolated

No executable changes should be required in the H2 CONNECT implementation except imports/dependency cleanup proven safe by compiler/search.

Specifically preserve:

- `H2StreamRead` / `H2StreamWrite`;
- `H2PoolKey` hop-index isolation;
- `H2PoolGuard` lifetime coupling;
- GOAWAY/pool/flow-control metrics;
- Basic auth semantics for H2 CONNECT;
- H2-specific tests.

Because H2 CONNECT still uses several dependencies that the old H1 implementation also used, do not remove shared manifest dependencies merely because their H1 references vanished.

No SOCKS, Shadowsocks, Trojan, WebSocket, SSH, QUIC/H3 or `httponly` handler should be rewritten in this pass.

---

# Workstream 8 — Packaging and release qualification

The final Eggress tree must be packageable from crates.io dependencies alone.

Run at least:

```sh
cargo package -p eggress-protocol-http --allow-dirty
cargo publish -p eggress-protocol-http --dry-run --allow-dirty
```

Use the repository's established clean-package workflow instead of `--allow-dirty` where practical; the examples above indicate the package-level checks required, not permission to weaken final release verification.

Inspect the generated package manifest and ensure the Eggfetch shared dependency resolves to a published version, not a workspace-only path or git SHA.

If the next Eggress release is prepared as part of implementation, follow the existing operator-driven release process. Do not introduce a new publishing workflow for this dependency change.

Do not hard-code a future Eggress version in this plan. Reconfirm the next available patch/minor version at release time.

---

# Workstream 9 — Footprint and dependency closure

Repeat the exact baseline measurements from Workstream 0 after migration using the same toolchain, target and profile.

Record at minimum:

```text
metric                         before       after       delta
----------------------------------------------------------------
eggress-protocol-http tree     ...          ...         ...
eggress-cli duplicate tree     ...          ...         ...
release raw bytes              ...          ...         ...
release stripped bytes         ...          ...         ...
size-optimized raw bytes       ...          ...         ...
size-optimized stripped bytes  ...          ...         ...
```

Also record:

- new direct package(s);
- new unique transitive packages;
- duplicate version families introduced/removed;
- local H1 client code removed;
- any direct dependencies that became removable, if any.

Classify the result honestly:

### Smaller

Measured artifact decreases and the dependency explanation supports the result.

### Neutral / bounded

Artifact change is negligible or modestly larger, but the shared protocol ownership removes meaningful duplicated maintenance without introducing heavy dependencies.

### Reject / redesign

The supposedly small primitive unexpectedly pulls substantial client/TLS/routing machinery into Eggress, creates material duplicate dependency families, or causes a binary increase disproportionate to the maintenance benefit.

There is no arbitrary byte threshold in this plan. Explain the cause and use engineering judgment. A few kilobytes from a tiny generic parser package is different from hundreds of kilobytes of accidentally imported client machinery.

Do not claim a footprint win based on source-line deletion or manifest appearance.

---

# Workstream 10 — Documentation and closure record

Update durable architecture/dependency documentation only where the ownership change matters.

At minimum ensure maintainers can discover that:

- H1 outbound CONNECT wire mechanics are shared with the Eggfetch protocol crate;
- Eggress still owns H2 CONNECT and proxy-chain behavior;
- the dependency is not `eggfetch-core` and does not make Eggress an Eggfetch HTTP-client consumer.

Avoid adding extraction history to user-facing README material unless users need it.

Append a closure record to this plan with:

- Eggress implementation commit;
- exact shared crate package/version;
- Eggfetch prerequisite implementation commit/release;
- MSRV result;
- focused test results;
- normal workspace/CI result;
- package/dry-run result;
- dependency tree before/after summary;
- binary-size before/after measurements;
- lines/modules deleted or retained at a responsibility level;
- any intentionally deferred follow-up.

---

# Required validation

Use the current repository's canonical scripts where they supersede these examples. At minimum run the equivalent of:

```sh
cargo fmt --all -- --check
cargo clippy -p eggress-protocol-http --all-targets -- -D warnings
cargo test -p eggress-protocol-http
cargo test -p eggress-server
cargo check --workspace
cargo test --workspace
cargo deny check
cargo audit
```

Also run the existing pproxy compatibility/feature checks that cover outbound HTTP chains. Do not expand routine CI merely for this pass; execute specialized parity suites locally/release-time according to existing repository policy.

If the repository has a minimum-Rust CI/helper, run it after the dependency lands and prove the shared crate does not silently raise Eggress's MSRV.

---

# Acceptance criteria

- [ ] Final Eggress depends on the published stream-level Eggfetch CONNECT primitive, not `eggfetch-core`.
- [ ] The dependency is introduced at `eggress-protocol-http`, not higher orchestration layers.
- [ ] Existing `http_connect` signature remains source-compatible.
- [ ] Existing `HttpConnectLimits` public shape/defaults remain source-compatible.
- [ ] Existing `validate_credentials` remains available with compatible behavior.
- [ ] Existing `connect::client::parse_status_code` remains available for current callers/fuzzing.
- [ ] `HttpHopHandler` and chain executor architecture remain unchanged except for delegation internals.
- [ ] IPv6 outbound CONNECT emits bracketed authority-form syntax.
- [ ] Response parsing no longer rejects an otherwise parseable CONNECT response solely because a header value contains non-UTF-8 bytes.
- [ ] Header-count semantics match the documented number of actual header fields.
- [ ] Exactly-limit and one-over-limit tests exist.
- [ ] Read-ahead bytes after the response head are preserved.
- [ ] Eggress retains any-2xx CONNECT success policy.
- [ ] 403/407/502/504 mappings remain unchanged.
- [ ] Shared primitive errors do not escape as public Eggfetch types from Eggress APIs.
- [ ] H2 CONNECT implementation/pool ownership is unchanged.
- [ ] No unrelated proxy protocol is refactored.
- [ ] No hidden retry, TLS, DNS or socket ownership is introduced.
- [ ] No silent Eggress MSRV increase occurs.
- [ ] Package/dry-run succeeds against registry-resolvable dependencies.
- [ ] Before/after dependency and stripped binary measurements are recorded.
- [ ] Any footprint claim matches measured evidence.
- [ ] Normal workspace, security and relevant pproxy compatibility verification passes.

## Explicit non-goals

- no direct `eggfetch-core` dependency;
- no replacement of Eggress routing/chain execution with Eggfetch `Client`/`Dialer`;
- no H2 CONNECT consolidation;
- no SOCKS consolidation;
- no inbound CONNECT server extraction;
- no TLS transport replacement;
- no public API cleanup bundled with the migration;
- no new feature matrix or benchmark infrastructure;
- no release-process redesign;
- no claim that Eggress is smaller unless the binary measurement proves it.

## Stop conditions

Stop and revise rather than forcing completion if:

- the published shared crate pulls Hyper client/TLS/resolver machinery into Eggress;
- the shared API cannot preserve buffered post-head bytes on arbitrary `BoxStream` inputs;
- adapting to the shared crate requires changing Eggress's public `http_connect` contract;
- Eggress's existing any-2xx policy cannot be retained independently of the shared parser;
- H2 CONNECT must be redesigned for the H1 integration to compile;
- the shared crate's MSRV exceeds Eggress's approved MSRV and no separate migration has landed;
- package publication would require a git/path-only dependency in the released manifest.

In those cases, retain the current small local implementation until the shared boundary can be corrected upstream.
