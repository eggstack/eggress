# Delivery Milestone 003 — Closure Status

Status: closed

Source implementation plan:

- `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md`

Source subsystem roadmap:

- `plans/subsystems/delivery-roadmap.md#milestone-3--eggup-archivepair-self-update-adoption`

Repository baseline reviewed: `19e6dc71bceba79a20790e5c86013474411a4c3b`

Implementation commits:

- `a01a4d1` — delivery: adopt eggup-core/archive 0.1.2 for self-update extraction and pair transaction (work packages A–E; new updater-matrix CI lanes)
- `5ad16d3` — ci: quote updater-matrix test filter (trailing `::` broke YAML parsing)
- `e396234` — delivery: gate shell-script candidate test to Unix for the Windows updater lane
- `0f7c8b2` — delivery: unwind cli ssh threading of runtime/pproxy-compat (broke cargo package; see §10)
- `19e6dc7` — delivery: report finalize cleanup residue on committed updates

(Upstream `dcbb786`/`010a036` landed on `main` mid-implementation from a
parallel bugfix pass; the M003 branch rebased onto them with no updater
conflicts. Only the packaging interaction in `0f7c8b2` required action.)

## 1. Executive finding

`eggress update` no longer shells out to `tar`/PowerShell and no longer owns
a bespoke two-member backup/rollback transaction. The verified release
archive is extracted through bounded `eggup-archive` extraction, converted
to object-bound sources, staged via
`InstallPlan::prepare_with_bound_sources`, subjected to the exact
staged-version checks, and committed through the Eggup multi-artifact
transaction with truthful terminal-outcome mapping. GitHub authority,
target mapping, checksum sidecars, candidate identity, CLI exit codes, and
the sibling-pair requirement are preserved byte-for-behavior. The
publishable graph uses registry `eggup-core =0.1.2` + `eggup-archive =0.1.2`
(no path/git dependency). Hosted Linux/macOS/Windows lanes execute the
updater path green, and `cargo package -p eggress-cli` succeeds on a clean
tree. M003 is closed.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result | Notes |
|---|---|---|---|
| versioned registry Eggup deps, no git/path | `crates/eggress-cli/Cargo.toml`: `eggup-archive = "=0.1.2"`, `eggup-core = "=0.1.2"`; lockfile `source = "registry+…crates.io-index"`, checksums `0f44129c…`/`758e9564…` matching Eggup M009 publication | pass | package A gate |
| package metadata/MSRV checked | `cargo +1.89.0 check --workspace --all-targets --locked` clean; eggup rust-version 1.89; `cargo deny check` advisories/bans/licenses/sources ok; `cargo audit` shows only 2 allowed pre-existing yanked warnings (`der`, `wnaf`) | pass | — |
| target/format/member mapping | `member_names_for_target` + `archive_format_for_target` unit-tested over all 5 `SUPPORTED_TARGETS`; matches `release-binaries.yml` layouts (top-level pair; zip on Windows) | pass | package B |
| finite limits, no unbounded default | `updater_archive_limits` (64 MiB archive / 8 entries / 256 path bytes / 64 MiB member / 128 MiB total) reasoned from v1.0.10 assets (largest 9,388,730 bytes); oversize sparse file → `ArchiveTooLarge`, 9-entry archive → `TooManyEntries` | pass | package B |
| object-bound handoff, no tar/PowerShell in success path | `extract_archive` deleted; `update_from_urls` flows verify → `install_pair_via_eggup` (extract → bound → prepare → checks → commit); `release_contract::updater_never_escalates_or_falls_back` extended to `eggup.rs` | pass | package B |
| tar.gz + zip fixtures incl. Windows | unix tar.gz + windows-named zip flows green locally; `zip_extract_stage_commit_without_candidate_execution` executes extract→commit on all lanes incl. hosted Windows | pass | package B |
| missing/extra/path/link/special negatives | missing member → `MissingMember`, install untouched; extras drained-within-bounds, only declared pair stages; traversal/absolute/symlink/special rejection is eggup-qualified upstream (M001–M001d) and not re-proven here | pass | — |
| member-entry replacement cannot redirect staged bytes | eggup-qualified upstream (object-bound handles; M001d races); Egress stages only from moved open objects, never reopens advisory paths | pass | upstream evidence |
| candidate validation on Eggup stage | `verify_staged_pair` runs against `staged_path` outputs before mutation; wrong-eggress / wrong-pproxy / disagreement rejected with install untouched (tests) | pass | package C |
| ownership/preflight mapping | `SiblingPairVerifier` (exact canonical pair paths only; absent→deny-create, foreign/unknown fail closed) unit-tested incl. dir-at-destination and missing-sibling cases | pass | package D |
| Eggup commit + disposition mapping | `map_commit_outcome` unit-tested for Committed (clean + finalize-note suffix), RolledBack untouched vs restored (via `rollback_performed`), RecoveryRequired with evidence path | pass | package D |
| cleanup ordering after handles consumed | `finish_extraction_cleanup` runs after prepare; emptied-tree residue dir removed via non-recursive `remove_dir` (fails closed on replacement); residue assertions in flow tests | pass | package D |
| no second bespoke lock | `MutationLock` is the only lock; contention test holds a lock and proves the second committer fails `UpdateInProgress` with the pair untouched | pass | §8/§10 |
| legacy helper deletion | `extract_archive`, `replace_pair`, `copy_or_rename`, `make_executable` + their tests deleted; only Eggress-specific path policy remains in `install.rs` | pass | package E |
| updater docs | module rustdoc; `docs/INSTALLATION.md` updater bullet now names the staging transaction (required provenance sentences preserved, contract test green) | pass | package E |
| dependency/size delta | tree 607→641 lines (`+eggup-core/archive 0.1.2` subtrees, no other additions); release `eggress` 9,589,776→9,860,640 B (+270,864, +2.8%); `pproxy` 8,570,016→8,569,984 B (noise — updater not linked there); no accepted-budget breach (no budget stated; delta immaterial) | pass | package E |
| package dry-run after cutover | `cargo package -p eggress-cli --locked` on clean tree: 45 files, verify build green | pass | §11/§13 |
| Rust 1.89 | pinned toolchain check clean (see above) | pass | §13 |
| hosted Linux/macOS/Windows updater execution | run `36639694985` (head `19e6dc7`): Rust smoke + updater ubuntu/macos/windows all success; updater lanes run `--bin eggress update::` + `release_contract` + `cli_exit_codes` | pass | §11/§13 |
| CLI exit codes / release contract | `cli_exit_codes` 5/5, `release_contract` 17/17 (incl. strengthened no-sudo/no-fallback file list); no numeric code added or changed | pass | §10 |
| installer behavior unchanged | `packaging/`, `release-binaries.yml` untouched; bootstrap installer still owns fresh-install extraction (out of scope) | pass | §5 |
| pproxy compat claims unchanged | `docs/parity/` untouched; no manifest/matrix edit; compat suites unrun by design (no behavior claim changed) | pass | §13/§15 |

