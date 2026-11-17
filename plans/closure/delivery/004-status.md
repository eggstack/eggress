# Delivery Milestone 004 — Closure Status

Status: closed

Source implementation plan:

- `plans/implementation/delivery/004-updater-trust-and-advisory-gate.md`

Source subsystem roadmap:

- `plans/subsystems/delivery-roadmap.md#milestone-4--updater-trust-advisory-gate-and-credential-output-hardening`
- `plans/subsystems/security-hardening-roadmap.md#milestone-s5--updater-trust-advisory-gate-and-credential-output-hardening`

Repository baseline reviewed: `ba64e166474ede7caeba736a885322323d965911`

Implementation commits or pull requests:

- none — this record covers an uncommitted working-tree implementation on
  `feat/auto-20261116-652bb64d` (workspace `1.0.12`). No commit, tag, or release
  was created.

Outstanding condition (single): the workspace-wide `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets`, and `cargo test --workspace --locked`
gate is run by the coordinator after all corrective waves land. This milestone
deliberately ran only the scoped verification listed in §4.

## 1. Executive finding

The updater now has one trust path for release builds: fixed-location `curl`,
in-process SHA-256, HTTPS-only URLs, a private per-transaction staging
directory, create-new download outputs, and a second archive hash immediately
before extraction. The `EGRESS_UPDATE_BASE_URL`/`file://` fixture hook is
compiled out of release/default builds and only exists under `cfg(test)` or the
new non-default `update-fixtures` feature. `eggress system-proxy inspect
--json` no longer serializes raw settings or credential-bearing dry-run
commands, and both CI and the release preflight run an advisory gate whose only
ignores are the four tracked `zip 6.0.0` advisories inherited from
`eggup-archive`.

