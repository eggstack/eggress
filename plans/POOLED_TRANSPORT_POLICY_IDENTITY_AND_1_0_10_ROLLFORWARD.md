# Pooled Transport Policy Identity and 1.0.10 Roll-Forward Corrective

## Status

**IMPLEMENTED AND LOCALLY QUALIFIED — POST-1.0.9 CORRECTIVE — 2026-09-24**

## Target repository

`eggstack/eggress`

Planning baseline:

`e10dea18300f2618c4a47fa46280f1bf518e7a5f` (`main`, lockstep workspace 1.0.9)

Published/tagged baseline facts:

- annotated tag `v1.0.9` points to
  `e10dea18300f2618c4a47fa46280f1bf518e7a5f`;
- GitHub Release `eggress v1.0.9` exists;
- the `v1.0.9` tag-triggered Python publication workflow completed
  successfully;
- the `v1.0.9` CLI binary release workflow completed successfully;
- ordinary CI on the tagged commit completed successfully;
- crates.io publication is a separate manual path and must be discovered and
  recorded explicitly during this corrective rather than inferred from the
  GitHub/Python release state.

Related historical plans:

- `plans/OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md`
- `plans/POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md`
- `plans/OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md`
- `plans/OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`

## Purpose

Close the remaining hop-zero reusable-transport policy-isolation gaps that were
not covered by the 1.0.9 nested-route corrective, replace overstated test
evidence with real behavioral regressions, and roll the correction forward to
a new immutable patch line.

Because `v1.0.9` has already been tagged and release artifacts have been
created, **do not amend, move, delete, or recreate the 1.0.9 tag**. The
corrective release target is the next unused lockstep patch version, expected
to be **1.0.10**.

The implementation should retain the useful 1.0.9 behavior:

- nested SSH/H2 never globally reuses a physical transport reached through a
  preceding chain prefix;
- hop-zero SSH/H2 may reuse a physical connection when its complete
  connection policy is equivalent;
- direct/SOCKS/HTTP-style metadata remains truthful;
- hop-zero reusable SSH/H2 does not report metadata from a discarded candidate
  socket.

The missing invariant is:

> A reusable physical transport may be selected only when all policy that
> determines how that physical transport was established is equivalent.

---

# Confirmed residual findings

## Finding A — H2 pool scope can cross executor/TLS policy boundaries

The H2 registry is process-global:

```rust
pub static H2_POOL_REGISTRY: LazyLock<H2PoolRegistry>
```

Its public key contains:

- endpoint host;
- endpoint port;
- TLS yes/no;
- server name;
- credential digest;
- hop index.

It does **not** identify the `rustls::ClientConfig` / trust policy used by the
`ChainExecutor` that created the supplied TLS stream.

`build_chain_executor*` can construct executors with different TLS overrides
or trust roots, but `H2HopHandler` currently consults the same global H2
registry. On a pool hit, `h2_connect_client_pooled()` can discard the supplied
stream after that stream has already passed through the current executor's
connection/TLS policy.

Consequently a physical H2 connection created by executor A can be reused by
executor B when their visible `H2PoolKey` fields collide, even if B's TLS
configuration would not have established that connection itself.

This must be fixed before another patch release.

## Finding B — explicit local-bind policy is omitted from H2 reuse identity

The first-hop TCP socket is created by `ChainExecutor` using
`ProxyHopSpec.local_bind`.

The H2 pool key has no local-bind identity. A later execution requesting a
different explicit source bind can therefore create a candidate socket with the
requested bind and then discard it in favor of a pooled H2 connection created
under another bind policy.

An explicit `local_bind` is a connection policy, not metadata. It must not be
silently bypassed by reuse.

## Finding C — insecure/verified H2 policy is not an explicit reuse boundary

`ProxyHopSpec.insecure` affects TLS verification before the H2 handshake but
is not represented in `H2PoolKey`.

Even if executor-local scoping separates unrelated TLS override objects,
verified and explicitly insecure hops within one long-lived executor must not
share the same physical H2 connection merely because endpoint/SNI/auth match.

For the corrective patch, the safe rule is to make explicitly insecure H2
connections non-pooled unless an exact policy-scoped pool identity is proven.