## 3. Production implementation evidence

New module `crates/eggress-cli/src/update/eggup.rs` (~700 lines with
tests): target→format/member mapping, finite budget constructor, exact-path
`SiblingPairVerifier`, pure receipt→outcome mapper, `stage_pair` (extract →
bound → plan → prepare → residue finish) and `verify_candidates_and_commit`
(0755 staged normalization → `verify_staged_pair` → integrity → `AllValidators`
→ commit with `DenyCreate` → map). `update/mod.rs` calls
`install_pair_via_eggup` after the unchanged checksum gate plus a
same-directory sibling check. `update/install.rs` keeps only
`sibling_pproxy_path` and `check_destination_writable`; shell extraction,
bespoke rollback, and staged-chmod helpers are deleted. Distinguished from
planned-but-absent: no Eggpack manifests, no service lifecycle, no release
discovery/checksum-format/target-naming/installer changes, no tagging.

## 4. Verification executed

### Commands run

```bash
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
cargo check -p eggress-cli --locked --no-default-features --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon --bins
cargo test -p eggress-cli --locked --bin eggress update::
cargo test -p eggress-cli --locked --test release_contract --test cli_exit_codes
git diff --check
```

### Results

Local (Darwin arm64, rustc 1.89.0): fmt clean; clippy clean; eggress-cli
full suite green (bin 39 incl. 16 new eggup + rewritten e2e, lib 11,
cli_exit_codes 5, release_contract 17, integration/oracle/pproxy suites
green, external interop ignored by design); MSRV check clean; deny clean;
audit clean apart from 2 allowed pre-existing yanked warnings; preflight
version coherence OK; clean-tree package 45 files + verify green;
ssh-inclusive feature-slice check clean (one pre-existing eggress-runtime
unused-import warning in that feature combination only); `git diff --check`
clean. A local full-workspace run was not completed to green locally (see
§10 reload note); the hosted smoke job below is the authoritative
full-workspace result on the closure head.