The advisory gate needed one correction beyond the plan's `[graph]
all-features = true`: the root manifest is the `eggress-bench` package (not a
virtual manifest), so `cargo deny check` roots the graph there and silently
omits `eggress-cli` — and therefore `eggup-archive -> zip` — while printing
`advisories ok`. The gate is therefore `cargo deny --workspace check`, and the
misleading pass mode is documented in `deny.toml`, `docs/CI_STATUS.md`, and
`docs/release/RELEASE_PROCESS.md`. With `--workspace`, the all-features graph
is actually scanned: `russh 0.65.2` and `zip 6.0.0` both appear, and the four
ignores match real crate nodes instead of warning `advisory was not
encountered`.

## 2. Requirement-to-evidence matrix

| Requirement | Evidence | Result | Notes |
|---|---|---|---|
| Release/default builds ignore `EGRESS_UPDATE_BASE_URL` | `update_base_url_override()` has a `#[cfg(not(any(test, feature = "update-fixtures")))]` body returning `None` (`crates/eggress-cli/src/update/mod.rs`); `tests/updater_trust.rs::default_build_ignores_update_base_url_and_rejects_file_urls` runs the real binary with a `file://` fixture base URL and a dead HTTPS proxy and asserts exit 7 with a discovery failure | pass | binary under test is built without `cfg(test)` |
| Release/default builds reject `file://` (and every non-HTTPS scheme) | `is_fixture_url()` is `false` in release/default builds; `curl_command()` rejects non-HTTPS URLs and keeps `--proto =https --tlsv1.2` for HTTPS; `download::tests::refuses_non_https_update_urls` | pass | `http://` rejection pre-existing and preserved |
| Fixture hook still usable for tests only | `update-fixtures` non-default feature + `cfg(test)`; `update_fixtures_feature_honors_file_base_url` passes with `--features update-fixtures` and does not exist without it | pass | `EGRESS_UPDATE_CURL` test override also feature-gated |
| No PATH-resolved hash authority | `verify::compute_sha256` hashes in-process with `sha2 0.10`; the `sha256sum`/`shasum` discovery and parsing path is deleted; `update::tests::path_tools_are_not_trusted` plants a fake `sha256sum` on `PATH` and still gets `ba7816bf…15ad` for `abc` | pass | no process spawn remains in the hash path |
| Known-vector SHA-256 evidence | `verify::tests::sha256_matches_known_vectors` covers empty (`e3b0c442…`), `abc` (`ba7816bf…`), and 1,000,000 × `a` (`cdc76e5c…`) | pass | multi-block streaming path included |
| Sidecar remains the gate | `verify::tests::sidecar_comparison_uses_the_in_process_digest` accepts a matching sidecar, rejects tampered bytes, and re-accepts restored bytes | pass | — |
| `curl` resolves from fixed trusted locations only | `trusted_curl_candidates()` = `/usr/bin/curl`, `/bin/curl` (Unix) or `%SystemRoot%\System32\curl.exe` + `C:\Windows\System32\curl.exe` (Windows); no `PATH` read; `find_curl()` fails closed with an actionable message | pass | `trusted_candidates_are_fixed_system_locations` asserts the exact Unix set |
| Untrusted tool metadata refused | `validate_trusted_executable()` rejects symlinks, non-regular files, and (Unix) group/other-writable modes; `rejects_symlinked_or_writable_tools` covers all three | pass | Windows relies on `System32` ACLs; no portable group/other metadata |
| PATH-only fake `curl` not selected | `update::tests::path_tools_are_not_trusted` puts a fake `curl` first in `PATH`; `find_curl()` never returns it | pass | — |
| Private, unpredictable staging | `staging_dir()` uses `tempfile::Builder` with an explicit `0o700` mode (the crate default is umask-derived `0755`); `staging_directory_is_private_and_unique` asserts mode `0700`, distinct paths, and that a second transaction does not disturb the first | pass | default temp root retained |
| Concurrent staging attempts | `concurrent_staging_attempts_do_not_share_a_directory` creates 8 concurrent staging dirs and asserts 8 unique paths | pass | — |
| Create-new, non-symlink output paths | `create_private_output()` uses `OpenOptions::create_new` (`O_CREAT\|O_EXCL`, mode `0600`) and `curl` writes into that handle via stdout; `staging_outputs_are_created_new_and_owner_only` and `pre_existing_symlink_at_archive_path_is_refused` | pass | a symlink at the archive path fails `UnsafeDestination` and the victim file is untouched |
| Post-verification replacement fails closed | second `verify::verify_archive` via `verify_archive_before_extraction()` immediately before `install_pair_via_eggup`; `post_verification_archive_replacement_fails_closed` swaps the archive after the first gate and asserts the "changed after checksum verification" failure with exit 1 | pass | extraction never runs |
| JSON output has no credentials | `InspectionResult.settings` is `#[serde(default, skip_serializing)]`; `InspectionResult::new` always derives `redacted_settings`; dry-run commands are generated from the redacted copy; `inspection_result_json_omits_raw_credentials` and `dry_run_commands_use_redacted_values` (macOS/Windows/Linux command shapes) | pass | raw settings stay available in-process |
| CLI JSON redaction evidence | `tests/system_proxy_inspect.rs` runs the binary with credential-bearing `http_proxy`/`https_proxy`/`all_proxy`, asserts no `settings` key, a present `redacted_settings`, and no `user:pass@`/`admin:secret@` in stdout or stderr | pass | also verified manually (§4) |
| Advisory gate in CI | `.github/workflows/ci.yml` `advisories` job: pinned `cargo install cargo-deny --locked --version 0.19.0` then `cargo deny --workspace check` | pass (workflow change; not executed here) | fourth workflow count unchanged |
| Advisory gate in release preflight | `.github/workflows/release-binaries.yml` preflight job runs the same install + `cargo deny --workspace check` immediately after `scripts/release-preflight.sh --tag …` | pass (workflow change; not executed here) | preflight timeout 5 → 20 min |
| Only tracked `zip` ignores | `deny.toml` `[advisories] ignore` holds exactly RUSTSEC-2026-0367/-0368/-0369/-0429 with dependency path, why `ZipArchive::extract` is unused, and the removal condition (`eggup-archive` on `zip >= 9.0.0`) | pass | no TLS/SSH/QUIC/rustix ignore |
| Optional-feature dependencies actually scanned | `deny.toml` `[graph] all-features = true` + `--workspace`; `cargo deny --workspace check advisories` reports zero `advisory-not-detected` warnings (each ignore matched a real `zip 6.0.0` node) and the graph shows `russh 0.65.2` | pass | bare `cargo deny check` was proven to omit `eggress-cli` |
| No release/tag | no commit, tag, or release performed | pass | — |

## 3. Production implementation evidence

- `crates/eggress-cli/Cargo.toml`: new non-default `update-fixtures = []`
  feature; promoted `tempfile` from `[dev-dependencies]` to `[dependencies]`
  (removed the duplicate dev entry); added `sha2 = "0.10"`. `zip` stays
  dev-only — `grep -rn "zip::" crates/eggress-cli/src` matches only the
  `#[cfg(test)]` archive-fixture helper in `update/eggup.rs`, so no production
  `zip` edge was added or removed.
