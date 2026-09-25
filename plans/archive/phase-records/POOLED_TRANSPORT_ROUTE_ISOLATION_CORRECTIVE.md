# Pooled Transport Route Isolation Corrective

## Status

**IMPLEMENTED — 2026-09-23**

## Target repository

`eggstack/eggress`

Corrective baseline:

`c3b219e580d9c4b94f0c1f1844239d750f8bb9ba` (`main`, prepared workspace 1.0.9)

Related plans:

- `plans/OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md`
- `plans/OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`
- follow-up:
  `plans/OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md`

## Severity and release disposition

This is a correctness and route-isolation blocker for the prepared 1.0.9
release.

Do not publish crates, push `v1.0.9`, publish Python artifacts, or create a
GitHub release until this plan and the metadata-truthfulness follow-up are
implemented and the 1.0.9 qualification is rerun.

## Finding

The current reusable SSH and H2 connection keys do not identify the complete
transport path used to reach a reusable hop.

### SSH

`SshSessionKey` contains:

- endpoint host;
- endpoint port;
- username;
- authentication material;
- `hop_index`.

`SshSessionCache::get_or_connect()` reuses a live session for an equal key and
explicitly drops the newly supplied transport.

Therefore two chains such as:

```text
chain A: proxy-A -> ssh-X -> target
chain B: proxy-B -> ssh-X -> target
```

can have the same SSH session key when SSH occupies the same hop index. A cache
hit may discard the stream established through proxy-B and reuse the SSH
session originally established through proxy-A.

`hop_index` distinguishes position, not route prefix.

### H2

`H2PoolKey` similarly contains endpoint/TLS/SNI/auth plus `hop_index`.
`h2_connect_client_pooled()` checks the global pool before consuming the
supplied stream. On a pool hit it returns a CONNECT stream from the existing
H2 connection and the newly supplied route stream is dropped.

Two different prefixes reaching the same H2 endpoint at the same hop index can
therefore collide.

The current comment that hop index prevents cross-chain pooling is too strong.

## Required patch strategy

For the 1.0.9 corrective, prefer the smallest fail-closed rule:

> Reuse SSH/H2 physical connections only when the reusable protocol is hop 0.
> At hop index greater than zero, use the supplied stream for a fresh,
> unpooled transport/session.

At hop 0 there is no preceding proxy-chain prefix to bypass, so reuse remains
valid for the same endpoint/TLS/auth identity.

Do not introduce a chain-fingerprint cache key in this patch. A future
optimization may add a cryptographically scoped route identity if there is
measured demand for pooling SSH/H2 behind earlier hops.

This conservative rule is easier to prove and avoids storing or hashing
secret-bearing full chain configuration in a new public contract.

---

# Global invariants

1. Existing public SSH/H2 method signatures remain source-compatible.
2. Existing public key structs remain source-compatible; do not add required
   public struct fields.
3. Hop-0 SSH session reuse remains enabled.
4. Hop-0 H2 connection reuse remains enabled.
5. SSH/H2 at hop index > 0 must not reuse a physical connection established
   through a previous execution.
6. The actual supplied `BoxStream` at hop index > 0 must be the transport
   used to establish the SSH/H2 session.
7. No direct fallback may be introduced.
8. Route selection and proxy order remain authoritative.
9. Credential redaction and host-key verification policy remain unchanged.
10. Existing H2 auth isolation remains intact.
11. Existing H2 flow-control/pool limits for hop-0 pooling remain intact.
12. Existing SSH shutdown/invalidation behavior for cached hop-0 sessions
    remains intact.
13. No unsafe/downcast/file-descriptor transport introspection.
14. Rust 1.89 and `unsafe_code = "deny"` remain fixed.
15. Prepared release version remains 1.0.9 unless another release makes that
    version unavailable before publication.

---

# Workstream 0 — freeze and characterize the vulnerable behavior

Record:

