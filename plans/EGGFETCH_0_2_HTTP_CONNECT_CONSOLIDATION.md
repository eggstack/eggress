# Eggfetch 0.2 HTTP CONNECT Consolidation

## Status

**READY FOR IMPLEMENTATION — 2026-09-22**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `8e18cb67d3c2b0f4e29c94b8c7ae0bdaf7caa76b`
- Workspace release line: `1.0.7`
- Upstream dependency target: `eggfetch-http-connect 0.2.0`
- Upstream coordinated release commit: `eggstack/eggfetch@8959ca890ee34f4cf456aed648315322f1e83ef7`
- Governing constraint: reduce duplicate HTTP/1 CONNECT wire ownership and maintenance burden without changing Eggress' established Rust/Python/CLI/configuration/protocol/pproxy behavior.

## Objective

Adopt only the narrow `eggfetch-http-connect 0.2.0` wire primitive where it can replace duplicated outbound HTTP/1 CONNECT mechanics without moving Eggress routing, transport, timeout, status policy, listener, forwarding, H2/H3, or chaining responsibilities into Eggfetch.

This plan does **not** authorize adoption of `eggfetch-core`.

The intended ownership after implementation is:

```text
Eggress
├─ inbound HTTP CONNECT parsing/authentication
├─ ordinary HTTP forwarding
├─ HTTP status -> HttpError policy
├─ dialing / TLS / routing / chaining
├─ H2/H3 CONNECT
├─ timeout and lifecycle policy
└─ eggfetch-http-connect
   ├─ outbound HTTP/1 CONNECT target/authority representation
   ├─ outbound CONNECT request wire serialization
   └─ bounded CONNECT response-head parsing only where fully conformant
```

A partial but clean consolidation is an acceptable result. Do not force parser reuse if the upstream primitive cannot preserve the current public Eggress limit/error contract exactly.

## Why this plan exists

Current Eggress no longer contains `reqwest`, so there is no general HTTP client dependency to replace. The remaining meaningful overlap is in `crates/eggress-protocol-http/src/connect/client.rs`, where Eggress independently owns:

- CONNECT authority formatting, including IPv6 bracketing;
- outbound CONNECT request framing;
- Basic `Proxy-Authorization` wire construction;
- response-head framing/parsing;
- response header/count/size limits;
- read-ahead preservation across tunnel establishment.

`eggfetch-http-connect 0.2.0` is intentionally a small protocol-wire crate over a caller-owned async stream. It does not dial sockets, perform TLS, own retries, decide accepted status codes, or control deadlines. That boundary is compatible with Eggress architecture and avoids importing the larger `eggfetch-core` client/routing/pooling policy layer.

The migration has three material compatibility constraints that must be handled deliberately:

1. Eggress declares Rust 1.85 while Eggfetch 0.2.0 declares Rust 1.89.
2. Eggress currently uses `base64 0.22`; `eggfetch-http-connect 0.2.0` uses `base64 0.23`.
3. Eggfetch's helper contracts are not identical to Eggress' existing contracts:
   - `basic_auth_value()` rejects `:` in usernames while current Eggress accepts it;
   - `ConnectTarget` is not a substitute for Eggress' current domain-shape validation;
   - `ConnectResponseLimits::max_headers_bytes` accounts aggregate header-field bytes, while current Eggress implementation enforces its public `HttpConnectLimits::max_headers_bytes` while reading the complete response head.

These differences are why this is a conformance migration, not a mechanical dependency bump.

---

## Governing constraints

1. Preserve the public signatures and observable behavior of:
   - `eggress_protocol_http::http_connect`;
   - `eggress_protocol_http::validate_credentials`;
   - `HttpConnectLimits`;
   - `HttpError` mapping for HTTP CONNECT failures;
   - public re-exports from `eggress-protocol-http`.