- `crates/eggress-cli/src/update/download.rs` (rewritten trust path):
  `DownloadError::MissingCurl(String)` and `DownloadError::UnsafeDestination`
  are new; `trusted_curl_candidates()`, `validate_trusted_executable()`,
  `create_private_output()`, shared `curl_command()` with the HTTPS policy, and
  feature-gated `is_fixture_url()`/`fixture_curl_override()`. Downloads now
  redirect `curl` stdout into an already-opened `create_new` file handle
  instead of passing `-o <path>`.
- `crates/eggress-cli/src/update/verify.rs`: `compute_sha256` is in-process
  `sha2` streaming with a 64 KiB buffer; the `std::process::Command` tool
  discovery/parse path is deleted.
- `crates/eggress-cli/src/update/mod.rs`: `update_base_url_override()` is
  cfg-gated; `staging_dir()` returns a `tempfile::TempDir` with an explicit
  `0o700` mode; the transaction drops the guard to clean only its own tree;
  `verify_archive_before_extraction()` re-hashes immediately before Eggup
  extraction; `map_download_error` maps `UnsafeDestination` to the runtime
  failure category. Module documentation lists the new hard guarantees.
- `crates/eggress-system-proxy/src/inspection.rs`: raw `settings` is
  `#[serde(default, skip_serializing)]`, `InspectionResult::new` derives
  `redacted_settings` via a shared `redact_settings()`, and inspection dry-run
  commands are generated from the redacted copy so the serialized/human report
  cannot carry `user:pass@`.
- `crates/eggress-cli/src/cli.rs`: `system-proxy inspect --json` help now states
  that settings are emitted redacted only.
- `deny.toml`: `[graph] all-features = true` (with an explanation that
  `--workspace` is the invocation-level requirement), four commented `zip`
  ignores with owner/removal condition, and
  `Apache-2.0 WITH LLVM-exception` allowed (`target-lexicon`, a build
  dependency of `pyo3-build-config`, became visible only once the gate scanned
  the whole workspace).
- `.github/workflows/ci.yml`: new `advisories` job (pinned `cargo-deny 0.19.0`
  via `cargo install --locked`, then `cargo deny --workspace check`).
- `.github/workflows/release-binaries.yml`: advisory gate added to the
  `preflight` job after the version/tag preflight (timeout 5 → 20 minutes).
- `scripts/release-preflight.sh`: header documents that the script stays
  offline/version-only and that the advisory gate runs immediately after it in
  the release workflow. No release-discovery, asset-naming, checksum-policy,
  staged-version, exit-code, or no-sudo behavior changed.
- `Cargo.lock`: `rustix 1.1.4 -> 1.1.6` (RUSTSEC-2026-0486).

Unplanned but required: `cargo deny --workspace check` exposed a
`licenses` rejection that a root-only graph could not see, and the
`tempfile` default mode was `0755` rather than `0700`. Both are recorded above
and in §10.

## 4. Verification executed

### Commands run

```bash
cargo test -p eggress-cli update
cargo test -p eggress-cli --features update-fixtures update
cargo test -p eggress-cli --features update-fixtures --test updater_trust
cargo test -p eggress-cli --test system_proxy_inspect
cargo test -p eggress-system-proxy
cargo test -p eggress-cli --test cli_exit_codes
cargo clippy -p eggress-cli -p eggress-system-proxy --all-targets -- -D warnings
cargo fmt -p eggress-cli -p eggress-system-proxy
cargo audit --ignore RUSTSEC-2023-0071
cargo deny --workspace check
```

### Results

- `cargo test -p eggress-cli update`: 44 updater unit tests pass; the
  default-build integration test
  `default_build_ignores_update_base_url_and_rejects_file_urls` passes (exit 7,
  `download failed`). No failures. The updater test subset was repeated three
  times (`cargo test -p eggress-cli --bin eggress update::tests`, 8 tests each)
  with no flakes, including the `PATH`-mutation test alongside the `tar`-based
  fixture flows.
- `cargo test -p eggress-cli --features update-fixtures update`: 44 updater
  unit tests pass; `update_fixtures_feature_honors_file_base_url` passes
  (exit 0, `already the latest stable version`). No failures.
- `cargo test -p eggress-system-proxy`: 55 tests pass (includes the two new
  redaction tests and the dry-run-command redaction test).
- `cargo test -p eggress-cli --test system_proxy_inspect`: 1 test passes.
  Manual transcript: `eggress system-proxy inspect --json` with
  `http_proxy=http://user:pass@proxy.invalid:8080` printed top-level keys
  `['apply_supported','capabilities','dry_run_commands','platform','redacted_settings']`,
  `has raw settings key: False`, `contains user:pass@: False`.
- `cargo test -p eggress-cli --test cli_exit_codes`: 5 tests pass.
- `cargo clippy -p eggress-cli -p eggress-system-proxy --all-targets -- -D warnings`:
  clean, no diagnostics.
