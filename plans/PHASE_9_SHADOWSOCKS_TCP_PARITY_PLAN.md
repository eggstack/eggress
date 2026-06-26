# Phase 9 Detailed Plan: Shadowsocks TCP Parity

## Purpose

Phase 9 closes the largest TCP protocol gap: interoperable Shadowsocks TCP. Eggress currently has partial/experimental Shadowsocks code and must not claim compatibility until the full TCP stream is encrypted/decrypted correctly and tested end to end.

This phase promotes Shadowsocks TCP only if the implementation is spec-compatible, runtime-tested, and documented. Otherwise it must remain experimental.

---

# Non-goals

Do not implement:

- Shadowsocks UDP parity; that is Phase 10;
- inbound Shadowsocks listener unless Phase 7 proves it is required and TCP upstream is complete;
- legacy insecure ciphers unless explicitly approved as intentional compatibility;
- obfs/plugin transports;
- multi-hop UDP;
- Python bindings;
- broad scheduler changes.

---

# Workstream 1: Confirm Shadowsocks spec and supported methods

## Required output

Create:

```text
docs/protocols/SHADOWSOCKS_PARITY.md
```

Required sections:

- pproxy Shadowsocks behavior from Phase 7;
- supported Shadowsocks specifications/SIPs;
- supported AEAD methods;
- key derivation;
- salt/subkey lifecycle;
- TCP stream framing;
- length chunk encryption;
- payload chunk encryption;
- nonce sequencing;
- max chunk size;
- unsupported legacy ciphers;
- interop test plan.

## Initial supported method set

Start narrow:

- `aes-128-gcm`;
- `aes-256-gcm`;
- `chacha20-ietf-poly1305` only if dependency policy allows.

Do not support legacy stream ciphers by default.

## Acceptance criteria

- Spec document exists before implementation is marked compatible.
- Unsupported methods produce explicit config/URI errors.

---

# Workstream 2: Implement AEAD key derivation and nonce primitives

## Goal

Build correct low-level primitives before stream integration.

## Target crate

Existing or new module in:

```text
crates/eggress-protocol-shadowsocks/
```

Suggested modules:

```text
src/method.rs
src/kdf.rs
src/nonce.rs
src/aead.rs
src/tcp_stream.rs
```

## Requirements

- deterministic key derivation tests;
- salt generation isolated and injectable for tests;
- independent read/write nonce counters;
- nonce increment overflow checked;
- method metadata includes key length, salt length, tag length, nonce length;
- no raw password in debug output.

## Tests

- method parsing;
- key length per method;
- salt length per method;
- nonce increments as expected;
- nonce overflow returns error;
- wrong method rejects.

## Acceptance criteria

- Primitive tests pass without network fixtures.

---

# Workstream 3: Implement Shadowsocks AEAD TCP stream adapter

## Goal

Encrypt/decrypt the whole bidirectional TCP stream, not only the target header.

## Required behavior

- client writes salt once;
- subkey is derived from password-derived key and salt;
- target address header is sent through encrypted stream framing;
- every payload chunk is framed and encrypted;
- read side decrypts length chunk then payload chunk;
- write side encrypts length chunk then payload chunk;
- read and write nonces are independent;
- max payload chunk size enforced;
- authentication failure maps to structured protocol error;
- implements `AsyncRead` and `AsyncWrite` for wrapped stream;
- flush/shutdown semantics preserve inner stream behavior.

## Suggested type

```rust
pub struct ShadowsocksAeadStream<S> {
    inner: S,
    method: CipherMethod,
    write_nonce: NonceCounter,
    read_nonce: NonceCounter,
    read_plain: BytesMut,
    read_encrypted: BytesMut,
    write_buf: BytesMut,
}
```

## Tests

- single chunk round-trip;
- multi-chunk round-trip;
- large payload split;
- tampered length chunk fails;
- tampered payload fails;
- wrong key fails;
- half-close behavior;
- no plaintext payload visible in wire capture test buffer.

## Acceptance criteria

- No plaintext-after-header path remains.
- Existing old partial function is removed, renamed experimental, or refactored to use stream adapter.

