# Phase 5 Detailed Plan: Broader Upstream Protocol Parity

## Purpose

Phase 4 closed one-hop SOCKS5 UDP upstream relay support. Phase 5 broadens upstream protocol parity for TCP and carefully expands UDP where it is natural and bounded.

The goal is not to implement every proxy protocol at once. The goal is to move Eggress closer to `pproxy`-style practical parity while preserving the project’s architecture:

- typed protocol modules;
- explicit route and upstream capability classification;
- bounded lifecycle and metrics;
- no unsafe Rust;
- no native TLS/OpenSSL dependency;
- deterministic tests and local synthetic upstreams;
- honest docs for unsupported behavior.

This phase should be implemented in small commits. Keep each new protocol behind clear parser/handshake tests and runtime integration tests.

---

# Target protocols

Phase 5 should implement upstream parity in this priority order:

1. **HTTP upstream polish** — finalize HTTP CONNECT upstream behavior, authentication handling, error mapping, and tests.
2. **SOCKS4/SOCKS4a upstream polish** — complete SOCKS4/SOCKS4a CONNECT behavior and tests.
3. **Shadowsocks TCP foundation** — implement TCP upstream support for a small, explicit AEAD method set.
4. **Shadowsocks UDP foundation** — only after Shadowsocks TCP and cipher framing are correct.
5. **Trojan TCP foundation** — implement Trojan TCP CONNECT framing and tests.
6. **Protocol capability matrix** — make all upstream capabilities explicit for TCP and UDP.

Do not implement QUIC/MASQUE/HTTP3 in this phase. Do not add TLS-native dependencies unless there is an explicit later decision.

---

# Non-goals

Do not implement:

- HTTP/3, QUIC, MASQUE, CONNECT-UDP;
- transparent proxying;
- SSH upstream;
- VMess/VLESS;
- obfs plugins;
- Shadowsocks plugin transport modes;
- UDP fragmentation/reassembly beyond existing SOCKS5 behavior;
- chain-level UDP across mixed protocols;
- native TLS/OpenSSL;
- unsafe Rust.

---

# Current baseline assumptions

Before this phase starts, the repo supports:

- HTTP/SOCKS/SOCKS5 direct inbound handling;
- TCP upstream routing/groups/schedulers/health;
- SOCKS5 UDP ASSOCIATE direct relay;
- one-hop SOCKS5 UDP upstream relay;
- TOML config and route rules;
- metrics/admin/reload/shutdown integration.

The implementation must inspect current code before modifying it. Some TCP upstream protocol support may already exist partially. Treat this phase as gap closure plus new protocol foundations.

---

# Workstream 1: Upstream protocol capability matrix

## Problem

As protocols grow, code must not infer behavior ad hoc. A single capability matrix should answer what each upstream chain can do.

## Required model

Add or extend a capability module:

```rust
pub enum TransportCapability {
    TcpConnect,
    UdpAssociate,
}

pub enum UpstreamCapabilityResult {
    Supported,
    UnsupportedProtocol { protocol: String },
    UnsupportedChain { reason: String },
}

pub struct UpstreamCapabilities {
    pub tcp_connect: UpstreamCapabilityResult,
    pub udp_associate: UpstreamCapabilityResult,
}

pub fn classify_upstream_chain(chain: &ProxyChainSpec) -> UpstreamCapabilities;
```

Initial rules:

- Direct route: TCP and UDP direct are handled outside upstream capability.
- HTTP upstream: TCP CONNECT supported; UDP unsupported.
- SOCKS4 upstream: TCP CONNECT supported; UDP unsupported.
- SOCKS5 upstream: TCP CONNECT supported; UDP supported for one-hop only.
- Shadowsocks TCP: TCP supported after implemented; UDP unsupported until WS5.
- Shadowsocks UDP: UDP supported only after WS6 and only one-hop.
- Trojan TCP: TCP supported after implemented; UDP unsupported in this phase unless explicitly added later.
- Multi-hop: TCP may be supported if existing chain executor supports it; UDP unsupported except explicitly tested combinations.

## Required tests

- classify HTTP/SOCKS4/SOCKS5/Shadowsocks/Trojan;
- classify multi-hop TCP vs UDP;
- unsupported reason labels stable;
- docs matrix generated or manually updated from same semantics.

## Acceptance criteria

- route execution and UDP relay use a shared capability model.

---

# Workstream 2: HTTP upstream polish

## Objective