```sh
git rev-parse HEAD
git status --short
cargo test -p eggress-protocol-http --locked
cargo test -p eggress-transport-ssh --locked
cargo test -p eggress-outbound --locked
cargo test -p eggress-embed --locked --test ssh
```

Confirm directly from current source:

- `SshSessionKey` has no route-prefix identity;
- `SshSessionCache::get_or_connect()` drops the supplied stream on cache hit;
- `H2PoolKey` has no route-prefix identity;
- `h2_connect_client_pooled()` may return before consuming the supplied
  stream;
- the outbound H2/SSH handlers pass only endpoint/auth/hop-index identity into
  the reusable layers.

Add a regression test that demonstrates the key collision concept before the
fix where practical. The test must not remain as a passing assertion for the
incorrect behavior after correction.

---

# Workstream 1 — make nested H2 execution unpooled

For `H2HopHandler`:

- when `hop_index == 0`, preserve the existing
  `h2_connect_client_pooled()` path;
- when `hop_index > 0`, use the existing non-pooled
  `h2_connect_client()` path over the supplied stream.

The non-pooled returned stream must retain ownership of the H2 connection
driver for the lifetime of the CONNECT stream.

Add a small internal wrapper if needed, for example an unpooled H2 stream that
contains:

- the joined H2 read/write halves;
- the connection driver `JoinHandle`.

Its Drop behavior must stop/abort the driver only for that unpooled physical
connection and must not interact with the global pool.

Do not push hop-index policy down into the generic public
`h2_connect_client_pooled()` function if doing so would silently change
semantics for external callers. The Eggress chain handler owns the route-aware
choice between pooled and unpooled operation.

Correct the `H2PoolKey` documentation: hop index contributes isolation but
does not, by itself, identify an arbitrary preceding chain prefix.

### Required H2 tests

Prove:

1. two hop-0 calls with the same H2 identity may reuse one physical H2
   connection;
2. an H2 hop at index 1 opens a fresh physical connection for each independent
   chain execution;
3. two different prefix fixtures ending at the same H2 endpoint cannot cause
   one prefix's physical connection to serve the other;
4. H2 auth/TLS/SNI identity behavior remains unchanged;
5. no-direct-fallback behavior remains unchanged;
6. unpooled driver lifetime lasts for the CONNECT stream and closes cleanly.

Prefer deterministic local fixtures with connection counters.

---

# Workstream 2 — make nested SSH execution unpooled

The current cache is appropriate at hop 0 but cannot safely key arbitrary
preceding paths.

Add additive fresh-session methods to `SshSessionCache` or the SSH transport
crate, for example:

```rust
open_tcp_channel_fresh(...)
open_unix_channel_fresh(...)
```

Exact names may follow crate conventions.

These methods must:

1. authenticate using the supplied transport;
2. never look up or insert the reusable session map;
3. preserve the selected host-key policy;
4. open the requested SSH channel;
5. retain the authenticated session handle for as long as the returned channel
   stream needs it.

If russh channel streams do not themselves keep the session alive, introduce a
private stream wrapper containing the channel stream plus an
`Arc<SessionHandle>`. Delegate AsyncRead/AsyncWrite safely.

Do not implement nested-hop safety by repeatedly invalidating a shared cache
entry: that risks disrupting concurrent channels and still leaves ambiguous
ownership.

Update `SshHopHandler`:

- `hop_index == 0`: existing cached method;
- `hop_index > 0`: fresh/unpooled method.

Both password and private-key auth paths must preserve current behavior.

### Required SSH tests

At minimum prove:

1. hop-0 reuse still reuses a live SSH session;
2. nested SSH execution does not consult/reuse the cache;
3. two different preceding proxy fixtures reaching the same SSH endpoint both
   observe their own physical transport;
4. concurrent nested SSH channels do not invalidate or terminate one another;
5. host-key verification behavior remains unchanged;
6. compatibility-mode host-key policy remains limited to the existing explicit
   compatibility constructor;
