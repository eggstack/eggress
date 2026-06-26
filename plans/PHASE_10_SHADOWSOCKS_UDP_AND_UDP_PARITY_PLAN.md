# Phase 10 Detailed Plan: Shadowsocks UDP and Broader UDP Parity

## Purpose

Phase 10 implements interoperable Shadowsocks UDP and closes the remaining UDP parity questions identified in the pproxy parity specification. It builds on Phase 9 Shadowsocks TCP primitives and must not proceed until the AEAD method/key/salt/nonce foundations are correct.

UDP parity is stricter than packet encode/decode: it requires association lifecycle, routing semantics, target-flow cleanup, metrics, and compatibility tests.

---

# Prerequisites

Required from Phase 9:

- supported Shadowsocks methods defined;
- key derivation and salt handling implemented;
- method parsing and validation;
- docs/protocols/SHADOWSOCKS_PARITY.md established;
- capability matrix updated for TCP status.

If Phase 9 is incomplete, do not mark Shadowsocks UDP compatible.

---

# Non-goals

Do not implement:

- new non-Shadowsocks protocols;
- obfs/plugin transports;
- legacy weak ciphers unless approved;
- arbitrary multi-hop UDP until WS10.5 decision;
- transparent UDP;
- multicast/broadcast forwarding;
- Python bindings.

---

# Workstream 1: Shadowsocks UDP packet specification

## Required output

Extend:

```text
docs/protocols/SHADOWSOCKS_PARITY.md
```

or create:

```text
docs/protocols/SHADOWSOCKS_UDP_PARITY.md
```

Document:

- packet layout;
- salt placement;
- key derivation;
- address encoding;
- payload encryption;
- response decoding;
- target validation;
- max datagram size;
- supported methods;
- unsupported behavior.

## Acceptance criteria

- Spec is clear enough to implement an independent decoder.

---

# Workstream 2: UDP encode/decode implementation

## Target crate

```text
crates/eggress-protocol-shadowsocks/
```

Suggested module:

```text
src/udp.rs
```

## Required API

```rust
pub fn encode_udp_packet(
    method: CipherMethod,
    password: &str,
    target: &TargetAddr,
    payload: &[u8],
    rng: impl RngCore,
) -> Result<Vec<u8>, ShadowsocksError>;

pub fn decode_udp_packet(
    method: CipherMethod,
    password: &str,
    packet: &[u8],
) -> Result<(TargetAddr, Vec<u8>), ShadowsocksError>;
```

Exact API may differ, but it must support deterministic tests with injected salt/rng.

## Required tests

- IPv4 encode/decode round-trip;
- IPv6 encode/decode round-trip;
- domain encode/decode round-trip;
- wrong password fails;
- tampered salt/ciphertext/tag fails;
- overlong domain rejects;
- oversized datagram rejects;
- zero-length payload decision documented and tested;
- supported method matrix tests.

## Acceptance criteria

- UDP packet helpers are interoperable with the documented format.
- No encode/decode test relies only on the same opaque helper without inspecting structure.

---

# Workstream 3: Shadowsocks UDP upstream flow

## Goal

Route SOCKS5 UDP ASSOCIATE datagrams through a one-hop Shadowsocks UDP upstream.

## Required behavior

- create per-target or per-association flow state as appropriate;
- encode outgoing datagrams to Shadowsocks UDP packet format;
- send to Shadowsocks server UDP endpoint;
- decode responses;
- validate response target is compatible with flow target;
- forward response as SOCKS5 UDP datagram to client;
- enforce association idle timeout;
- enforce target-flow idle timeout;
- release active lease/flow state on cleanup;
- record UDP upstream metrics;
- reject unsupported methods/protocols deterministically.

## Target files

Likely areas:

```text
crates/eggress-udp/src/relay.rs
crates/eggress-udp/src/flow.rs
crates/eggress-udp/src/udp_capability.rs
crates/eggress-runtime/src/*
```

## Acceptance criteria

- SOCKS5 UDP client can send through Shadowsocks UDP upstream to local UDP echo.

---

# Workstream 4: Synthetic Shadowsocks UDP server

## Goal

Avoid false interop by using a test server with independent decode/encode logic where possible.

## Requirements