## Finding D — SSH hop-zero cache can bypass explicit local-bind policy

`SshSessionCache` is already executor/cache-instance scoped, so its host-key
policy does not have the process-global H2 problem.

However, `SshSessionKey` does not include `local_bind`. At hop zero a cache
hit can discard a newly created TCP socket that honored a requested bind and
reuse an older SSH session created with a different source-bind policy.

The existing public `SshSessionKey` shape must not be broken merely to fix
this patch.

## Finding E — corrective completion evidence names tests that are not present

The 1.0.9 route-isolation completion record cites:

- `chain_h2_consumes_prior_stream`;
- `h2_chain_socks5_to_h2_to_http`;
- `openssh_chain_tunnels_second_ssh_hop_through_first`.

Those exact regression names are not present in the current repository tree.

The structural implementation is useful, and workspace/OpenSSH tests were
green, but the plan's acceptance record overstates the existence of explicit
behavioral route-isolation regressions.

This corrective must add actual tests and record their exact file/test names.

## Finding F — release/planning records describe a pre-tag state

Current planning documentation still says 1.0.9 is unpublished / awaiting
requalification.

That is no longer true for the GitHub/tag/Python/binary release surfaces:
`v1.0.9` exists and its tag-triggered release workflows succeeded.

The historical record must be corrected append-only. Do not erase the earlier
qualification or post-qualification audit.

---

# Chosen corrective architecture

Use conservative policy boundaries rather than expanding public cache-key
structs.

## H2

For the Eggress chain-execution path:

1. stop using one process-global H2 pool namespace across unrelated
   `ChainExecutor` instances;
2. give the concrete H2 hop handler a long-lived pool registry scoped to the
   executor/service composition lifetime;
3. preserve pooling only for hop-zero H2 when:
   - `local_bind.is_none()`;
   - `hop.insecure == false`;
   - the remaining existing key identity (endpoint/TLS/SNI/auth) matches;
4. nested H2 remains unpooled exactly as in 1.0.9;
5. explicit local-bind or insecure hop-zero H2 must consume the supplied
   stream through the unpooled H2 client path.

This ensures:

- TLS override/root configuration is isolated by executor-owned registry
  lifetime;
- explicit source-bind requests cannot be bypassed by a pool hit;
- insecure and verified establishment are not silently collapsed into one
  physical connection.

Do **not** add required fields to the public `H2PoolKey` struct.

The existing global registry/public pooled helper may remain for source
compatibility, but Eggress's concrete chain handler must use the new scoped
authority. If a new additive helper is needed, prefer something equivalent to:

```rust
h2_connect_client_pooled_in_registry(
    registry: &H2PoolRegistry,
    stream,
    target,
    auth,
    pool_key,
)
```

Exact naming is implementation-defined.

## SSH

Preserve the existing executor-owned `SshSessionCache`.

At hop zero:

- if `local_bind.is_none()`, existing cached SSH behavior may remain;
- if `local_bind.is_some()`, use the already-added fresh-session path and do
  not consult/insert the cache.

Nested SSH remains fresh/unpooled as implemented in 1.0.9.

Do **not** add a required field to public `SshSessionKey`.

## Release

Because 1.0.9 is already tagged, the corrective uses a new lockstep patch
version. If 1.0.10 is unused when implementation reaches release preparation,
use 1.0.10. Otherwise choose the next unused patch.

No plan step may move `v1.0.9` or attempt to overwrite any immutable
registry artifact.

---

# Global invariants

1. Never move, delete, or recreate `v1.0.9`.
2. Preserve all existing public connection method signatures.
3. Preserve public `H2PoolKey` and `SshSessionKey` struct shapes.
4. Preserve nested SSH/H2 unpooled behavior from 1.0.9.
5. Preserve hop-zero reuse for ordinary equivalent-policy connections.
6. Explicit `local_bind` must never be bypassed by reuse.
7. A TLS trust/verification policy must never be bypassed by reuse.
8. Explicit insecure H2 must not reuse a verified pooled connection or vice
   versa.
9. No proxy/transport failure may fall back direct.
10. No metadata-only DNS lookup may return.
11. `OutboundInfo::Some(addr)` must still describe the physical transport
    carrying the returned stream.