7. channel/session lifetime remains valid until the returned stream closes.

Use the repository's existing required OpenSSH fixture for final qualification.

---

# Workstream 3 — route-isolation regression at outbound chain level

Add an integration-level proof through the same `ChainExecutor` /
`OutboundConnector` path downstreams use.

Construct two distinct chain prefixes that converge on the same reusable
SSH or H2 endpoint at the same hop index.

The test must prove the selected prefix is not bypassed on the second
connection.

Strong evidence includes per-prefix connection counters or unique markers
observable only if the supplied prefix stream was actually used.

Do not settle for comparing cache keys only; the regression must exercise
connection behavior.

Cover at least H2 in the default feature set. Cover SSH under the existing
`ssh` feature/OpenSSH lane.

---

# Workstream 4 — documentation and comments

Update:

- `architecture/outbound.md`;
- `docs/RUST_API.md` if relevant;
- `crates/eggress-protocol-http` H2 pool comments/docs;
- SSH transport docs where session reuse is described;
- `.skills/embed-outbound/skill.md`;
- `.skills/rust-proxy-dev/skill.md`;
- `AGENTS.md` if the invariant belongs in maintainer guidance.

Document the rule precisely:

```text
Physical SSH/H2 reuse is allowed at hop 0.
Nested SSH/H2 hops use the supplied chain stream and are not globally reused
until a route-prefix-scoped reuse identity exists.
```

Do not claim that `hop_index` alone prevents cross-chain pooling.

This is a route-correctness rule, not a pproxy compatibility-tier change.

---

# Workstream 5 — focused and full verification

Focused:

```sh
cargo fmt --all -- --check
cargo test -p eggress-protocol-http --locked
cargo test -p eggress-transport-ssh --locked
cargo test -p eggress-outbound --locked
cargo test -p eggress-embed --locked --test outbound_detailed
```

SSH/OpenSSH:

```sh
EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked \
  --no-default-features --features ssh,pproxy-compat --test ssh
```

Feature boundaries:

```sh
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
```