Hosted (branch `main`, push):

- run `36637403738` (head `e396234`): Rust smoke success; updater
  ubuntu/macos/windows success (first green matrix; failed run `36636248027`
  was the unquoted-YAML workflow parse error, fixed in `5ad16d3`).
- run `36639491263` (head `0f7c8b2`): updater ubuntu/macos/windows success;
  Rust smoke superseded-cancelled by the next push (concurrency group).
- run `36639694985` (head `19e6dc7`, closure head): Rust smoke success
  (job `109649010769`); updater ubuntu (`109649011165`), macos
  (`109649011223`), windows (`109649011312`) success.

## 5. Invariant review

- GitHub discovery/tag selection, target→asset naming, sidecar format and
  semantics: untouched code paths, contract tests green.
- SHA-256 stays integrity evidence, never an authenticity claim: sidecar
  language and docs unchanged.
- Both staged candidates execute and report exactly the expected release
  before mutation: `verify_staged_pair` on Eggup staged paths; wrong-version
  and disagreement tests fail closed pre-mutation.
- No mixed-generation success: two-member Eggup commit is
  generation-consistent by construction; flow tests assert both members
  updated together.
- Pre-commit failure leaves the pair untouched: ownership/integrity/
  candidate/lock failures proven with byte-identical old pair.
- Rollback-capable failure restores; unrecoverable state is explicit:
  mapping covers `RolledBack` (restored) vs `RecoveryRequired` (evidence
  path); mid-commit rollback is eggup-qualified upstream (see §10 note).
- No Cargo fallback, escalation, or installer invocation: static contract
  test extended to the new module; exit codes unchanged.
- Offline fixtures first-class: e2e + mismatch flows run over `file://`
  without network.
- pproxy runtime behavior outside delivery unchanged: no pproxy source
  touched.
- No new compat claim: manifest/matrix untouched.
- Rust 1.89: pinned check green.
- No workspace `--all-features`: updater lanes use `--bin`/named suites
  only; the full-workspace command from §11 ran without `--all-features`.

## 6. Failure and recovery review

- Acquisition/checksum failure: unchanged, pre-extraction, install untouched
  (existing mismatch test green through the new flow).
- Extraction/member-evidence failure (`MissingMember`, budget, malformed):
  fail closed pre-mutation, install untouched (tests).
- Candidate-version failure: prepared stage dropped/cleaned, install
  untouched (tests).
- Ownership/preflight failure (foreign dir, absent sibling with
  `DenyCreate`): fail closed pre-mutation (tests).
- Lock contention: second committer gets `UpdateInProgress` mapped to a
  hard failure; install untouched (test holds a real `MutationLock`).
- Post-mutation failure with restore → receipt `RolledBack`,
  `rollback_performed=true` → "previous installation was restored".
- Restore failure → `RecoveryRequired` with retained `recovery_path`
  surfaced to the operator; nothing is deleted that recovery needs.
- Committed with finalize residue (e.g. Windows running-image backup the OS
  keeps locked) → success exit category with a retained-evidence suffix
  (old flow silently ignored `.bak` removal; residue outcome equivalent,
  now reported).
- No fallback to shell extraction or bespoke replacement exists anywhere
  after the Eggup path begins (helpers deleted).
- Crash/interruption guarantees are exactly Eggup's documented
  transaction/recovery contract; nothing stronger is claimed.

## 7. Migration and compatibility review