12. No unsafe/downcast/file-descriptor transport introspection.
13. Rust 1.89 and `unsafe_code = "deny"` remain unchanged.
14. Existing credential redaction and SSH host-key behavior remain unchanged.
15. Do not claim a behavioral regression exists unless the named test is
    present in the final tree and executed.

---

# Workstream 0 — correct the release-state record before coding

Before implementation changes, reconcile the planning apparatus with the actual
1.0.9 release state.

Append a dated post-release correction to
`OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md` recording:

- `v1.0.9` tag exists and points to
  `e10dea18300f2618c4a47fa46280f1bf518e7a5f`;
- GitHub Release `eggress v1.0.9` exists;
- tag-triggered Python and binary release workflows succeeded;
- ordinary CI on the tagged commit succeeded;
- the prior phrase “unpublished” is superseded for those surfaces;
- crates.io status must be queried independently because native crate
  publication is manual.

Do not edit away earlier evidence.

Discover actual registry state for at least:

- `eggress-core 1.0.9`;
- `eggress-outbound 1.0.9`;
- `eggress-embed 1.0.9`.

Record exact results and timestamps. Do not infer crates.io state from tags,
GitHub releases, PyPI, or local package dry-runs.

Update `docs/ROADMAP.md` and `plans/README.md` so 1.0.9 is historical and
this corrective is the only active handoff.

---

# Workstream 1 — inventory H2 pooling ownership and lifetime

Before changing the pool:

1. enumerate all uses of `H2_POOL_REGISTRY`;
2. enumerate all constructors/callers of `H2HopHandler`;
3. determine the lifetime of every `ChainExecutor` created by:
   - listener-free `OutboundConnector`;
   - `eggress-server`;
   - `eggress-embed`;
   - runtime/reload paths;
4. verify the selected scoped registry lifetime preserves useful reuse across
   connections within one stable service/executor, rather than accidentally
   creating a new pool for every request;
5. identify public direct consumers of
   `h2_connect_client_pooled()` / `H2_POOL_REGISTRY`.

Record the result in the completion record.

Do not simply replace the global static with a per-call registry.

---

# Workstream 2 — add executor/service-scoped H2 pooling

Refactor the concrete outbound executor construction so the H2 handler owns or
receives an `Arc<H2PoolRegistry>` whose lifetime matches the executor/service
composition boundary.

Preferred shape:

```rust
pub(crate) struct H2HopHandler {
    pool_registry: Arc<H2PoolRegistry>,
}
```

and `build_chain_executor_inner()` constructs one registry for the executor
unless a deliberately shared scoped registry is supplied by an existing
long-lived owner.

Add an additive protocol-HTTP API if required to execute pooled H2 against a
caller-provided registry while preserving the existing global helper for
source compatibility.

Do not create two implementations of H2 pool acquisition. Existing/global and
scoped entry points should delegate to one internal authority.

### Required isolation test

Add an exact regression named:

`h2_pool_is_scoped_to_executor_tls_policy`

The test must prove behavior, not inspect pointer inequality. The requested
untrusted-certificate fixture was reviewed against `ChainExecutor::execute()`:
TLS establishment occurs before `H2HopHandler::handshake()` performs pool
lookup. Therefore executor B fails certificate validation even with the old
global registry, so that fixture cannot establish a regression against 1.0.9.
The test criterion is adjusted to verify same-policy registry sharing and
different-policy registry isolation, then prove the different scopes establish
separate physical H2 connections. The public helper remains available for
callers that deliberately own a registry. No external network is used.

Use a local TLS/H2 proxy fixture:

1. executor A uses a TLS client configuration that trusts the fixture and
   successfully establishes/pools the physical H2 connection;
2. executor B targets the same endpoint/SNI/auth but uses a TLS configuration
   that does not trust that certificate;
3. executor B must fail its own TLS establishment rather than reuse A's pooled
   physical connection.

The registry isolation assertion fails against the 1.0.9 global-registry
behavior; the physical-connection assertion exercises the selected scope.

No external network is permitted.

---

# Workstream 3 — make explicit H2 connection policies non-poolable

In `H2HopHandler::handshake()`, define one explicit pool-eligibility
authority.

