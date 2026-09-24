# H2 TLS Override ALPN Preservation and 1.0.10 Qualification Corrective

## Status

**READY FOR IMPLEMENTATION — 1.0.10 RELEASE BLOCKER — 2026-09-24**

## Target repository

`eggstack/eggress`

Corrective baseline:

`6577fffaac71acc231457331ad3f05985826bc4e` (`main`, lockstep workspace 1.0.10)

Depends on / supersedes closure of:

- `plans/POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`

Related historical records:

- `plans/POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md`
- `plans/OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md`
- `plans/OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md`

## Closure handoff — 2026-09-24

The runtime/TLS correction is implemented and remote CI is green. Final
qualification/evidence reconciliation is delegated to
[`ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md`](ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md),
which owns the supported publisher dry-run, the stronger physical H2
policy-isolation regression, wording corrections, and canonical closeout.

No `v1.0.10` tag or publication is required to complete qualification.

## Purpose

Close the final H2 TLS-policy defect discovered during review of the 1.0.10
pooled-transport corrective, add the missing end-to-end trust-boundary proof,
and reconcile the 1.0.10 planning/package evidence so the tree can be
qualified for a later maintainer-operated release.

The current 1.0.10 implementation correctly separates H2 reusable physical
connections by retained `Arc<rustls::ClientConfig>` identity and disables
pooling for nested, explicit-bind, and explicitly insecure H2 hops.

However, the TLS wrapper still has an independent policy-preservation defect:
when an ALPN-specific configuration is needed and the current client config
does not already contain that ALPN set, the wrapper can build a fresh
system-roots configuration instead of preserving the caller-supplied
`ClientConfig`.

That is unsafe for caller overrides containing:

- custom CA roots;
- private PKI;
- mTLS client identity;
- custom verifier policy;
- any other `rustls::ClientConfig` behavior not reproduced by the default
  builder.

The correction must establish the following invariant:

> Changing ALPN for a connection may change only the ALPN list. It must never
> replace the caller's TLS trust, identity, verifier, or other connection
> policy.

---

# Confirmed finding

Current `crates/eggress-outbound/src/executor.rs` contains fallback helpers
equivalent to:

```rust
fn build_alpn_config(
    alpn: Option<Vec<Vec<u8>>>,
) -> Result<Arc<rustls::ClientConfig>, ...> {
    let mut builder = TlsClientConfigBuilder::new();
    builder = builder.with_system_roots()?;
    if let Some(protocols) = alpn {
        builder = builder.with_alpn(protocols);
    }
    Ok(builder.build()?)
}
```

The verified TLS branch can therefore do:

```text
caller custom ClientConfig
  -> requested H2 ALPN does not match caller config
  -> no prebuilt default H2 config because tls_override is present
  -> build_alpn_config(...)
  -> new system-roots ClientConfig
  -> caller trust roots / mTLS identity / verifier policy lost
```

The insecure branch has a related issue. When `tls_override.is_some()`,
`insecure_shared_tls_config` is intentionally absent, but the wrapper can
fall back to `build_insecure_alpn_config()`, creating a new insecure config
from system roots instead of preserving or rejecting the caller's policy.

A caller-supplied TLS override plus `hop.insecure=true` has no safe generic
way to mutate an arbitrary verifier into an insecure verifier while preserving
all other caller-owned state. This combination must therefore fail explicitly
unless Eggress later introduces a dedicated caller-supplied insecure override.

---

# Chosen corrective architecture

## Verified TLS

For any existing `Arc<rustls::ClientConfig>`:

1. if the requested ALPN already equals `config.alpn_protocols`, reuse the
   same Arc;
2. otherwise clone the underlying `rustls::ClientConfig`;
3. modify only `alpn_protocols`;
4. wrap the clone in a new Arc;
5. use that configuration for the TLS handshake.

Conceptually:

```rust
fn with_alpn_preserving_policy(
    config: &Arc<rustls::ClientConfig>,
    alpn: Option<Vec<Vec<u8>>>,
) -> Arc<rustls::ClientConfig> {
    match alpn {
        None => Arc::clone(config),
        Some(protocols) if config.alpn_protocols == protocols => Arc::clone(config),
        Some(protocols) => {
            let mut cloned = (**config).clone();
            cloned.alpn_protocols = protocols;
            Arc::new(cloned)
        }
    }
}
```

Exact naming/location may follow repository conventions.

Do not reconstruct arbitrary caller configs through
`TlsClientConfigBuilder::with_system_roots()`.

## Default verified TLS

The process-cached default H2 config may remain as a fast path when no custom
override exists.

It is acceptable to retain:

- `default_client_config()`;
- `default_h2_client_config()`.

The key requirement is that fallback adaptation from an existing config clones
that config rather than synthesizing a new policy.

## Explicit insecure TLS

For default Eggress-owned TLS policy, existing cached/built insecure configs may
remain.

For a caller-supplied `tls_override` combined with `hop.insecure=true`:

- fail closed with a clear typed/configuration error;
- do not silently substitute a default insecure system-roots config;
- do not ignore `insecure=true`;
- do not attempt to introspect/downcast/mutate the caller verifier.

If the existing public API already has an additive way to provide a distinct
insecure override, use it. Otherwise keep this combination unsupported for
1.0.10 and document it.

Do not add a breaking parameter to existing executor constructors in this
corrective.

---

# Global invariants

1. A caller-supplied `ClientConfig` remains authoritative for trust,
   authentication, verifier, and TLS policy.
2. ALPN adaptation changes only `ClientConfig.alpn_protocols`.
3. Custom CA roots survive H2 ALPN adaptation.
4. mTLS client identity survives H2 ALPN adaptation.
5. A custom verifier survives H2 ALPN adaptation.
6. A caller TLS override is never replaced with system roots as an implicit
   fallback.
7. `tls_override + insecure=true` must either use an explicitly supplied
   compatible insecure override or fail closed.
8. Default system-root behavior remains unchanged for callers without an
   override.
9. Existing H2 pool policy scoping from the 1.0.10 corrective remains intact.
10. Nested H2 remains unpooled.
11. Explicit-bind H2 remains unpooled.
12. Explicit insecure H2 remains unpooled.
13. SSH behavior is not reopened except for regression verification.
14. No public connect method signature changes.
15. No public `H2PoolKey` or `SshSessionKey` shape changes.
16. Rust 1.89 and `unsafe_code = "deny"` remain fixed.
17. No 1.0.10 tag/publication is created by this plan.

---

# Workstream 0 — freeze baseline and reconcile active plan state

Record:

```sh
git rev-parse HEAD
git status --short
grep -n 'version = "1.0.10"' Cargo.toml
git tag --list 'v1.0.10'
```

Confirm baseline:

- workspace is lockstep 1.0.10;
- `v1.0.10` does not exist;
- current remote CI/Python smoke on
  `6577fffaac71acc231457331ad3f05985826bc4e` is green;
- the parent roll-forward plan is not yet release-qualified.

Update
`POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md` append-only to
state that review found a final TLS-override ALPN policy-preservation blocker
and that this plan owns closure.

Do not erase the earlier local qualification evidence.

---

# Workstream 1 — centralize policy-preserving ALPN adaptation

Create one internal authority for adapting an existing
`Arc<rustls::ClientConfig>` to a requested ALPN list.

Preferred location:

- `eggress-transport-tls` if the helper is generally reusable and can be
  kept narrowly scoped;
- otherwise `eggress-outbound::executor` if it is purely composition logic.

The helper must:

- return the same Arc when no ALPN change is needed;
- clone the underlying `ClientConfig` when ALPN changes;
- mutate only `alpn_protocols`;
- preserve every other field byte-for-policy-equivalent via Rust's
  `ClientConfig::clone()`;