Ensure HTTP CONNECT upstream behavior is correct, authenticated, observable, and tested.

## Required behavior

For TCP CONNECT through an HTTP upstream:

- open TCP connection to HTTP proxy;
- send `CONNECT target_host:target_port HTTP/1.1`;
- include `Host` header;
- include `Proxy-Authorization: Basic ...` if credentials are configured;
- reject credentials with invalid control characters;
- parse status line and headers with bounded size;
- accept 2xx status, preferably 200 only unless docs say otherwise;
- treat 407 as authentication failure;
- map non-2xx responses to structured upstream error;
- do not log credentials;
- enforce route-open timeout across TCP connect + CONNECT response.

## Parser limits

Add or verify:

```rust
pub struct HttpConnectLimits {
    pub max_status_line: usize,
    pub max_headers_bytes: usize,
    pub max_header_count: usize,
}
```

Do not use unbounded `read_until` without maximum.

## Tests

Add a synthetic HTTP proxy test server with modes:

- CONNECT 200 success;
- CONNECT 407;
- CONNECT 403;
- malformed status;
- headers too large;
- slow response timeout;
- Basic auth success;
- Basic auth missing/wrong failure.

Runtime tests:

- TOML-configured HTTP upstream routes TCP echo;
- auth HTTP upstream routes TCP echo;
- bad auth fails with proper category/metric;
- HTTP upstream selected for UDP remains unsupported.

## Acceptance criteria

- HTTP upstream TCP behavior is deterministic and bounded.

---

# Workstream 3: SOCKS4/SOCKS4a upstream polish

## Objective

Complete SOCKS4 and SOCKS4a upstream TCP CONNECT behavior.

## Required behavior

SOCKS4 upstream:

- encode VN=4, CD=1, DSTPORT, DSTIP, USERID, NUL;
- parse VN=0, CD=90 success;
- map CD=91/92/93 to structured errors;
- support optional user ID if URI/config exposes it;
- route-open timeout covers TCP connect + handshake.

SOCKS4a upstream:

- for domain targets, encode DSTIP as `0.0.0.x` where x != 0;
- append USERID NUL then domain NUL;
- validate domain is non-empty and bounded;
- reject unsupported target forms explicitly.

## Tests

Synthetic SOCKS4 server modes:

- IPv4 CONNECT success;
- domain SOCKS4a success;
- request rejected;
- identd unavailable/user mismatch codes;
- malformed response;
- slow response timeout.

Runtime tests:

- TOML-configured SOCKS4 upstream routes TCP echo;
- TOML-configured SOCKS4a upstream routes domain target where practical;
- SOCKS4 selected for UDP is unsupported and metriced.

## Acceptance criteria

- SOCKS4/SOCKS4a upstream support is robust enough for parity claims.

---

# Workstream 4: Shadowsocks TCP foundation

## Objective

Add Shadowsocks TCP upstream support for a small, explicit AEAD method set.

## Method scope

Start with one or two modern AEAD methods only:

- `aes-128-gcm` if implemented through pure Rust crates;
- `aes-256-gcm` if implemented through pure Rust crates;
- `chacha20-ietf-poly1305` only if a pure Rust implementation is available and dependency policy permits it.

Do not implement legacy stream ciphers in this phase.

## Dependency policy

- Use pure Rust crypto crates only.
- No OpenSSL/native TLS.
- Run `cargo deny check` and document accepted crypto crates.
- Keep cipher method list explicit, not arbitrary string-to-algorithm dynamic dispatch.

## Required TCP behavior

For Shadowsocks AEAD TCP:

- derive subkey per method correctly;
- send encrypted target address header using Shadowsocks address format;
- stream encrypted payload chunks according to AEAD framing;
- enforce nonce sequence and frame size limits;
- map decrypt/auth failures to structured errors;
- route-open timeout covers TCP connect + initial encrypted header send.

## Suggested module layout

```text
crates/eggress-protocol-shadowsocks/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── method.rs
    ├── aead.rs
    ├── address.rs
    ├── tcp.rs
    ├── udp.rs   # stub or later WS
    └── error.rs
```

If a shadowsocks crate is used instead of implementing framing, wrap it behind Eggress-owned traits and tests. Do not let dependency APIs leak through routing/runtime.

## Config/URI support

Support URIs only if existing parser can represent them safely. Otherwise add TOML first.

Possible URI:

```text
ss://method:password@host:port
```

