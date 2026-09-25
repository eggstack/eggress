# pproxy Strict Phase 1 — Shadowsocks AEAD Wire Correction

## Priority

P0. Complete this before expanding any parity claim.

## Problem statement

Eggress currently treats all supported Shadowsocks AEAD methods as having a 16-byte salt, while pproxy 2.7.9's AEAD cipher classes use method-specific IV/salt lengths. The current Eggress TCP implementation allocates and reads the on-wire salt using `CipherMethod::salt_size()`, so Eggress-to-Eggress tests can pass while both endpoints share the same incorrect framing.

The strict target must be externally interoperable, not merely self-consistent.

## Primary files

Inspect and expect to modify:

- `crates/eggress-protocol-shadowsocks/src/method.rs`
- `crates/eggress-protocol-shadowsocks/src/aead.rs`
- `crates/eggress-protocol-shadowsocks/src/tcp.rs`
- `crates/eggress-protocol-shadowsocks/src/tcp_stream.rs`
- `crates/eggress-protocol-shadowsocks/src/udp.rs`
- `crates/eggress-protocol-shadowsocks/src/server.rs`
- URI/config lowering paths that parse Shadowsocks methods
- parity manifest/matrix entries after runtime proof

Python cipher facades may need updates only after the Rust method inventory is correct.

## Required method inventory

From the exact 2.7.9 cipher maps, classify modern AEAD methods separately from legacy stream ciphers.

At minimum implement exact pproxy behavior for:

- `aes-128-gcm`: 16-byte key, 16-byte IV/salt;
- `aes-192-gcm`: 24-byte key, 24-byte IV/salt;
- `aes-256-gcm`: 32-byte key, 32-byte IV/salt;
- `chacha20-ietf-poly1305`: 32-byte key, 32-byte IV/salt.

If Phase 0 confirms an XChaCha AEAD name is part of the strict 2.7.9 callable/cipher surface, either implement it in this phase or classify it explicitly for the legacy/extended crypto tail. Do not silently omit it.

## Implementation requirements

### 1. Make sizes method-specific

`CipherMethod` must expose exact values for:

- password-derived master key size;
- salt/IV size;
- nonce size;
- tag size.

Do not encode a single shared salt size for all methods.

### 2. Audit key derivation

Verify the exact pproxy 2.7.9 AEAD derivation sequence:

- EVP_BytesToKey-compatible MD5 expansion for the master key;
- salt as HKDF salt;
- `ss-subkey` info;
- HKDF-SHA1 output length equal to the selected key size.

Use known-answer vectors captured from pproxy for all four required methods.

### 3. Audit TCP framing

Verify independently in each direction:

- salt length;
- first encrypted two-byte payload length;
- tag placement;
- encrypted payload block;
- little-endian nonce increment behavior;
- response-side independent salt and nonce sequence;
- partial reads/writes and frame splitting.

Do not rely on an Eggress client talking to an Eggress server as proof.

### 4. Audit UDP framing

pproxy's packet cipher prepends a fresh method-sized IV/salt to each datagram and encrypts the remainder with that packet-local state. Verify Eggress matches this for each supported method.

Keep UDP replay/amplification protections if they do not alter legitimate wire behavior. Compatibility does not require reproducing unsafe implementation internals.

### 5. Add AES-192-GCM

Add the enum variant and cryptographic implementation. If the current `aes-gcm` crate supports the needed 192-bit key type, use it directly. Otherwise choose a narrowly scoped RustCrypto implementation rather than introducing OpenSSL.

### 6. Parser/config propagation

All compatibility and native parsing paths that currently accept method names must agree on the new method set and canonical spelling.

Unknown/legacy methods must continue to fail clearly rather than being mapped to another cipher.

## Differential and interoperability evidence

Required local test topologies:

1. Eggress SS client -> pproxy 2.7.9 SS server.
2. pproxy 2.7.9 SS client -> Eggress SS server.
3. Eggress standard-method client -> a maintained Shadowsocks implementation.
4. maintained Shadowsocks client -> Eggress server where practical.
5. UDP request/reply for each standard method supported by the external implementation.

Use `aes-128-gcm`, `aes-256-gcm`, and `chacha20-ietf-poly1305` for standards interop. Use pproxy itself as the oracle for `aes-192-gcm` if the external implementation intentionally omits that nonstandard method.

These tests may live behind an explicit local/interop command. At least deterministic known-answer/unit tests must remain in normal crate tests.

## Regression tests

Add tests that fail if:

- AES-256 or ChaCha salt size returns 16;
- AES-192 returns any size other than 24;
- TCP accept reads the wrong amount of salt;
- UDP packet code hard-codes 16 bytes;
- client/server nonce sequences share state incorrectly;
- a legacy method is accidentally accepted as a modern AEAD method.

## Non-goals

- Legacy RC4/CFB/OFB/CTR/Salsa/ChaCha stream methods.
- OTA.
- SSR plugins.
- A generalized crypto-provider abstraction.
- New default cipher choices unrelated to compatibility.

## Acceptance criteria

1. `CipherMethod` reports exact method-specific key/salt/nonce/tag sizes.
2. AES-192-GCM is accepted everywhere the pproxy compatibility layer accepts AEAD method names.
3. Eggress TCP client interoperates with pproxy 2.7.9 for every required AEAD method.
4. pproxy 2.7.9 TCP client interoperates with Eggress for every required AEAD method.
5. Standard methods interoperate bidirectionally with at least one maintained Shadowsocks implementation.
6. UDP is validated against pproxy for each required method and against the external implementation for standard methods.
7. Existing Eggress-to-Eggress tests still pass but are no longer the sole evidence.
8. Active parity docs are updated only after the external/oracle evidence passes.
9. No legacy/insecure cipher is enabled by default as part of this phase.