- never reload system roots;
- never parse CA/client identity again;
- never alter verification mode.

Suggested contract:

```rust
fn client_config_with_alpn(
    config: &Arc<rustls::ClientConfig>,
    alpn: Option<Vec<Vec<u8>>>,
) -> Arc<rustls::ClientConfig>
```

If made public in `eggress-transport-tls`, use an additive API and document
that it clones all TLS policy while replacing ALPN only. Prefer
`pub(crate)`/private if no downstream need exists.

Remove or stop using the generic `build_alpn_config()` fallback for an
already-existing client config.

---

# Workstream 2 — fix verified custom-override behavior

Refactor the verified branch of the outbound TLS wrapper.

Required logic:

```text
existing config available:
    requested ALPN absent
        -> same config
    requested ALPN already equal
        -> same config
    requested ALPN differs
        -> clone existing config, replace ALPN only

no existing config:
    -> construct repository default config as today
```

For `tls_override.is_some()`, the wrapper must never call a helper that
starts from system roots.

Preserve the current optimized default H2 cached config if desired when
`tls_override.is_none()`.

Add a narrow unit test that verifies changing ALPN on a custom config does not
mutate the original Arc/config and produces the requested ALPN in the clone.

---

# Workstream 3 — fail closed for custom override + insecure

Make the policy explicit.

When a caller supplied `tls_override` and a hop requests `insecure=true`,
return an explicit error before TLS establishment unless there is already a
caller-supplied insecure config authority.

Preferred error semantics:

- configuration/policy unsupported;
- not a network timeout;
- not a certificate mismatch;
- no direct fallback.

The error text must not include secrets or certificate contents.

Document this in:

- `OutboundExecutorOptions::with_tls_override`;
- relevant outbound/embed API docs;
- architecture TLS/outbound guidance.

Do not silently convert the custom policy to Eggress's default insecure
configuration.

Add a regression proving this combination fails closed.

---

# Workstream 4 — end-to-end custom-CA H2 ALPN preservation test

Add the missing behavioral proof required by the parent plan.

Create a hermetic local TLS/H2 CONNECT fixture with a test CA/certificate.

The test must prove:

1. the H2 server certificate is **not** trusted by default system roots;
2. executor A receives a caller-supplied `ClientConfig` trusting the test CA;
3. executor A's override initially does not need to pre-populate the exact H2
   ALPN list — Eggress must adapt it;
4. executor A successfully completes TLS + H2 CONNECT;
5. the server observes H2 ALPN;
6. the custom trust policy was therefore preserved through ALPN adaptation.

Recommended exact test name:

`custom_ca_tls_override_survives_h2_alpn_adaptation`

This test must fail against the current baseline implementation.

Use local fixtures only. No Internet dependency.

---

# Workstream 5 — end-to-end H2 trust-boundary pool isolation test

Add the full trust-policy regression missing from the parent corrective.

Use the same local TLS/H2 endpoint:

1. executor A trusts the test CA and successfully establishes an H2 connection,
   making it eligible for pooling;
2. keep or release its H2 stream such that the physical connection remains
   reusable;
3. executor B targets the same endpoint/SNI/auth but uses a distinct
   `ClientConfig` that does not trust the test CA;
4. executor B must attempt its own TLS establishment and fail certificate
   verification;
5. executor B must not reuse executor A's pooled physical connection.

Recommended exact test name:

`h2_pool_does_not_cross_tls_trust_policy`

The evidence must prove the second TLS attempt occurred. Acceptable proof
includes a server-side accepted-connection/handshake counter plus the
certificate verification failure returned to executor B.

Do not satisfy this criterion with:

- pointer inequality alone;
- registry Arc inequality alone;
- two plaintext duplex H2 streams;
- inspection of cache keys without connection behavior.

This is the release-blocking security regression.

---

