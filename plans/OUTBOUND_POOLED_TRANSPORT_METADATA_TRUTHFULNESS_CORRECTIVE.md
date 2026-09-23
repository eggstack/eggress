# Outbound Pooled Transport Metadata Truthfulness Corrective

## Status

**IMPLEMENTED — 2026-09-23**

## Target repository

`eggstack/eggress`

Corrective baseline:

`c3b219e580d9c4b94f0c1f1844239d750f8bb9ba` (`main`, prepared workspace 1.0.9)

Depends on:

- `plans/POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md`

Related:

- `plans/OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md`
- `plans/OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`

## Objective

Make `OutboundInfo.local_addr` and `peer_addr` truthful when the first hop
uses a transport that may reuse a previously established physical connection.

The 1.0.9 metadata recovery correctly captures the newly opened TCP socket
before boxing. That is sufficient for direct, SOCKS, HTTP CONNECT, Raw, and
other handlers that continue using the supplied stream.

It is not sufficient for hop-0 SSH and H2, because those handlers may reuse an
existing cached/pool connection and discard the freshly opened stream. In that
case the metadata captured before the handshake refers to a socket that does
not carry the returned stream.

For the 1.0.9 corrective, prefer truthfulness over completeness:

> When hop 0 is SSH or HTTP/2, return unknown socket metadata unless the
> reusable layer itself can prove the actual physical connection addresses.

The route-isolation prerequisite makes nested SSH/H2 unpooled, so deeper
SSH/H2 hops can continue to preserve the first-hop metadata from the actual
supplied prefix stream.

Do not delay 1.0.9 on adding socket-address storage to SSH/H2 pools. That can
be a later additive optimization.

---

## Required semantics

After this corrective:

```text
direct TCP
  -> actual local_addr / peer_addr

hop 0 SOCKS / HTTP CONNECT / Raw / ordinary TCP-preserving wrapper
  -> actual first-hop local_addr / peer_addr

hop 0 SSH with session cache
  -> local_addr=None, peer_addr=None unless actual cached-session metadata
     is explicitly available

hop 0 H2 with connection pool
  -> local_addr=None, peer_addr=None unless actual pooled-connection metadata
     is explicitly available

hop N>0 SSH/H2 after route-isolation corrective
  -> nested transport is unpooled and consumes supplied chain stream;
     retain metadata for the chain's actual TCP-backed hop 0

Unix / QUIC / H3 first-hop transport
  -> None unless that transport already supplies truthful SocketAddr metadata
```

Unknown metadata is an expected supported result. Never report the address of a
discarded candidate socket as if it were the active transport.

---

# Global constraints

1. Preserve `OutboundInfo` field names and types.
2. Preserve every existing `connect_tcp*` method signature.
3. Preserve `ChainExecutor::execute()` and
   `execute_with_metadata()` signatures.
4. Preserve the `BoxStream` boundary.
5. Do not add downcasting or file-descriptor introspection.
6. Do not perform DNS lookup solely to populate metadata.
7. Do not infer active pool/session socket addresses from configured
   endpoints.
8. Metadata absence must not fail or alter a successful connection.
9. Route-isolation corrective must land first.
10. No direct fallback.
11. No version bump beyond the already-prepared 1.0.9 unless a separate
    release collision requires it.
12. No registry/tag publication in this plan.

---

# Workstream 0 — characterize stale metadata

Before editing, prove the failure mode with deterministic local tests where
possible.

For H2:

1. create a hop-0 H2 connector;
2. establish one stream so the H2 pool owns a physical connection;
3. make a second connect that reuses that pool entry;
4. prove the second supplied TCP stream is not the physical connection serving
   the returned H2 stream;
5. show current `OutboundInfo` still reflects the supplied/discarded socket.

For SSH, use the existing OpenSSH fixture/session cache:

1. establish a hop-0 SSH session;
2. open another channel through the same connector/cache;
3. prove cache reuse;
4. show the newly supplied transport can be discarded.

Do not depend on external Internet services.

---

# Workstream 1 — encode transport-identity preservation explicitly

Add a small internal/core concept describing whether a successful hop
handshake preserves the physical transport identity represented by current
metadata.

Do not add a new required method to the public `HopHandler` trait.

Preferred implementation options, in order:

1. a private `ChainExecutor` helper that recognizes protocols whose handler
   may replace/reuse the supplied physical transport;
2. an additive defaulted trait method only if it materially improves
   correctness/maintainability and remains source-compatible for third-party
   implementors.