- `cargo fmt -p eggress-cli -p eggress-system-proxy`: applied; only this
  milestone's files were rewritten (other corrective work in the same crates
  kept its earlier mtimes).
- `cargo audit --ignore RUSTSEC-2023-0071`: `error: 4 vulnerabilities found!`
  — exactly `RUSTSEC-2026-0429`, `-0367`, `-0368`, `-0369`, all `zip 6.0.0`
  (`patched >= 9.0.0`), plus 1 allowed warning (`wnaf 0.14.0` yanked).
  `RUSTSEC-2026-0486` (rustix) is gone after the 1.1.6 update.
- `cargo deny --workspace check`: `advisories ok, bans ok, licenses ok,
  sources ok`; zero `advisory-not-detected` warnings (all four ignores matched
  `zip 6.0.0` nodes); remaining output is warnings only (duplicate versions,
  `wnaf` yanked, one unmatched `Unicode-DFS-2016` allowance).
- Proven negative control: `cargo deny check` (no `--workspace`) reports
  `advisories ok` with four `advisory was not encountered` warnings and a graph
  that contains neither `eggress-cli` nor `zip`; after enabling only
  `[graph] all-features = true` the result was unchanged, which is why
  `--workspace` is part of the gate.

Not run here (deliberate scope): `cargo test --workspace`, `cargo clippy
--workspace`, `cargo fmt --all`, any `--all-features` build/test, and the
hosted workflows themselves.

**Coordinator broad gate (2026-11-16, after all six corrective waves):** pass —
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --locked`, the documented CLI/embed feature slices, the
embed OpenSSH regression (6/6), and `cargo deny --workspace check`
(advisories/bans/licenses/sources). `cargo audit --ignore RUSTSEC-2023-0071`
reports only the four tracked `zip 6.0.0` advisories. The condition recorded in
§11 is therefore satisfied.

## 5. Invariant review

- **Latest-stable-only discovery**: unchanged — `latest_release_api_url()`,
  `parse_github_tag`, `parse_tag` untouched; fixtures only redirect the base.
- **Sidecar as integrity gate**: unchanged in contract, strengthened in
  implementation (in-process hash, second hash before extraction).
- **Both staged executables executed and matched**: unchanged
  (`verify_staged_pair` before commit).
- **Never `sudo`, no shell/PATH mutation, no Cargo/source fallback**: unchanged;
  `release_contract` (unchanged) still asserts the no-escalation/no-fallback
  property over `update/`.
- **Failed update leaves the installed pair untouched**: preserved by ordering
  (tool + URL checks → download → hash → re-hash → extraction → staged
  versions → commit) and covered by the existing fixture-mismatch test plus the
  new TOCTOU test.
- **Pair replaced as one unit**: unchanged (`SiblingPairVerifier`, sibling
  parent check).
- **No new runtime HTTP client**: unchanged; still trusted `curl` only.
- **No shell evaluation of downloaded data**: unchanged.
- **No release/tag**: none created.

## 6. Failure and recovery review

- **Missing/untrusted tool**: `find_curl()` fails before any download; the
  error names the checked absolute locations. `compute_sha256` needs no tool.
- **Non-HTTPS URL**: rejected before output files are created.
- **Unsafe staging path**: `create_new` returns `UnsafeDestination`, mapped to
  `EXIT_RUNTIME_FAILURE` (`Failed`), with the install untouched.
- **Concurrent updates**: independent private directories; the Eggup
  `MutationLock` contention behavior is unchanged.
- **Cancellation/kill mid-download**: only the private temp tree is left; the
  installed pair is never touched. `TempDir` drop removes only its own path.
- **Post-verification replacement**: second hash fails closed with the
  "changed after checksum verification" detail and exit 1.
- **Checksum mismatch**: unchanged message and exit 1 with the old pair intact.
- **Extraction/commit failure**: unchanged Eggup disposition mapping
  (`Committed` / `RolledBack` / `RecoveryRequired`).

## 7. Migration and compatibility review

- Release/default builds no longer honor `EGRESS_UPDATE_BASE_URL` or `file://`.
  Any private fixture workflow must build with `--features update-fixtures`
  (documented in `docs/INSTALLATION.md`).
- The user-visible updater flow (discover → download → verify → stage →
  execute → replace) and messages are unchanged apart from the new
  tool/staging diagnostics.
