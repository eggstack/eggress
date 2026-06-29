# Phase 21 Plan: Shadowsocks Standardization and Inbound Parity

## Purpose

Phase 21 repairs the largest current protocol credibility gap: Shadowsocks. Eggress currently has useful cryptographic pieces and UDP AEAD support, but the TCP upstream path is documented as non-standard and not wire-compatible with standard Shadowsocks. Eggress also lacks Shadowsocks inbound server support.

This phase replaces the current partial Shadowsocks implementation with real standard Shadowsocks AEAD TCP/UDP behavior and pproxy-compatible inbound and upstream support.

## Dependencies

Phase 21 depends on Phase 18 for real pproxy differential evidence. It should follow or run after Phase 20 if UDP semantics are being refactored, because Shadowsocks UDP must integrate cleanly with standalone pproxy UDP and Eggress-native UDP modes.

## Non-goals

Do not implement ShadowsocksR in this phase. SSR is Phase 22.

Do not implement legacy stream ciphers in the main default path unless the project explicitly decides to support insecure compatibility modes. Legacy ciphers should be inventoried here, but implemented only if the phase owner and security review agree.

Do not treat synthetic encode/decode tests as sufficient evidence. Shadowsocks requires external interop tests.

## Work items

### 21.1 Protocol behavior inventory

Capture and document the exact Shadowsocks behavior required for pproxy parity.

Inventory:

- pproxy URI forms for `ss://` and `shadowsocks://`;
- method/password parsing;
- default method behavior, if any;
- TCP listener behavior;
- TCP upstream behavior;
- UDP listener behavior;
- UDP upstream behavior;
- supported AEAD methods;
- supported stream ciphers;
- OTA behavior;
- malformed ciphertext behavior;
- authentication failure behavior;
- DNS/domain target handling;
- IPv4 and IPv6 target handling;
- logging and exit diagnostics for invalid methods.

Persist results in:

```text
docs/protocols/SHADOWSOCKS.md
docs/PPROXY_PARITY_SPEC.md
tests/compat/fixtures/pproxy_shadowsocks_behavior.md
```

### 21.2 Replace non-standard TCP AEAD framing

Refactor `eggress-protocol-shadowsocks` so TCP AEAD framing is standard-compatible.

Requirements:

- implement SIP003-compatible AEAD TCP stream framing;
- generate and parse salts correctly;
- derive subkeys correctly;
- encrypt and decrypt length chunks and payload chunks as specified;
- support streaming reads and writes without requiring full message buffering;
- preserve backpressure and half-close behavior;
- enforce maximum chunk sizes;
- reject authentication failures without leaking secrets;
- handle fragmented ciphertext and partial frames.

Implementation guidance:

- Separate cryptographic primitives from stream framing.
- Use typed state machines for decrypting length and payload frames.
- Add property tests for frame round trips and fragmentation.
- Avoid exposing raw keys or salts in logs or errors.

### 21.3 AEAD method support

Support at minimum:

- `aes-128-gcm`;
- `aes-256-gcm`;
- `chacha20-ietf-poly1305`.

For each method:

- verify key length;
- verify salt length;
- verify nonce construction;
- verify tag validation;
- verify password-to-key derivation compatibility;
- add known-vector tests if available;
- add interop tests against `shadowsocks-rust`.

### 21.4 Shadowsocks TCP upstream/client

Rebuild the upstream client on top of the standard stream framing.

Requirements:

- connect to standard Shadowsocks server;
- send encrypted target address header;
- relay bidirectional TCP payload;
- support domain, IPv4, and IPv6 targets;
- respect connect and handshake timeouts;
- expose protocol-specific error categories;
- integrate with multi-hop TCP chain executor;
- update chain capability validation.

Tests:

- Eggress client to `shadowsocks-rust` server;
- Eggress client to pproxy Shadowsocks server;
- pproxy client to Eggress server after inbound support lands;
- multi-hop chain involving Shadowsocks plus HTTP/SOCKS where supported.

### 21.5 Shadowsocks inbound TCP server/listener

Implement Shadowsocks server mode as an inbound listener protocol.

Requirements:

- parse encrypted initial target address;
- decrypt subsequent TCP stream frames;
- route target through Eggress routing engine;
- support direct and chained upstreams;
- support per-listener method/password config;
- support pproxy-compatible URI credential parsing;
- expose listener protocol in mixed-protocol classification only if detection is feasible and safe;
- otherwise require explicit Shadowsocks listener mode and document why autodetection is constrained.

Important note:

Shadowsocks is encrypted and does not present a reliable plaintext protocol signature. Mixed listener autodetection should not guess. If pproxy supports mixed Shadowsocks autodetection, capture its behavior before attempting to reproduce it.

### 21.6 Shadowsocks UDP client/upstream standardization

Audit existing Shadowsocks UDP support and ensure it is standard-compatible.

Requirements:

- standard AEAD UDP packet format;
- correct salt and nonce use;
- correct address header encryption/decryption;
- support IPv4, IPv6, and domain targets;
- support standalone pproxy UDP mode from Phase 20;
- support SOCKS5 UDP ASSOCIATE routes where appropriate;
- support one-hop upstream first;
- support UDP chain semantics only where Phase 20 has defined them.

