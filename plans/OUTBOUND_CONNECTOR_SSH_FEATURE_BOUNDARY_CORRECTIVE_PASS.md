# OutboundConnector SSH Feature-Boundary Corrective Pass

## Status

**READY FOR IMPLEMENTATION**

## Parent context

- `plans/OUTBOUND_CONNECTOR_PPROXY_CHAIN_CORRECTIVE_PASS.md`
- `plans/PPROXY_STRICT_PHASE_7_SSH_TRANSPORT.md`
- `plans/LEAN_COMPOSITION_AND_FEATURE_BOUNDARIES.md`
- `docs/EMBED_API.md`
- `crates/eggress-embed/README.md`

This is a narrow follow-on correction for Eggress's public Rust embedding surface. It was exposed by a downstream consumer that needs listener-free SSH proxy execution, but the work is intentionally generic: the end state must improve `eggress-embed` for any Rust consumer and must not introduce an Eggpool-specific mode, feature, type, or compatibility layer.

## Baseline

Written against repository head:

```text
68c656265dacf76d96d8dc28836d4603374c6f6b
```

## Motivation

`eggress-embed::outbound::OutboundConnector` is the intended in-process egress primitive for Rust applications that need Eggress proxy-chain semantics without starting a listener service. The API already exposes `from_toml()`, `from_pproxy_uri()`, `connect_tcp()`, and timeout/UDP variants, and the existing embed documentation presents this crate as the stable downstream Rust boundary.

The current SSH-enabled constructor path is incomplete at that boundary.

When `eggress-embed` is compiled with `ssh`, `OutboundConnector` constructs the chain executor with:

```rust
eggress_server::build_chain_executor(None, None, None)
```

The third argument is the optional SSH session cache. Eggress's SSH transport already provides `SshSessionCache`, including the secure native constructor and the explicitly compatibility-oriented constructor, but `OutboundConnector` does not install either one. As a result, an SSH pproxy chain can parse and compile through the public embed facade yet still lack the runtime state required to execute correctly. A downstream consumer must currently bypass the facade, recreate compatibility parsing/config compilation, instantiate `SshSessionCache`, and call `eggress_server::build_chain_executor()` directly.

That is an abstraction-boundary defect. The downstream should not need to know about `eggress-core`, `eggress-config`, `eggress-server`, `eggress-uri`, `eggress-transport-ssh`, or the executor's SSH cache slot merely to use a supported `OutboundConnector` transport.

There is a second, independent feature-boundary issue in `crates/eggress-embed/Cargo.toml`:

```toml
pproxy-compat = ["dep:eggress-pproxy-compat"]
ssh = ["eggress-runtime/ssh", "eggress-pproxy-compat/ssh"]
```

Because the `ssh` feature forwards a feature to an optional dependency without weak dependency syntax, selecting `eggress-embed/ssh` also activates `eggress-pproxy-compat`. Native/TOML SSH consumers therefore pay for the pproxy compatibility crate even when they did not request the compatibility constructor. The runtime/server feature topology is already capable of representing SSH independently; the coupling is introduced at the embed facade.

This plan fixes those two seams while preserving the existing public API and security model.

---

# Objective

Make `OutboundConnector` a complete, listener-free SSH-capable facade and make `eggress-embed`'s Cargo feature graph accurately represent optional capabilities.

The desired end state is:

```text
native TOML outbound config + ssh
  -> OutboundConnector
  -> verified SSH session cache
  -> ChainExecutor
  -> target stream

pproxy expression + pproxy-compat + ssh
  -> OutboundConnector
  -> explicit pproxy compatibility SSH session cache
  -> ChainExecutor
  -> target stream
```

Downstream consumers should not need a second chain parser, direct implementation-crate dependencies, or their own `ChainExecutor` construction to make SSH work.

At the same time:

```text
eggress-embed feature = ssh
```

must not activate `eggress-pproxy-compat` unless the consumer also requests `pproxy-compat`.

---

# Security invariants

This pass must preserve the SSH host-key policy distinction already established by Eggress.

1. Native/TOML Eggress SSH is **not** pproxy compatibility mode. It must retain verified host-key behavior through `SshSessionCache::new()` or an equivalent existing verified policy.
2. `OutboundConnector::from_pproxy_uri()` is explicitly the pproxy compatibility constructor. When the caller has opted into both `pproxy-compat` and `ssh`, it may use the existing `SshSessionCache::new_compatibility()` behavior that implements the established compatibility contract.
3. Do not globally change `SshSessionCache::default()` or `SshSessionCache::new()` to insecure compatibility behavior.
4. Do not make native/TOML SSH inherit `InsecureCompatibility` merely because the embed crate also has `pproxy-compat` enabled.
5. Authentication, host-key, transport, and target failures must fail closed. No SSH failure may degrade to a direct connection or silently remove an SSH hop.
6. Credentials and private-key paths must retain existing redaction guarantees in errors and debug output.