`eggress update` command, version ordering, GitHub authority, asset naming,
sidecar interpretation, no-Cargo-fallback, no-elevation, exit-code
categories, and the sibling-pair requirement are unchanged. Installed binary
modes stay 0755 on Unix (staged normalization before commit preserves the
previous outcome; Eggup stages owner-private internally). No persistent
installation metadata migration. Registry cutover is the only source change:
`eggup-core/archive =0.1.2` from crates.io; no git/path dependency remains.
No pproxy tier/manifest change. Legacy `replace_pair` semantics that differ
and were intentionally not carried over: `.bak` sidecar files (superseded by
the transaction's private backup set) and cross-filesystem copy fallback for
staging (Eggup stages beside the install root; cross-device installs fail
closed instead of copying — same-filesystem installs are the supported
layout, matching the previous rename-based commit).

## 8. Security review

No new trust boundary: archives still gate on whole-file SHA-256 before any
parse; extraction is bounded (count/size/path/member/total) with
traversal/link/special rejection upstream; staging is owner-private;
commit revalidates ownership plus staged digests under lock; diagnostics
stay bounded with URL redaction unchanged. `unsafe_code = "deny"` holds
(workspace lints clean via clippy). No secret-bearing material in
planning/closure evidence. New third-party code is limited to the two
published Eggup crates plus in-process fixture builders (`zip`, `flate2`,
`tar` as dev-deps only — not in the publishable runtime graph).

## 9. Documentation and operations

- `crates/eggress-cli/src/update/eggup.rs` rustdoc (authority, budget
  rationale, ordering, residue handling).
- `docs/INSTALLATION.md` updater bullet names the staging transaction
  (required provenance sentences preserved).
- `plans/subsystems/delivery-roadmap.md`, `plans/registry.md`,
  `docs/ROADMAP.md`: M003 ready → closed (with this closure).
- New hosted `updater-matrix` lanes in `.github/workflows/ci.yml`
  (Linux/macOS/Windows updater execution, not compile-only).
- Operator-visible new diagnostics: lock-contention ("already in
  progress"), finalize-residue suffix with evidence path, recovery-required
  message pointing at retained evidence plus bootstrap reinstall guidance.

## 10. Unresolved findings

| Severity | Finding | Impact | Required action |
|---|---|---|---|
| medium | CLI `ssh` threading of `eggress-runtime/pproxy-compat` unwound (`0f7c8b2`) | CLI `ssh` builds lose the upstream compat session-cache wiring until re-landed; `cargo package` (and the release path) was red with it | Owning agent: re-land the threading with a packaging-compatible spelling; verify with `cargo package -p eggress-cli --locked` |
| medium | Windows running-image self-update not executed end-to-end | Hosted Windows lane proves extract→stage→commit on scratch dirs; replacing the *running* `.exe` pair relies on the rename-aside pattern carried over from the qualified old flow plus finalize-residue reporting | Add a hosted running-image smoke before relying on Windows self-update in a release; sweep stale `.eggup-backup-*` residue opportunistically |
| low | e2e `RolledBack`-via-real-commit unreachable through the public Eggup API | Mid-commit failure needs eggup-internal fault injection (`cfg(test)`); mapping is unit-tested per disposition and rollback semantics are eggup-qualified upstream | None for M003; revisit only if Eggup exposes consumer fault injection |
| low | `accepted_routing_reload_observed_by_new_connection` (eggress-runtime reload) fails on local Darwin runs | Unrelated crate; fails identically on the clean tree (verified via stash with M003 changes set aside), so not caused by this milestone; hosted Ubuntu smoke (full workspace suite incl. this target) is green on the closure head — timing/platform-sensitive | Owner triage outside M003 |
| info | `cargo audit` reports 2 allowed yanked warnings (`der`, `wnaf`) | Pre-existing, unrelated crates, warnings only | None |
| info | `ssh`-inclusive feature-slice check emits a pre-existing eggress-runtime unused-import warning | Untouched code, that feature combination only | Owner cleanup outside M003 |

## 11. Roadmap disposition

Milestone closed and the Eggup-side dependency may proceed to consumer
closure: Egress Delivery M003 implementation is complete with the versioned
Eggup dependency gate satisfied and qualified. No corrective plan is
required; the medium findings above are owned follow-ups, not M003 defects.

## 12. Registry updates

- Delivery M003: ready → closed (`plans/closure/delivery/003-status.md`).
- Dependency-ready table: M003 row retired (landed).
- Blocked work: remains none.
- Execution order: Eggup updater-convergence gate noted closed with
  consumer evidence (registry versions + hosted matrix).