2. Do not re-export `eggfetch-http-connect` types as part of Eggress' public API.
3. Do not add `eggfetch-core`.
4. Do not move dialing, TLS, routing, chaining, retries, timeouts, metrics, or accepted-status policy into Eggfetch.
5. Do not change inbound CONNECT parsing or authentication in `connect/server.rs`.
6. Do not change ordinary HTTP forwarding, H2 CONNECT, H3/QUIC, SOCKS, Shadowsocks, Trojan, WebSocket, raw, reverse, or relay semantics.
7. Do not change pproxy compatibility tiers or manifest claims unless testing exposes a pre-existing claim defect.
8. Do not replace a behavior-preserving local implementation with an upstream helper whose accepted/rejected input domain differs.
9. No git/path dependency is allowed for the landed state. Use the published crates.io release and a normal lockfile.
10. Do not add a second Base64 major/minor line intentionally. Align Eggress to `base64 0.23` before accepting the new crate.
11. Do not add new hosted CI matrices or evidence machinery solely for this migration. Preserve the repository's lean-CI policy.
12. Performance/size measurements are qualification signals, not hard CI thresholds.
13. If exact behavior cannot be preserved through a specific Eggfetch primitive, retain the existing Eggress logic for that primitive and document the boundary.

---

## Phase 0 — Registry and baseline preflight

Before editing runtime code:

1. Verify that crates.io resolves `eggfetch-http-connect 0.2.0` without a git/path override.
2. Record the current lockfile resolution for:
   - `base64`;
   - `tokio`;
   - `thiserror`.
3. Run the focused current-state CONNECT suite and record a green baseline.
4. Run the current upstream CONNECT benchmark once for comparison.
5. Capture the current release CLI binary size and duplicate dependency report as informational baseline.

Suggested commands:

```bash
cargo test -p eggress-protocol-http --locked
cargo bench --bench http_connect_upstream

cargo tree -p eggress-protocol-http --duplicates
cargo tree -p eggress-cli --duplicates

cargo build --release -p eggress-cli --locked
# Record target/release/eggress and target/release/pproxy sizes on the same host/toolchain.
```

Do not commit generated benchmark/evidence artifacts.

### Acceptance

- [ ] `eggfetch-http-connect 0.2.0` resolves from crates.io.
- [ ] No git/path override is required.
- [ ] Existing CONNECT tests are green before migration.
- [ ] Baseline dependency and size/performance observations are recorded for the implementer/review, not checked into a new evidence framework.

---

## Phase 1 — Align the Rust compiler contract to 1.89

### Rationale

`eggfetch-http-connect 0.2.0` inherits Eggfetch's Rust 1.89 MSRV. Current Eggress declares and pins 1.85:

- root `Cargo.toml`: `workspace.package.rust-version = "1.85"`;
- `rust-toolchain.toml`: `channel = "1.85.0"`;
- `AGENTS.md`: release-contract wording names MSRV 1.85.

A dependency requiring 1.89 makes 1.85 support impossible. Do not hide this behind Cargo's resolver or a transitive build failure.

### Required implementation

1. Update the workspace Rust version to `1.89`.
2. Update `rust-toolchain.toml` to `1.89.0`.
3. Update maintained documentation/agent guidance that states the old MSRV.
4. Search for all remaining maintained `1.85`/MSRV references and classify them:
   - live release contract -> update;
   - historical phase/evidence record -> retain as historical if accurate.
5. Do not opportunistically adopt Edition 2024 or newer Rust-only syntax merely because the floor moves.
6. Do not add a hosted cross-version matrix. A single exact-toolchain qualification is sufficient for this change under current CI policy.

Required exact-floor check:

```bash
cargo +1.89.0 check --workspace --locked
```

If the project decision is to retain Rust 1.85 compatibility, stop here: `eggfetch-http-connect 0.2.0` cannot be adopted in this release line.

### Acceptance

- [ ] Root `rust-version` is 1.89.
- [ ] `rust-toolchain.toml` is 1.89.0.
- [ ] Maintained MSRV documentation agrees.
- [ ] Historical 1.85 evidence is not rewritten as though it never existed.
- [ ] `cargo +1.89.0 check --workspace --locked` passes.
- [ ] No Edition/API/capability change is bundled with the MSRV bump.

---

## Phase 2 — Converge Base64 on 0.23

### Rationale

`eggfetch-http-connect 0.2.0` has an unconditional `base64 0.23` dependency. Leaving Eggress on `base64 0.22` would create duplicate Base64 versions and undermine the dependency-consolidation goal.

### Required implementation

1. Inventory every direct Eggress use of `base64`:
   - Cargo dependency edges;
   - `Engine` imports;
   - standard/general-purpose encoder/decoder usage;
   - proxy authentication paths;
   - protocol-specific auth or framing helpers.
