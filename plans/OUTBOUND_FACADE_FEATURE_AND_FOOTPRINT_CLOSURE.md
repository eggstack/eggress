# Outbound Facade Feature, Footprint, and Release Closure

## Status

**COMPLETE — VERIFIED 2026-09-19**

## Target repository

`eggstack/eggress`

Planning code baseline:

`0d348977974d41fb08675504181009569e3273c4` (`main`, Eggress 1.0.7)

Parent roadmap:

- `plans/OUTBOUND_FACADE_DEPENDENCY_CLEANUP_ROADMAP.md`

Required predecessor:

- `plans/OUTBOUND_EXECUTION_CRATE_EXTRACTION.md`

Do not execute this plan against a partial extraction. First establish one implementation authority for outbound composition and `OutboundConnector`.

## Objective

Finish the outbound cleanup after extraction by proving that the new direct crate actually provides a smaller maintenance/dependency boundary, preserving existing `eggress-embed` compatibility, updating package/release topology, and making the result publishable for downstream adoption.

This plan is deliberately evidence-driven. The goal is to remove unnecessary service-layer reachability and duplicated ownership. A binary-size reduction is desirable but must be measured rather than assumed.

---

# Expected starting state

After the predecessor plan:

- `eggress-outbound` owns listener-free outbound TCP composition;
- `eggress-outbound` owns the detailed typed error classifier;
- `OutboundConnector` implementation lives in `eggress-outbound`;
- `eggress-server` consumes the outbound crate;
- `eggress-embed::outbound::*` re-exports the outbound API;
- UDP/config/pproxy/SSH/extended/legacy/QUIC behavior is feature-gated in the new crate.

If any of those are not true, stop and finish the extraction first.

---

# Workstream 0 — Record the post-extraction manifest graph

Capture:

```sh
cargo tree -p eggress-outbound -e features --no-default-features
cargo tree -p eggress-outbound -e features --no-default-features --features pproxy-compat
cargo tree -p eggress-outbound -e features --no-default-features --features ssh,pproxy-compat
cargo tree -p eggress-outbound -e features --no-default-features --features pproxy-compat,pproxy-legacy,legacy-crypto,ssh
cargo tree -p eggress-embed -e features --no-default-features --features pproxy-compat
cargo tree -p eggress-server -e features --no-default-features
```

Also run inverse queries for the minimal/direct consumer slices:

```sh
cargo tree -p eggress-outbound -i eggress-server
cargo tree -p eggress-outbound -i eggress-runtime
cargo tree -p eggress-outbound -i eggress-metrics
cargo tree -p eggress-outbound -i eggress-udp
cargo tree -p eggress-outbound -i eggress-config
```

Interpret "package not found" for inverse queries as the desired result when that package should be absent for the selected features.

Do not inspect only the workspace-wide lockfile. Optional workspace packages may be present in `Cargo.lock` without being reachable in a selected build.

---

# Workstream 1 — Close the direct outbound feature graph

Audit `crates/eggress-outbound/Cargo.toml` after extraction and remove accidental dependency reachability.

## Base TCP profile

The base profile must not resolve:

- `eggress-server`;
- `eggress-runtime`;
- `eggress-metrics`;
- `eggress-admin`;
- `eggress-system-proxy`;
- `eggress-udp`;
- reverse-proxy service machinery.

If one remains, identify the actual source edge and move or feature-gate that ownership rather than accepting the graph.

## pproxy-only profile

`--features pproxy-compat` should resolve the compatibility parser/translator but should not require full service configuration merely to retain the translated chain.

If `eggress-config` appears only because `eggress-pproxy-compat` itself requires it, determine whether that is legitimate general translation ownership or another avoidable compatibility-layer coupling. Do not expand this plan into a redesign of all pproxy translation unless the edge is straightforward to remove.

The hard requirement is that `eggress-outbound` itself does not synthesize/store `RuntimeConfig` merely to execute a pproxy chain.

## TOML profile

`--features toml` may resolve `eggress-config` and its legitimate compile/validation dependencies.

Do not make TOML parsing part of the base feature set.

## UDP profile

`eggress-udp` must enter only through the explicit `udp` feature.

## Extended/legacy profile

Shadowsocks/Trojan/WebSocket/SSR and legacy cipher families must remain opt-in according to their existing capability gates.

Do not fold `legacy-crypto` into `pproxy-compat` or `extended` automatically.

## SSH profile

`ssh` should pull the SSH transport/session cache but not pproxy translation unless `pproxy-compat` is also selected.

---

# Workstream 2 — Close `eggress-embed` compatibility wiring

Audit `crates/eggress-embed/Cargo.toml` and remove direct dependencies that existed only because `outbound.rs` was implemented locally.

Likely candidates must be proven by compiler/tree before removal. Do not mechanically delete dependencies also used by:

- `EggressConfig`;
- `EggressService`;
- `EggressHandle`;
- reload/status/metrics;
- full-service compatibility hooks.

## Preserve existing outbound availability

Current `eggress-embed` users must retain the same public methods under the same relevant feature selections.

Because the direct `eggress-outbound` crate intentionally makes UDP/TOML optional, `eggress-embed` may enable those features on its internal dependency to preserve its established facade.

The direct crate and full-service facade are allowed to have different default feature philosophies:

- `eggress-outbound`: narrow, compositional;
- `eggress-embed`: compatibility/full-service oriented.

Do not broaden the direct crate merely to make the embed manifest simpler.

## Rustdoc/API check

Generate docs or compile a small compatibility test proving these paths remain valid:

```rust
eggress_embed::outbound::OutboundConnector
eggress_embed::outbound::OutboundInfo
eggress_embed::outbound::OutboundConnectError
eggress_embed::outbound::OutboundConnectErrorKind
eggress_embed::outbound::OutboundConnectStage
eggress_embed::outbound::UdpAssociation
```

No duplicate wrapper type should be introduced solely for path preservation.

---

# Workstream 3 — Close `eggress-server` feature forwarding

Audit server features after it begins depending on `eggress-outbound`.

The server's features must forward the outbound capability required by that server build without activating unrelated direct-crate features.

Examples:

- server `extended` -> outbound `extended`;
- server `pproxy-legacy` -> outbound `pproxy-legacy`;
- server `legacy-crypto` -> outbound `legacy-crypto`;
- server `ssh` -> outbound `ssh`;
- server `quic` -> outbound `quic`.

Do not enable outbound `toml`, `pproxy-compat`, or `udp` merely because the server crate exists unless server source actually requires those surfaces.

The server should consume executor construction/classification, not the high-level pproxy/TOML facade.

---

# Workstream 4 — Package boundary and API hygiene

The new crate must have a clean package surface.

## Public API

Prefer exposing:

- `OutboundConnector`;
- `OutboundInfo`;
- detailed error types;
- native-chain/direct constructor(s);
- optional constructor/method families under explicit features;
- any small executor-builder type genuinely useful to `eggress-server`.

Keep concrete hop implementation details private where practical.

Do not expose:

- server session types;
- runtime supervisor types;
- EggPool/Eggfetch adapter types;
- internal protocol error enums solely for classification;
- raw credential-bearing configuration state.

## README

The new crate README should answer:

- use this crate for listener-free outbound chain execution;
- use `eggress-embed` to run/manage a full proxy service;
- ordinary HTTP/SOCKS TCP is available in the base profile;
- optional features add syntax/protocol families;
- typed errors are diagnostic facts, not retry policy;
- proxy failure never silently falls back to direct.

---

# Workstream 5 — Update manual publication topology

The workspace currently documents/publishes 27 crates. Adding `eggress-outbound` changes the internal release DAG and total count.

Audit all hard-coded crate counts and package lists, including at minimum:

- root `AGENTS.md`;
- `scripts/publish-remaining.sh`;
- release documentation under `docs/release/`;
- preflight/version-validation scripts;
- any publish-order tests or comments;
- architecture/project-structure docs.

Do not update a count without confirming the actual current workspace member set.

## Publish order

Compute the dependency DAG from the final manifests.

At a high level, `eggress-outbound` must be published:

- after all mandatory and optional same-version internal dependencies that Cargo package resolution requires;
- before `eggress-server`;
- before `eggress-embed`;
- before any other facade that gains a direct dependency on it.

Because the proposed crate may have optional `eggress-config` and `eggress-pproxy-compat` dependencies, those published packages may need to precede it even when an ordinary minimal build does not enable their features.

Do not copy the old server tier unchanged.

After editing `scripts/publish-remaining.sh`, revalidate that each internal dependency appears in an earlier tier than its dependent.

The expected total becomes 28 only if no other workspace membership change has occurred by implementation time.

---

# Workstream 6 — Packaging qualification

Run:

```sh
cargo package -p eggress-outbound
cargo publish -p eggress-outbound --dry-run
cargo package -p eggress-server
cargo publish -p eggress-server --dry-run
cargo package -p eggress-embed
cargo publish -p eggress-embed --dry-run
```

Use the repository's canonical clean-tree release process where it supersedes these examples.

Inspect generated manifests and confirm:

- internal versions resolve to the intended exact workspace version;
- no path-only unpublished dependency is required;
- optional dependencies are correctly versioned;
- feature forwarding survives package normalization;
- README/license metadata are present.

Do not use `--no-verify`.

---

# Workstream 7 — CI boundary updates

Keep hosted CI small.

Add only the checks necessary to protect the new architectural boundary.

Recommended compile-only slices:

```sh
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
```

Consider `udp` as one additional compile slice if it is not exercised by the default workspace test run.

Do not add all feature combinations.

The existing required OpenSSH runtime gate should be moved to the behavior-owning crate if practical, or retained through the embed re-export if that keeps fixture maintenance simpler. There must still be one required runtime regression proving:

- SSH bytes traverse;
- authentication failure is fail-closed/redacted;
- native untrusted-host behavior remains enforced;
- pproxy compatibility mode uses its intended policy.

Do not duplicate the expensive OpenSSH fixture in two crates without need.

Update `docs/CI_STATUS.md` and `AGENTS.md` only to reflect actual checks.

---

# Workstream 8 — Dependency-policy qualification

This is a dependency graph change, so run:

```sh
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0009
```

Review:

- licenses of all newly reachable packages for minimal profiles;
- duplicate-version changes;
- whether extracting the crate unexpectedly activates a new crypto backend;
- whether SSH/legacy feature slices retain their known advisory posture;
- whether the minimal TCP profile avoids SSH/RSA entirely when `ssh` is off.

Do not broaden advisory ignores as part of extraction unless a separately reviewed dependency change requires it.

---

# Workstream 9 — Measure direct-consumer footprint

The important comparison is not the full `eggress-embed` binary after re-export; the full-service crate still intentionally owns service dependencies.

Measure the cost of the listener-free capability itself.

## Reproducible local comparison harness

Use two tiny temporary binaries with identical source behavior and the same toolchain/target/profile.

### Baseline consumer

Depend on `eggress-embed` outbound path with the selected feature profile.

### Candidate consumer

Depend directly on `eggress-outbound` with the equivalent outbound protocol features.

Each binary should do enough to prevent dead-code elimination of construction/public types without making a live network call mandatory. For example:

- parse/construct a representative pproxy chain;
- reference the detailed error/type surface;
- optionally build but do not execute a connector.

For an EggPool-like stress profile, compare:

```text
pproxy-compat + pproxy-legacy + legacy-crypto + ssh
```

Also compare a normal minimal profile:

```text
HTTP/SOCKS TCP only
```

Record:

```text
metric                         embed baseline   outbound direct   delta
----------------------------------------------------------------------
resolved package count
unique package count
duplicate package families
release binary bytes
stripped binary bytes
```

Use the same rustc, target, linker, release profile, and stripping method.

## Expected graph result

For direct outbound consumption, the following should disappear unless explicitly selected for another reason:

- `eggress-runtime`;
- `eggress-server`;
- `eggress-metrics`;
- `eggress-admin`;
- `eggress-system-proxy`;
- `eggress-udp` when UDP is off;
- reverse-proxy-only packages.

A materially unchanged graph means the extraction has not achieved its main purpose and should be investigated before release.

## Size classification

Classify honestly.

### Strong win

Service/runtime dependency families leave the graph and artifact shrinks measurably.

### Maintenance/graph win, byte-neutral

Service/runtime families leave the graph, but linker/codegen makes final bytes roughly unchanged. This is acceptable if the maintenance and dependency boundary is clearly improved.

### Regression requiring explanation

The direct outbound graph is larger or still includes service/runtime machinery. Identify why before claiming completion.

Do not advertise a size reduction without measured bytes.

---

# Workstream 10 — Full behavioral verification

Run the repository's normal broad gate:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Run the current optional compatibility compile gate, adjusted for the new crate as necessary.

Run relevant pproxy differential/interop suites only if compatibility behavior or claims changed. Pure crate movement with behavior-preserving translation should not manufacture a new parity claim.

Run the required OpenSSH regression.

If outbound HTTP CONNECT implementation itself changed rather than merely moving crates, run its dedicated protocol tests and respect the separate Eggfetch CONNECT consolidation plan/stop conditions.

---

# Workstream 11 — Documentation closure

Update durable ownership docs:

- `architecture/overview.md`;
- new `architecture/outbound.md`;
- `architecture/server.md`;
- `architecture/embed.md`;
- `docs/ARCHITECTURE.md` if it duplicates the high-level map;
- root README Rust library/outbound examples;
- `crates/eggress-embed/README.md`;
- new outbound crate README.

Document the decision boundary clearly:

```text
Need full in-process proxy lifecycle? -> eggress-embed
Need listener-free outbound proxy-chain dialing? -> eggress-outbound
Need only generic byte relay? -> eggress-relay
```

Do not mention EggPool in user-facing crate descriptions except, if desired, as one non-normative example consumer in development notes.

---

# Workstream 12 — Release readiness and downstream handoff

The cleanup is not consumable by external repositories until `eggress-outbound` and the aligned Eggress crates are published to crates.io.

Prepare the next normal Eggress release according to `docs/release/RELEASE_PROCESS.md`.

