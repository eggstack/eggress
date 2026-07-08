# Track A.04: HTTP and SOCKS Exactness

## Objective

Harden the strongest current parity area—HTTP and SOCKS TCP proxy behavior—until it is defensible as the common drop-in baseline for pproxy users. Track A should convert existing broad confidence into granular differential evidence across edge cases, authentication, chaining, half-close behavior, malformed input handling, and connection lifecycle.

## Scope

This plan covers:

- HTTP CONNECT server behavior;
- ordinary HTTP forward proxy behavior;
- HTTP CONNECT upstream behavior;
- SOCKS4 server/client behavior;
- SOCKS4a domain behavior;
- SOCKS5 server/client behavior;
- SOCKS5 username/password auth;
- common HTTP/SOCKS chain combinations;
- TCP relay lifecycle and half-close behavior;
- compatibility diagnostics for unsupported BIND or other command gaps.

It does not cover SSR, SSH, QUIC/H3, PF, or legacy Shadowsocks ciphers.

## Current Strength

Existing docs and manifests already indicate differential coverage for core HTTP/SOCKS paths against `pproxy==2.7.9`. This pass should not discard that work. It should turn it into a scenario matrix, expand missing edge cases, and ensure every `drop_in` capability is backed by named oracle scenarios.

## HTTP CONNECT Test Matrix

Add or verify differential scenarios for:

- IPv4 target connect;
- IPv6 target connect;
- domain target connect;
- refused target;
- unreachable target;
- upstream timeout;
- proxy auth success;
- proxy auth failure;
- fragmented CONNECT request line;
- CONNECT request with extra headers;
- CONNECT with large post-handshake payload;
- client half-close;
- remote half-close;
- remote closes before CONNECT success;
- ensure CONNECT success is deferred until upstream connect succeeds if pproxy behaves that way.

Record exact HTTP status behavior. If egress intentionally returns a different status code or message than pproxy, classify as `compatible_with_warning`, not `drop_in`.

## HTTP Forward Proxy Test Matrix

Add or verify differential scenarios for:

- absolute-form GET;
- absolute-form HEAD;
- absolute-form POST with `Content-Length`;
- chunked request body;
- persistent connection with sequential requests;
- `Connection: close` behavior;
- hop-by-hop header stripping;
- proxy auth success/failure;
- unsupported transfer-coding rejection;
- malformed absolute URI;
- origin-form request handling if pproxy accepts it;
- host header mismatch behavior;
- response streaming with large body.

Pay special attention to persistence. If pproxy and egress differ in connection reuse or close timing, record the difference.

## SOCKS4/SOCKS4a Test Matrix

Add or verify scenarios for:

- SOCKS4 IPv4 connect;
- SOCKS4a domain connect;
- user ID handling;
- empty user ID;
- auth-like URI user behavior if pproxy maps it;
- refused target;
- malformed VN/CD fields;
- truncated handshake;
- overly long user ID/domain;
- remote close before reply;
- client half-close after connect.

SOCKS4 BIND is currently deferred. Track A should decide whether pproxy exposes BIND materially. If yes, mark as `unsupported` and Track C blocker, or implement if low-cost. Do not call it full parity while BIND remains unimplemented.

## SOCKS5 Test Matrix

Add or verify scenarios for:

- no-auth negotiation;
- username/password negotiation;
- auth success;
- auth failure;
- no acceptable methods;
- IPv4 target;
- IPv6 target;
- domain target;
- refused target;
- malformed method negotiation;
- truncated request;
- invalid address type;
- invalid command;
- CONNECT command;
- UDP ASSOCIATE command;
- BIND command behavior;
- large relay payload;
- half-close both directions.

If BIND is not implemented, return the exact pproxy-compatible unsupported-command reply where possible and classify appropriately.

## Chain Matrix

Add or verify oracle scenarios for:

- HTTP listener -> direct;
- SOCKS5 listener -> direct;
- HTTP listener -> HTTP upstream;
- SOCKS5 listener -> HTTP upstream;
- HTTP listener -> SOCKS5 upstream;
- SOCKS5 listener -> SOCKS5 upstream;
- SOCKS4a listener -> HTTP upstream;
- mixed listener -> chosen protocol -> upstream;
- two-hop `__` chain;
- three-hop `__` chain if pproxy oracle is stable.

For Track A, multi-hop TCP can remain `compatible_with_warning` or `integration` if differential test reliability is poor, but the manifest must be honest.

## Authentication Semantics

Verify URI credential mapping:

- HTTP Basic auth listener credentials;
- HTTP upstream credentials;
- SOCKS5 listener credentials;
- SOCKS5 upstream credentials;
- credentials with percent-encoded characters;
- missing password;
- empty username;
- auth in fragment form if pproxy uses `#user:pass`;
- credentials redacted from logs and generated TOML.

Ensure pproxy compatibility mode and native egress mode have the same redaction guarantees.

## Error and Timeout Semantics

Add structured tests for:

- handshake timeout;
- upstream connect timeout;
- relay idle timeout if configured;
- too-large handshake buffers;
- invalid UTF-8 in HTTP request line or domain names;
- DNS resolution failure;
- cancelled client connection during handshake.

Do not require byte-identical log output, but protocol-visible output should match pproxy for `drop_in` claims.

## Resource and Security Bounds

While pursuing compatibility, keep egress's resource-bounded posture:

- maximum request line length;
- maximum header count/size;
- maximum SOCKS domain length;
- bounded replay/sniff buffer;
- per-connection handshake timeout;
- no credential leakage in diagnostics;
- no unbounded task leaks on failed handshakes.

If pproxy accepts unbounded hostile input, egress may classify stricter behavior as `compatible_with_warning` with security rationale.

## Manifest Updates

Split broad capabilities into granular entries where needed:

- `http_connect_ipv4`
- `http_connect_ipv6`
- `http_connect_domain`
- `http_forward_get`
- `http_forward_post_content_length`
- `http_forward_chunked`
- `http_forward_persistent`
- `socks4_connect_ipv4`
- `socks4a_connect_domain`
- `socks5_auth_success`
- `socks5_auth_failure`
- `socks5_connect_ipv6`
- `socks5_udp_associate`
- `chain_http_to_socks5`
- `chain_socks5_to_http`

This granularity prevents one passing happy-path test from overclaiming full protocol parity.

## Acceptance Criteria

- HTTP/SOCKS core capabilities have named oracle scenario IDs.
- All currently claimed `drop_in` HTTP/SOCKS features have differential evidence or explicit exemption.
- Unsupported BIND behavior is manifest-backed and protocol-correct.
- Connection lifecycle and half-close behavior have tests.
- Auth and redaction are tested for listener and upstream paths.
- HTTP/SOCKS README claims are consistent with the manifest.

## Non-goals

This task does not require full UDP multi-hop parity, SSR, SSH, H2/H3, QUIC, PF, or legacy cipher implementation. It strengthens the common drop-in baseline.