The implementation should make the policy choice at the constructor boundary rather than hiding the distinction in a global mutable setting.

---

# Scope guardrails

## In scope

- complete SSH initialization inside `eggress_embed::outbound::OutboundConnector`;
- internal consolidation of duplicated chain-executor construction if useful;
- preserving different native and pproxy compatibility SSH host-key policies;
- decoupling `eggress-embed/ssh` from unconditional activation of `eggress-pproxy-compat`;
- focused runtime interoperability coverage at the embed boundary;
- low-cost feature-slice CI checks;
- embed README/API documentation of the corrected feature contract;
- dependency-graph evidence showing the intended feature isolation.

## Out of scope

Do not use this correction to introduce:

- an `eggpool` Cargo feature or any downstream-product-specific feature;
- an `EggpoolConnector` or other consumer-specific type;
- a new public SSH session-cache/executor API solely to let downstreams rebuild Eggress internals;
- a new `eggress-outbound` crate;
- a second SSH implementation;
- new SSH authentication mechanisms;
- global weakening of host-key verification;
- a caller-configurable Rustls/test-root API solely for one downstream's test fixtures;
- unrelated pproxy parity work;
- removal of supported protocols to chase binary size;
- changes to release-profile/LTO/strip settings intended to tune a downstream binary;
- a feature-power-set CI matrix.

If a clean implementation can remain entirely inside the existing `eggress-embed` facade plus test infrastructure, prefer that.

---

# Confirmed defect 1 — OutboundConnector does not install SSH session state

Primary file:

- `crates/eggress-embed/src/outbound.rs`

Current SSH-enabled construction uses a `ChainExecutor` with the SSH cache argument set to `None` in multiple constructor paths. The SSH transport itself already owns the correct reusable session abstraction:

- `SshSessionCache::new()` — verified native host-key policy;
- `SshSessionCache::new_compatibility()` — explicitly opted-in pproxy compatibility behavior;
- `SshSessionCache::with_known_hosts(path)` — caller-selected verified file for the lower-level transport surface.

The public embed facade should instantiate the appropriate existing cache instead of forcing each consumer to reconstruct the executor.

A constructor-only test is not sufficient for this defect. The current code already gets far enough to parse/compile an SSH expression; the failure is in runtime chain execution. Closure therefore requires at least one listener-free `OutboundConnector::connect_tcp()` interoperability test through a real SSH server fixture.

---

# Workstream 1 — Centralize OutboundConnector executor construction

## Goal

Give every `OutboundConnector` constructor a single internal path for building `ChainExecutor` with the correct optional transport state.

## Primary files

- `crates/eggress-embed/src/outbound.rs`
- `crates/eggress-embed/Cargo.toml`

## Required changes

1. Identify every `eggress_server::build_chain_executor(...)` call in `outbound.rs` and remove policy duplication where practical.

2. Add a private helper or other private internal structure that selects executor construction based on the constructor's semantic mode. The exact implementation is intentionally not prescribed, but conceptually it may distinguish:

   ```text
   native verified outbound
   pproxy compatibility outbound
   direct/no-proxy outbound
   ```

   Do not expose this policy enum/type publicly unless implementation proves a broader public requirement independently of this correction.

3. Under `#[cfg(feature = "ssh")]`, native/TOML outbound construction must provide a reusable SSH session cache using the verified native policy.

4. Under `#[cfg(all(feature = "ssh", feature = "pproxy-compat"))]`, `from_pproxy_uri()` must provide a reusable SSH session cache using the established pproxy compatibility policy.

5. Direct mode should not create SSH state merely because the crate was compiled with `ssh`.

6. Under `#[cfg(not(feature = "ssh"))]`, the helper must compile without referring to `eggress_transport_ssh` and must continue using the non-SSH `build_chain_executor` signature.

7. Preserve the current `OutboundConnector` public constructor signatures. The preferred implementation requires no breaking public API change.

8. Keep the cache alive for the connector lifetime. It must not be created per `connect_tcp()` call in a way that defeats session reuse.

