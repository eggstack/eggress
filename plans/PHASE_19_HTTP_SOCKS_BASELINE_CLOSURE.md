# Phase 19 Plan: HTTP/SOCKS Baseline Closure

## Purpose

Phase 19 closes the high-value conventional proxy surface before the project invests further in long-tail protocols. The target is real pproxy compatibility for HTTP CONNECT, ordinary HTTP forward proxying, SOCKS4/SOCKS4a CONNECT, and SOCKS5 CONNECT, with differential evidence from the Phase 18 oracle harness.

This phase should make the common browser, curl, package-manager, and agent proxy use cases boringly reliable. It should also remove current ambiguity around ordinary HTTP persistence and SOCKS edge cases.

## Dependencies

Phase 19 depends on Phase 18 enough that a real pproxy differential harness exists. If Phase 18 is incomplete, implement only local refactors and synthetic tests, but do not mark compatibility complete.

## Non-goals

Do not implement Shadowsocks, standalone UDP, Trojan server mode, SSH, transparent proxying, HTTP/2, HTTP/3, QUIC, or Python API drop-in compatibility in this phase.

Do not redesign the router. Use the existing routing/upstream abstraction unless a small interface change is required for protocol-correct behavior.

## Work items

### 19.1 Ordinary HTTP forward-proxy session model

Current ordinary HTTP forwarding is single-exchange. pproxy supports persistent HTTP proxy connections in normal usage. Implement a proper per-connection loop for ordinary HTTP requests.

Requirements:

- read one request at a time from a client connection;
- support persistent connections when both client and upstream semantics permit;
- preserve correct `Connection: close` behavior;
- support request bodies with `Content-Length`;
- support chunked request bodies if currently supported in the single-exchange path;
- reject unsupported transfer codings deterministically;
- convert absolute-form proxy requests to origin-form upstream requests;
- filter hop-by-hop headers per request, not only once per connection;
- avoid cross-request header/body contamination;
- ensure upstream connection lifetime is correct for direct origin forwarding and upstream HTTP proxy forwarding.

Implementation guidance:

- Separate HTTP parsing/framing from proxy session orchestration.
- Prefer a small state machine over an ad hoc loop.
- Preserve existing limits for header size, request-line size, body size, and timeout.
- Make keep-alive behavior explicit in tests.

### 19.2 HTTP CONNECT differential closure

Expand HTTP CONNECT cases beyond byte-exact echo.

Cases:

- no-auth CONNECT success;
- Basic auth CONNECT success;
- missing auth rejection;
- wrong auth rejection;
- malformed Basic auth rejection;
- CONNECT with IPv4 target;
- CONNECT with domain target;
- CONNECT with IPv6 target if pproxy supports it in the same syntax;
- upstream connection refused;
- upstream timeout;
- client half-close after tunnel establishment;
- server half-close after tunnel establishment;
- payload relay under fragmented client writes;
- payload relay under fragmented upstream writes.

Compare:

- status code;
- reason phrase where stable;
- proxy-authenticate header presence;
- tunnel payload equality;
- close behavior.

Avoid overfitting to non-semantic header ordering unless a real client depends on it.

### 19.3 Ordinary HTTP forward-proxy differential tests

Add pproxy oracle cases for ordinary HTTP requests.

Cases:

- GET absolute-form request to local HTTP origin;
- HEAD request;
- POST with `Content-Length` body;
- chunked request body if pproxy accepts it;
- persistent connection with two sequential requests;
- client sends `Connection: close`;
- upstream sends `Connection: close`;
- malformed request;
- unsupported transfer coding;
- proxy auth success and failure.

Compare:

- origin-observed request target;
- origin-observed headers after hop-by-hop filtering;
- response status/body;
- whether the client connection remains open after the first response.

### 19.4 SOCKS4 and SOCKS4a differential closure

Add real pproxy differential coverage for SOCKS4/4a.

Cases:

- SOCKS4 IPv4 CONNECT success;
- SOCKS4 user ID propagation if observable;
- SOCKS4a domain CONNECT success;
- SOCKS4 domain-like request that should fail under pure SOCKS4;
- rejected target;
- malformed version;
- truncated request;
- unsupported command such as BIND before BIND is implemented;
- authentication/user-id edge cases if pproxy behavior is observable.

