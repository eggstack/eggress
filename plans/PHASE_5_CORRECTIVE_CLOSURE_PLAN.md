# Phase 5 Corrective Closure Plan: Upstream Protocol Parity

## Purpose

Phase 5 added a large amount of upstream-protocol work: HTTP CONNECT polish, SOCKS4/SOCKS4a, Shadowsocks, Trojan, a shared capability classifier, and an additional TLS transport layer. The implementation volume is real, but the current support claims are ahead of the verified behavior.

This corrective plan is strict. Protocol support must not be advertised as complete unless the implementation is interoperable, bounded, configured cleanly, runtime-tested, and documented accurately.

The priority is to prevent false parity claims. If a protocol is partial, mark it experimental or unsupported in capability, README, admin output, and completion docs until it satisfies the criteria below.

---

# Current blockers

1. Shadowsocks TCP sends an encrypted target header but returns the raw stream; subsequent bidirectional data is plaintext.
2. Shadowsocks AEAD TCP chunk framing is not implemented as a stream adapter with nonce sequencing.
3. Shadowsocks UDP packet format appears non-standard and may not interoperate with real Shadowsocks servers.
4. The capability classifier marks Shadowsocks as TCP+UDP supported despite the above.
5. Trojan encodes domain length with `domain.len() as u8` without validation.
6. Trojan uses credentials awkwardly: username is password, password is server name, even though `ProxyHopSpec.server_name` exists.
7. Trojan tests visibly include manual TLS/request construction instead of exercising `trojan_connect()` in all important cases.
8. TLS transport introduced rustls dependency behavior that appears to pull native build tooling (`aws-lc-sys`, `cc`, `cmake`) despite prior “no native dependencies” constraints.
9. Phase numbering is muddled: an additional “Phase 4 TLS” was inserted after UDP Phase 4 was already closed.
10. Runtime-level TOML-driven tests for HTTP/SOCKS4/Shadowsocks/Trojan parity are incomplete or not sufficiently visible.
11. Docs and completion records overstate Shadowsocks/Trojan support.
12. Phase 6 hardening should not proceed until these Phase 5 support claims are corrected.

---

# Non-goals

Do not add:

- new proxy protocols;
- multi-hop UDP support;
- QUIC/MASQUE/CONNECT-UDP;
- transparent proxying;
- unsafe Rust;
- native TLS/OpenSSL;
- additional crypto backends without explicit policy;
- public-internet integration tests.

---

# Policy: support levels

Introduce or document explicit support levels for every upstream protocol:

```rust
pub enum ProtocolSupportLevel {
    Supported,
    Experimental,
    Partial,
    Unsupported,
}
```

If adding this enum is too invasive, at minimum reflect these categories in docs/admin/README.

Definitions:

- **Supported**: full runtime path works, local deterministic tests pass, parser/framing is bounded, docs are exact.
- **Experimental**: code exists and crate-level tests pass, but interop/runtime coverage is incomplete.
- **Partial**: one component exists, but the protocol does not function end-to-end.
- **Unsupported**: intentionally rejected in routing/config.

Rules:

- `Supported` requires a `ServiceSupervisor` runtime test unless the protocol is purely internal.
- `Supported` for Shadowsocks requires interoperable TCP stream encryption, not just an initial address header.
- `Supported` for UDP requires an echo test through a synthetic or known-good server using the documented wire format.
- README and completion docs must not use `[x]` for partial protocols.

---

# Workstream 1: Correct capability classification and support claims immediately

## Problem

The capability classifier currently marks Shadowsocks as TCP+UDP supported. That is not defensible while TCP payload encryption is missing and UDP interoperability is uncertain.

## Required change

Until Shadowsocks is fully implemented and runtime-tested, classify it conservatively.

Preferred:

```rust
ProtocolSpec::Shadowsocks => UpstreamCapabilities {
    tcp_connect: CapabilityResult::UnsupportedProtocol {
        protocol: "ShadowsocksExperimental".to_string(),
    },
    udp_associate: CapabilityResult::UnsupportedProtocol {
        protocol: "ShadowsocksExperimental".to_string(),
    },
}
```