9. Preserve existing cancellation/drop semantics. If `SshSessionCache` needs explicit shutdown to avoid lingering handles, determine whether dropping the connector is already sufficient. Do not add a public shutdown method unless a resource-lifetime test demonstrates it is required for correctness. If an internal drop/shutdown hook is needed, keep it minimal and document why.

## Acceptance criteria

- `OutboundConnector::from_toml()` can execute an SSH upstream when `ssh` is enabled.
- `OutboundConnector::from_pproxy_uri()` can execute an SSH pproxy expression when both `pproxy-compat` and `ssh` are enabled.
- Existing direct, HTTP, SOCKS, Shadowsocks/Trojan, UDP, and multi-hop behavior is unchanged.
- SSH failure never falls back to direct egress.
- The facade no longer requires a downstream to construct `ChainExecutor` or `SshSessionCache` manually for normal supported SSH use.

---

# Workstream 2 — Preserve native vs compatibility SSH policy

## Goal

Fix the facade without turning pproxy's compatibility exception into Eggress's global SSH policy.

## Required changes

### Native/TOML constructor

For `OutboundConnector::from_toml()`:

- use the normal verified SSH policy;
- preserve known-host verification failures as hard connection failures;
- do not silently switch to compatibility mode when `pproxy-compat` happens to be enabled in the same build;
- do not require `pproxy-compat` merely to instantiate native SSH state.

### pproxy constructor

For `OutboundConnector::from_pproxy_uri()`:

- require `pproxy-compat` as it already does;
- when SSH support is selected, use the existing explicit compatibility cache/policy;
- keep the compatibility choice local to this compatibility constructor;
- retain credential/path redaction in all surfaced errors.

### Mixed/multi-hop chains

A pproxy chain containing SSH in any hop must receive the same compatibility session cache for the executor lifetime. Do not special-case only an SSH first hop. Existing multi-hop ordering remains authoritative.

A native compiled chain containing SSH at any hop must receive the verified native cache.

## Qualification requirements

At least one test must demonstrate that native/TOML mode does **not** accidentally inherit `InsecureCompatibility`. If the current OpenSSH fixture cannot easily install a temporary `known_hosts` entry for a positive native test, a deterministic host-key rejection test is sufficient to prove fail-closed native policy, provided a separate pproxy compatibility test proves successful SSH traversal.

Prefer a positive native known-hosts test if it can be implemented without adding a test-only public API.

---

# Confirmed defect 2 — `ssh` force-enables pproxy compatibility

Primary file:

- `crates/eggress-embed/Cargo.toml`

Current feature wiring:

```toml
pproxy-compat = ["dep:eggress-pproxy-compat"]
ssh = ["eggress-runtime/ssh", "eggress-pproxy-compat/ssh"]
```

This activates the optional compatibility dependency when a consumer requests only native SSH.

## Required change

Use weak optional-dependency feature forwarding:

```toml
pproxy-compat = ["dep:eggress-pproxy-compat"]
ssh = ["eggress-runtime/ssh", "eggress-pproxy-compat?/ssh"]
```

or the semantically equivalent Cargo representation if the manifest is reorganized during implementation.

The intended feature combinations are:

| Embed features | Expected capability / graph |
|---|---|
| none | lean embed base; no SSH, no pproxy compatibility |
| `ssh` | native/TOML SSH available; `eggress-pproxy-compat` is not activated |
| `pproxy-compat` | pproxy constructor available; SSH remains unavailable/fails closed when requested |
| `ssh,pproxy-compat` | pproxy constructor plus SSH compatibility transport available |

Do not add `ssh` to `full` as part of this pass unless the repository already has a separate documented decision that default/full builds must include SSH. The objective is feature isolation, not broadening default capability.

## Acceptance criteria

- `cargo tree` for an `ssh`-only `eggress-embed` build contains no `eggress-pproxy-compat` dependency path.
- `cargo tree` for `ssh,pproxy-compat` contains `eggress-pproxy-compat` with its SSH support enabled.
- native SSH still compiles in the `ssh`-only slice.
- pproxy SSH still compiles and executes in the combined slice.

---

# Workstream 3 — Add embed-level SSH interoperability regression coverage

## Goal

Catch the exact facade defect rather than merely retesting the lower-level SSH transport.

## Existing reusable coverage

`crates/eggress-transport-ssh/tests/openssh.rs` already contains a real OpenSSH fixture using local `sshd`/`ssh-keygen`, local echo targets, public-key authentication, authentication-failure checks, concurrent channels, multi-hop SSH, Unix forwarding, and remote forwarding. The new embed regression should reuse this fixture logic rather than introducing a fake executor or external Internet dependency.