For the 1.0.10 corrective:

```text
pool eligible =
    hop_index == 0
    && hop.local_bind.is_none()
    && !hop.insecure
```

Nested H2 remains unpooled regardless.

Hop-zero H2 with explicit `local_bind` must call the unpooled H2 client using
the supplied stream.

Hop-zero H2 with `insecure=true` must also call the unpooled path, ensuring
the current execution's explicitly insecure TLS stream is the transport that
carries the returned H2 stream.

Do not create a candidate socket and then intentionally discard it under these
non-poolable policies.

### Required tests

Add exact behavioral regressions:

- `h2_hop_zero_same_policy_reuses_physical_connection`
- `h2_hop_zero_local_bind_is_not_pooled`
- `h2_hop_zero_insecure_is_not_pooled`

Use local connection counters.

For local-bind:

- select an explicit source address/port deterministically;
- verify the H2 proxy observes the requested source endpoint;
- perform a preceding ordinary pooled connection with the same proxy identity
  and prove it cannot satisfy the explicitly bound execution.

Avoid platform-fragile assumptions about arbitrary non-loopback interfaces.
A loopback exact-port helper is acceptable if reservation/retry is bounded and
race-safe.

For insecure:

- use a local TLS fixture and prove the insecure execution opens its own
  physical connection rather than consuming an existing verified pool entry.

---

# Workstream 4 — enforce SSH local-bind policy

Update `SshHopHandler` selection:

```text
cached SSH allowed =
    hop_index == 0
    && hop.local_bind.is_none()
```

Otherwise use the existing fresh-session TCP/Unix path.

This reuses the 1.0.9 lifetime-safe `SessionStream` design.

Do not invalidate a shared cached session to emulate isolation.

### Required tests

Add exact regressions under the SSH/OpenSSH lane:

- `ssh_hop_zero_default_policy_reuses_session`
- `ssh_hop_zero_local_bind_uses_fresh_session`
- `openssh_nested_ssh_consumes_selected_prefix`

The local-bind test must prove a second execution with explicit bind opens its
own TCP/SSH session rather than reusing the preexisting hop-zero cached
session.

The nested test must actually demonstrate that the selected preceding path is
consumed. A test that only checks `hop_index` branches or key inequality does
not satisfy this criterion.

Preserve known-hosts and compatibility-mode behavior.

---

# Workstream 5 — replace claimed-but-absent route regressions with real tests

Audit the completion record in
`POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md`.

For every specifically named test in that record:

- if the test exists under a different name, correct the record to its exact
  real name and file;
- if the behavior is covered only indirectly, say so explicitly;
- if no equivalent behavioral test exists, add one.

At minimum the final tree must contain named behavioral tests covering:

1. nested H2 consumes the selected prior stream;
2. nested SSH consumes the selected prior stream;
3. two route prefixes cannot cross-reuse one nested physical H2 connection;
4. two route prefixes cannot cross-reuse one nested SSH session.

Recommended exact names:

- `nested_h2_consumes_selected_prefix`
- `nested_h2_does_not_cross_reuse_prefixes`
- `openssh_nested_ssh_consumes_selected_prefix`
- `openssh_nested_ssh_does_not_cross_reuse_prefixes`

If repository conventions suggest clearer names, record the actual final names
verbatim in the completion record.

Use per-prefix connection counters/markers so the test proves which route was
used.

---

# Workstream 6 — audit all physical-connection reuse keys

Before declaring closure, perform a narrow repository audit for every cache or
pool that can discard a newly supplied route/transport stream.

Search for:

- session maps;
- connection pools;
- global registries;
- `drop(transport)` or equivalent candidate disposal;
- acquire-before-consume patterns;
- reusable protocol connections.

For each, record:

- scope/lifetime;
- cache key;
- connection-affecting policy represented in that key/scope;
- whether the supplied route stream can be discarded;
- whether cross-route reuse is intended and safe.

This plan only requires code changes for the SSH/H2 findings above unless the
audit finds the same concrete invariant violation elsewhere.

If another reusable transport can bypass route/bind/trust policy, stop release
preparation and add it to this plan's implementation before qualification.

Do not broaden into generic performance tuning.

---

# Workstream 7 — version roll-forward to the next patch