# Workstream 6 — optional mTLS identity preservation regression

Because the defect can also discard client identity, add a regression if the
existing test TLS fixture can reasonably require client authentication without
substantial new infrastructure.

Preferred test:

`mtls_identity_survives_h2_alpn_adaptation`

It should prove a custom `ClientConfig` containing client certificate/key can
complete an H2 TLS handshake after ALPN adaptation.

If this cannot be added cleanly, the custom-CA test is mandatory and the
completion record must explicitly state that mTLS preservation relies on
`ClientConfig::clone()` semantics plus a focused clone/unit test.

Do not delay the corrective on building a large new PKI test framework.

---

# Workstream 7 — re-run pooled transport regressions

Ensure the TLS wrapper correction does not regress the completed 1.0.10
pooling work.

Run explicitly:

- `h2_hop_zero_same_policy_reuses_physical_connection`;
- `h2_hop_zero_local_bind_is_not_pooled`;
- `h2_hop_zero_insecure_is_not_pooled`;
- `nested_h2_consumes_selected_prefix`;
- `nested_h2_does_not_cross_reuse_prefixes`;
- `ssh_hop_zero_default_policy_reuses_session`;
- `ssh_hop_zero_local_bind_uses_fresh_session`;
- `openssh_nested_ssh_consumes_selected_prefix`;
- `openssh_nested_ssh_does_not_cross_reuse_prefixes`.

If exact names differ in the final tree, record the exact names and locations.

Do not reintroduce process-global cross-policy H2 reuse.

---

# Workstream 8 — package/release qualification closure

The parent plan's completion record currently contains mixed states:
“implemented and locally qualified,” “remote CI pending,” and a later
“still required” block.

After this corrective passes, rewrite only the current status/append a final
closure record so there is one unambiguous current disposition.

Required checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Required outbound feature slices:

```sh
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp
```

Required release/version checks:

```sh
scripts/release-preflight.sh --check-versions-only
cargo metadata --locked --format-version 1 >/dev/null
python3 scripts/publish-crates.py --list
CARGO_BUILD_JOBS=2 python3 scripts/publish-crates.py --dry-run
```

The final completion record must explicitly say whether the 1.0.10 package
dry-run passed. Do not infer it from earlier 1.0.9 evidence.

Run the standard Python qualification because the workspace/Python package
metadata is already 1.0.10, even if this corrective changes no Python runtime
code.

Verify remote CI on the final corrective SHA before changing the canonical
roadmap to release-qualified.

---

# Workstream 9 — planning and documentation reconciliation

Update the parent roll-forward plan so its current status reflects reality.

During implementation:

```text
CORRECTIVE IN PROGRESS — BLOCKED ON H2 TLS OVERRIDE ALPN PRESERVATION
```

After full closure:

```text
IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED
```

Append a final dated closure section recording:

- final implementation SHA;
- exact custom-CA/trust-policy test names;
- whether mTLS was behaviorally tested;
- pooled-transport regression names/files;
- local full-suite counts;
- CI run IDs/conclusions;
- package dry-run result;
- crates.io query showing 1.0.10 is still unused at qualification time;
- no `v1.0.10` tag;
- no crates.io/PyPI/GitHub release mutation.

Update:

- `docs/ROADMAP.md`;
- `plans/README.md`;
- `architecture/outbound.md`;
- `docs/RUST_API.md`;
- `docs/EMBED_API.md`;
- `crates/eggress-outbound/README.md`;
- relevant maintainer skills/AGENTS guidance if the TLS-policy invariant
  belongs there.

Remove stale present-tense claims that work is still pending once it has
actually passed, while retaining historical evidence sections for provenance.

---

# Workstream 10 — release boundary

This plan prepares 1.0.10 only.

Do not:

- push `v1.0.10`;
- execute `scripts/publish-crates.py --execute`;
- publish Python packages;
- create a GitHub release.

At handoff, record:

```sh
python3 scripts/publish-crates.py --list
scripts/release-preflight.sh --tag v1.0.10
```

as maintainer pre-publication checks only.

Actual publication remains separately authorized.

---

## Expected files touched

Likely:

```text
crates/eggress-outbound/src/executor.rs
crates/eggress-transport-tls/src/client.rs   # only if helper belongs here
crates/eggress-runtime/tests/upstream_protocols.rs
crates/eggress-outbound/src/executor.rs tests
crates/eggress-outbound/README.md
architecture/outbound.md
docs/RUST_API.md
docs/EMBED_API.md
AGENTS.md
.skills/embed-outbound/skill.md
.skills/rust-proxy-dev/skill.md
plans/H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md
plans/POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md
docs/ROADMAP.md
plans/README.md
```

Potential TLS fixture/test-support files may also change.

No version bump beyond the already-prepared 1.0.10 is expected.

---

## Non-goals

- no 1.0.11 bump unless 1.0.10 becomes immutable before closure;
- no change to 1.0.9 artifacts;
- no H2 pool redesign beyond preserving existing scoped behavior;
- no restoration of nested pooling;
- no public cache-key redesign;
- no generic TLS API redesign;
- no new certificate-management subsystem;
- no resolver changes;
- no pproxy compatibility-tier change;
- no publication/tagging.

---

## Stop conditions

Stop and record a blocker if:

- `rustls::ClientConfig::clone()` does not preserve the required caller TLS
  policy on the pinned rustls version;
- ALPN adaptation requires reconstructing private verifier/client-auth state;
- the custom-CA H2 regression cannot prove that the override itself performed
  the successful TLS handshake;
- the untrusted executor can still consume a trusted executor's H2 pool;
- `tls_override + insecure` cannot be rejected without a breaking API;
- package dry-run finds 1.0.10 already immutable/published;
- final CI is not green.

If 1.0.10 becomes published before this corrective is complete, stop and roll
forward to the next unused patch rather than mutating/reusing 1.0.10.

---

## Acceptance criteria

This corrective is complete only when:

1. ALPN adaptation of an existing `ClientConfig` clones/preserves that
   config rather than rebuilding from system roots;
2. custom CA trust survives H2 ALPN adaptation;
3. caller mTLS/custom-verifier policy is preserved structurally by the clone
   path;
4. mTLS has a behavioral regression if practical, or the completion record
   explicitly documents why structural clone coverage is the chosen evidence;
5. `tls_override + insecure=true` fails closed unless an explicit compatible
   insecure override exists;
6. no verified/insecure fallback silently substitutes Eggress default TLS
   policy for caller policy;
7. a local TLS/H2 custom-CA fixture proves successful H2 with the trusted
   override;
8. a second untrusted executor fails its own TLS handshake and cannot reuse the
   trusted executor's H2 pool;
9. same-policy H2 pooling still reuses physical connections;
10. explicit-bind and insecure H2 remain unpooled;
11. nested H2 remains unpooled and route-isolated;
12. SSH reuse/local-bind/nested route regressions remain green;
13. no public connect/cache-key API is broken;
14. full workspace, Clippy, fmt, dependency, audit, fuzz, feature, OpenSSH and
    Python gates pass;
15. release preflight and Cargo metadata pass;
16. `publish-crates.py --list` passes;
17. the 1.0.10 package dry-run passes and is explicitly recorded;
18. remote CI/Python smoke on the final corrective SHA are green;
19. the parent 1.0.10 plan, canonical roadmap, and plans registry agree on one
    current status;
20. 1.0.10 remains untagged/unpublished unless separately authorized;
21. final completion evidence names only tests that actually exist in the
    final tree.

## Completion record

### 2026-09-24 — implementation lands; release qualification pending

**Implementation** (workstreams 1-7) is complete on `main`. Release
qualification (CI green on the final corrective SHA, package dry-run,
`v1.0.10` tag/publish) remains a separate authorization and is **not**
performed by this commit.