Be careful: standard Shadowsocks SIP002 URI encoding can be nuanced. If SIP002 parsing is not implemented, document the supported subset precisely.

Recommended initial TOML representation:

```toml
[[upstreams]]
id = "ss-a"
protocol = "shadowsocks"
address = "127.0.0.1:8388"
method = "aes-256-gcm"
password = "..."
```

Only add URI support after config model is clear.

## Tests

Unit tests:

- method parse;
- key derivation known vectors if available;
- address encode/decode;
- AEAD frame round trips;
- decrypt failure on tamper;
- frame size bounds.

Synthetic server tests:

- local Shadowsocks-like test server decrypts address and echoes encrypted payload;
- wrong password fails;
- unsupported method rejects config;
- oversized frame rejected.

Runtime tests:

- TOML-configured Shadowsocks TCP upstream routes TCP echo;
- health probe behavior against Shadowsocks upstream is explicit, even if health is TCP connect only initially.

## Acceptance criteria

- Shadowsocks TCP works for one explicit AEAD method with local deterministic tests.

---

# Workstream 5: Shadowsocks UDP foundation

## Preconditions

Only start after Shadowsocks TCP foundation and method/framing modules are reviewed.

## Required behavior

Shadowsocks UDP packet format differs from SOCKS5 UDP. Implement only one-hop Shadowsocks UDP upstream after TCP pieces are sound.

Behavior:

- encode target address + payload into Shadowsocks UDP packet;
- encrypt/authenticate with supported method;
- send via UDP socket to Shadowsocks server;
- receive encrypted response;
- decrypt and parse returned target/payload;
- validate target compatibility with flow target;
- integrate as `UdpFlowKind::ShadowsocksUpstream` or generic upstream enum.

## Tests

- UDP packet encode/decode round trip;
- tampered packet rejected;
- local synthetic Shadowsocks UDP server echo;
- runtime TOML-configured SS UDP upstream echo;
- unsupported method/config rejected;
- metrics/admin distinguish SS upstream from SOCKS5 upstream if labels or counters support it.

## Acceptance criteria

- Shadowsocks UDP is implemented only if it reaches the same lifecycle, metrics, and test quality as SOCKS5 UDP upstream.

---

# Workstream 6: Trojan TCP foundation

## Objective

Add Trojan TCP upstream support without adding native TLS. If TLS is required, either use rustls through pure Rust dependencies or defer Trojan.

## Required protocol facts to verify before implementation

Before coding, confirm the Trojan wire format from a primary spec or established implementation docs and write a short `docs/protocols/TROJAN.md` note. Do not rely on memory.

Expected high-level behavior:

- TLS connection to Trojan server;
- password hash line;
- command/address/port framing;
- CRLF terminators;
- then bidirectional stream.

If TLS/rustls setup is too large, split Trojan into a separate later phase.

## Dependency policy

- Prefer `rustls` stack if TLS is implemented.
- No OpenSSL/native-tls.
- Validate `cargo deny` compatibility.

## Tests

- frame encoder tests;
- synthetic Trojan server if TLS can be locally generated with rustls test certs;
- wrong password failure;
- malformed response handling;
- runtime TCP echo through Trojan upstream.

## Acceptance criteria

- Trojan is either completed with pure Rust TLS and tests, or explicitly deferred with documentation. Do not half-claim support.

---

# Workstream 7: Routing, metrics, and admin integration

## Required updates

For every new protocol:

- capability matrix updated;
- route-open errors categorized;
- metrics counters increment on success/failure;
- admin upstream list exposes protocol and capability summary;
- docs matrix updated;
- config validation rejects unsupported combinations.

## Metrics

Add or verify bounded labels:

```text
eggress_upstream_open_total{protocol,outcome}
eggress_upstream_open_failures_total{protocol,reason}
eggress_unsupported_transport_total{protocol,transport,reason}
```

Avoid target/source/user labels.

## Admin

`/-/upstreams` should expose safe metadata:

```json
{
  "id": "ss-a",
  "protocols": ["shadowsocks"],
  "tcp_connect": "supported",
  "udp_associate": "supported|unsupported",
  "health": "healthy"
}
```

No credentials.

## Acceptance criteria

- adding a protocol does not require custom admin/metrics hacks.

---

# Workstream 8: Documentation and examples

## Update README checklist

Add a capability table:

| Upstream protocol | TCP CONNECT | UDP relay | Phase |
|---|---:|---:|---|
| Direct | yes | yes | 3 |
| HTTP CONNECT | yes | no | 5 |
| SOCKS4/SOCKS4a | yes | no | 5 |
| SOCKS5 | yes | one-hop yes | 4 |
| Shadowsocks | TCP planned/yes | UDP planned/yes | 5 |
| Trojan | planned or deferred | no | 5/deferred |

## Add examples

- HTTP upstream with auth;
- SOCKS4a upstream;
- Shadowsocks TCP upstream;
- Shadowsocks UDP upstream if implemented;
- policy route selecting protocol-specific group;
- unsupported UDP through HTTP example and expected metric.

## Protocol docs

Create:

```text
docs/protocols/HTTP_CONNECT.md
docs/protocols/SOCKS4.md
docs/protocols/SHADOWSOCKS.md
docs/protocols/TROJAN.md   # only if implemented or explicitly deferred
```

Each should state supported subset, limitations, and test coverage.

---

# Recommended commit sequence

## Commit 1: Capability matrix

- Add shared upstream capability classifier.
- Convert UDP code to use it or wrap existing capability function.
- Add tests and docs matrix skeleton.

## Commit 2: HTTP upstream polish

- Bound parser and auth handling.
- Add synthetic server tests.
- Add runtime TCP echo via HTTP upstream.

## Commit 3: SOCKS4/SOCKS4a upstream polish

- Add/complete encoder/parser.
- Add synthetic server tests.
- Add runtime TCP echo tests.

## Commit 4: Shadowsocks protocol crate and TCP method foundation

- Add crate/module.
- Implement method/address/AEAD framing for one method.
- Add unit tests and vectors.

## Commit 5: Shadowsocks TCP runtime integration

- Add config parsing/validation.
- Add synthetic server and runtime echo test.
- Add metrics/admin support.

## Commit 6: Shadowsocks UDP foundation

- Implement UDP packet encode/decode and one-hop upstream flow.
- Add synthetic UDP server and runtime UDP echo test.

## Commit 7: Trojan decision and/or implementation

- Write protocol note.
- Either implement pure-Rust TLS Trojan TCP with tests or explicitly defer.

## Commit 8: Docs, examples, and completion record

- README table.
- Protocol docs.
- Completion doc.
- Run final verification.

---

# Required tests

## Unit tests

- capability matrix for all protocols;
- HTTP CONNECT request/response parser bounds;
- HTTP Basic auth header generation without leaking secrets;
- SOCKS4/SOCKS4a request encoding;
- Shadowsocks method parse/key/address/framing;
- Trojan frame encoding if implemented.

## Integration tests

- TCP echo through HTTP upstream;
- TCP echo through authenticated HTTP upstream;
- TCP echo through SOCKS4 upstream;
- TCP echo through SOCKS4a upstream with domain target;
- TCP echo through Shadowsocks upstream;
- UDP echo through Shadowsocks upstream if implemented;
- unsupported UDP through HTTP/SOCKS4 metriced;
- routing group fallback behavior unchanged;
- reload applies new upstream protocol config to new sessions only.

---

# Verification commands

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Focused checks:

```bash
cargo test -p eggress-uri
cargo test -p eggress-routing capability
cargo test -p eggress-protocol-socks
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-runtime upstream
```

If external interop gates are added:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_pproxy_upstreams
```

No test may depend on public internet.

---

# Definition of done

Phase 5 is complete only when:

1. Upstream protocol capability classification is centralized and tested.
2. HTTP CONNECT upstream TCP behavior is bounded, authenticated, and runtime-tested.
3. SOCKS4/SOCKS4a upstream TCP behavior is bounded and runtime-tested.
4. Shadowsocks TCP supports at least one explicit modern method with deterministic local tests.
5. Shadowsocks UDP is either implemented to the same standard or explicitly deferred.
6. Trojan is either implemented with pure Rust dependencies and tests or explicitly deferred.
7. Metrics and admin expose protocol capability and failures without credential/target leakage.
8. Config validation rejects unsupported protocol/transport combinations.
9. README and protocol docs accurately describe the supported subset.
10. All tests, lint, audit, and applicable interop checks pass.
11. No unsafe Rust, OpenSSL dependency, or native dependency is introduced.

## Completion record

When complete, add:

```text
docs/PHASE_5_UPSTREAM_PROTOCOL_PARITY_COMPLETION.md
```

with commit list, supported protocol matrix, limitations, and verification output.