Alternative if you want to keep the code path available behind an explicit flag:

```rust
ProtocolSpec::Shadowsocks => UpstreamCapabilities {
    tcp_connect: CapabilityResult::SupportedExperimental,
    udp_associate: CapabilityResult::SupportedExperimental,
}
```

Do not allow route selection to silently treat experimental Shadowsocks as fully supported unless a config flag opts in.

Suggested config flag:

```toml
[experimental]
enable_shadowsocks = true
enable_trojan = true
```

If no experimental framework exists, use `UnsupportedProtocol` for now.

## Required docs changes

- README capability table: Shadowsocks TCP/UDP must be `experimental` or `partial`, not `supported`.
- `docs/PHASE_5_UPSTREAM_PROTOCOL_PARITY_COMPLETION.md`: update checklist and limitations.
- Admin capability output should distinguish supported vs experimental/partial if it exposes this.

## Required tests

- capability classifier does not mark Shadowsocks supported by default;
- UDP route selecting Shadowsocks drops or rejects unless experimental opt-in exists;
- docs test/snapshot if docs are checked.

## Acceptance criteria

- no public path claims Shadowsocks is supported until it is truly interoperable.

---

# Workstream 2: Decide Shadowsocks path: implement correctly or explicitly defer

## Problem

`shadowsocks_connect()` sends an encrypted address header and then returns the raw stream. That cannot be advertised as Shadowsocks TCP support.

## Option A: Implement real Shadowsocks AEAD TCP stream support

Implement a bidirectional stream adapter that performs AEAD TCP chunk framing with independent read/write nonce sequences.

Required behavior:

- client sends salt once;
- derive subkey using method-compatible key derivation;
- encrypt/decrypt each length chunk and payload chunk according to the Shadowsocks AEAD stream format;
- maintain write nonce counter and read nonce counter separately;
- increment nonce after each encrypted chunk;
- enforce maximum payload chunk length;
- reject auth/decrypt failures;
- implement `AsyncRead` and `AsyncWrite` for the wrapped stream;
- `shadowsocks_connect()` must return the encrypted stream adapter, not the raw stream.

API sketch:

```rust
pub struct ShadowsocksAeadStream<S> {
    inner: S,
    method: CipherMethod,
    enc_key: Vec<u8>,
    dec_key: Vec<u8>,
    write_nonce: NonceCounter,
    read_nonce: NonceCounter,
    read_buf: BytesMut,
    plain_buf: BytesMut,
}
```

Tests:

- frame length encryption/decryption against known-good vectors if available;
- round-trip through a synthetic spec-compatible Shadowsocks server;
- tampered length chunk fails;
- tampered payload chunk fails;
- nonce increments per chunk;
- large payload splits into bounded chunks;
- runtime TCP echo through Shadowsocks upstream.

## Option B: Defer Shadowsocks support

If implementing the stream adapter is too large for this corrective pass:

- remove Shadowsocks from active route handlers, or make handler return `Unsupported` unless experimental flag is enabled;
- keep protocol crate as experimental/internal;
- document `Shadowsocks TCP: partial header prototype only, not supported`;
- update capability matrix and README accordingly;
- remove or revise completion claims.

## Strict decision rule

Do not leave the current state: wired into route execution as if supported while comments admit payload data is plaintext.

## Acceptance criteria

Either:

- real encrypted TCP stream support with runtime echo and interop-style tests, or
- Shadowsocks marked unsupported/experimental everywhere and not selected by default.

---

# Workstream 3: Correct Shadowsocks UDP or defer it

## Problem

Current UDP packet format is documented in code as `nonce + encrypted(address + payload)`. Standard Shadowsocks AEAD UDP uses a salt and encrypted payload structure tied to the method. The current implementation may only round-trip against itself.

## Required investigation

Before coding, write or update:

```text
docs/protocols/SHADOWSOCKS.md
```

with a precise description of the supported format and whether it is interoperable with Shadowsocks SIP002/AEAD servers.