Do not hard-code a future version number in implementation logic. Determine the next available release version at release time.

The release candidate must include:

- the new crate;
- updated internal exact pins;
- updated publish order;
- package/dry-run success;
- normal CI;
- release-specific security checks;
- documentation.

Actual crates.io publication remains an operator action under the repository's existing manual release policy.

## Downstream adoption note

Once the release exists, record in this plan's closure note:

- released Eggress version;
- `eggress-outbound` version;
- implementation commit;
- public feature names needed by direct consumers;
- whether `eggress_embed::outbound::*` remained source-compatible;
- package-count and binary-size measurements;
- any intentionally retained service dependency.

EggPool should then perform its own dependency bump/removal plan. Do not edit EggPool from this plan.

---

# Closure record template

Append a concise record when implemented:

```text
Implementation commit: 022db65 (refactor(outbound): close facade feature, footprint, and release topology)
Released version: unreleased (workspace still 1.0.7; operator publishes per docs/release/RELEASE_PROCESS.md)
eggress-outbound package: 1.0.7 — `cargo package` + `cargo publish --dry-run` pass; internal deps pinned `=1.0.7`, optional flags preserved
Publish-order update: scripts/publish-remaining.sh re-tiered to 28 crates (outbound tier 8, before server tier 9; metrics/admin/runtime shifted downstream); topology validated programmatically, zero violations
MSRV: 1.85 (unchanged; packaged manifest rust-version = 1.85)
Minimal graph before/after: embed-baseline 147 resolved / 111 unique pkgs -> outbound-direct 93 / 74 (service families server/runtime/metrics/config/routing/udp gone)
EggPool-like graph before/after: 356 / 218 -> 333 / 206 (server/runtime/metrics/embed gone; config/routing/udp retained via pproxy-compat translation ownership)
Minimal binary bytes before/after: 1899136 -> 1899136 (delta 0)
EggPool-like binary bytes before/after: 3829544 -> 3829544 (delta 0)
Size classification: maintenance/graph win, byte-neutral (tiny LTO/GC-converged harnesses; no size reduction claimed)
Workspace tests: 134 suites ok, 0 failures (cargo test --workspace --locked)
Feature checks: outbound base/pproxy-compat/ssh/ssh,pproxy-compat/udp + embed ssh/pproxy-compat/ssh,pproxy-compat + cli full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon bins + fuzz bins — all pass
OpenSSH regression: 3 passed (EGRESS_REQUIRE_OPENSSH_TESTS=1, embed ssh,pproxy-compat --test ssh)
cargo deny: clean (advisories/bans/licenses/sources ok)
cargo audit: exit 0 (only allowed der/wnaf yanked warnings)
Package/dry-run: outbound ok; server/embed packaging structurally blocked until outbound 1.0.7 reaches the crates.io index (expected new-crate ordering; release follows tier order)
Deferred follow-ups: none — operator publication (outbound tier 8 before server tier 9) is the remaining release step; downstream EggPool bump is out of scope for this plan
```

---

# Acceptance criteria

- [ ] `eggress-outbound` is independently packageable.
- [ ] The minimal direct crate excludes server/runtime/metrics/admin/system-proxy.
- [ ] UDP is absent unless selected.
- [ ] TOML/config parsing is not mandatory for native-chain execution.
- [ ] Existing `eggress_embed::outbound::*` paths compile unchanged.
- [ ] Existing full-service embed behavior is unchanged.
- [ ] Server feature forwarding activates only required outbound capabilities.
- [ ] Workspace crate count/publish script/docs reflect the actual new member count.
- [ ] Publish order is topologically valid.
- [ ] `cargo package`/dry-run succeeds for the new crate and affected dependents.
- [ ] Minimal and EggPool-like dependency trees are recorded before/after.
- [ ] Consumer binary measurements are recorded with controlled methodology.
- [ ] No unmeasured size claim is made.
- [ ] Normal workspace gate passes.
- [ ] Dependency/security gates pass.
- [ ] Required SSH runtime regression passes.
- [ ] Release documentation is ready for operator publication.
- [ ] Downstream adoption requires no Eggress source patch or EggPool-specific upstream feature.

## Stop conditions

Stop and revise if:

- the direct outbound tree still pulls `eggress-server` or `eggress-runtime`;
- `udp` cannot be disabled without breaking TCP compilation;
- direct pproxy execution still requires a synthetic `RuntimeConfig`;
- package publication creates an internal dependency cycle;
- preserving embed compatibility requires duplicate implementations;
- minimal binary/package footprint materially regresses without a compelling architectural explanation;
- MSRV must be raised solely to support the extraction;
- the release would require a git/path dependency for external consumers.

The correct outcome is a clean reusable Eggress boundary, not merely another crate name around the same service graph.
