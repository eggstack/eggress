# Delivery Milestone 003 — Eggup Archive/Pair Self-Update Adoption

Status: implemented; closed by `plans/closure/delivery/003-status.md` (registry 0.1.2 pair adopted, hosted Linux/macOS/Windows updater lanes green)

Repository baseline: `8cb2caf3977e98c00569a551e9466b6ad1654c89`

Source roadmap:

- `plans/subsystems/delivery-roadmap.md`

Long-term requirements:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`

Cross-repository contract:

- `eggstack/eggup: plans/implementation/consumer-adoption/006-egress-archive-pair-adoption.md`
- `eggstack/eggup: plans/implementation/verified-update-core/008-core-archive-consumer-package-qualification.md`
- Eggup Archive M001d closure: `eggstack/eggup@18d83decd0c894279dd5e5d89407e1423f839d99`, hosted run `36335233644`

Primary class: capability / delivery convergence

## 1. Objective

Replace Eggress's duplicated generic archive extraction and two-binary backup/rollback mechanics in `eggress update` with the qualified Eggup archive extraction and multi-artifact transaction layers, while preserving every Eggress-owned release, integrity, candidate, CLI, and publication policy.

The resulting updater must still treat `eggress` and `pproxy` as one version-aligned release unit, but Eggress should no longer own generic tar/zip extraction or generic two-member transactional replacement.

## 2. Why this milestone is ready (gate satisfied 2026-09-28)

The required Eggup runtime contracts are implemented, qualified upstream, and
now published as a compatible registry pair:

- Eggup M001d added `eggup-core::BoundSources`, `InstallPlan::prepare_with_bound_sources`, and `eggup-archive` bound extraction APIs after Eggup's published 0.1.1 wave.
- Eggup Verified Core M009 closed 2026-09-28: `eggup-core 0.1.2` (registry id
  `3351376`) then `eggup-archive 0.1.2` (first publication, registry id
  `3351378`), both from `eggstack/eggup@e8e07eb`, tagged `v0.1.2` with GitHub
  Release `0.1.2`; registry-only consumption proven 3/3 with no path/git
  overrides.
- A crates.io-published Eggress package must not acquire git/path-only Eggup dependencies — dependency cutover may now use the registry versions.

The pre-gate research rule below is retained as history; the gate it guarded
is closed:

Implementation research/prototyping may use the immutable qualified Eggup revision for evidence, but dependency cutover MUST NOT merge into a publishable Eggress release state until compatible versioned packages are available from the registry.

## 3. Current implementation evidence

Current updater ownership is concentrated under `crates/eggress-cli/src/update/`:

- `mod.rs`
  - resolves the latest stable GitHub release;
  - preserves `EGRESS_UPDATE_BASE_URL` offline fixture injection;
  - downloads the target archive and `.sha256` sidecar with curl;
  - verifies sibling installation identity before network work;
  - verifies the archive;
  - extracts;
  - verifies both staged candidate versions;
  - invokes pair replacement.
- `install.rs`
  - `extract_archive` shells out to `tar`; Windows may fall back to PowerShell `Expand-Archive -Force`;
  - `replace_pair` implements two-file backup/rename/copy rollback;
  - `copy_or_rename` owns cross-device fallback;
  - writable-destination and sibling-path policy are Eggress-specific.
- `verify.rs`
  - parses the SHA-256 sidecar;
  - computes archive SHA-256 through external `sha256sum` / `shasum`;
  - runs staged `eggress version` and `pproxy --version`;
  - requires both candidates to equal the release tag and one another.
- Existing offline fixture tests prove success and checksum mismatch behavior without public network access.

The current release contract is explicit: GitHub Releases is binary update authority, there is no Cargo/source fallback, no background checking, no telemetry, and no implicit sudo/UAC elevation.

## 4. Invariants that must not regress

- GitHub release discovery/tag selection remains Eggress policy.
- Target-to-asset naming remains Eggress policy.
- Existing archive-sidecar format and integrity semantics remain unchanged unless separately planned.
- SHA-256 remains integrity evidence, not an independent authenticity guarantee.
- Both candidates must execute and report exactly the expected release before live mutation.
- A successful update cannot leave mixed old/new `eggress` and `pproxy` generations.
- Pre-commit failure leaves the current pair untouched.
- Rollback-capable commit failure restores the previous pair; unrecoverable state is surfaced explicitly and never rendered as success.
- No automatic Cargo/source fallback, privilege escalation, or installer invocation is introduced.
- Existing CLI numeric exit-code contract remains unchanged.
- Offline fixture-driven correctness remains first-class.
- pproxy compatibility/runtime behavior outside updater delivery is unchanged.
- No new compatibility-manifest claim is implied by this migration.
- Rust 1.89 remains supported.
- No workspace-wide `--all-features` qualification is introduced because it enables test-only `insecure-quic`.

## 5. Scope

### In scope

- versioned dependencies on `eggup-core` and `eggup-archive` once the upstream package gate is satisfied;
- a narrow Eggress updater adapter translating the verified release archive into an exact two-member `ArchivePlan`;
- bound-source preparation through Eggup;
- candidate execution against Eggup private staged paths;
- existing-install ownership classification for the exact sibling pair;
- Eggup transactional commit/rollback for the pair;
- deletion of local generic extraction and pair transaction helpers after parity evidence;
- error/outcome mapping into existing Eggress CLI categories/messages;
- dependency and release-binary size measurement;
- Linux/macOS/Windows updater qualification;
- package dry-run after dependency cutover.

### Explicitly out of scope

- replacing GitHub release discovery;
- replacing curl acquisition in this milestone;
- adopting Eggpack ReleaseManifest as an Eggress requirement;
- changing target naming or release archive layout;
- changing checksum sidecar publication;
- changing staged-version command surfaces;
- service lifecycle integration;
- changing bootstrap installer behavior;
- tagging/publishing Eggress 1.0.10;
- changing pproxy capability/parity claims;
- changing unrelated runtime/protocol crates.

## 6. Required production changes

### Delivery / updater composition

Preserve the current flow through successful whole-archive checksum verification. After that boundary:

1. derive exactly two declared archive members for the selected target:
   - Unix: `eggress`, `pproxy`;
   - Windows: `eggress.exe`, `pproxy.exe`;
2. construct finite `eggup_archive::ArchiveLimits` from an explicit Eggress updater policy; no default may be effectively unbounded;
3. create an `ArchivePlan` for exactly those members with finite limits. `ArchiveMember` size/SHA-256 expectations are optional: the current Egress release contract may pass `None` because the complete archive has already passed its release SHA-256 gate; extraction then computes exact member byte/digest evidence from that verified snapshot. If a future release publishes member expectations, pass them as additional checks;
4. extract into an Eggup-owned private root;
5. persist and convert to object-bound sources;
6. construct the `ArtifactMember`/`ArtifactSet`/`InstallPlan` while advisory paths are still valid;
7. transfer the already-open member objects into `BoundSources`;
8. call `prepare_with_bound_sources`, never ordinary path preparation for archive members.

The current release workflow publishes whole-archive SHA-256 sidecars but no member manifest. That is compatible with the Eggup API: optional member expectations MUST NOT be synthesized from advisory extracted paths. Integrity continuity is whole verified archive -> bounded extraction from that exact file -> object-bound member evidence -> staged object.

### Candidate validation

Keep `verify_staged_pair` semantics.

Run `eggress version` and `pproxy --version` against `PreparedTransaction::staged_path` outputs before mutation. Both exact versions must equal the selected release.

Do not execute files through advisory extraction paths.

### Ownership and commit

Translate the installed sibling pair into one Eggup artifact set with explicit ownership/replaceability policy.

Use Eggup's coherent multi-artifact commit/rollback machinery. Map terminal outcomes truthfully:

- committed -> existing success message;
- rolled back -> runtime failure with previous installation restored;
- recovery required -> hard runtime failure with retained recovery evidence;
- validation/ownership failure -> hard failure before mutation.

Do not keep `replace_pair` as a hidden fallback after Eggup migration qualifies.

### Local helper deletion

After parity is proven, remove or reduce:

- `extract_archive`;
- `replace_pair`;
- `copy_or_rename`;
- any staging helper whose only remaining purpose duplicates Eggup.

Keep genuinely Eggress-specific helpers such as sibling identity, release discovery, target mapping, checksum policy, and CLI error mapping.

### Dependency/package boundary

The merged implementation must use compatible registry versions. Do not commit git/path Eggup dependencies to the publishable workspace.

Record dependency-tree and binary-size deltas before and after. If archive/core adoption materially expands the default CLI beyond the accepted delivery budget, stop for an explicit footprint decision rather than silently accepting it.

## 7. Ordered work packages

### Work package A — Dependency and evidence gate

Intent: prove the versioned upstream package boundary exists and that the current Eggress release evidence can supply a sound `ArchivePlan`.

Required changes/evidence:

- registry-visible compatible `eggup-core` + `eggup-archive` versions;
- package metadata/MSRV checked;
- whole-archive integrity continuity and optional-member-expectation posture recorded;
- baseline dependency tree and binary sizes recorded.

### Work package B — Eggress archive adapter

Intent: replace shell extraction with bounded typed extraction.

Required changes:

- map target archive format/member names;
- enforce finite limits;
- use object-bound archive handoff;
- no `tar`/PowerShell extraction in updater success path.

Acceptance evidence:

- tar.gz fixtures on Unix;
- zip fixtures including Windows;
- missing/extra/path/link/special-file negatives;
- post-plan member-entry replacement cannot redirect staged bytes.

### Work package C — Candidate validation on Eggup stage

Intent: preserve the existing version identity contract.

Required changes:

- map member IDs to staged transaction paths;
- run current candidate version checks there;
- preserve exact diagnostics/exit mapping where practical.

Acceptance evidence:

- correct pair accepted;
- wrong Eggress rejected;
- wrong pproxy rejected;
- disagreement rejected before mutation.

### Work package D — Transaction cutover

Intent: delete bespoke pair commit/rollback.

Required changes:

- ownership/preflight mapping;
- Eggup commit;
- terminal disposition mapping;
- explicit cleanup ordering after bound handles are consumed.

Acceptance evidence:

- injected failure before first mutation;
- failure after one member mutation with successful rollback;
- rollback failure -> RecoveryRequired;
- no mixed-generation success.

### Work package E — Legacy helper removal and qualification

Intent: ensure this is convergence, not a wrapper over duplicate machinery.

Required changes:

- delete superseded extraction/transaction helpers/tests;
- update updater docs;
- measure dependency/size delta;
- package dry-run and hosted matrix.

## 8. Failure, cancellation, restart, and contention semantics

- Acquisition/checksum failure: no extraction or live mutation.
- Extraction/member-evidence failure: fail closed; installed pair untouched.
- Candidate-version failure: prepared private stage is discarded/cleaned; installed pair untouched.
- Ownership/preflight failure: no live mutation.
- Transaction failure with successful rollback: old pair restored and reported as failure, not success.
- RecoveryRequired: preserve/report Eggup recovery evidence; do not delete evidence that an operator needs.
- Concurrent update attempts: obey Eggup mutation-lock semantics; no second bespoke lock.
- Process interruption/crash guarantees must not be overstated beyond Eggup's documented transaction/recovery contract.
- No fallback to old shell extraction or bespoke pair replacement after the Eggup path has begun.

## 9. Compatibility and migration

The user-visible command remains `eggress update`.

Preserve:

- current version ordering;
- latest stable GitHub authority;
- asset naming;
- checksum sidecar interpretation;
- no Cargo fallback;
- no automatic elevation;
- current exit-code categories;
- sibling pair requirement.

No persistent installation metadata migration should be required.

The source dependency migration is gated on published Eggup packages. A temporary immutable revision may be used only on a qualification branch/fixture and must not be the final publishable Cargo dependency.

No pproxy compatibility tier or manifest entry changes merely because updater internals change.

## 10. Required tests

### Focused unit tests

- target -> archive member name mapping;
- finite archive limits;
- Eggup terminal result -> Eggress error/exit mapping;
- ownership classification for exact sibling pair.

### Integration tests

- existing offline successful update rewritten through Eggup;
- checksum mismatch leaves install untouched;
- tar.gz extraction success;
- zip extraction success;
- candidate pair exact-version success.

### Restart and recovery tests

- commit failure after one member with rollback;
- rollback failure retains RecoveryRequired evidence;
- cleanup ordering after bound handle consumption.

### Contention and cancellation tests

- second updater blocked/fails under Eggup lock semantics;
- no stale local second lock path.

### Security and negative tests

- missing member;
- extra/unexpected member according to Eggress policy;
- traversal/absolute path;
- symlink/link/special file;
- member size/digest mismatch;
- member-entry replacement after core plan construction;
- wrong candidate versions;
- unwritable/foreign destinations.

### Migration and compatibility tests

- current CLI exit-code tests;
- existing release-contract tests;
- installer tests unchanged;
- no pproxy parity manifest change;
- package dry-run after dependency cutover.

## 11. Required verification commands

Use the repo's canonical verification guidance. At minimum:

~~~bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p eggress-cli --locked
cargo test -p eggress-cli --test cli_exit_codes --locked
cargo test -p eggress-cli --test release_contract --locked
cargo test --workspace --locked
cargo +1.89.0 check --workspace --all-targets --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
scripts/release-preflight.sh --check-versions-only
cargo package -p eggress-cli --locked
git diff --check
~~~

Run the repository's additional feature-slice checks required by `docs/CI_STATUS.md` / `docs/TESTING.md`.

Do NOT use workspace `--all-features`.

Hosted closure evidence must include Linux, macOS, and Windows lanes sufficient to execute the updater/archive path, not compile-only Windows evidence.

## 12. Documentation updates

Update as applicable:

- updater module rustdoc;
- `docs/INSTALLATION.md`;
- release/update documentation;
- `plans/subsystems/delivery-roadmap.md`;
- `plans/registry.md`;
- `docs/ROADMAP.md`;
- closure `plans/closure/delivery/003-status.md`.

Do not alter pproxy compatibility claims unless an independent behavior change requires the normal manifest/oracle process.

## 13. Acceptance criteria

M003 closes only when:

- `eggress update` no longer shells out to tar/PowerShell for generic archive extraction;
- Eggress no longer owns a second generic two-member backup/rollback transaction;
- archive members are staged through Eggup object-bound handles;
- both staged binaries still pass exact release-version checks before mutation;
- success cannot leave a mixed pair;
- fault-injected commit/rollback semantics are truthful;
- existing release/origin/checksum/CLI policy is preserved;
- default publishable Eggress Cargo graph uses registry Eggup dependencies;
- `cargo package -p eggress-cli` succeeds;
- Rust 1.89 and hosted Linux/macOS/Windows evidence are green;
- dependency/binary-size delta is recorded and accepted;
- no pproxy compatibility claim changed accidentally.

## 14. Stop conditions

Stop and report rather than improvise if:

- compatible versioned Eggup core/archive packages are unavailable for final dependency cutover;
- whole-archive integrity cannot be proven before extraction or finite extraction limits cannot be expressed;
- a product-specific change to Eggup core/archive would be required;
- Windows currently-running-image semantics regress under Eggup commit;
- package dry-run becomes impossible due to dependency source type;
- binary/dependency growth is material and no accepted budget decision exists;
- implementation would change release naming, bootstrap installers, or pproxy compatibility claims;
- another subsystem must change to complete this safely.

## 15. Closure evidence required

The closure record must include:

- exact Eggup registry versions and upstream M008/package evidence;
- before/after Cargo dependency tree;
- before/after default release binary sizes;
- exact updater modules/helpers removed;
- current policy retained vs generic mechanism delegated matrix;
- archive integrity-continuity rationale, including why current optional member expectations are `None` and how extraction output evidence remains bound to the verified archive snapshot;
- fault-injection/rollback/RecoveryRequired results;
- offline fixture results;
- package dry-run;
- Rust 1.89 result;
- hosted Linux/macOS/Windows run IDs;
- confirmation that pproxy capability manifest/matrix did not change, or normal evidence if it did;
- remaining limitations by severity.

## 16. Handoff notes

The principal implementation hazard is accidentally replacing Eggress policy along with its generic mechanism. Keep GitHub authority, target mapping, checksum sidecars, exact candidate-version execution, CLI messaging, and no-fallback policy local.

The second hazard is source authority: build the Eggup `InstallPlan` while archive advisory paths are still valid, then stage only from the moved open objects. After handoff, paths are diagnostics only.

The third hazard is packaging: the final dependency must be versioned/registry-resolvable. Do not solve a package gate with a permanent git dependency.
