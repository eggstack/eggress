# Post-Parity Phase 3 — SOCKS5 Protocol Correctness

## Status

**PLANNED**

Parent roadmap: `POST_PARITY_CORRECTIVE_AND_REDUCTION_ROADMAP.md`

## Problem statement

Two bounded SOCKS5 correctness issues remain at the codec boundary.

First, TCP request parsers read the RFC 1928 reserved byte but do not reject a
non-zero value. The UDP decoder already rejects a non-zero reserved field, so
TCP and UDP currently enforce different validity rules.

Second, `SocksAddr::encode_reply()` accepts a domain string longer than the
one-byte SOCKS domain length and silently truncates it to 255 bytes. Silent
target rewriting is undesirable even if normal parsed requests cannot construct
that value.

These are protocol hygiene issues. They do not justify a SOCKS subsystem
redesign.

## Objective

Make SOCKS5 address/request handling fail explicitly on invalid wire/state inputs
instead of accepting or silently rewriting them.

## Likely files

Primary:

```text
crates/eggress-protocol-socks/src/socks5/server.rs
crates/eggress-protocol-socks/src/socks5/udp_codec.rs
crates/eggress-protocol-socks/src/error.rs
crates/eggress-protocol-socks/tests/
```

Potential callers to audit after changing encoding:

```text
crates/eggress-server/src/reply.rs
crates/eggress-udp/
crates/eggress-protocol-socks/src/socks5/client.rs
```

## Implementation requirements

### 1. Reject non-zero TCP RSV

Apply the RFC invariant to every SOCKS5 TCP request parser that consumes:

```text
VER CMD RSV ATYP ...
```

This includes synchronous parsing exposed for fuzzing and asynchronous request
reading used by the server.

A non-zero RSV must produce a deterministic protocol error before routing or
outbound connection creation.

Prefer a dedicated error variant if one improves diagnostics, for example:

```rust
InvalidReservedByte(u8)
```

Do not reuse an unrelated malformed-address error merely to avoid one enum
variant.

### 2. Keep UDP behavior consistent

The existing UDP decoder rejects either non-zero reserved byte. Preserve that
behavior.

Tests should make the TCP/UDP distinction explicit:

- TCP RSV byte must be zero;
- UDP RSV field bytes must both be zero;
- UDP FRAG handling remains unchanged.

This phase does not implement UDP fragmentation.

### 3. Eliminate silent domain truncation

Do not preserve:

```rust
domain.len().min(255)
```

as the public encoding behavior.

Choose the smallest API-safe approach after auditing callers:

#### Preferred if signature change is local

Make address encoding fallible:

```rust
fn encode_reply(&self) -> Result<Vec<u8>, Socks5Error>
```

or provide a checked encoder used by all production paths.

Return a specific overlong-domain error when the encoded domain is >255 bytes.

#### Acceptable if changing the public signature is too disruptive

Validate `SocksAddr::Domain` construction at the boundary used by production
callers and make the unchecked encoder private/internal with a proved invariant.

Do not silently truncate and do not panic on untrusted network input.

### 4. Consider byte length, not character count

The SOCKS5 domain length is encoded in bytes. Any validation must use the UTF-8
byte length actually written to the wire.

No IDNA normalization work is authorized in this phase.

### 5. Preserve valid-wire behavior

For valid addresses:

- IPv4 replies are byte-for-byte unchanged;
- IPv6 replies are byte-for-byte unchanged;
- domain replies <=255 bytes are byte-for-byte unchanged;
- CONNECT, UDP ASSOCIATE, auth negotiation, and BIND refusal semantics are
  unchanged.

### 6. Do not expand scope

Do not:

- implement SOCKS5 BIND;
- implement UDP fragmentation;
- add GSSAPI;
- redesign `SocksAddr` across the workspace;
- change DNS resolution policy;
- alter authentication policy;
- add new pproxy parity claims.

## Required tests

Add focused unit/property cases for:

```text
parse_socks5_request: RSV=0 -> accepted
parse_socks5_request: RSV=1 -> rejected
async read_socks5_request: RSV=1 -> rejected
UDP RSV[0]!=0 -> rejected
UDP RSV[1]!=0 -> rejected
domain length 255 bytes -> encodes successfully
domain length 256 bytes -> explicit error
multibyte UTF-8 domain whose byte length >255 -> explicit error
IPv4 encode/decode regression
IPv6 encode/decode regression
valid domain encode/decode regression
```

If production callers change to a fallible encoder, tests must prove the error is
translated into an existing protocol/session failure rather than an unwrap/panic.

Run:

```bash
cargo test -p eggress-protocol-socks
cargo test -p eggress-server socks5
cargo test -p eggress-udp socks5
```

Then the normal workspace gate.

Fuzzing is optional and not an acceptance gate. If the changed parser already has
a fuzz target, a compile-check is reasonable:

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Do not add fuzzing to hosted CI.

## Explicit acceptance criteria

Phase 3 is complete only when:

1. Every SOCKS5 TCP request parser validates RSV == 0.
2. A non-zero TCP RSV is rejected before routing/outbound connection creation.
3. Synchronous parser tests cover non-zero RSV.
4. Asynchronous parser/handshake tests cover non-zero RSV.
5. Existing UDP reserved-field rejection remains unchanged.
6. UDP fragmentation behavior remains unchanged and unsupported.
7. `SocksAddr` encoding never silently truncates an overlong domain.
8. A 255-byte domain can be encoded successfully.
9. A 256-byte domain returns an explicit error or is impossible to construct on
   the production path through a checked invariant.
10. UTF-8 byte length, not character count, determines the SOCKS domain limit.
11. No untrusted overlong domain path can panic.
12. Valid IPv4 wire output is unchanged.
13. Valid IPv6 wire output is unchanged.
14. Valid <=255-byte domain wire output is unchanged.
15. SOCKS5 CONNECT behavior is unchanged for valid requests.
16. SOCKS5 UDP ASSOCIATE behavior is unchanged for valid requests.
17. SOCKS5 username/password auth behavior is unchanged.
18. SOCKS5 BIND remains unsupported.
19. No new protocol feature or dependency is added.
20. `cargo test -p eggress-protocol-socks` passes.
21. Relevant `eggress-server` and `eggress-udp` SOCKS tests pass.
22. `cargo fmt --all -- --check` passes.
23. `cargo clippy --workspace --all-targets -- -D warnings` passes.
24. `cargo test --workspace --locked` passes.

## Stop condition

If all production construction paths already prove domain length <=255 and only a
test/fuzz helper can construct the invalid enum state, still remove misleading
silent truncation from any public encoder. Do not redesign the type solely to
make an impossible production state unrepresentable.