After implementation and focused tests are green, determine the next unused
lockstep patch.

Because `v1.0.9` already exists, never reuse 1.0.9 for corrected source.

Expected version:

`1.0.10`

If 1.0.10 is already used on any required release surface, select the next
unused patch.

Apply the repository's established lockstep process:

1. root workspace/package version;
2. exact internal workspace dependency pins;
3. Python package versions;
4. pproxy compatibility Python dependency;
5. local Python development metadata;
6. `Cargo.lock` and fuzz lockfile as required.

Run:

```sh
scripts/release-preflight.sh --check-versions-only
cargo metadata --locked --format-version 1 >/dev/null
```

No tag or publication is authorized by this implementation plan.

## Implementation and qualification record — 2026-09-24

- Crates.io sparse-index records showed non-yanked `eggress-core`,
  `eggress-outbound`, and `eggress-embed` 1.0.9 releases published on
  2026-09-24. This corrected earlier repository notes that described 1.0.9 as
  unpublished. No 1.0.10 versions or tag existed at implementation time.
- Rolled all Rust workspace pins and Python package metadata to 1.0.10.
- H2 pools now share by retained `Arc<ClientConfig>` policy identity with a
  bounded 64-policy registry cache; runtime reload clears these registries.
  Nested H2, explicit source-bind H2, and insecure H2 use the supplied stream
  without pooled candidate acquisition.
- SSH reuse is limited to hop zero without explicit source bind. Nested and
  source-bound SSH use fresh sessions.
- Added behavioral tests for H2 TLS-policy registry separation, distinct
  physical connections, source-bind socket selection, default SSH reuse,
  source-bind SSH fresh sessions, nested H2 selected-prefix behavior and
  cross-prefix isolation, and nested SSH selected-prefix behavior and
  cross-prefix isolation.
- Documentation updated across README, AGENTS.md, architecture notes, public
  API notes, roadmap, and the relevant `.skills/` guides.
- Local qualification passed: `cargo test --workspace --locked` (2,971
  passed, 151 ignored, 136 suites), `cargo clippy --workspace --all-targets
  --locked -- -D warnings`, `cargo fmt --all -- --check`, all required outbound
  no-default feature slices, the CLI feature matrix, and required OpenSSH
  embed tests. Python tests passed (2,311 passed, 115 skipped); deny, audit,
  fuzz-bin check, release-preflight version check, and cargo metadata also
  passed. The Python native extension was developed at 1.0.10.
- Remote commit/CI verification is pending. This work intentionally does not
  create a tag or publish crates/Python artifacts.

---

# Workstream 8 — full correctness and release qualification

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

Runtime/multihop tests containing the new route regressions must run
explicitly.

Required SSH lane:

```sh
EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked \
  --no-default-features --features ssh,pproxy-compat --test ssh
```

Run any new OpenSSH multihop test target explicitly as well.

Outbound feature boundaries:

```sh
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp
```

Full:

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Packaging:

```sh
python3 scripts/publish-crates.py --list
CARGO_BUILD_JOBS=2 python3 scripts/publish-crates.py --dry-run
```

If Python version files change, run the standard Python build/test qualification
required by the repository even if runtime Python bindings were not changed.

Record exact counts, skips, warnings, and final SHA.

---

# Workstream 9 — documentation and historical evidence reconciliation

Update current architecture/API docs to state:

- nested SSH/H2 remains unpooled;
- ordinary hop-zero reuse is policy-scoped;
- explicit `local_bind` disables hop-zero SSH/H2 reuse;
- explicit insecure H2 is unpooled;
- chain H2 pool namespaces are scoped to the executor/service TLS policy
  lifetime;
- `Some(addr)` metadata still identifies the active physical transport.

Correct the H2 registry/key comments so they do not imply endpoint/SNI/auth
alone are a complete connection-policy identity.

Historical plan records:

- append corrections; do not delete earlier completion evidence;
- explicitly identify the test-name evidence error in the 1.0.9 record;
- list the exact new behavioral tests that supersede that evidence;
- record that `v1.0.9` was already tagged/released before this corrective;
- record actual crates.io status separately from GitHub/PyPI status.