## Option A: Implement interoperable Shadowsocks UDP

Required behavior:

- derive key according to the selected method and password;
- encode packet according to standard Shadowsocks AEAD UDP format;
- include salt where required by method;
- decrypt responses according to the same format;
- validate response target compatibility;
- support one-hop Shadowsocks UDP upstream in relay flow;
- synthetic server must follow the same documented format;
- add runtime UDP echo through Shadowsocks upstream.

Tests:

- packet encode/decode against known vectors if available;
- tampered packet fails;
- wrong key fails;
- local synthetic interoperable UDP server echo;
- runtime UDP echo through Shadowsocks upstream;
- route cleanup releases flow/lease.

## Option B: Defer Shadowsocks UDP

- Capability classifier marks Shadowsocks UDP unsupported by default.
- UDP relay selecting Shadowsocks records unsupported protocol.
- Docs say UDP packet helpers are experimental and not parity support.
- Completion doc removes Shadowsocks UDP from supported matrix.

## Acceptance criteria

Shadowsocks UDP is either interoperable and runtime-tested or explicitly unsupported.

---

# Workstream 4: Fix Trojan credential and server-name model

## Problem

Current Trojan handler overloads credentials:

- `credentials.username` = Trojan password;
- `credentials.password` = TLS server name.

This is not maintainable and conflicts with `ProxyHopSpec.server_name`.

## Required model

Use:

- `credentials.password` or a protocol-specific config field for Trojan password;
- `ProxyHopSpec.server_name` for TLS SNI/certificate name;
- fallback to `endpoint.host` if `server_name` absent.

Recommended interim URI model:

```text
trojan://password@server.example:443
```

or if parser requires username/password:

```text
trojan://ignored:password@server.example:443
```

but this must be documented clearly. Better config model:

```toml
[[upstreams]]
id = "trojan-a"
protocol = "trojan"
address = "server.example:443"
password = "secret"
server_name = "server.example"
```

## Handler signature issue

`HopHandler::handshake()` currently receives only credentials, not the full hop. To access `server_name`, refactor carefully.

Option A: change trait to pass the hop:

```rust
fn handshake<'a>(
    &'a self,
    stream: BoxStream,
    target: &'a TargetAddr,
    hop: &'a ProxyHopSpec,
) -> HandshakeFuture<'a>;
```

Option B: keep trait stable and encode Trojan server name into a dedicated credential only at compile time. This is less clean.

Preferred: Option A, because TLS wrappers and protocol-specific handshakes will keep needing hop metadata.

## Required tests

- Trojan handler uses `hop.server_name` when provided;
- Trojan handler falls back to endpoint host;
- password is not taken from server-name field;
- redacted URI/config does not expose password;
- wrong password fails against synthetic server;
- wrong server name fails TLS verification.

## Acceptance criteria

- Trojan config is understandable and does not overload password as SNI.

---

# Workstream 5: Fix Trojan wire encoding bounds and tests

## Problem

Trojan domain encoding uses `domain.len() as u8`. This can truncate names over 255 bytes.

## Required change

Add checked encoding:

```rust
fn encode_domain_len(domain: &str) -> Result<u8, TrojanError> {
    if domain.is_empty() || domain.len() > 255 {
        return Err(TrojanError::Protocol("invalid domain length".into()));
    }
    Ok(domain.len() as u8)
}
```

Apply in `trojan_connect()` and any test helpers.

## Tests

- domain length 0 rejected if representable;
- domain length 255 accepted;
- domain length 256 rejected;
- IPv4/IPv6 unchanged;
- request framing remains exact.

## Test exported path

Ensure tests call `trojan_connect()` directly for:

- successful synthetic TLS Trojan server;
- wrong password server closes/fails;
- wrong server name fails;
- long domain rejected before writing lossy length.

## Acceptance criteria

- Trojan request encoding has no lossy length casts and exported function is tested.

---

# Workstream 6: TLS dependency and crypto-provider policy

## Problem