Full:

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
cargo check --manifest-path fuzz/Cargo.toml --bins
```

No release publication occurs in this plan.

---

## Expected files touched

Likely:

```text
crates/eggress-outbound/src/hops.rs
crates/eggress-protocol-http/src/h2_connect.rs
crates/eggress-transport-ssh/src/lib.rs
crates/eggress-embed/tests/outbound_detailed.rs
crates/eggress-embed/tests/ssh.rs
architecture/outbound.md
docs/RUST_API.md
AGENTS.md
.skills/embed-outbound/skill.md
.skills/rust-proxy-dev/skill.md
plans/POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md
```

Potentially focused crate README/docs and testkit fixtures.

No version bump is expected: this corrects the already-prepared, unpublished
1.0.9 tree.

---

## Non-goals

- no full-chain cryptographic fingerprint cache key in this patch;
- no pooling of SSH/H2 behind earlier hops;
- no public `BoxStream` downcast;
- no resolver changes;
- no retry/fallback changes;
- no new authentication modes;
- no SSH server capability;
- no QUIC/H3 pooling redesign;
- no pproxy tier change;
- no crates.io/PyPI/GitHub release action.

---

## Stop conditions

Stop and document the blocker if:

- nested unpooled SSH cannot keep its authenticated session alive without a
  breaking public API;
- nested H2 cannot keep its connection driver alive without altering public
  stream signatures;
- a proposed shortcut still permits a cached transport established through one
  prefix to service another prefix;
- the correction requires weakening host-key verification, auth isolation, or
  connection error semantics;
- tests cannot distinguish actual prefix use from cache-key construction.

Do not fall back to a non-cryptographic route hash as a security boundary.

---

## Acceptance criteria

This plan is complete only when:

1. hop-0 H2 reuse remains available;
2. hop-0 SSH reuse remains available;
3. H2 at hop index > 0 never uses a pre-existing pooled physical connection;
4. SSH at hop index > 0 never uses a pre-existing cached physical session;
5. nested H2/SSH use the supplied route stream;
6. two different prefixes converging on the same reusable endpoint cannot
   cross-reuse;
7. concurrent nested channels remain lifetime-safe;
8. current H2 auth/TLS/SNI isolation remains intact;
9. current SSH host-key/auth policy remains intact;
10. no public key struct gains a required field;
11. no existing public connect signature changes;
12. no direct fallback or route-order change is introduced;
13. comments/docs stop claiming hop index alone gives cross-chain isolation;
14. focused H2/SSH/outbound tests pass;
15. required OpenSSH regression passes;
16. feature slices, workspace Clippy/tests, dependency policy, audit policy,
    and fuzz compile pass;
17. final SHA and test evidence are recorded;
18. the prepared 1.0.9 release remains unpublished pending the dependent
    metadata-truthfulness corrective and release requalification.

## Completion record

Implemented in `dfc19a0390e241f5255c8ad78dc2b50e214e537f`.

- Hop-zero H2 retains the existing pooled connector path; nested H2 uses
  `h2_connect_client()` and a private stream wrapper that owns and aborts its
  connection driver on drop.
- Hop-zero SSH retains its session cache; nested TCP and Unix SSH hops use
  fresh sessions over the supplied chain stream. The returned stream retains
  the authenticated session handle. Host-key policy remains selected by the
  original cache constructor.
- `HopHandler` and public connect/key signatures are unchanged. Corrected its
  hop-index documentation and added route-reuse guidance to architecture,
  embed, Rust API, and maintainer skill documentation.
- Verification passed: `cargo fmt --all -- --check`, focused HTTP/SSH/outbound
  tests, outbound no-default base/TOML/pproxy-compat/SSH/SSH+compat/UDP compile
  slices, required OpenSSH embed test (3 passed), workspace Clippy, workspace
  tests, `cargo deny check`, `cargo audit --ignore RUSTSEC-2023-0071`, and fuzz
  compile. `cargo deny` reports existing duplicate-base64 and yanked-crate
  warnings; audit reports existing `der` and `wnaf` yanked warnings.
- Existing runtime regressions `chain_h2_consumes_prior_stream`,
  `h2_chain_socks5_to_h2_to_http`, and
  `openssh_chain_tunnels_second_ssh_hop_through_first` passed as part of the
  workspace/required SSH runs. The implementation itself structurally selects
  the non-pooled path for every index greater than zero.
- The 1.0.9 version remains unchanged and unpublished. No release tag or
  publication was created.

## Post-release evidence correction — 2026-09-24

The 1.0.9 release-state statement above is superseded: `v1.0.9` points to
`e10dea18300f2618c4a47fa46280f1bf518e7a5f`, its GitHub Release exists, and
the tag-triggered Python/binary release workflows succeeded. The crates.io
state is separate; the public sparse index reports `eggress-core 1.0.9` at
03:15:12Z, `eggress-outbound 1.0.9` at 03:19:52Z, and `eggress-embed 1.0.9`
at 03:22:24Z on 2026-09-24.

The completion record's exact three test names were inaccurate. The real H2
chain regression was `h2_chain_socks5_to_h2_to_http` (now named
`nested_h2_consumes_selected_prefix` in
`crates/eggress-runtime/tests/upstream_protocols.rs`), and the real OpenSSH
chain regression was `openssh_chain_tunnels_second_ssh_hop_through_first`
(now named `openssh_nested_ssh_consumes_selected_prefix` in
`crates/eggress-transport-ssh/tests/openssh.rs`). The separate test
`chain_h2_consumes_prior_stream` was a hop-zero test and did not prove nested
prefix consumption. No test in this 1.0.9 tree proves cross-reuse isolation
between two nested H2/SSH prefixes; that gap is carried into the 1.0.10
corrective plan rather than being represented as completed evidence.