For the prepared 1.0.9 behavior, SSH and HTTP/2 at hop 0 are the known
transport-reusing cases.

QUIC/H3 already use `open()` as non-TCP first-hop transports and begin with
empty metadata.

After a successful hop-0 SSH/H2 handshake, clear socket metadata unless the
handler returns metadata from the actual reused physical connection.

Do not clear metadata before the handshake succeeds; failures continue to
return errors exactly as today.

---

# Workstream 2 — preserve nested first-hop metadata after route isolation

Once `POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md` is implemented, SSH/H2
at hop index > 0 must use the supplied stream without cross-execution pooling.

Therefore:

- do not blanket-clear metadata merely because an SSH/H2 protocol appears
  later in a chain;
- preserve the metadata captured for the actual TCP-backed hop 0 when the
  nested handler is guaranteed to consume that supplied stream;
- clear metadata only where transport identity may actually be replaced.

This distinction matters for chains such as:

```text
SOCKS5 hop 0 -> H2 hop 1 -> target
SOCKS5 hop 0 -> SSH hop 1 -> target
```

After the route-isolation fix, the reported metadata should still describe the
SOCKS5 physical connection at hop 0.

---

# Workstream 3 — H2 regression coverage

Required tests:

1. direct/hop-0 ordinary HTTP CONNECT continues returning real metadata;
2. first hop H2 returns `None` metadata when pooling/reuse can occur;
3. repeated hop-0 H2 calls do not report the discarded second candidate
   socket;
4. H2 at hop index > 0 after a TCP-preserving hop retains the real hop-0
   metadata because nested H2 is unpooled by the prerequisite plan;
5. H2 timeout/auth/protocol errors are unchanged;
6. no additional DNS lookup occurs.

If the H2 pool is enhanced during implementation to store actual
`ConnectionMetadata` for every physical entry, it is acceptable to return
that exact metadata instead of `None`, but only if tests prove the metadata
belongs to the reused entry. Do not broaden the plan to require this.

---

# Workstream 4 — SSH regression coverage

Required tests under the SSH/OpenSSH lane:

1. first-hop SSH returns `None` metadata when cached session reuse can occur,
   unless actual cached-session metadata is available;
2. a second channel that reuses the SSH session does not report the newly
   discarded candidate transport's address;
3. nested SSH behind a preceding TCP proxy retains the real hop-0 metadata
   after the route-isolation plan makes nested SSH fresh/unpooled;
4. host-key verification/authentication behavior is unchanged;
5. session/channel reuse at hop 0 remains functional.

Again, storing exact socket metadata with cached SSH sessions is optional and
out of scope for the 1.0.9 correction.

---

# Workstream 5 — downstream-facing contract and Python behavior

Update Rust and Python-facing documentation so optional address metadata has an
accurate contract.

For `OutboundInfo`:

- `Some(addr)` means Eggress knows that address belongs to the physical
  transport carrying the returned stream;
- `None` means the transport does not expose trustworthy socket metadata;
- `None` is expected for pooled/reused hop-0 SSH/H2 in 1.0.9.

For Python `OutboundStream`:

- `peername` / `sockname` remain optional;
- direct TCP tests continue requiring populated values;
- pooled hop-0 SSH/H2 may expose `None`;
- do not synthesize strings from configuration.

Update as needed:

- `architecture/outbound.md`;
- `crates/eggress-outbound/README.md`;
- `docs/EMBED_API.md`;
- `docs/RUST_API.md`;
- Python documentation/tests;
- maintainer skills/comments.

Do not weaken the direct/SOCKS/HTTP metadata guarantees already added.

---

# Workstream 6 — release requalification

After both corrective plans are implemented, rerun the prepared 1.0.9
qualification.

Focused:

```sh
cargo fmt --all -- --check
cargo test -p eggress-core --locked
cargo test -p eggress-protocol-http --locked
cargo test -p eggress-transport-ssh --locked
cargo test -p eggress-outbound --locked
cargo test -p eggress-embed --locked --test outbound_detailed
cargo test -p eggress-embed --locked --test public_api
```

SSH:

```sh
EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked \
  --no-default-features --features ssh,pproxy-compat --test ssh
```

Python if metadata projections/tests change:

```sh
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests/test_outbound_stream_verification.py -q
```

Release/package checks:

```sh
scripts/release-preflight.sh --check-versions-only
cargo metadata --locked --format-version 1 >/dev/null
python3 scripts/publish-crates.py --list
CARGO_BUILD_JOBS=2 python3 scripts/publish-crates.py --dry-run
```

