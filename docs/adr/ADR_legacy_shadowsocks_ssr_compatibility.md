# ADR: Legacy Shadowsocks Stream Ciphers and ShadowsocksR Compatibility

| Field | Value |
|-------|-------|
| Status | Accepted |
| Date | Phase 22 |
| Decision makers | Eggress maintainers |
| Related | `docs/PPROXY_PARITY_SPEC.md`, `docs/PARITY_MATRIX.md`, `docs/protocols/SHADOWSOCKS.md` |

## Context

pproxy 2.7.9 supports the following legacy Shadowsocks features:

1. **Legacy stream ciphers**: `aes-128-ctr`, `aes-192-ctr`, `aes-256-ctr`,
   `aes-128-cfb`, `aes-192-cfb`, `aes-256-cfb`, `rc4-md5`, `chacha20-ietf`
   (stream mode without poly1305 authentication).

2. **ShadowsocksR (SSR)**: A fork-based protocol extension adding protocol
   obfuscation (`origin`, `verify_simple`, `verify_deflate`) and transport
   obfuscation (`plain`, `http_simple`, `tls1.2_ticket_auth`) layers on top
   of stream ciphers.

eggress must decide whether to implement these features for pproxy compatibility,
or intentionally exclude them.

## Decision

**SSR and legacy stream ciphers are NOT implemented in eggress.**

eggress will:

1. Recognize SSR URIs (`ssr://`) and legacy stream cipher method names during
   URI parsing and translation.
2. Reject them with clear, structured `UnsupportedFeature` diagnostics.
3. Not provide a feature gate (since nothing is implemented to gate).
4. Track rejection counts via metrics.

## Rationale

### Security

Legacy stream ciphers have critical security weaknesses:

- **No authentication**: Stream ciphers provide confidentiality only. There is
  no integrity protection. An attacker can modify ciphertext without detection
  (bit-flipping attacks on address headers and payload).
- **No replay protection**: There is no mechanism to detect replayed ciphertexts.
- **Known weaknesses**: `rc4-md5` uses the RC4 stream cipher which has documented
  statistical biases. `chacha20-ietf` without poly1305 lacks authentication.
- **No RFC**: Stream cipher Shadowsocks was never formally standardized. AEAD
  ciphers are the standard defined in SIP003.

SSR adds obfuscation layers but does not provide authentication:

- Protocol modes (`origin`, `verify_simple`, `verify_deflate`) add handshake
  verification but not encryption authentication.
- Obfuscation modes (`http_simple`, `tls1.2_ticket_auth`) disguise traffic as
  HTTP or TLS but do not protect confidentiality.
- The underlying ciphers remain unauthenticated stream ciphers.

### Non-Standard Protocol

- SSR has no formal specification or RFC. The protocol is defined by
  implementation (a fork of a fork of the original Shadowsocks).
- SSR wire format is incompatible with standard Shadowsocks AEAD.
- Cross-implementation interoperability is limited to SSR-compatible servers.

### Maintenance Burden

Implementing SSR would require:

- Protocol obfuscation layer (`origin`, `verify_simple`, `verify_deflate`)
- Transport obfuscation layer (`plain`, `http_simple`, `tls1.2_ticket_auth`)
- Legacy stream cipher support (8+ cipher methods)
- A separate UDP handling path for SSR
- Ongoing maintenance of a non-standard protocol with no upstream specification

This adds significant code complexity with no security benefit over standard AEAD.

### No Clear Use Case

- Modern proxy deployments should use AEAD ciphers with authenticated encryption.
- SSR's obfuscation layers are a historical artifact from a period when traffic
  obfuscation was needed to evade deep packet inspection.
- Users who need SSR compatibility are better served by pproxy or SSR-specific
  implementations.

## Consequences

### Positive

- **Security posture preserved**: Default remains modern AEAD only. No downgrade
  path from authenticated encryption.
- **Reduced code surface**: No protocol/obfs layer code to maintain, test, or
  audit.
- **Clear diagnostics**: Users who attempt SSR or legacy cipher usage receive
  structured error messages explaining why and suggesting AEAD alternatives.
- **Metrics visibility**: Rejection counts are tracked, providing visibility into
  attempted usage of unsupported features.

### Negative

- **pproxy compatibility gap**: Users who rely on SSR or legacy stream ciphers
  cannot use eggress as a drop-in replacement for pproxy.
- **Migration friction**: Users with existing SSR configurations must migrate to
  AEAD ciphers before adopting eggress.

### Neutral

- **URI recognition**: SSR URIs are recognized during parsing but rejected
  immediately. This prevents confusing "unknown protocol" errors and provides
  targeted guidance.
- **No feature gate needed**: Since nothing is implemented, there is no need for
  a compile-time or runtime feature gate.

## Alternatives Considered

### 1. Compile-time Feature Gate

Add SSR and legacy stream ciphers behind a Cargo feature flag (e.g.,
`legacy-ss`).

**Rejected because**: The feature would still require significant implementation
effort (protocol/obfs layers, 8+ ciphers). A feature gate does not reduce the
maintenance burden — it only makes the code optional to compile. The security
concerns remain regardless of how the code is gated.

### 2. Runtime Opt-in

Allow SSR and legacy ciphers via a configuration flag (e.g.,
`allow_insecure_ciphers = true`).

**Rejected because**: This creates a downgrade path from authenticated encryption.
Users could inadvertently enable insecure ciphers. The security posture of the
project is better served by a hard exclusion.

### 3. Full Implementation

Implement SSR and legacy stream ciphers for full pproxy compatibility.

**Rejected because**: The security, maintenance, and specification concerns
outlined above outweigh the compatibility benefit. eggress targets modern proxy
deployments, not legacy configurations.

## Security Posture

eggress's security posture remains:

- **Default**: Modern AEAD ciphers only (`aes-128-gcm`, `aes-256-gcm`,
  `chacha20-ietf-poly1305`).
- **No downgrade path**: There is no configuration option to enable legacy
  ciphers.
- **Authenticated encryption**: All Shadowsocks traffic uses AEAD with 16-byte
  authentication tags.
- **Standard wire format**: SIP003 AEAD framing is interoperable with standard
  Shadowsocks implementations.

## References

- `docs/PPROXY_PARITY_SPEC.md` — Section 14: Behaviors Eggress Will Intentionally Reject
- `docs/PARITY_MATRIX.md` — Remaining Protocol Audit
- `docs/protocols/SHADOWSOCKS.md` — Supported cipher methods
- `docs/protocols/SHADOWSOCKS_LEGACY.md` — Legacy stream cipher behavior documentation
- `docs/protocols/SHADOWSOCKSR.md` — SSR behavior documentation
- `tests/compat/fixtures/pproxy_ssr_behavior.md` — SSR behavior fixture
- `crates/eggress-protocol-shadowsocks/src/method.rs` — AEAD-only method parsing
