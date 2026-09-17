# Eggfetch HTTP CONNECT Consolidation Roadmap

## Status

**READY FOR IMPLEMENTATION — 2026-09-16**

## Baselines

This roadmap was written against:

- Eggress `main`: `c09f46b11e90848f29e50c9feb68c5e025f470e5` (`1.0.7` release closure complete).
- Eggfetch `main`: `0f720b4fd9efea80d180ce5942fce5e3453d8003` (`eggfetch-core 0.1.5`, release preparation).

Before implementation, re-read the current heads and revalidate every path and public API named below. Do not execute this plan mechanically if either repository has already moved the relevant ownership boundary.

## Objective

Consolidate duplicated HTTP/1.x CONNECT client framing/parsing behind a small, general-purpose Eggfetch-owned protocol primitive, then make `eggress-protocol-http` consume that primitive without turning Eggfetch into an Eggress adapter and without changing Eggress's proxy-chain architecture.

The goal is primarily maintenance consolidation and correctness hardening. Binary-size reduction is a hypothesis to measure, not a promised outcome.

The desired ownership after this work is:

```text
Eggfetch workspace
  eggfetch-http-connect      <- generic stream-level HTTP CONNECT protocol primitive
          ^          ^
          |          |
  eggfetch-core      eggress-protocol-http
  HTTP client        proxy-chain hop wrapper
```

Eggress remains responsible for:

- proxy-chain planning and hop ordering;
- `BoxStream` composition;
- inbound HTTP proxy handling;
- H2 CONNECT pooling/flow-control semantics;
- HTTP-only/absolute-form transparent stream adaptation;
- SOCKS, Shadowsocks, Trojan, WebSocket, SSH, QUIC/H3 and other hop protocols;
- DNS-rebinding and outbound policy decisions;
- Eggress-specific status/error mapping and public API compatibility.

Eggfetch remains responsible for its full HTTP client semantics. The new shared crate owns only protocol-level HTTP CONNECT request/response mechanics over an already-established asynchronous byte stream.

## Architectural finding

A direct dependency on `eggfetch-core` is not the right boundary for Eggress.

Eggress's outbound chain executor is stream-oriented: a hop receives an existing `BoxStream`, performs a protocol handshake, and returns another `BoxStream`. The HTTP hop currently calls `eggress_protocol_http::http_connect(stream, target, auth, ...)` from `crates/eggress-server/src/execute/hops.rs`.

`eggfetch-core`, by contrast, is an HTTP client engine. Even after the recent custom `Dialer` work, Eggfetch owns HTTP framing, destination TLS, retries, redirects, pooling and response semantics while the dialer controls only the physical route. Replacing Eggress's chain executor with an Eggfetch `Client` or forcing a CONNECT hop through `ClientBuilder::dialer` would invert the intended ownership and couple Eggress to unrelated HTTP-client behavior.

There is nevertheless real overlap. Both repositories independently implement:

- CONNECT authority formatting;
- `Host` construction;
- Basic `Proxy-Authorization` serialization;
- credential/header-injection validation;
- bounded response-head parsing;
- status extraction;
- preservation of bytes read after the CONNECT response head;
- malformed/truncated response handling.

Current Eggfetch code also already handles two correctness cases that should become shared behavior:

1. IPv6 authority-form targets are bracketed (`[::1]:443`). Eggress's outbound client currently formats an IPv6 `TargetHost::Ip` as `::1:443`, which is ambiguous/invalid authority syntax.
2. Eggfetch preserves non-UTF-8/obs-text header values as bytes. Eggress's outbound CONNECT client currently converts the complete response head to UTF-8 even though it only needs the status, causing unnecessary rejection of otherwise parseable responses.

Eggress also currently increments its response `header_count` for CRLF boundaries in a way that includes framing lines despite `HttpConnectLimits::max_header_count` being documented as the number of header lines. The shared parser must make the implementation match the documented contract rather than preserving that counting defect.

## Program structure

This work is intentionally split into two implementation plans plus this roadmap:

1. `EGGFETCH_SHARED_HTTP_CONNECT_PRIMITIVE_PREREQUISITE.md`
   - implemented first in `eggstack/eggfetch`;
   - creates and publishes the small reusable primitive;
   - converts Eggfetch's own CONNECT path to consume it;
   - proves no public `eggfetch-core` regression.