2. Update the workspace dependency to `base64 = "0.23"`.
3. Make only source-compatibility corrections required by the version change.
4. Preserve byte-for-byte encoding/decoding behavior and existing validation.
5. Add/retain focused tests for:
   - `user:pass` Basic auth encoding;
   - empty username/password behavior where currently accepted;
   - usernames containing `:` where current Eggress accepts them;
   - non-ASCII accepted inputs;
   - invalid Base64 decode handling;
   - credential redaction.
6. Refresh `Cargo.lock`.
7. Confirm `cargo tree --duplicates` does not retain both Base64 0.22 and 0.23 because of Eggress' own direct edges.

### Acceptance

- [ ] Workspace direct Base64 version is 0.23.
- [ ] No Eggress-auth behavior changes.
- [ ] Existing credential acceptance/rejection boundaries remain intact.
- [ ] No duplicate 0.22/0.23 Base64 lines remain due to this migration.
- [ ] Focused protocol/auth tests pass.

---

## Phase 3 — Add the narrow Eggfetch dependency

### Required dependency shape

Add `eggfetch-http-connect 0.2.0` as a workspace dependency and consume it only from `eggress-protocol-http`.

Preferred workspace declaration:

```toml
eggfetch-http-connect = "0.2.0"
```

Then in `crates/eggress-protocol-http/Cargo.toml`:

```toml
eggfetch-http-connect.workspace = true
```

Use the normal Cargo lockfile to freeze the exact artifact for the current build. Do not use a git SHA or local path in the landed tree.

### Dependency-policy checks

Because this is a dependency change, run:

```bash
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0009
```

Confirm the MIT license is accepted by current policy.

### Acceptance

- [ ] Only `eggress-protocol-http` directly consumes `eggfetch-http-connect` unless a separately justified use is found.
- [ ] `eggfetch-core` is absent from the dependency graph.
- [ ] No git/path Eggfetch dependency exists.
- [ ] Dependency/license/security checks pass under current repository policy.

---

## Phase 4 — Delegate outbound CONNECT target and request-wire ownership

This is the minimum required runtime consolidation and should land even if response-parser delegation fails its conformance gate.

### Target conversion

Replace local authority rendering with `eggfetch_http_connect::ConnectTarget` only after preserving Eggress' existing target validation.

Current Eggress rejects domain targets that are:

- empty;
- contain C0 controls, space, or DEL;
- contain `:`, `@`, or `/`.

`ConnectTarget::new()` has a different validation domain. Therefore:

1. retain a private Eggress validation adapter for the established `TargetAddr` domain;
2. after validation, construct `ConnectTarget` from the already-approved host and port;
3. let `ConnectTarget::authority()` own DNS/IPv4/IPv6 authority rendering.

Do not expose `ConnectTarget` in public Eggress signatures.

### Authentication compatibility

Do **not** blindly replace existing auth behavior with `eggfetch_http_connect::basic_auth_value()`.

Current Eggress allows `:` in usernames; the Eggfetch helper rejects it. That is an observable accepted-input difference.

Required first-pass behavior:

1. retain `validate_credentials()` exactly as a public Eggress contract;
2. retain Eggress' existing `username:password` Base64 semantics;
3. build the pre-encoded `Basic ...` value locally;
4. pass that value through `eggfetch_http_connect::ConnectRequest::proxy_authorization`.

The Eggfetch Basic helper may be used only if a complete boundary matrix later proves identical behavior or a compatibility adapter preserves every current Eggress input. No accepted input may become rejected merely to reduce a few lines of code.

### Request serialization

Use `eggfetch_http_connect::encode_connect_request()` to own:

- request line;
- generated Host header;
- IPv6 authority consistency;
- `Proxy-Authorization` header placement;
- CRLF framing;
- unsafe header framing rejection.

Eggress currently has no independently documented outbound request-head-size public limit. Before choosing `ConnectRequest::max_head_bytes`, determine the effective existing bound from URI/config credential limits.

- If a pre-existing bound exists, use the same effective bound.
- If no bound exists, do not introduce a materially smaller new limit through this migration. Use a compatibility-preserving value and document why.

Do not add extra CONNECT headers in this campaign.

