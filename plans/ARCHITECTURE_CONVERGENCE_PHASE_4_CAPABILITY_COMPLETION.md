# Architecture Convergence Phase 4 — Bounded Capability Completion

## Status

**IMPLEMENTED**

## Baseline

- Repository: `eggstack/eggress`
- Roadmap: `plans/ARCHITECTURE_CONVERGENCE_ROADMAP.md`
- Dependency: Phases 1–3 complete

## Objective

Fill two high-value capability gaps that fit the existing architecture and public surface:

1. implement listener-free outbound UDP associations using the existing UDP subsystem;
2. add TLS and optional mutual TLS to the existing native reverse control channel using the existing TLS transport stack.

This is deliberately not a general feature-expansion phase. The two items were selected because one is already represented in the public embed API and the other hardens an existing remotely exposed subsystem without introducing a new protocol family.

Do not pull MASQUE, TPROXY, certificate hot reload, Trojan UDP, new reverse protocols, or unrelated transport work into this phase.

---

## Workstream 1 — Implement listener-free outbound UDP

### Problem

The listener-free outbound API exposes UDP concepts but currently cannot establish an outbound UDP association. Eggress already has mature listener-side UDP association, routing, limits, datagram encoding/decoding, and supported upstream relay logic. The missing capability is a clean embeddable client-side facade over those primitives.

### Required product behavior

The outbound API must support at least:

- direct UDP to a target through Eggress routing semantics;
- supported upstream UDP transport(s) already implemented by the UDP subsystem, especially SOCKS5 UDP relay where the existing code provides it;
- bounded send/receive datagrams;
- target selection semantics appropriate to connected versus unconnected associations;
- timeout/cancellation;
- explicit close/shutdown;
- sync Rust facade where already consistent with `OutboundConnector`;
- Python bindings/wrappers if `UdpAssociation` is already part of the documented Python/native public surface.

Do not claim support for upstream protocols whose UDP path is not implemented by the underlying subsystem.

### Architecture direction

Prefer a listener-free object built directly over existing UDP routing/upstream abstractions rather than launching a hidden local SOCKS listener and sending packets through it.

Conceptually:

```text
OutboundConnector
      |
      v
route target/transport
      |
      +--> direct UdpSocket
      |
      +--> existing supported upstream UDP relay/session
      |
      v
UdpAssociation
  send_to / recv_from / close
```

The exact object model should follow current `eggress-embed::outbound` conventions.

### Implementation tasks

1. Inventory `eggress-udp` primitives that can be reused without listener state:
   - association registry/limits where relevant;
   - SOCKS5 UDP upstream session/codec;
   - target/address codecs;
   - routing integration;
   - DNS/rebinding/private-egress policy helpers;
   - timeout/cancellation primitives.
2. Define a minimal `UdpAssociation` API with explicit ownership and close semantics. Avoid socket-compatible methods that cannot be honored across proxy transports.
3. Add `OutboundConnector::associate_udp(...)` or complete the existing stub using compiled connector routing/upstream state.
4. Decide and document whether the association is:
   - fixed-target/connected; or
   - multi-target with `send_to`/`recv_from`.
   Prefer the form already implied by the public API and existing pproxy compatibility needs. Do not expose both unless both are needed.
5. Enforce existing UDP datagram-size and target/address validation rules.
6. Preserve DNS rebinding/private-address policy consistent with listener UDP behavior where the same policy applies.
7. Use the same upstream selection/health semantics as TCP outbound routing where applicable, with a clear failure if the selected upstream chain cannot carry UDP.
8. Return structured unsupported-composition errors rather than silently falling back direct when policy selected an unsupported upstream.
9. Ensure association drop/close releases sockets, upstream sessions, leases, registry counts, and tasks promptly.
10. Add Python/PyO3 exposure only through the existing binding/package conventions. Use the canonical async bridge established in Phase 2 for blocking operations.

### Required Rust tests

At minimum:

- direct UDP echo round trip;
- multiple datagrams over one direct association;
- target metadata/address preservation if multi-target;
- supported SOCKS5 UDP upstream round trip using an in-process test server;
- routing selection chooses the configured UDP-capable upstream;
- unsupported upstream composition produces a structured error;
- DNS failure is classified correctly;
- oversized datagrams are rejected according to configured/architectural limits;
- timeout does not leak a task/socket;
- cancellation/close is idempotent;
- dropped association decrements active association/resource accounting;
- connector reuse can create more than one independent UDP association.

### Required Python tests

If the public Python API exposes UDP associations:

- construct an association;
- direct echo send/receive;
- async send/receive does not block the event loop;
- cancellation is stable;
- close/wait-close is idempotent;
- use-after-close raises the established exception type;
- no cross-loop misuse for loop-affine async wrappers.

### Documentation

Update:

- `architecture/embed.md`;
- `architecture/udp.md`;
- Python stubs/docs if exposed;
- capability documentation only for the specific newly functional listener-free UDP surface.

### Acceptance criteria

- the existing outbound UDP stub no longer returns “not implemented” for supported direct/UDP-capable upstream configurations;
- implementation directly reuses `eggress-udp`/routing components rather than starting a hidden proxy listener;
- direct and at least one already-supported upstream UDP path have deterministic tests;
- unsupported UDP chain compositions fail explicitly;
- close/cancellation/resource lifecycle is test-backed;
- Python surface, if present, follows the same lifecycle and async bridge conventions as other outbound APIs.

---

## Workstream 2 — Secure native reverse control channels with TLS/mTLS

### Problem

The native reverse subsystem provides control-channel based reverse/backward proxying but does not expose first-class TLS/mTLS for that control connection. Reverse operation may cross untrusted networks, making transport authentication/confidentiality a high-value hardening item.

Eggress already has Rustls-based TLS infrastructure. The reverse subsystem should compose that transport rather than implement reverse-specific cryptography.

### Scope

Support TLS for the native Eggress reverse control connection. Where the existing reverse compatibility adapter must remain byte-compatible with pproxy, do not silently alter its wire protocol or force TLS unless an explicit wrapper/config already maps cleanly.

Required modes:

- plaintext existing behavior remains available where explicitly configured/currently expected;
- server-authenticated TLS;
- optional mutual TLS requiring and validating a client certificate.

Do not implement certificate issuance, ACME, secret management, dynamic certificate reload, or a private CA framework.

### Configuration design

Add a bounded TLS block to reverse server/client configuration, reusing conventions/types from listener/upstream TLS where practical without forcing incompatible abstractions.

A suitable model may include:

**Reverse server**

- enabled TLS flag implied by presence of TLS config;
- certificate PEM/path input consistent with active config conventions;
- private key;
- optional client CA roots and `require_client_cert`/equivalent;
- optional ALPN only if the reverse protocol needs a stable identifier.

**Reverse client**

- server name / SNI when not derivable safely;
- trust roots/system verifier consistent with existing TLS policy;
- optional client certificate/key for mTLS;
- no insecure verification flag in normal native configuration unless the repository already has a tightly gated test-only/insecure convention.

### Architecture direction

Wrap the control stream before reverse framing/authentication:

```text
TCP connect/accept
      |
      v
existing eggress-transport-tls client/server wrapper
      |
      v
reverse control framing + reverse auth
```

This preserves separation between transport security and reverse protocol semantics.

### Implementation tasks

1. Reuse `eggress-transport-tls` builders/wrappers for client and server sides.
2. Add compiled config representation for reverse TLS material.
3. Validate impossible combinations before runtime startup:
   - client cert without key;
   - mTLS required without client trust roots;
   - TLS client configuration missing a usable server-name/verifier context;
   - malformed PEM/key material.
4. Apply TLS before reverse application-level authentication so credentials/control frames are not sent in plaintext when TLS is configured.
5. Ensure reconnect loops recreate or reuse TLS client configuration safely without rebuilding expensive immutable config on every attempt where avoidable.
6. Preserve cancellation/reconnect/backoff behavior around TLS handshake failures.
7. Classify certificate failures separately enough for diagnostics/logs without leaking certificate/private-key material.
8. Integrate metrics only if existing reverse metrics already have a natural failure category. Do not create a large new metric taxonomy for this feature.
9. Keep pproxy-compatible reverse mode behavior unchanged unless an explicit compatible TLS wrapper is already part of the compatibility contract and can be implemented without ambiguity.

### Required tests

Use generated test certificates through existing Rust test dependencies/helpers.

At minimum:

- native reverse server + client succeeds with trusted server certificate;
- client rejects an untrusted/self-signed server when its CA is not configured;
- hostname/SNI mismatch is rejected;
- mTLS server accepts a client signed by the configured client CA;
- mTLS server rejects a client with no certificate;
- mTLS server rejects a client signed by an untrusted CA;
- reverse control reconnect still works after a transient TLS handshake failure once a valid endpoint becomes available;
- shutdown/cancellation interrupts pending TLS handshake/reconnect cleanly;
- plaintext existing native reverse tests remain passing when TLS config is absent;
- secret/debug output does not print private keys or credential material.

### Documentation

Update:

- `architecture/protocols-reverse.md`;
- `architecture/transports-tls.md`;
- configuration reference/examples;
- active capability manifest for native reverse TLS/mTLS only after tests pass.

Do not describe pproxy reverse compatibility as TLS-equivalent unless oracle/interop evidence supports that separate claim.

### Acceptance criteria

- native reverse control traffic can be protected by Rustls TLS;
- mTLS can require a trusted client certificate;
- plaintext behavior remains available/compatible when TLS is not configured;
- TLS uses shared Eggress TLS infrastructure rather than reverse-specific crypto;
- certificate validation failure cases are deterministic and tested;
- reconnect/shutdown behavior remains correct.