Interop tests:

- Eggress UDP client to `shadowsocks-rust` UDP server;
- `shadowsocks-rust` UDP client to Eggress UDP server;
- pproxy UDP through Eggress Shadowsocks server if supported;
- Eggress UDP through pproxy Shadowsocks server.

### 21.7 Shadowsocks inbound UDP server

Implement UDP server mode for Shadowsocks.

Requirements:

- receive encrypted datagrams from clients;
- decrypt address and payload;
- route to target;
- encrypt replies to the correct client;
- maintain flow state where required;
- enforce per-client and global UDP limits;
- expose metrics and admin status;
- apply anti-amplification controls.

### 21.8 Legacy cipher and OTA decision

Use the behavior inventory to decide whether to implement legacy stream ciphers and OTA.

Decision options:

1. Implement behind `legacy-shadowsocks` feature gate and explicit runtime config.
2. Implement URI parsing and diagnostics only, with final non-parity rationale.
3. Defer to Phase 22 if coupled to SSR compatibility.

If implemented:

- isolate legacy ciphers from modern AEAD code;
- require explicit insecure compatibility enablement;
- add warnings;
- add tests proving legacy behavior cannot be enabled accidentally;
- document security risks.

### 21.9 URI/config integration

Update URI and config support.

Requirements:

- parse `ss://method:password@host:port` correctly;
- support percent-encoding behavior matching pproxy;
- support aliases accepted by pproxy;
- validate methods early;
- redact passwords in all display paths;
- support inbound and upstream config forms;
- include method/password fields in typed config rather than ad hoc strings.

### 21.10 Error handling and observability

Add protocol-specific errors and metrics.

Suggested errors:

- unsupported method;
- invalid key material;
- malformed address;
- decrypt failed;
- frame length invalid;
- UDP packet invalid;
- auth/tag verification failed;
- target connection failed.

Suggested metrics:

- Shadowsocks TCP sessions accepted;
- Shadowsocks TCP upstream sessions opened;
- Shadowsocks UDP packets in/out;
- decrypt failures;
- unsupported method rejects;
- frame parse failures;
- active Shadowsocks flows.

Do not expose passwords, keys, salts, plaintext target metadata beyond what existing routing logs already expose under redaction policy.

### 21.11 Differential and interop tests

Required tests:

- Eggress upstream to real pproxy Shadowsocks listener;
- real pproxy client to Eggress Shadowsocks listener;
- Eggress upstream to `shadowsocks-rust`;
- `shadowsocks-rust` client to Eggress listener;
- TCP payload echo through Shadowsocks;
- UDP echo through Shadowsocks;
- invalid password/auth failure;
- unsupported method diagnostic;
- IPv4/domain/IPv6 targets;
- fragmented TCP frames;
- large payload chunking;
- shutdown and half-close behavior.

Mark compatibility only when the relevant interop tests pass.

### 21.12 Documentation updates

Update:

- `docs/protocols/SHADOWSOCKS.md`;
- `docs/PARITY_MATRIX.md`;
- `docs/PPROXY_PARITY_SPEC.md`;
- `docs/PPROXY_MIGRATION.md`;
- `docs/CONFIG_REFERENCE.md`;
- `docs/SECURITY_REVIEW.md`;
- README capability table;
- compatibility manifest.

Remove or rewrite any existing language saying Shadowsocks TCP is non-standard once the implementation is replaced and tested. Do not remove that warning before interop evidence exists.

## Validation commands

At minimum:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-runtime shadowsocks
cargo test --test differential_pproxy -- shadowsocks --nocapture
cargo test --test interoperability_shadowsocks -- --nocapture
```

External tool setup should be scripted:

```bash
./scripts/install_shadowsocks_interop.sh
./scripts/compat_shadowsocks.sh
```

## Acceptance criteria

Phase 21 is complete when:

- Shadowsocks TCP AEAD framing is standard-compatible;
- Eggress can act as Shadowsocks TCP client/upstream;
- Eggress can act as Shadowsocks TCP server/listener;
- Eggress Shadowsocks UDP client and server paths interoperate with standard implementations;
- pproxy differential tests pass for supported Shadowsocks cases;
- current warnings about non-standard TCP framing are removed only after evidence exists;
- legacy cipher/OTA behavior has an explicit decision and documented status;
- docs and manifest accurately classify every Shadowsocks feature.

## Risks

Cryptographic framing bugs are high-impact. Keep primitives small, test vectors explicit, and interop tests mandatory.

Mixed listener autodetection may be impossible or unsafe for encrypted Shadowsocks. Do not guess. Prefer explicit listener mode unless pproxy behavior is clearly captured and reproducible.

Legacy cipher support can weaken the product. If implemented, isolate behind explicit feature gates and insecure-compat configuration.

## Handoff notes

This phase should be treated as a rewrite/standardization pass, not a small patch. The current non-standard Shadowsocks TCP path should not be preserved as the default behavior once standard framing lands.