- binds UDP socket;
- decodes Shadowsocks UDP packet;
- sends payload to local UDP echo target or echoes directly;
- encodes Shadowsocks response;
- supports wrong-password failure scenario;
- supports at least IPv4 and domain target tests.

## Tests

Add:

```text
crates/eggress-runtime/tests/shadowsocks_udp.rs
```

Required cases:

1. SOCKS5 UDP client -> Eggress -> Shadowsocks UDP upstream -> UDP echo.
2. Wrong password drops/fails without leaking state.
3. Target flow idle cleanup returns gauges to zero.
4. Unsupported method config rejects.
5. Metrics increment packets/bytes/upstream failures.

---

# Workstream 5: Broader UDP parity decision

## Goal

Decide whether pproxy true parity requires multi-hop UDP or mixed UDP upstream chains.

## Required output

Update:

```text
docs/PARITY_MATRIX.md
docs/PPROXY_PARITY_SPEC.md
```

For each UDP combination:

- SOCKS5 UDP direct;
- SOCKS5 UDP -> SOCKS5 UDP upstream;
- SOCKS5 UDP -> Shadowsocks UDP upstream;
- SOCKS5 UDP -> HTTP upstream;
- SOCKS5 UDP -> SOCKS4 upstream;
- SOCKS5 UDP -> Trojan upstream;
- multi-hop UDP chains.

Mark each as:

- compatible;
- partial;
- unsupported;
- intentional non-parity.

## Decision rule

Do not implement multi-hop UDP unless:

- pproxy behavior is clear;
- semantics are safe;
- lifecycle/metrics can be bounded;
- tests can be deterministic.

## Acceptance criteria

- No UDP behavior is left ambiguous in the matrix.

---

# Workstream 6: pproxy differential UDP tests

## Goal

Compare comparable UDP behavior with pproxy.

## Scenarios

Gated behind `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`:

1. SOCKS5 UDP direct local echo.
2. SOCKS5 UDP through SOCKS5 upstream.
3. SOCKS5 UDP through Shadowsocks upstream, if pproxy supports equivalent setup.
4. UDP control TCP close behavior.
5. UDP unsupported route coarse failure/drop behavior.

## Acceptance criteria

- At least the core supported UDP scenarios have differential coverage or documented reason they cannot be compared.

---

# Workstream 7: Capability, metrics, and docs

## Required updates

- capability classifier marks Shadowsocks UDP supported only after runtime tests;
- UDP unsupported transport metrics still bounded;
- `docs/METRICS.md` includes Shadowsocks UDP counters if new labels are added;
- `docs/SECURITY_REVIEW.md` covers UDP amplification and target validation;
- `docs/CONFIG_REFERENCE.md` adds Shadowsocks UDP examples;
- README support table updated.

## Acceptance criteria

- Docs distinguish TCP Shadowsocks from UDP Shadowsocks.
- Unsupported UDP paths cannot silently direct-route.

---

# Recommended commit sequence

1. UDP spec docs.
2. UDP packet encode/decode helpers and tests.
3. Synthetic UDP server fixture.
4. UDP relay flow integration.
5. Runtime tests and metrics.
6. Differential UDP tests.
7. UDP parity matrix decision and docs.
8. Completion record.

---

# Required verification

```bash
cargo fmt --all -- --check
cargo test -p eggress-protocol-shadowsocks udp
cargo test -p eggress-runtime shadowsocks_udp
cargo test -p eggress-runtime udp_upstream
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Optional/gated:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored udp
```

---

# Definition of done

Phase 10 is complete only when:

1. Shadowsocks UDP packet format is documented.
2. Encode/decode helpers pass tamper/wrong-key/target tests.
3. SOCKS5 UDP through Shadowsocks upstream works in runtime tests.
4. Cleanup releases target flows and active counts.
5. Metrics are live and bounded.
6. Unsupported UDP combinations are explicit.
7. Differential or documented interop evidence exists.
8. Parity matrix covers UDP combinations.
9. No multi-hop UDP claim is made unless implemented and tested.
10. Workspace checks pass locally.

## Completion record

Add:

```text
docs/PHASE_10_SHADOWSOCKS_UDP_AND_UDP_PARITY_COMPLETION.md
```

Include supported methods, UDP matrix, tests, differential evidence, and deferred UDP non-parity.