---

## Explicit non-goals for Phase 4

The following must not be implemented as incidental follow-up:

- CONNECT-UDP / MASQUE;
- QUIC reverse control channels;
- TLS interception/MITM;
- ACME or certificate provisioning;
- hot certificate reload;
- TPROXY;
- IPv6 transparent proxy completion;
- Trojan UDP unless separately authorized;
- arbitrary UDP-over-every-protocol adaptation;
- generalized datagram transport traits solely for theoretical future protocols;
- new proxy discovery/service mesh features.

If implementing outbound UDP exposes a missing low-level abstraction that is genuinely required for both direct and already-supported SOCKS5 UDP, add only that narrow abstraction and document why it is necessary.

---

## Phase verification

During implementation use focused UDP/reverse/TLS tests. At phase closure run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-udp
cargo test -p eggress-embed
cargo test -p eggress-protocol-reverse
cargo test -p eggress-transport-tls
cargo test -p eggress-runtime --test reverse_runtime
cargo test -p eggress-runtime --test reverse_interop
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

If Python UDP bindings are changed, build/install the wheel extension and run the Python suites from the repo root.

External pproxy interoperability is required only if this phase changes a pproxy compatibility claim. Native reverse TLS does not by itself justify upgrading pproxy parity.

## Phase acceptance criteria

Phase 4 is complete only when:

- listener-free outbound UDP works for direct routing and at least one already-supported UDP-capable upstream path;
- unsupported UDP compositions fail explicitly;
- UDP association lifecycle, timeout, cancellation, and cleanup are deterministic and tested;
- native reverse control channels support server-authenticated TLS;
- native reverse control channels optionally support mTLS;
- TLS verification failures and reconnect/shutdown behavior are tested;
- existing plaintext reverse behavior remains intact when TLS is not configured;
- no out-of-scope protocol or infrastructure expansion is introduced;
- active architecture/capability documentation accurately describes the completed surfaces.

## Closure record

- Implementation commit: `de460fc` (`convergence phase 4: listener-free outbound UDP and native reverse TLS/mTLS`), on top of `b906010` (phase 3 close).
- Final outbound UDP API (`crates/eggress-embed/src/outbound.rs`): fixed-target `UdpAssociation` with `send/recv/send_timeout/recv_timeout/close/wait_closed/is_closed/local_addr/target/relay_addr`, `OutboundConnector::associate_udp/associate_udp_timeout/active_udp_associations`. Supported: direct routing and single-hop SOCKS5 UDP relay via `open_socks5_udp_upstream` + SOCKS5 datagram codec. No hidden listener.
- Final reverse TLS/mTLS config: `[[reverse_servers.tls]]` (`cert`, `key`, optional `client_ca`, `require_client_cert`) and `[[reverse_clients.tls]]` (`ca` optional/system-roots default, required `server_name`, optional `client_cert`/`client_key` pair); compiled to `CompiledReverseServerTls`/`CompiledReverseClientTls` with PEM validation at compile; protocol types `ReverseServerTlsConfig`/`ReverseClientTlsConfig` build via shared `eggress-transport-tls` (server `with_client_ca_pem`/`with_require_client_cert`, client `with_client_cert_pem`); TLS wraps control TCP before auth; `pproxy_compat` + TLS rejected.
- Principal tests: `eggress-embed --lib udp` (12 tests: direct echo/multi, SOCKS5 echo, unsupported/multi-hop, DNS, oversized, timeout-no-leak, close idempotency, drop accounting, reuse); `eggress-protocol-reverse --test tls` (9 tests: trusted success, untrusted reject, SNI mismatch, mTLS accept/no-cert/untrusted-CA, reconnect after transient, shutdown interrupts handshake, redaction); `eggress-config --lib reverse` (12 compile/validation tests); `eggress-runtime --test reverse_runtime reverse_tls_server_and_client_spawn`; existing plaintext suites (`eggress-protocol-reverse --lib`, `reverse_runtime`, `reverse_interop` ungated) remain green.
- Explicitly unsupported UDP in listener-free surface: HTTP/SOCKS4/multi-protocol single-hop, multi-hop non-composed, composed multi-hop SOCKS5/Shadowsocks, and Shadowsocks UDP — all fail as `UnsupportedFeature`, never silent fallback. Python does not expose UDP associations (Rust-only surface, documented).
- Deferred items remained out of scope: MASQUE/CONNECT-UDP, QUIC reverse control, TLS interception/MITM, ACME/provisioning, hot certificate reload, TPROXY, IPv6 transparent completion, Trojan UDP, arbitrary UDP-over-every-protocol, generalized datagram traits, discovery/mesh. No new crates, no new hosted CI workflows, no pproxy parity upgrade (manifest notes scope native vs pproxy-wire TLS).