## Preferred test-infrastructure approach

If the fixture can be reused with a small amount of shared code, extract only the generally useful process/fixture helpers into `eggress-testkit` or another existing test-only location. Keep this extraction deliberately narrow:

- OpenSSH process startup/teardown;
- generated host/client keys;
- fixture user/address/private-key path;
- known-hosts material/path if needed for native verification;
- local echo server helper if it is not already provided elsewhere.

Do not move SSH production abstractions into testkit and do not create a large new test framework.

If extraction would cause more churn than a small local embed fixture, a concise embed-local fixture is acceptable. Avoid copy-pasting the entire existing transport test file.

## Mandatory runtime tests

### A. pproxy compatibility SSH actually transports bytes

Compile/run under `ssh,pproxy-compat`.

1. Start the existing/local OpenSSH fixture.
2. Start a local TCP echo target.
3. Build a pproxy SSH expression using the fixture credentials/key.
4. Construct with `OutboundConnector::from_pproxy_uri()`.
5. Call `connect_tcp()` to the echo target.
6. Write a sentinel payload and read it back.
7. Assert the bytes traversed SSH successfully.

This test must fail on the pre-fix facade. Constructor success alone does not satisfy this requirement.

### B. authentication failure remains fail-closed and redacted

Use an intentionally invalid password/key configuration through `OutboundConnector`. Assert:

- connection fails;
- no direct target connection is returned;
- the secret is absent from `Display`/diagnostic text;
- the result is classified through the existing stable embed error surface rather than panicking.

### C. native/TOML SSH preserves verified host-key policy

Construct the equivalent native/TOML upstream and verify one of the following, in preference order:

1. positive traversal with a temporary deterministic `known_hosts` file/state already supported by native Eggress configuration; plus a mismatched-key rejection; or
2. deterministic rejection of the untrusted fixture host, proving native mode did not inherit the pproxy compatibility bypass.

Do not add a public insecure-native escape hatch merely to make this test convenient.

### D. optional multi-hop regression

If the fixture extraction makes it inexpensive, add an embed-level `ssh://...__ssh://...` or mixed SSH chain smoke test because the lower-level transport already supports two-hop SSH and the public pproxy connector advertises ordered multi-hop chains. This is valuable but secondary to A-C and should not block the correction if it materially increases test complexity.

## Test portability

The existing OpenSSH tests skip when `sshd`/`ssh-keygen` are unavailable. Preserve that convention for local/full interoperability coverage.

Do not make the default CI depend on installing/configuring a system SSH daemon unless the current runner image already supplies the prerequisites reliably. The cheap compile/feature graph checks belong in default CI; the live SSH test may remain conditional in the workspace test suite if that matches existing project policy.

---

# Workstream 4 — Feature-slice compile and dependency-graph guards

## Goal

Prevent future facade changes from re-coupling optional pproxy compatibility to native SSH.

## Required local qualification

Run all four meaningful feature slices:

```bash
cargo check -p eggress-embed --locked --no-default-features
cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat"
```

Then inspect the feature graph:

```bash
cargo tree -p eggress-embed --locked --no-default-features --features ssh -e features
cargo tree -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat" -e features
```

Record in closure evidence that:

- `ssh`-only does not activate `eggress-pproxy-compat`;
- the combined slice does activate it and its SSH subfeature;
- `eggress-runtime/ssh`, `eggress-server/ssh`, and `eggress-transport-ssh` remain present where expected.

## CI change

The current CI already has one low-cost optional compatibility compile check. Extend the existing Rust smoke job rather than adding a new matrix/job.

Add enough `eggress-embed` checks to lock the important boundary, preferably:

```bash
cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat"
```

If `pproxy-compat`-only has meaningful cfg-sensitive source, include that slice as well. Do not add a Cartesian feature matrix.

Cargo-tree output does not need to run in CI if compile checks plus a manifest-level regression test or review evidence sufficiently locks the relationship; keep CI proportional.

---

# Workstream 5 — Documentation and public contract

## Primary files

- `crates/eggress-embed/README.md`
- `docs/EMBED_API.md`
- release/changelog documentation according to current repository convention

## Required documentation changes

Clarify the feature relationship:

- `ssh` enables native/TOML SSH upstream transport;
- `pproxy-compat` enables `from_pproxy_uri()`;
- pproxy-style SSH through `from_pproxy_uri()` requires both `ssh` and `pproxy-compat`;
- selecting `ssh` alone does not imply pproxy compatibility;
- `OutboundConnector` owns required SSH session state internally.