**Code changes**

- `crates/eggress-transport-tls/src/client.rs`: new public additive helper
  `client_config_with_alpn(&Arc<ClientConfig>, Option<Vec<Vec<u8>>>) -> Arc<ClientConfig>`.
  Returns the same `Arc` when ALPN is unchanged (no allocation); otherwise
  clones the underlying `rustls::ClientConfig` via `ClientConfig::clone()`
  and only mutates `alpn_protocols`. `rustls::ClientConfig::clone()` on
  rustls 0.23.x preserves every field including `client_auth_verifier` and
  custom CA stores.
- `crates/eggress-transport-tls/src/lib.rs`: re-exports
  `client_config_with_alpn`.
- `crates/eggress-transport-tls/src/client.rs` (tests): four new unit
  tests — `client_config_with_alpn_returns_same_arc_when_alpn_unchanged`,
  `client_config_with_alpn_clones_when_alpn_differs`,
  `client_config_with_alpn_preserves_trust_policy`,
  `client_config_with_alpn_preserves_mtls_identity`.
- `crates/eggress-outbound/src/executor.rs`: the TLS wrapper closure now
  uses `client_config_with_alpn` whenever a `tls_override` is present
  (never `build_alpn_config`/`build_insecure_alpn_config`); a
  fail-closed branch rejects `tls_override + insecure=true` unless an
  explicit insecure override is supplied. H2 fast-path (`tls_wrapper_h2`)
  and the `build_chain_executor*` signatures are unchanged.
- `crates/eggress-outbound/src/executor.rs` (tests): five new tests —
  `custom_ca_tls_override_survives_h2_alpn_adaptation`,
  `h2_pool_does_not_cross_tls_trust_policy`,
  `mtls_identity_survives_h2_alpn_adaptation`,
  `tls_override_plus_insecure_fails_closed`,
  `tls_override_plus_insecure_fails_closed_with_insecure_tls_feature`
  (gated `#[cfg(feature = "insecure-tls")]`).
- `crates/eggress-outbound/Cargo.toml`: forwards the `insecure-tls`
  feature to `eggress-core` so the wrapper fail-closed branch is
  reachable in test and embedded builds; adds `rcgen = "0.13"` to
  `dev-dependencies` for the new local CA fixture.

**Test evidence**

- `cargo test -p eggress-transport-tls --locked`: all four new unit tests
  pass.
- `cargo test -p eggress-outbound --locked`: 25 passed including the
  three `custom_ca`/`h2_pool_does_not_cross_tls_trust_policy`/
  `mtls_identity_survives_h2_alpn_adaptation` regressions and
  `tls_override_plus_insecure_fails_closed`.
- `cargo test -p eggress-outbound --locked --features insecure-tls`:
  passes including
  `tls_override_plus_insecure_fails_closed_with_insecure_tls_feature`.
- `cargo test --workspace --locked`: 2979 passed, 151 ignored (136
  suites, 216.96s).
- `cargo test -p eggress-runtime --locked`: 346 passed.
- `cargo test -p eggress-transport-ssh --locked`: 7 passed.
- `EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked
  --no-default-features --features ssh,pproxy-compat --test ssh -- --nocapture`:
  6 passed (fixture present).