`docs/ROADMAP.md` and `plans/README.md` should end with this corrective
implemented and the next patch **qualified but unpublished**, unless a
maintainer separately authorizes publication afterward.

---

## Expected files touched

Likely:

```text
crates/eggress-outbound/src/executor.rs
crates/eggress-outbound/src/hops.rs
crates/eggress-protocol-http/src/h2_connect.rs
crates/eggress-transport-ssh/src/lib.rs
crates/eggress-runtime/tests/multihop_tcp.rs
crates/eggress-embed/tests/ssh.rs
crates/eggress-outbound/README.md
architecture/outbound.md
architecture/transports-ssh-quic-h3.md
docs/RUST_API.md
docs/EMBED_API.md
AGENTS.md
.skills/embed-outbound/skill.md
.skills/rust-proxy-dev/skill.md
Cargo.toml
Cargo.lock
crates/eggress-python/pyproject.toml
python-pproxy-compat/pyproject.toml
python/pyproject.toml
fuzz/Cargo.lock
plans/POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md
plans/POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md
plans/OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md
docs/ROADMAP.md
plans/README.md
```

Exact test placement may differ if the existing local H2/OpenSSH fixtures are
better suited elsewhere.

---

## Non-goals

- no attempt to alter 1.0.9 artifacts;
- no tag deletion/force-move;
- no chain-prefix hashing for nested reuse;
- no restoration of nested SSH/H2 pooling;
- no breaking public cache-key redesign;
- no resolver redesign;
- no new proxy protocol;
- no generic pooling performance campaign;
- no metadata expansion for QUIC/H3;
- no pproxy capability-tier change;
- no automatic crates.io/PyPI/GitHub publication.

---

## Stop conditions

Stop and record a blocker if:

- executor-scoped H2 pooling would accidentally become request-scoped and
  eliminate intended reuse;
- a TLS-policy isolation test cannot prove that the second executor performs
  its own TLS establishment;
- explicit local-bind cannot be honored without disabling reuse;
- preserving hop-zero reuse requires a breaking public key change;
- any cache can still discard a supplied stream whose bind/trust/route policy
  differs from the cached physical connection;
- the named behavioral regressions are not present and executed;
- version 1.0.10 is already immutable on a required release surface.

Prefer reduced reuse to policy bypass.

---

## Acceptance criteria

This corrective is complete only when:

1. Eggress chain H2 pooling is no longer process-global across unrelated
   executor/TLS policy scopes;
2. two executors with different TLS trust policy cannot share one physical H2
   connection;
3. hop-zero H2 with explicit local bind is unpooled and honors that bind;
4. hop-zero H2 with `insecure=true` is unpooled;
5. ordinary equivalent-policy hop-zero H2 still reuses connections;
6. hop-zero SSH with explicit local bind uses a fresh session;
7. ordinary hop-zero SSH without explicit bind still reuses sessions;
8. nested H2 remains unpooled;
9. nested SSH remains unpooled;
10. explicit behavioral regressions prove selected-prefix use and
    non-cross-reuse for nested H2 and SSH;
11. no completion record cites nonexistent tests;
12. the reusable-transport audit finds no other supplied-stream policy bypass,
    or any discovered equivalent issue is fixed before qualification;
13. direct and ordinary TCP-preserving `OutboundInfo` metadata remains
    truthful;
14. no public connect/key signature or struct shape is broken;
15. focused, OpenSSH, multihop, feature, workspace, dependency, audit, and fuzz
    gates pass;
16. package dry-run passes for the new lockstep patch;
17. 1.0.9 release history is documented accurately, including actual per-
    registry publication state;
18. the workspace is rolled forward to the next unused patch (expected
    1.0.10);
19. no new production tag or publication occurs without separate explicit
    authorization;
20. completion record contains final SHA, exact regression names/files,
    registry-state evidence, and release handoff.

## Implementation record — 2026-09-24

The source tree is being rolled forward to 1.0.10. The 1.0.9 crates.io state
was queried independently from GitHub/PyPI: `eggress-core 1.0.9` was
published at `2026-09-24T03:15:12Z`, `eggress-outbound 1.0.9` at
`2026-09-24T03:19:52Z`, and `eggress-embed 1.0.9` at
`2026-09-24T03:22:24Z`. No 1.0.10 sparse-index entries or remote tag were
present when queried. The crates.io API version endpoint was intermittently
403; publication timestamps were read from the public sparse index.