Full:

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
cargo check --manifest-path fuzz/Cargo.toml --bins
```

The release plan may return to `RELEASE QUALIFIED — UNPUBLISHED` only after
these gates pass on the final corrective SHA.

---

## Expected files touched

Likely:

```text
crates/eggress-core/src/chain.rs
crates/eggress-outbound/src/hops.rs
crates/eggress-outbound/src/connector.rs
crates/eggress-outbound/README.md
crates/eggress-embed/tests/outbound_detailed.rs
crates/eggress-embed/tests/ssh.rs
python/tests/test_outbound_stream_verification.py
architecture/outbound.md
docs/EMBED_API.md
docs/RUST_API.md
AGENTS.md
.skills/embed-outbound/skill.md
.skills/rust-proxy-dev/skill.md
plans/OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md
plans/OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md
docs/ROADMAP.md
plans/README.md
```

No version file should need another edit if 1.0.9 remains unpublished and
available.

---

## Non-goals

- no requirement to store socket metadata in SSH/H2 pools;
- no new pool/session introspection API;
- no public `OutboundInfo` type change;
- no resolver injection;
- no direct fallback;
- no QUIC/H3 metadata expansion;
- no pproxy parity change;
- no release publication/tagging.

---

## Stop conditions

Stop and record a blocker if:

- truthful metadata would require guessing from configuration;
- preserving nested first-hop metadata conflicts with route-isolation safety;
- a solution requires breaking `HopHandler`, `ChainExecutor`, or
  `OutboundConnector` signatures;
- tests cannot prove whether returned metadata belongs to the transport
  carrying the result;
- release qualification no longer passes at the selected patch version.

Unknown metadata is preferable to stale metadata.

---

## Acceptance criteria

This corrective is complete only when:

1. direct TCP metadata remains actual;
2. ordinary TCP-preserving proxy-hop metadata remains actual;
3. pooled/reused hop-0 H2 never reports metadata from a discarded candidate
   socket;
4. cached/reused hop-0 SSH never reports metadata from a discarded candidate
   socket;
5. nested H2/SSH after the route-isolation corrective preserve actual hop-0
   metadata;
6. Unix/QUIC/H3 semantics remain truthful;
7. no metadata-only DNS lookup is reintroduced;
8. no successful connection fails because metadata is unavailable;
9. Python optional metadata semantics match Rust;
10. current public API signatures remain unchanged;
11. docs distinguish actual metadata from unavailable metadata;
12. focused H2/SSH/outbound/embed tests pass;
13. required OpenSSH and Python lanes pass;
14. full workspace and release/package qualification pass;
15. completion record contains final SHA and exact transport-specific metadata
    semantics;
16. `OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md` is updated with
    fresh post-corrective evidence before 1.0.9 is considered publishable.

## Completion record

Implemented in `dfc19a0390e241f5255c8ad78dc2b50e214e537f` after route
isolation was implemented.

- `ChainExecutor` clears `ConnectionMetadata` only after a successful hop-zero
  SSH or H2 handshake. Nested SSH/H2 keeps the metadata from the actual
  TCP-backed hop zero because those handlers now consume the supplied stream.
- Direct TCP and TCP-preserving chain metadata behavior is unchanged. No
  metadata-only DNS, introspection, API signature, or `BoxStream` change was
  introduced. Missing metadata does not affect successful connects.
- Rust and Python-facing docs now define `Some(addr)` as belonging to the
  transport carrying the returned stream; pooled hop-zero SSH/H2 may return
  `None`. No Python projection code changed, so the existing direct metadata
  tests remain applicable without rebuilding the extension.
- Added a deterministic core regression proving hop-zero H2 clears candidate
  addresses, while the existing HTTP chain metadata test proves ordinary
  first-hop socket metadata remains available. `outbound_detailed` passed all
  23 tests, including direct and HTTP chain metadata.
- Verification passed: focused HTTP/SSH/outbound/core/embed tests, required
  OpenSSH fixture (3 passed), outbound feature slices, `cargo fmt`, workspace
  Clippy/tests, dependency policy, audit, fuzz compile, 1.0.9 version
  coherence, metadata generation, and the 28-crate non-mutating publish dry
  run. Existing policy warnings are recorded in the route-isolation
  completion record.
- Release qualification is unblocked but remains required before publication;
  the 1.0.9 tree remains unpublished and untagged.