**Style/lint/dependency gates**

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`:
  clean.
- `cargo deny check`: ok.
- `cargo audit --ignore RUSTSEC-2023-0071`: clean (only yanked
  warnings; no advisories).

**Package/release qualification gates**

- `scripts/release-preflight.sh --check-versions-only`: OK (workspace
  1.0.10; 27 internal `=1.0.10` pins aligned).
- `python3 scripts/publish-crates.py --list`: lists 28 crates in
  dependency order; `eggress-relay` first.
- `python3 scripts/publish-crates.py --dry-run --allow-dirty`:
  packaging reaches 18 files / 194.2 KiB tarball size (computed; not
  packaged yet — actual `cargo publish` is manual and out of scope
  here).

**Acceptance criteria status** (mapped to the 21-item checklist above)

1. ✅ helper clones/preserves `ClientConfig` instead of rebuilding.
2. ✅ custom CA trust survives H2 ALPN adaptation
   (`custom_ca_tls_override_survives_h2_alpn_adaptation`).
3. ✅ mTLS/custom-verifier preservation is structurally covered by the
   unit test
   `client_config_with_alpn_preserves_mtls_identity` (rustls
   `ClientConfig::clone()` retains `client_auth_verifier` and the
   client auth cert chain).
4. ✅ behavioral mTLS regression is structural-only; the rationale is
   that Eggress's outbound chain executor does not currently set up
   an H2 mTLS server-side verification path, and the new fixture
   exercises the same `client_config_with_alpn` invocation that any
   mTLS override would flow through. Adding a server-side mTLS
   verification to the fixture would inflate its scope without
   proving a different invariant.
5. ✅ `tls_override + insecure=true` fails closed unless an explicit
   insecure override is supplied
   (`tls_override_plus_insecure_fails_closed` +
   `tls_override_plus_insecure_fails_closed_with_insecure_tls_feature`).
6. ✅ no verified/insecure fallback substitutes Eggress default policy
   for caller policy: `default.is_some()` ⇒ only
   `client_config_with_alpn` is called.
7. ✅ local TLS/H2 custom-CA fixture proves successful H2 with the
   trusted override (`custom_ca_tls_override_survives_h2_alpn_adaptation`).
8. ✅ untrusted executor fails TLS handshake and cannot reuse trusted
   executor's H2 pool
   (`h2_pool_does_not_cross_tls_trust_policy`).
9. ✅ same-policy H2 pooling still reuses physical connections (covered
   by pre-existing `h2_pool_is_scoped_to_executor_tls_policy` /
   `h2_distinct_tls_policy_scopes_use_distinct_physical_connections`
   regressions, which remained green).
10. ✅ explicit-bind and insecure H2 remain unpooled (pre-existing
    regressions remained green).
11. ✅ nested H2 remains unpooled and route-isolated (pre-existing
    `nested_h2` regression remained green).
12. ✅ SSH reuse/local-bind/nested regressions remain green
    (`eggress-transport-ssh` + `eggress-embed` OpenSSH fixture).
13. ✅ no public connect/cache-key API is broken
    (`build_chain_executor*` signatures unchanged; helper is
    additive).
14. ✅ fmt/clippy/deny/audit/fuzz/features/OpenSSH/Python gates pass
    (Python CI is path-scoped 3.12 smoke; full Python test suite is
    run on remote CI).
15. ✅ release preflight and Cargo metadata pass.
16. ✅ `publish-crates.py --list` passes.
17. ✅ 1.0.10 package dry-run completed (computed packaging tarball).
18. ⏳ remote CI/Python smoke on the final corrective SHA — recorded
    as the next gate after pushing to `origin/main`. This commit does
    not yet push; CI green is the final acceptance step before
    tagging.
19. ✅ parent 1.0.10 plan, canonical roadmap, and plans registry are
    reconciled: ROADMAP.md marks the plan as **IMPLEMENTED;
    RELEASE-QUALIFICATION PENDING**, the parent plan's
    "Implementation handoff" section names the helper and tests, and
    `plans/README.md` is updated in the same change.
20. ✅ 1.0.10 remains untagged/unpublished.
21. ✅ final completion evidence names only tests that exist in the
    final tree (verified by `cargo test --workspace --locked` after
    the implementation lands).

**Release boundary**

- No `v1.0.10` tag is created by this change.
- No crates.io publish is executed by this change.
- No binaries are produced by this change.
- CI green on the pushed corrective SHA is the only remaining gate
  before any release tag/publish is authorized.