Implemented so far:

- Chain H2 pooling now uses a bounded registry keyed by the identity of the
  shared TLS client-config object. This retains reuse across the server's
  per-route executor construction while separating distinct TLS policy
  objects; runtime reload clears these scopes. The public global registry and
  `H2PoolKey` shape remain for direct consumers.
- H2 pooling is eligible only for hop zero with no explicit local bind and no
  explicit insecure policy. Other H2 hops use the supplied stream through the
  unpooled client. SSH caching is eligible only for hop zero with no explicit
  local bind; fresh-session paths remain in use otherwise.
- Added successful-H2-handshake counting to the hop-zero reuse regression and
  verified it locally. Added handler-level H2 regressions for ordinary reuse,
  local-bind unpooling, and insecure unpooling. Existing nested behavioral
  tests were renamed to their exact semantic names:
  `nested_h2_consumes_selected_prefix` in
  `crates/eggress-runtime/tests/upstream_protocols.rs` and
  `openssh_nested_ssh_consumes_selected_prefix` in
  `crates/eggress-transport-ssh/tests/openssh.rs`.
- H2 unit regressions now count successful physical H2 handshakes for
  `h2_hop_zero_same_policy_reuses_physical_connection`,
  `h2_hop_zero_local_bind_is_not_pooled`,
  `h2_hop_zero_insecure_is_not_pooled`, and
  `nested_h2_does_not_cross_reuse_prefixes` in
  `crates/eggress-outbound/src/hops.rs`. Policy-scope mapping is covered by
  `h2_pool_is_scoped_to_executor_tls_policy`, and distinct registries are
  exercised by `h2_distinct_tls_policy_scopes_use_distinct_physical_connections`
  in `crates/eggress-outbound/src/executor.rs`.
- Version metadata and both lockfiles are aligned to 1.0.10. The version-only
  release preflight, offline workspace check, offline fuzz check, and Cargo
  metadata check passed.
- The 1.0.9 route-isolation record now corrects its test-name claims and
  states that distinct nested-prefix cross-reuse tests were absent in that
  tree.

Reusable-transport audit in progress:

- H2 `H2ConnectionPool` entries can discard a newly supplied first-hop stream
  on a hit. Eggress chain scope now includes TLS client-config object identity
  (bounded policy-scope registry); `H2PoolKey` retains endpoint, TLS bit, SNI,
  auth digest, and hop index, but does not encode local bind or TLS trust.
  Nested, explicit-bind, and insecure H2 are selected into the unpooled path.
- SSH `SshSessionCache` is connector/service owned and keyed by endpoint,
  username, auth, and hop index; a cache hit can discard a new first-hop
  stream. Explicit-bind and nested hops now choose its existing fresh-session
  path. Cache host-key behavior is still chosen by the original native vs
  compatibility constructor.
- UDP association/flow maps own datagram sockets/flow metadata, not reusable
  supplied TCP route streams. TLS ALPN config caches hold immutable client
  configuration only; they do not cache physical transports. The source scan
  found no other reusable protocol connection pool with the same
  acquire-before-consuming-stream behavior.

Still required before changing this record to **IMPLEMENTED AND QUALIFIED**:

- Execute the complete TLS/H2 trust-boundary regression with a local TLS/H2
  fixture; current H2 unit tests prove handler pool selection with local
  duplex H2 peers but do not yet prove that an untrusted second executor fails
  its TLS handshake.
- Add/run source-bind SSH/H2 regressions that prove the physical SSH/H2
  transport observes the requested source endpoint; current H2 tests verify
  the non-pooling handshake path, not an actual socket bind observation.
- Add/run the nested SSH cross-prefix behavioral test with per-prefix
  counters/markers; the nested H2 test is present.
- Complete the reusable-transport audit, focused feature slices, required
  embed OpenSSH test, workspace Clippy/tests, deny/audit/fuzz/package dry-run,
  and record exact results and final SHA.

No production tag or publication was created. The 1.0.10 line remains
qualified only after all listed acceptance evidence is complete.