Compare:

- SOCKS4 response code;
- target observed by fixture;
- payload relay;
- close behavior on malformed input.

### 19.5 SOCKS5 differential closure

Expand SOCKS5 tests to cover negotiation and edge behavior.

Cases:

- no-auth method negotiation;
- username/password method negotiation;
- no acceptable methods;
- wrong username/password;
- IPv4 CONNECT;
- IPv6 CONNECT;
- domain CONNECT;
- malformed address type;
- unsupported command BIND;
- unsupported command UDP before entering the UDP-specific phase, if applicable;
- fragmented greeting;
- fragmented request;
- early client close during greeting;
- early client close during request;
- server half-close during tunnel.

Compare:

- selected method;
- reply code;
- bound address fields where stable;
- fixture-observed target;
- tunnel payload equality;
- close behavior.

### 19.6 SOCKS BIND decision point

Capture pproxy behavior for SOCKS4 BIND and SOCKS5 BIND. Decide whether to implement now or classify as a later phase.

If implementing:

- design listener allocation and lifecycle;
- define route interaction;
- add direct BIND support;
- add pproxy differential tests.

If deferring:

- return pproxy-compatible failure codes;
- document the deferral;
- add manifest entries showing `unimplemented` or `intentional_non_parity` but not `compatible`.

### 19.7 Mixed-protocol listener robustness

Exercise mixed HTTP/SOCKS listeners under ambiguous first bytes.

Cases:

- HTTP CONNECT and SOCKS5 on same listener;
- HTTP ordinary request and SOCKS4 on same listener;
- fragmented first byte;
- garbage first byte;
- slow client during detection;
- auth-required protocols mixed with no-auth protocols.

Requirements:

- detection timeout is explicit;
- replay buffer limits are enforced;
- errors are deterministic;
- metrics distinguish detection failures from protocol failures.

### 19.8 curl/browser/package-manager smoke tests

Add integration smoke tests or documented manual scripts for common real clients.

Suggested clients:

- `curl -x http://... https://example` via CONNECT to a local TLS fixture;
- `curl -x socks5h://...` to a local domain fixture;
- Python `urllib.request` through HTTP proxy;
- Python `requests` if optional dev dependency is acceptable;
- browser manual smoke documentation.

These smoke tests do not replace differential tests; they catch client tolerance issues.

### 19.9 Documentation and manifest updates

Update:

- `docs/PARITY_MATRIX.md` for HTTP/SOCKS compatibility evidence;
- `docs/PPROXY_PARITY_SPEC.md` for captured edge behavior;
- `docs/PPROXY_MIGRATION.md` for keep-alive and SOCKS behavior;
- README checkboxes only where evidence is present;
- `tests/compat/pproxy_manifest.toml` entries for all baseline features.

## Validation commands

Run at minimum:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test --test differential_pproxy -- --nocapture
cargo test -p eggress-protocol-http
cargo test -p eggress-protocol-socks
cargo test -p eggress-runtime
```

If smoke tests are scripted:

```bash
./scripts/smoke_http_socks.sh
```

## Acceptance criteria

Phase 19 is complete when:

- ordinary HTTP forward proxying supports persistent sessions where pproxy does;
- HTTP CONNECT compatibility has expanded differential evidence;
- ordinary HTTP forward proxying has pproxy differential evidence;
- SOCKS4/SOCKS4a have pproxy differential evidence;
- SOCKS5 negotiation and edge behavior have pproxy differential evidence;
- SOCKS BIND has an explicit implementation or deferral decision backed by captured pproxy behavior;
- mixed-protocol listener behavior remains robust under fragmented and malformed inputs;
- parity docs and manifest are synchronized with actual evidence.

## Risks

Persistent HTTP forwarding can accidentally create request smuggling or body-boundary bugs. Keep parser bounds strict, add negative tests, and avoid connection reuse across ambiguous framing.

SOCKS edge behavior may differ between pproxy versions. Record the target pproxy version in every differential result.

Mixed listener autodetection can become fragile under slow clients. Treat detection timeout and replay buffer limits as part of the protocol contract.

## Handoff notes

This phase should leave Eggress with a defensible compatibility claim for the highest-volume pproxy use cases. Later phases should not disturb this baseline without rerunning the full Phase 19 differential suite.