### Error mapping

Map `ConnectError` into existing `HttpError` categories without:

- adding public `HttpError` variants;
- exposing raw credentials;
- exposing unbounded proxy-controlled bytes;
- changing current user-visible status/error behavior.

### Required differential request tests

For every current accepted target/auth case, compare the old expected wire contract against the new serializer:

- DNS target;
- IPv4 target;
- bracketed IPv6 wire output;
- username/password absent;
- ordinary Basic auth;
- username containing colon;
- empty username/password where accepted;
- non-ASCII credentials where accepted;
- control-character rejection;
- empty/unsafe domain rejection;
- request-line and Host authority are identical.

### Acceptance

- [ ] Production outbound CONNECT authority formatting is owned by `ConnectTarget`.
- [ ] Production outbound CONNECT request framing is owned by `encode_connect_request()`.
- [ ] Eggress public target/auth acceptance behavior is unchanged.
- [ ] `validate_credentials()` remains source/behavior compatible.
- [ ] No Eggfetch type leaks into the Eggress public surface.
- [ ] Error redaction remains intact.

---

## Phase 5 — Response-parser conformance gate

Response parsing is the one area where forced deduplication is not authorized.

### Existing Eggress contract

`HttpConnectLimits` is public and currently contains:

```rust
pub struct HttpConnectLimits {
    pub max_status_line: usize,
    pub max_headers_bytes: usize,
    pub max_header_count: usize,
}
```

The current implementation checks `max_headers_bytes` while accumulating the complete response head. This behavior is part of the de facto contract and is covered by boundary tests.

`eggfetch_http_connect::ConnectResponseLimits` instead separates:

- status-line limit;
- individual header-line limit;
- aggregate header-field bytes;
- header count.

Those accounting domains are not automatically equivalent.

### Required conformance matrix

Before replacing `read_response_status()`, build a focused differential fixture against current Eggress behavior covering:

Valid:

- 200 with no headers;
- every accepted 2xx status;
- 403 / 407 / 502 / 504;
- arbitrary other status;
- reason phrase present/absent;
- mixed-case headers;
- OWS;
- non-UTF8 obs-text in header values;
- read-ahead tunnel bytes in the same socket write;
- later tunnel bytes after the response head.

Boundaries:

- status line at and above `max_status_line`;
- one header line near `max_headers_bytes`;
- total response head at exactly the current boundary and one byte over;
- header count at and over `max_header_count`;
- truncated status line;
- truncated headers;
- malformed status;
- malformed header shape where current parser accepts/rejects it.

Errors:

- preserve `AuthRequired`, `AuthFailed`, `BadGateway`, `GatewayTimeout`, `UnexpectedStatus(code)`;
- preserve malformed/too-large/header-count categories;
- no proxy-controlled response bytes in diagnostics.

### Outcome A — full parser delegation

Use `read_connect_response_head()` in production only if the adapter can preserve the current public `HttpConnectLimits` accepted/rejected domain and error mapping without:

- changing public fields;
- silently making limits stricter or looser;
- adding a second complex local head parser around Eggfetch;
- reducing tunnel throughput through a one-byte transport shim;
- losing read-ahead bytes.

If Outcome A is viable:

1. retain a `BufReader<BoxStream>`-backed stream wrapper so read-ahead survives tunnel establishment;
2. translate Eggress limits explicitly; do not use Eggfetch defaults;
3. keep status acceptance policy in Eggress after syntactic parsing;
4. remove the old response grammar parser after differential tests pass.

### Outcome B — retain Eggress response parser

If exact conformance cannot be achieved cleanly with the 0.2.0 API, retain the existing Eggress response parser.

This is a successful implementation outcome.

Under Outcome B:

- request/authority wire ownership still moves to `eggfetch-http-connect`;
- response parsing remains local because its public limit semantics differ;
- add a concise ownership comment explaining the mismatch;
- retain differential tests so future Eggfetch versions can be reconsidered;
- do not copy/fork the Eggfetch response parser locally;
- do not add a git dependency on an unpublished upstream change.

If a small upstream enhancement would allow later exact delegation, record it separately in Eggfetch; do not block this Eggress migration or widen this plan.

### Acceptance