Keep the documentation consumer-neutral. Do not mention Eggpool configuration fields, provider routing, account selection, HTTP retries, or other downstream concerns.

If the existing `full` feature documentation says it is a union of every optional protocol but the manifest does not include SSH, correct the wording only if necessary to make the documentation factual. Do not silently expand `full` in this pass.

---

# Workstream 6 — Distribution and footprint qualification

## Goal

Verify that the feature-boundary change improves dependency selection without making unsupported binary-size claims.

This correction is primarily a maintenance/API-ownership win. Consumers that actually use SSH will still link the SSH transport and its cryptographic dependencies, so a dramatic executable-size reduction is not expected merely because direct implementation-crate dependencies disappear downstream.

The measurable generic footprint win is narrower: an `eggress-embed` consumer selecting `ssh` without `pproxy-compat` should no longer compile/link the pproxy compatibility crate solely as a feature-resolution side effect.

## Required evidence

Capture before/after feature trees for the `ssh`-only and combined slices. If `cargo bloat` is already available, a small representative embed consumer measurement may be recorded, but do not add `cargo-bloat` as a project dependency or CI requirement.

Do not change Eggress's release profiles to shrink binaries produced by downstream workspaces. Final profile/LTO/strip/panic settings are controlled by the root consuming workspace and are outside this facade correction.

---

# Downstream migration signal

This section documents why the generic fix matters; it is not part of Eggress implementation closure.

Once a released Eggress version contains this correction, a downstream application that currently reconstructs the executor only to supply the SSH session cache should be able to:

1. depend on `eggress-embed` as its normal Eggress production boundary;
2. enable `ssh` and `pproxy-compat` only when those capabilities are required;
3. delete direct production dependencies on implementation crates such as `eggress-core`, `eggress-config`, `eggress-server`, `eggress-uri`, `eggress-transport-ssh`, and direct compatibility internals when those dependencies exist solely for this workaround;
4. remove its custom SSH executor/session-cache fallback;
5. rerun its own protocol and same-profile binary qualification.

Do not encode those downstream dependency names or migration rules into Eggress source code/features. Successful downstream simplification is evidence that the facade boundary is complete, not a reason to specialize Eggress.

---

# Implementation order

Implement in this order:

```text
1. Add a failing embed-level pproxy SSH runtime regression
2. Centralize/private-helper executor construction in outbound.rs
3. Install verified native SSH session state for from_toml()
4. Install compatibility SSH session state for from_pproxy_uri()
5. Add auth-failure/redaction and native host-key-policy regressions
6. Change embed SSH feature forwarding to weak optional forwarding
7. Run the four feature-slice compile checks and inspect cargo tree
8. Add the minimal CI feature guards
9. Update embed README/API documentation
10. Run full workspace qualification and record closure evidence
```

If step 2 starts requiring a new public API, stop and verify that the requirement is truly generic. The expected implementation should remain private to `eggress-embed`.

---

# Verification matrix

## Formatting and workspace qualification

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --doc --workspace --all-features --locked
```

If the repository's normal full qualification intentionally does not use `--all-features` for every workspace test because mutually exclusive/operational feature combinations exist, preserve the repository's established command and add the focused embed commands below rather than weakening existing feature design.

## Focused embed slices

```bash
cargo check -p eggress-embed --locked --no-default-features
cargo check -p eggress-embed --locked --no-default-features --features ssh
cargo check -p eggress-embed --locked --no-default-features --features pproxy-compat
cargo check -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat"

cargo test -p eggress-embed --locked --all-features
```

Run the live OpenSSH/embed test explicitly if it is feature- or environment-gated and therefore not guaranteed to execute in the preceding command.

## Feature graph

```bash
cargo tree -p eggress-embed --locked --no-default-features --features ssh -e features
cargo tree -p eggress-embed --locked --no-default-features --features "ssh,pproxy-compat" -e features
```

## Repository policy/hygiene

Where available in the current repository workflow:

```bash
cargo deny check
git diff --check
```

Do not add a new policy tool solely for this plan if the repository does not already use it.

---

# Closure evidence

Before changing this plan's status to complete, append a concise `## Closure evidence` section containing:

- implementation commit SHA;
- exact `eggress-embed` feature definition after the change;
- confirmation that `ssh`-only no longer activates `eggress-pproxy-compat`;
- confirmation that `ssh,pproxy-compat` still enables pproxy SSH;
- focused embed SSH interoperability results, including whether the real OpenSSH fixture actually executed or was skipped due missing host tools;
- native host-key-policy regression result;
- authentication failure/redaction result;
- full workspace fmt/check/clippy/test results;
- any binary/dependency footprint measurement performed;
- explicit statement that no Eggpool/downstream-specific public API or feature was introduced.

If the live OpenSSH test is skipped on the implementation host, do not claim runtime SSH closure from compile-only evidence. Run it on a host/CI environment with `sshd` and `ssh-keygen` before final closure, or leave the plan partially open with that remaining gate documented.

---

# Completion criteria

This corrective pass is complete when all of the following are true:

- [ ] `OutboundConnector` installs the SSH session state required for real listener-free SSH execution.
- [ ] `from_toml()` preserves verified native host-key policy.
- [ ] `from_pproxy_uri()` preserves the explicit pproxy compatibility host-key policy.
- [ ] SSH failure cannot silently become direct egress or a shorter chain.
- [ ] A real embed-level pproxy SSH connection transports bytes through the OpenSSH fixture.
- [ ] Authentication failure remains fail-closed and credential-safe.
- [ ] Native host-key verification is covered by a deterministic regression.
- [ ] `eggress-embed/ssh` no longer force-activates `eggress-pproxy-compat`.
- [ ] `ssh`-only, `pproxy-compat`-only, and combined feature slices compile.
- [ ] `ssh,pproxy-compat` still provides working pproxy SSH behavior.
- [ ] Existing non-SSH `OutboundConnector` behavior remains green.
- [ ] CI has a proportional guard for the important embed feature slices.
- [ ] Embed README/API docs describe the feature contract accurately.
- [ ] No consumer-specific feature, type, mode, or API was added.
- [ ] No unnecessary public executor/session-cache surface was introduced.
- [ ] Full repository qualification passes.
- [ ] Closure evidence is appended to this plan.

---

# Non-goals reiterated

Do not broaden this work into a new Eggress architecture project. In particular, do not:

- split `eggress-embed` merely to reduce theoretical linker footprint;
- expose internal executor construction as the primary solution;
- add a downstream-specific adapter;
- make compatibility-mode SSH the default native security policy;
- redesign pproxy URI parsing already covered by the prior corrective pass;
- add QUIC/UDP/relay work unrelated to this defect;
- remove protocols from default/full builds as a size optimization;
- tune downstream release profiles from inside Eggress.

## Closure evidence

- Implementation commit: `4f6ec74` (`fix(embed): initialize SSH outbound connector state`).
- Final `eggress-embed` feature definition:

  ```toml
  ssh = [
      "dep:eggress-transport-ssh",
      "eggress-runtime/ssh",
      "eggress-pproxy-compat?/ssh",
  ]
  ```

- `ssh`-only dependency-tree inspection contains `eggress-transport-ssh` and
  does not activate `eggress-pproxy-compat`; the combined
  `ssh,pproxy-compat` tree contains both and enables the compatibility crate.
- The embed OpenSSH fixture executed locally: 3 passed (pproxy byte
  traversal, fail-closed redacted authentication failure, and native TOML
  untrusted-host-key rejection). The existing transport fixture also ran: 7
  passed, with only its optional password-success case skipped because
  `EGRESS_SSH_TEST_PASSWORD` was unset.
- Native host-key policy regression passed: the verified native cache rejected
  the fixture host without a known_hosts entry.
- Authentication failure/redaction regression passed: the invalid SSH
  password did not appear in the surfaced error and no direct fallback
  occurred.
- Verification passed locally:
  - `cargo fmt --all -- --check`
  - `cargo check --workspace --all-targets --locked`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace --locked`
  - focused no-default embed checks for `ssh`, `pproxy-compat`, and
    `ssh,pproxy-compat`
  - SSH-enabled CLI compile gate from `AGENTS.md`
  - focused embed Clippy with SSH and compatibility features
  - `cargo test -p eggress-embed --locked --all-features`
- Footprint evidence was limited to the feature trees; no binary-size or
  `cargo-bloat` measurement was performed.
- No Eggpool/downstream-specific public API, feature, type, or mode was
  introduced, and no public executor/session-cache construction surface was
  added.

The narrow architectural target is simple: the stable public outbound facade should completely own the internal state required to execute the capabilities it advertises, and optional Cargo features should not activate unrelated compatibility layers unless the consumer requests them.