- `system-proxy inspect --json` is a deliberate public output change: raw
  `settings` is no longer serialized; `redacted_settings` remains and is
  always populated when settings exist. Human output is unchanged. Documented
  in command help, `docs/INSTALLATION.md`, `docs/SECURITY_REVIEW.md`, and
  `docs/security/SECURE_CONFIGURATION.md`. No documented consumer required raw
  settings (the only in-repo consumer is the CLI renderer).
- No pproxy compatibility claim or manifest entry changed.
- No version bump and no release.

## 8. Security review

- Release redirection hooks are compiled out; the env var is not read at all in
  release/default builds (not merely ignored after reading).
- Integrity decisions are in-process; `PATH` cannot supply the hasher or the
  fetcher.
- Tool trust checks cover symlink, regular-file, and (Unix) group/other-writable
  metadata; failures are structured and fail closed.
- Staging is owner-only, unpredictable, atomic, create-new, and re-verified
  before extraction; no output path is re-resolved after the trust decision.
- Credential-bearing proxy settings, including dry-run command lines, cannot be
  serialized by `system-proxy inspect --json`.
- Advisory gate now scans the real workspace graph; the exception list is
  explicit, commented, and has a removal condition. No TLS/SSH/QUIC/rustix
  advisories are ignored.

## 9. Documentation and operations

- `docs/INSTALLATION.md`: trusted-tool, private-staging, HTTPS-only, and
  no-`sudo` updater behavior plus two troubleshooting entries.
- `docs/SECURITY_REVIEW.md`: new `Self-Update and Distribution Trust` surface
  section.
- `docs/security/SECURE_CONFIGURATION.md`: new `8. Self-Update Trust Boundary`
  section (no other section touched).
- `docs/CI_STATUS.md`: fourth-job inventory, `advisories` job, the
  `--workspace` requirement, and the release workflow's advisory step.
- `docs/release/RELEASE_PROCESS.md`: advisory command in step 1 with the
  `--workspace` rationale.
- `deny.toml`: annotated graph/advisory configuration and the license
  allowance.

## 10. Unresolved findings

| Severity | Finding | Impact | Required action |
|---|---|---|---|
| low | Bare `cargo deny check` still prints `advisories ok` while skipping `eggress-cli`/`zip` (root-package graph) | A maintainer running the wrong command can believe the gate passed | Mitigated: CI/preflight/docs all use `cargo deny --workspace check`, and `deny.toml` documents the trap. Consider making `--workspace` the only documented form in `AGENTS.md` at the next docs pass. |
| low | `tempfile`'s default directory mode is umask-derived (`0755`), not `0700` | Would have left staged archives readable by other local users | Fixed: `staging_dir()` passes an explicit `0700` mode; asserted by test |
| low | `Apache-2.0 WITH LLVM-exception` (`target-lexicon`, via `pyo3-build-config`) had to be allow-listed once the gate scanned the whole workspace | Gate could not pass without it | Fixed in `deny.toml`; permissive license, no policy change |
| medium | Four `zip 6.0.0` advisories remain in the updater graph | Upstream `eggup-archive` blocker; Eggup's bounded extractor does not call the vulnerable `extract()` APIs and Eggress verifies the whole-archive SHA-256 first | Tracked as a narrow commented `deny.toml` exception; remove when `eggup-archive` depends on `zip >= 9.0.0` |
| low | Workspace-wide fmt/clippy/test gate not run in this wave | Cross-crate regressions outside the touched crates would be caught later | Coordinator runs it after all corrective waves |

## 11. Roadmap disposition

Milestone closed. The coordinator's workspace-wide format/lint/test gate passed
on 2026-11-16.

Delivery M004 exit criteria are satisfied: default builds reject
`EGRESS_UPDATE_BASE_URL`/`file://`, no PATH hash authority remains, staging is
private/create-new/symlink-safe with a closed verify/extract window, JSON has no
credentials, the advisory gate runs in CI and the release preflight, and the
`zip` blocker is explicit, commented, and tracked with an owner and review date.
Security S5 is satisfied by the same evidence.

## 12. Registry updates

- `plans/registry.md` (not edited in this wave — owned elsewhere): change the
  Delivery M004 plan row status from `ready` to `implemented` and point the
  roadmap/closure links at `plans/closure/delivery/004-status.md`. The
  `zip 6.0.0` blocked-work entry already exists and needs no change beyond
  noting the gate exception is now enforced with `cargo deny --workspace check`.
- `plans/subsystems/delivery-roadmap.md`: M004 header, dependency-graph, and
  milestone/status-table rows updated (done).
- `plans/subsystems/security-hardening-roadmap.md`: S5 requires no text change;
  the milestone now has evidence. Coordinator may flip its status row if the
  roadmap tracks one.
- No commit, tag, release, or registry mutation was performed by this wave.