- [ ] A differential response matrix exists before old parser deletion.
- [ ] Outcome A is used only with exact contract preservation.
- [ ] Outcome B is explicitly accepted when exact preservation is not cleanly possible.
- [ ] No public `HttpConnectLimits` field/signature changes.
- [ ] Read-ahead tunnel bytes remain lossless.
- [ ] Status-to-`HttpError` policy remains owned by Eggress.

---

## Phase 6 — Remove only proven duplicate ownership

After Phases 4–5:

Remove local code only where the shared primitive is now the production owner.

Expected removable code under the minimum successful outcome:

- local authority formatter;
- local CONNECT request framing builder.

Conditionally removable under response-parser Outcome A:

- local response-status/head grammar parser.

Expected retained code:

- Eggress public `validate_credentials()`;
- compatibility auth adapter/encoding if required;
- `HttpConnectLimits`;
- `http_connect()` public facade;
- status/error mapping;
- buffered stream glue needed to return a `BoxStream`;
- inbound CONNECT parser/server;
- HTTP forwarder;
- H2 CONNECT;
- protocol tests that freeze Eggress behavior.

Do not delete tests merely because equivalent tests exist upstream. Keep Eggress-level tests for its public contract and adapter mapping.

### Source-ownership check

Search the production tree after cleanup for duplicate outbound HTTP/1 CONNECT framing. There should be one production request serializer: `eggfetch-http-connect`.

If response Outcome B is selected, document that response parsing remains intentionally local due to limit-contract mismatch rather than accidentally duplicated.

### Acceptance

- [ ] No second production outbound CONNECT request serializer remains in Eggress.
- [ ] No second production authority formatter remains for this path.
- [ ] Any retained response parsing is explicitly justified by conformance constraints.
- [ ] Inbound and H2/H3 implementations remain untouched unless a compile-only adapter update is required.

---

## Phase 7 — Qualification

### Focused Rust tests

At minimum:

```bash
cargo test -p eggress-protocol-http --locked
cargo test -p eggress-outbound --locked
cargo test -p eggress-embed --locked
```

Run any narrower CONNECT/chain tests first during iteration.

### Public-surface qualification

The migration must leave existing downstream-shaped imports intact. Ensure representative tests continue to compile/use:

- `eggress_protocol_http::http_connect`;
- `HttpConnectLimits`;
- `validate_credentials`;
- `ConnectRequest` inbound type;
- embed/outbound chain paths that use HTTP upstreams.

Do not add Eggfetch types to `docs/RUST_API.md` as supported Eggress API.

### Workspace gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Because the dependency/MSRV changes are part of this plan:

```bash
cargo +1.89.0 check --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0009
```

### External pproxy differential

This migration intentionally preserves a compatibility path rather than changing a claim. Still, because upstream HTTP CONNECT wire behavior is being reimplemented behind the same facade, run the existing pproxy differential suite once for closure when the oracle environment is available:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
  cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