The project repeatedly stated no native dependencies. The current dependency graph appears to include native build tooling via rustls provider dependencies (`aws-lc-sys`, `cc`, `cmake`) in `Cargo.lock`.

## Required decision

Choose one policy and enforce it.

### Policy A: Pure-Rust crypto/provider only

- Configure rustls dependencies to avoid `aws-lc-rs/aws-lc-sys`.
- Use the ring provider only if accepted, noting ring may still have native/asm considerations.
- Ensure `cargo tree -i aws-lc-sys` returns nothing.
- Update `deny.toml` to ban `aws-lc-sys`, `cmake` if desired.

### Policy B: Accept aws-lc/native build tooling

- Update project constraints and docs honestly.
- Explain why aws-lc is accepted.
- Update `deny.toml` to explicitly allow it.
- Remove “no native dependency” claims from completion docs.

Recommended for current project constraints: Policy A unless TLS cannot be made reliable without native provider.

## Required checks

Run and record:

```bash
cargo tree -i aws-lc-sys
cargo tree -i openssl-sys
cargo tree -i native-tls
cargo tree -i cmake
cargo deny check
```

## Required docs

Add:

```text
docs/DEPENDENCY_POLICY.md
```

covering crypto/TLS dependency decisions.

## Acceptance criteria

- dependency graph and docs agree with the project’s portability policy.

---

# Workstream 7: Resolve phase numbering and roadmap drift

## Problem

The repo now has a new `PHASE_4_TLS_TRANSPORT_COMPLETION.md` after Phase 4 had already been UDP upstream relay. This creates contradictory phase history.

## Required cleanup

Choose one:

### Option A: Treat TLS as Phase 5A or transport subphase

- Rename docs:
  - `PHASE_4_TLS_TRANSPORT_COMPLETION.md` -> `PHASE_5A_TLS_TRANSPORT_COMPLETION.md` or `TRANSPORT_TLS_COMPLETION.md`.
- Update `.opencode/plans/phase4-tls-transport.md` title to non-conflicting numbering.
- Update roadmap links.

### Option B: Keep numbering but document forked plan history

Not recommended.

## Required docs

- `docs/ROADMAP.md` must list UDP Phase 4 and TLS transport subphase distinctly.
- `EGGRESS_ROADMAP.md` must match.
- README status must not imply TLS was the original Phase 4.

## Acceptance criteria

- a new maintainer can read the roadmap without seeing two incompatible Phase 4s.

---

# Workstream 8: Runtime-level tests for all claimed protocols

## Requirement

No upstream protocol should be marked supported without a TOML-driven `ServiceSupervisor` test that exercises the full path.

Required runtime tests:

```text
crates/eggress-runtime/tests/upstream_protocols.rs
```

Scenarios:

1. HTTP CONNECT upstream routes TCP echo.
2. Authenticated HTTP CONNECT upstream routes TCP echo.
3. SOCKS4 upstream routes TCP echo.
4. SOCKS4a upstream routes domain target if supported.
5. SOCKS5 TCP upstream still routes TCP echo.
6. SOCKS5 UDP upstream still routes UDP echo after refactors.
7. Shadowsocks TCP runtime echo only if support remains enabled.
8. Shadowsocks UDP runtime echo only if support remains enabled.
9. Trojan TCP runtime echo only if support remains enabled.
10. Unsupported protocol/transport combinations reject and metric correctly.

For any protocol lacking runtime test, downgrade support level.

## Acceptance criteria

- support matrix has a direct runtime-test reference for every supported item.

---

# Workstream 9: Documentation truth pass

## Required updates

Audit and correct:

- `README.md`;
- `docs/PHASE_5_UPSTREAM_PROTOCOL_PARITY_COMPLETION.md`;
- `docs/protocols/SHADOWSOCKS.md`;
- `docs/protocols/TROJAN.md`;
- `docs/protocols/HTTP_CONNECT.md`;
- `docs/protocols/SOCKS4.md`;
- `docs/ARCHITECTURE.md`;
- `docs/ROADMAP.md`;
- `EGGRESS_ROADMAP.md`;
- `AGENTS.md`.