---

# Workstream 4: Runtime upstream integration

## Goal

Wire Shadowsocks TCP upstream only after stream adapter correctness.

## Required changes

- update chain executor handler to return `ShadowsocksAeadStream` as `BoxStream`;
- validate method/password in URI/config parsing;
- map errors to bounded failure reasons;
- record upstream-open success/failure metrics using protocol `shadowsocks`;
- ensure direct fallback behavior follows route policy only;
- do not enable unsupported methods.

## Tests

Add runtime tests:

```text
crates/eggress-runtime/tests/shadowsocks_tcp.rs
```

Required scenarios:

1. SOCKS5 inbound -> Shadowsocks upstream -> TCP echo.
2. HTTP CONNECT inbound -> Shadowsocks upstream -> TCP echo if supported by routing.
3. Wrong password fails.
4. Unsupported method rejected at config/URI validation.
5. Upstream-open metrics increment success/failure.
6. Direct route does not use Shadowsocks.

## Acceptance criteria

- Shadowsocks TCP works through `ServiceSupervisor`.

---

# Workstream 5: Synthetic compatible Shadowsocks server

## Goal

Test against a server that follows the same documented wire format, not a loopback helper that shares implementation bugs.

## Options

Preferred:

- independent synthetic server in tests with separate parser/framer code.

Acceptable:

- known-good local Shadowsocks implementation gated behind env var.

## Requirements

- server decrypts client stream;
- parses target address;
- connects local TCP echo;
- relays encrypted traffic;
- fails on wrong password/tamper.

## Acceptance criteria

- At least one test proves interoperability against independently implemented test server behavior.

---

# Workstream 6: pproxy differential coverage

## Goal

Compare Eggress Shadowsocks TCP behavior with pproxy if pproxy supports equivalent local test mode.

## Test

Extend gated differential suite:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored shadowsocks
```

Scenario:

- local SOCKS5 client;
- Shadowsocks upstream to local echo target;
- pproxy equivalent;
- Eggress equivalent;
- compare echo payload.

If pproxy setup is too unstable, document why and keep synthetic interop as the compatibility proof.

---

# Workstream 7: Capability, docs, and matrix update

## Required updates

- capability classifier moves Shadowsocks TCP from experimental/unsupported to compatible only after tests pass;
- `docs/PARITY_MATRIX.md` updated;
- `docs/protocols/SHADOWSOCKS_PARITY.md` finalized;
- `docs/CONFIG_REFERENCE.md` URI examples added;
- `docs/SECURITY_REVIEW.md` updated for crypto handling;
- README support table updated.

## Acceptance criteria

- Docs do not claim UDP support in Phase 9.
- Shadowsocks TCP support statement names supported methods.

---

# Recommended commit sequence

1. Spec and method policy document.
2. KDF/nonce/method primitives and tests.
3. AEAD chunk codec tests.
4. Async stream adapter.
5. Synthetic Shadowsocks TCP server fixture.
6. Runtime chain integration.
7. Runtime and differential tests.
8. Capability/docs update and completion record.

---

# Required verification

```bash
cargo fmt --all -- --check
cargo test -p eggress-protocol-shadowsocks
cargo test -p eggress-runtime shadowsocks_tcp
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Optional/gated:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored shadowsocks
```

---

# Definition of done

Phase 9 is complete only when:

1. Shadowsocks TCP spec doc exists.
2. Supported methods are explicit.
3. AEAD stream adapter encrypts all payload data.
4. No plaintext-after-header path remains in supported code.
5. Runtime TCP echo through Shadowsocks upstream passes.
6. Wrong-password/tamper cases fail safely.
7. Capability classifier accurately reflects TCP support.
8. Docs do not claim UDP Shadowsocks support.
9. Differential or independent synthetic interop coverage exists.
10. Workspace checks pass locally.

## Completion record

Add:

```text
docs/PHASE_9_SHADOWSOCKS_TCP_PARITY_COMPLETION.md
```

Include supported methods, test list, interop evidence, remaining UDP blockers, and any intentional non-parity.