```

Do not change the parity manifest merely because the implementation owner changed.

### Acceptance

- [ ] Focused HTTP/outbound/embed suites pass.
- [ ] Workspace format/Clippy/tests pass.
- [ ] Fuzz targets compile.
- [ ] Exact Rust 1.89 floor compiles.
- [ ] Dependency security/license checks pass.
- [ ] External pproxy differential is green for closure when run in the supported oracle environment.
- [ ] No compatibility claim changes are required.

---

## Phase 8 — Footprint and performance check

The purpose is to ensure consolidation did not accidentally make the project heavier. This is not authorization for unrelated optimization.

### Dependency graph

Compare before/after:

```bash
cargo tree -p eggress-protocol-http --duplicates
cargo tree -p eggress-cli --duplicates
```

Expected:

- `eggfetch-http-connect` appears;
- `eggfetch-core` does not;
- Base64 0.22 is gone from Eggress' direct graph;
- no new TLS/URL/ICU/HTTP-client dependency family appears.

### Binary size

Build on the same host/toolchain/profile as the baseline:

```bash
cargo build --release -p eggress-cli --locked
```

Record `eggress` and `pproxy` sizes. A small codegen fluctuation is acceptable. Investigate any material unexplained growth rather than adding a permanent byte threshold.

### HTTP CONNECT benchmark

Run:

```bash
cargo bench --bench http_connect_upstream
```

Compare:

- `open_no_auth`;
- `open_with_basic_auth`;
- `rejected_407`.

No benchmark threshold belongs in CI. A meaningful regression should be understood before closure; do not reject the consolidation over noise.

### Acceptance

- [ ] No `eggfetch-core`/URL/ICU/TLS-client stack is pulled in.
- [ ] Base64 duplication is not introduced.
- [ ] Binary-size change is measured and has no material unexplained regression.
- [ ] CONNECT benchmark change is measured and has no material unexplained regression.
- [ ] No permanent performance CI/evidence machinery is added.

---

## Phase 9 — Documentation and closure

Update maintained architecture/dependency documentation only where ownership actually changed.

At minimum review:

- `architecture/protocols-http.md`;
- `architecture/overview.md`;
- `AGENTS.md` MSRV/dependency guidance;
- `docs/ROADMAP.md`;
- `docs/RUST_API.md` only if wording about implementation ownership needs clarification;
- `docs/CI_STATUS.md` only if an actual verification-policy change was made.

Document the final response-parser outcome explicitly:

- **Outcome A:** outbound HTTP/1 CONNECT request and response wire grammar shared through `eggfetch-http-connect`;
- **Outcome B:** request/authority shared, response parser retained locally because public Eggress limit accounting is not equivalent to Eggfetch 0.2.0.

Do not add a large completion/evidence document. Update this plan's status/acceptance section and the canonical roadmap when implementation lands.

---

## Stop conditions

Stop or narrow the migration if any implementation would require:

- adopting `eggfetch-core`;
- changing the public `HttpConnectLimits` shape;
- rejecting currently accepted credentials such as colon-bearing usernames;
- changing domain/IPv6 target acceptance;
- changing status-to-`HttpError` mapping;
- weakening credential redaction;
- adding a new proxy protocol or capability;
- changing inbound CONNECT behavior;
- changing H2/H3 semantics;
- introducing a git/path-only Eggfetch dependency;
- carrying both Base64 0.22 and 0.23 because the local code was not migrated;
- adding a complex local parser solely to make the Eggfetch response parser appear reused;
- retaining Rust 1.85 as a release requirement while depending on a Rust-1.89 crate.

When a stop condition applies to response parsing only, select Phase 5 Outcome B and continue the safe request-side consolidation.

---

## Final acceptance criteria

This line of work is complete when all applicable items are true:

- [ ] Eggress' declared/pinned MSRV is consistently 1.89 and the exact floor compiles.
- [ ] Eggress direct Base64 usage is aligned to 0.23 without behavior change.
- [ ] `eggfetch-http-connect 0.2.0` is consumed from crates.io.
- [ ] `eggfetch-core` is not added.
- [ ] Existing Eggress public Rust/Python/CLI/config/protocol surfaces are unchanged.
- [ ] Outbound HTTP/1 CONNECT authority/request framing has one production owner: `eggfetch-http-connect`.
- [ ] Eggress' existing credential acceptance and redaction semantics are preserved.
- [ ] Response parsing is delegated only if `HttpConnectLimits` and error semantics are exactly preserved; otherwise intentional local retention is documented.
- [ ] Inbound CONNECT, HTTP forwarding, H2/H3, routing, TLS, chaining, timeout, and status policy remain Eggress-owned.
- [ ] Focused, workspace, fuzz-compile, dependency-policy, and exact-MSRV gates pass.
- [ ] pproxy differential closure is green in the supported oracle environment.
- [ ] Dependency graph contains no accidental heavy Eggfetch client stack or Base64 duplication.
- [ ] Binary-size and `http_connect_upstream` benchmark comparisons show no material unexplained regression.
- [ ] Canonical roadmap and maintained architecture docs describe the final ownership accurately.

## Expected implementation shape

This should remain a small dependency/conformance migration. The expected runtime changes are concentrated in:

- root `Cargo.toml` / `Cargo.lock`;
- `rust-toolchain.toml`;
- `crates/eggress-protocol-http/Cargo.toml`;
- `crates/eggress-protocol-http/src/connect/client.rs`;
- focused HTTP CONNECT tests/fixtures;
- maintained MSRV/HTTP ownership documentation.

If the implementation starts spreading broadly through runtime/server/transport crates, stop and reassess: that is evidence that the migration boundary has become too large.