Required wording rules:

- Use “supported” only for runtime-tested, interoperable paths.
- Use “experimental” for crate-level or partial implementation.
- Use “partial” for known-incomplete paths like Shadowsocks header-only TCP.
- State exact unsupported transports.
- State dependency caveats for TLS.
- State no public-internet tests are required.

## Acceptance criteria

- docs match executable behavior and do not overclaim parity.

---

# Workstream 10: CI/status visibility before closure

## Problem

Commit messages claim tests pass, but GitHub combined status has shown no status contexts.

## Required action

Add or verify GitHub Actions workflow for:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

If CI already exists but statuses are absent, investigate trigger filters or branch protection.

## Acceptance criteria

- main branch commits show visible workflow runs/status checks;
- completion docs cite actual workflow run IDs or local verification separately.

---

# Recommended commit sequence

## Commit 1: Downgrade/guard overstated Shadowsocks support

- Update capability classifier.
- Prevent default route execution through incomplete Shadowsocks.
- Update README and completion doc to partial/experimental.
- Add tests for default unsupported behavior.

## Commit 2: Trojan config and encoding cleanup

- Pass full hop metadata to handlers or otherwise use `server_name` correctly.
- Fix Trojan domain length validation.
- Add direct `trojan_connect()` tests.

## Commit 3: Shadowsocks decision implementation

Choose A or B:

- A: implement real AEAD stream adapter and interoperable UDP format with tests;
- B: mark Shadowsocks crate experimental and remove support claims.

Do not keep current ambiguous state.

## Commit 4: TLS dependency policy

- Enforce pure-Rust/no-native policy or document accepted native build deps.
- Update `deny.toml` and `docs/DEPENDENCY_POLICY.md`.

## Commit 5: Runtime protocol test matrix

- Add `upstream_protocols.rs` runtime tests for every supported protocol.
- Downgrade any protocol without runtime coverage.

## Commit 6: Phase numbering and docs truth pass

- Rename/reclassify TLS phase docs.
- Fix roadmaps and README support matrix.
- Update Phase 5 completion doc.

## Commit 7: CI/status visibility

- Add/repair GitHub Actions workflow.
- Document verification source.

---

# Required final verification

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Run focused protocol tests:

```bash
cargo test -p eggress-protocol-http
cargo test -p eggress-protocol-socks
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-protocol-trojan
cargo test -p eggress-transport-tls
cargo test -p eggress-runtime upstream_protocols
cargo test -p eggress-runtime udp_upstream
```

Run dependency checks:

```bash
cargo tree -i aws-lc-sys || true
cargo tree -i openssl-sys || true
cargo tree -i native-tls || true
cargo tree -i cmake || true
```

---

# Definition of done

Phase 5 corrective closure is complete only when:

1. No protocol is marked supported unless it has full runtime coverage.
2. Shadowsocks is either fully implemented with encrypted TCP stream adapter and interoperable UDP, or marked partial/experimental/unsupported everywhere.
3. Shadowsocks route execution cannot accidentally send plaintext after an encrypted header under a “supported” label.
4. Trojan uses a sane password/server-name model and does not overload credentials incorrectly.
5. Trojan domain length encoding is bounded and tested.
6. TLS dependency policy is explicit and enforced by docs/deny configuration.
7. Phase numbering no longer conflicts with the already-closed UDP Phase 4.
8. HTTP CONNECT and SOCKS4/SOCKS4a runtime tests exist for supported claims.
9. README, roadmap, protocol docs, admin/capability output, and completion docs all agree.
10. CI/status checks are visible or the absence is explicitly documented with local verification.
11. All workspace tests, lint, audit, and dependency checks pass.
12. No unsafe Rust or unapproved native dependency is introduced.

## Completion record

When complete, update or add:

```text
docs/PHASE_5_CORRECTIVE_CLOSURE_COMPLETION.md
```

Include:

- commit list;
- final support matrix;
- protocols downgraded or enabled;
- dependency policy decision;
- runtime test list;
- verification commands and CI run identifiers if available.