2. `EGGFETCH_HTTP_CONNECT_INTEGRATION_AND_FOOTPRINT_CLOSURE.md`
   - implemented second in `eggstack/eggress`;
   - adopts the published primitive behind Eggress's existing public API;
   - removes duplicated outbound H1 CONNECT mechanics;
   - measures dependency and binary impact;
   - closes only if behavior/parity remains intact.

The downstream Eggress phase must not start against an unpublished path/git dependency unless an explicitly temporary local integration branch is used for development. The final Eggress manifest should consume a normal crates.io release so normal packaging and publication remain reproducible.

## Phase 0 — Reconfirm boundaries and capture baselines

Before executable changes:

1. Confirm Eggress still routes HTTP proxy hops through `HttpHopHandler` -> `eggress_protocol_http::http_connect`.
2. Confirm `connect::client` remains public and therefore `connect::client::parse_status_code` remains source-reachable even though it is not re-exported at crate root.
3. Confirm Eggfetch's HTTP proxy CONNECT code remains under `eggfetch-core/src/transport/connect.rs` and `transport/proxy.rs` (or their current equivalents).
4. Confirm Eggfetch has not already published a stream-level CONNECT primitive.
5. Record current MSRVs and do not silently turn this consolidation into an MSRV migration.
6. Record current Eggress artifact/dependency baselines before adding the shared crate:
   - `cargo tree -p eggress-protocol-http`;
   - `cargo tree -p eggress-cli -d`;
   - stripped `release` CLI size;
   - stripped `release-small` / `release-cli-small` CLI size where the existing profiles apply.

The size data is evidence only. Do not fail the program solely because the final binary is unchanged or slightly larger if the maintenance reduction is substantial and the increase is understood and bounded.

## Phase 1 — Extract the shared primitive in Eggfetch

Execute `EGGFETCH_SHARED_HTTP_CONNECT_PRIMITIVE_PREREQUISITE.md`.

The critical design rule is that the shared crate operates on caller-supplied `AsyncRead + AsyncWrite` streams and contains no Eggress types, no routing model, no Hyper client, no DNS, no TCP dialing, no TLS ownership and no retry/redirect policy.

Eggfetch's current public APIs must remain source-compatible. The extraction should be an additive workspace/package change plus an internal refactor of `eggfetch-core`.

A new full-client feature solely for Eggress is not acceptable. A feature that pulls all of `eggfetch-core` into Eggress is not acceptable.

## Phase 2 — Publish and qualify the upstream prerequisite

The shared crate must be publishable and available through the ordinary registry before the Eggress closure commit.

At minimum:

- `cargo package` / `cargo publish --dry-run` for the new crate succeeds;
- `eggfetch-core` tests and ordinary package checks remain green;
- Eggfetch's existing HTTP proxy, HTTPS CONNECT, timeout and credential-redaction tests remain green;
- any Eggfetch patch release required to depend on the new crate is published dependency-first;
- the exact shared-crate version selected for Eggress is recorded in the downstream plan closure.

Do not publish an Eggress-specific API or mention Eggress in the shared crate's semantic contract. Eggress may appear in planning/qualification notes as one motivating consumer, not in runtime types or behavior.

## Phase 3 — Adopt in Eggress without API churn

Execute `EGGFETCH_HTTP_CONNECT_INTEGRATION_AND_FOOTPRINT_CLOSURE.md`.

The existing Eggress API remains the compatibility boundary:

```text
eggress_protocol_http::http_connect
eggress_protocol_http::validate_credentials
eggress_protocol_http::HttpConnectLimits
eggress_protocol_http::connect::client::parse_status_code
```

These remain present unless a separate semver/API plan explicitly changes them. Their implementations may delegate to the shared primitive.

Eggress-specific response classification remains Eggress-owned:

```text
2xx -> success
407 -> AuthRequired
403 -> AuthFailed
502 -> BadGateway
504 -> GatewayTimeout
other -> UnexpectedStatus
```

Do not silently adopt Eggfetch's exact-200 CONNECT acceptance if Eggfetch still uses that policy internally.

## Phase 4 — Measure and close

After migration:

1. rerun the dependency trees and stripped binary measurements captured in Phase 0;
2. record direct/transitive additions and removals;
3. verify no second incompatible version of `tokio`, `http`, `base64` or `thiserror` was introduced by the shared package;
4. run targeted CONNECT tests, chain tests, pproxy compatibility tests and normal workspace verification;
5. classify footprint result as `smaller`, `neutral/bounded`, or `larger` with actual byte deltas;
6. record the maintenance result: which Eggress client parser/serializer logic was deleted and which compatibility wrappers remain.

No README or release claim should say Eggress became smaller unless the measured artifact supports that claim.

## MSRV policy

At the planning baseline Eggress declares Rust `1.85` while Eggfetch declares Rust `1.89`.

This consolidation must not hide an MSRV increase inside a dependency refactor. The upstream shared crate should use only language/library features compatible with the organization-selected minimum and must state its `rust-version` explicitly. If Eggress has already completed an independently approved move to `1.89+` before implementation, use that current baseline. Otherwise either keep the shared primitive compatible with Eggress's declared MSRV or defer the downstream dependency until the separate MSRV migration lands.

Do not lower `eggfetch-core`'s MSRV merely for Eggress.

## Explicit non-goals

Do not use this program to:

- replace Eggress's H2 CONNECT implementation or pool;
- add H2 proxy support to Eggfetch solely for Eggress;
- consolidate SOCKS in the same change;
- move inbound CONNECT parsing/server authentication into Eggfetch;
- replace Eggress TLS transport with Eggfetch TLS types;
- change Eggfetch's custom `Dialer` contract;
- replace Eggress's `httponly` transparent stream rewriter;
- redesign `ProxyChainSpec` or the chain executor;
- introduce request/response HTTP-client abstractions into Eggress hop execution;
- add new mandatory benchmark/CI matrices;
- make binary-size claims without measurements;
- add a git dependency in the final releasable state;
- change public Eggress status/error semantics except for the explicitly identified parsing correctness fixes.

## Program acceptance criteria

The program is complete only when all of the following hold:

- [ ] A small Eggfetch-owned stream-level H1 CONNECT primitive exists and is published.
- [ ] The primitive has no dependency on `eggfetch-core`, Eggress, Hyper client routing, TLS, DNS or socket creation.
- [ ] `eggfetch-core` itself consumes the primitive for its applicable HTTP CONNECT framing/parsing rather than retaining a duplicate implementation.
- [ ] Eggfetch 0.1.x public client APIs remain source-compatible.
- [ ] Eggress consumes the same primitive behind its existing public `http_connect` surface.
- [ ] Eggress chain planning, H2 CONNECT, SOCKS and other proxy protocols are unchanged in ownership.
- [ ] IPv6 CONNECT authority formatting is correct and regression-tested.
- [ ] CONNECT response parsing accepts byte-valued headers without requiring the whole head to be UTF-8.
- [ ] Eggress's documented header-count limit counts actual header lines.
- [ ] Read-ahead/pipelined tunnel bytes are preserved in both consumers.
- [ ] Credential/header injection and secret-redaction tests pass.
- [ ] Existing Eggress 2xx/status error mapping remains intact.
- [ ] Existing Eggfetch exact CONNECT acceptance semantics remain intact unless independently changed.
- [ ] No silent MSRV change occurs.
- [ ] Before/after dependency and stripped-artifact measurements are recorded.
- [ ] Normal Eggress and Eggfetch verification passes.
- [ ] No Eggress-specific type, feature or behavioral branch appears in Eggfetch runtime code.

## Stop conditions

Stop and revise the design rather than forcing integration if any of these become true:

- the shared primitive requires pulling Hyper client/pool machinery into its dependency graph;
- Eggfetch cannot preserve phase-specific timeout ownership without reimplementing a parallel CONNECT path;
- Eggress must expose Eggfetch client types in its public API;
- using the shared crate requires an unrelated routing/TLS rewrite;
- H2 CONNECT must be redesigned to make the H1 extraction work;
- the only feasible implementation is to depend on full `eggfetch-core` from `eggress-protocol-http`.

In those cases, keeping the small duplicated H1 implementation is preferable to creating a structurally wrong shared abstraction.
