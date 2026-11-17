# Delivery M004 — Updater Trust, Advisory Gate, and Credential-Output Hardening

Status: implemented (2026-11-16; the four `zip 6.0.0` advisories remain tracked as a narrow, commented `deny.toml` exception pending an upstream `eggup-archive` release with `zip >= 9.0.0`)

Repository baseline: `feat/auto-20261116-652bb64d` at `ba64e16` (workspace 1.0.12)

Source roadmaps:

- `plans/subsystems/delivery-roadmap.md` (new milestone M004)
- `plans/subsystems/security-hardening-roadmap.md` (S5)

Long-term requirements:

- `plans/000-long-term-specification.md` §2 (goal 7), §4 (invariants 3, 10)
- `plans/001-terminology-and-domain-model.md` §2 (deployment forms)
- `plans/002-long-term-roadmap.md` (release/distribution maintenance)

Applicable ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

Primary class: invariant / infrastructure

## 1. Objective

Harden the standalone updater and delivery/verification surfaces:

1. Remove production access to the updater’s `file://`/`EGRESS_UPDATE_BASE_URL`
   test hook.
2. Stop trusting `PATH` for integrity-critical tools (`curl`, `sha256sum`,
   `shasum`) and use a private, non-predictable staging directory.
3. Redact `egress system-proxy inspect --json` raw settings.
4. Add an advisory gate to CI and release preflight, with a narrow tracked
   exception for the upstream-blocked `zip 6.0.0` dependency.

This milestone does not change release discovery, target naming, checksum
policy, staged-version verification, or the no-sudo/no-elevation policy.

## 2. Why this milestone is ready

- `tempfile 3.27` is already in the workspace dependency graph; promoting it to
  a regular `eggress-cli` dependency is small and MSRV-compatible.
- `sha2 0.10` is already used by protocol crates; adding it to `eggress-cli`
  removes the `sha256sum`/`shasum` PATH dependency.
- `cargo update --dry-run` resolves the default TLS/QUIC/tokio advisories
  (Transports M004); `russh` is handled by Transports M005.
- `eggup-archive 0.1.2` and `0.2.0` both pin `zip = "6.0.0"`; RUSTSEC-2026-0367,
  -0368, -0369, and -0429 cannot be removed by a lockfile update alone. Eggup’s
  bounded extractor rejects symlinks, non-files, `enclosed_name() == None`, and
  `..`/absolute paths, and does not call `ZipArchive::extract`, so the interim
  risk is lower — but it must be tracked, not hidden.
- The existing updater already verifies the whole archive and staged versions;
  this milestone hardens the trust path without changing that contract.

## 3. Current implementation evidence

### Updater trust

- `EGRESS_UPDATE_BASE_URL` is read unconditionally in release code
  (`crates/eggress-cli/src/update/mod.rs:101-103`).
- `file://` URLs are accepted by `curl_fetch_text`/`curl_download`
  (`crates/eggress-cli/src/update/download.rs:63-84`); only non-HTTPS,
  non-file URLs are rejected.
- `find_curl` scans `PATH` (`download.rs:41-53`).
- `compute_sha256` runs `sha256sum` or `shasum` from `PATH`
  (`crates/eggress-cli/src/update/verify.rs:31-55`).
- `staging_dir` is `/tmp/eggress-update-<pid>` with
  `remove_dir_all` + `create_dir_all` (`update/mod.rs:268-273`); `curl -o`
  writes into it and follows symlinks.
- The archive is verified at `update/mod.rs:175-176` and extracted afterward at
  `:178-190`; a writable staging directory creates a swap window.

### Credential-output

- `InspectionResult` serializes both `settings` (raw) and `redacted_settings`
  (`crates/eggress-system-proxy/src/inspection.rs:51-60`).
- `egress system-proxy inspect --json` serializes the whole result
  (`crates/eggress-cli/src/commands/system_proxy.rs:10-13`).
- `settings` values can contain `http://user:pass@...` from OS/env settings.

### Advisory gate

- `cargo audit --ignore RUSTSEC-2023-0071` reports 21 vulnerabilities and 5
  warnings.
- `cargo deny check` reports `advisories FAILED`, while `bans`, `licenses`, and
  `sources` pass; `deny.toml` has no ignores.
- `.github/workflows/ci.yml` and `scripts/release-preflight.sh` do not run
  either tool.

## 4. Invariants that must not regress

- Release discovery remains latest-stable-only; no prerelease/floating branch.
- The SHA-256 sidecar remains the integrity gate; a mismatch leaves the current
  installation untouched.
- Both staged executables are executed and must match the release tag exactly
  and agree with one another before replacement.
- The updater never invokes `sudo`, never mutates shell/PATH state, and never
  falls back to Cargo/source builds.
- A failed update leaves the installed pair untouched.
- The `egress`/`pproxy` pair is replaced as one unit; no split update.
- No new runtime HTTP client dependency; no shell evaluation of downloaded data.
- No release/tag is created by this milestone.

## 5. Scope

### In scope

- `eggress-cli` updater feature gate, tool discovery, hash implementation,
  staging, and tests.
- `eggress-system-proxy`/`eggress-cli` JSON redaction.
- `deny.toml`, CI workflow, release preflight, and advisory documentation.
- Moving the direct `zip` dependency to `[dev-dependencies]` if it is used only
  by tests (verify before changing).
- Tracking the upstream `eggup-archive`/`zip` blocker in the registry.

### Explicitly out of scope

- Implementing the upstream Eggup `zip >= 9` release.
- Changing release discovery, asset naming, version validation, or exit codes.
- Adding a signature/provenance system beyond the existing checksum.
- Admin TLS or protocol/runtime changes.
- Publishing a release.

## 6. Required production changes

### A. Gate the updater test hook

Add a non-default `update-fixtures` feature to `eggress-cli` (or an equivalent
`#[cfg(any(test, feature = "update-fixtures"))]` gate) and compile
`EGRESS_UPDATE_BASE_URL` plus `file://` acceptance only under it.

Release default behavior:

- ignore `EGRESS_UPDATE_BASE_URL`;
- reject `file://` (and every non-HTTPS scheme);
- keep the existing HTTPS `--proto =https --tlsv1.2` constraints.

Update in-module updater tests to use the feature/cfg; add a negative test that
the default release behavior rejects a `file://` base URL and ignores the env
override.

### B. Remove PATH trust from integrity-critical operations

1. **Hashing:** add `sha2 = "0.10"` and compute SHA-256 in-process. Delete the
   `sha256sum`/`shasum` discovery and parsing path; if an OS tool is retained for
   diagnostics, it must not be the verification authority.
2. **curl:** prefer fixed absolute locations:
   - Unix: `/usr/bin/curl`, then `/bin/curl`;
   - Windows: `%SystemRoot%\System32\curl.exe`;
   - do not scan arbitrary `PATH` entries. If no trusted location exists, fail
     with an actionable error. A test-only override may be compiled under the
     `update-fixtures` feature, never in release builds.
3. Verify the selected executable is a regular file and not writable by
   group/other where the platform exposes that metadata. Do not follow a
   symlinked tool path.

### C. Replace predictable staging

Use `tempfile::Builder` (or equivalent):

- a private directory with `0700` permissions, created atomically;
- archive/checksum files created with `create_new`, not overwrite;
- no symlink following for output paths;
- ensure the directory is on the same filesystem/installation path where the
  transaction requires atomic replacement;
- hold the archive open or re-hash it immediately before extraction to close the
  verify/extract swap window;
- keep cleanup best-effort and never remove an unrelated path.

Add tests for: pre-existing symlink at the archive path, private directory mode,
concurrent staging attempts, and hash mismatch after a post-verification
replace (should fail closed).

### D. Redact `system-proxy inspect --json`

Make `InspectionResult` serialize redacted settings only:

- mark raw `settings` `#[serde(skip_serializing)]` (or implement a custom
  `Serialize`);
- ensure `redacted_settings` is always populated when `settings` is present;
- keep `settings` available for in-process callers that need raw values;
- add CLI tests asserting credentials never appear in `--json`.

This is a public JSON behavior change; document it in the command help and
release notes.

### E. Add the advisory gate

1. Add a CI job (or step) that installs a pinned `cargo-deny` and runs
   `cargo deny check` (advisories/bans/licenses/sources). Prefer a pinned action
   SHA or `cargo install --locked` consistent with the repository’s supply-chain
   posture.
2. Add the same check to `scripts/release-preflight.sh` or the release workflow
   preflight job so a tag cannot ship with an unreviewed advisory.
3. Add a narrow `deny.toml` ignore list only for the currently upstream-blocked
   `zip` IDs, with comments naming the Eggup dependency, the reason the
   vulnerable `extract()` APIs are not used, and the removal condition. Do not
   ignore TLS/SSH/QUIC advisories that this program fixes.
4. Record the `zip` blocker in `plans/registry.md` blocked work.

If the gate lands before Transports M004/M005 close, use temporary ignores for
those IDs only with a dated removal condition; prefer landing after S1/S2.

### F. `zip` direct dependency hygiene

Verify `crates/eggress-cli/Cargo.toml:92` is used only by
`#[cfg(test)]` code (`update/eggup.rs` test helpers). If so, move `zip` to
`[dev-dependencies]` and test with `zip 9.x` where the test API permits. This
does not remove the transitive `zip 6.0.0` from `eggup-archive`; it only keeps
the production CLI graph honest.

## 7. Ordered work packages

### Work package A — Updater source and tool trust

Intent: release builds cannot be redirected or tricked by PATH.

Acceptance evidence: env/file rejection tests; absolute tool resolution; no
`sha256sum` authority; docs.

### Work package B — Private staging and TOCTOU

Intent: no local user can swap the archive or symlink the output.

Acceptance evidence: private mode, create-new, symlink, and re-hash tests.

### Work package C — JSON redaction

Intent: `system-proxy inspect --json` never prints credentials.

Acceptance evidence: CLI/unit tests for env/system settings with userinfo.

### Work package D — Advisory gate and zip exception

Intent: advisories are visible in CI/release, with a tracked exception.

Acceptance evidence: gate run output; `deny.toml` comments; registry blocker;
no unreviewed ignores.

## 8. Failure, cancellation, restart, and contention semantics

- A missing trusted curl/hash tool fails before download/mutation.
- A checksum mismatch, extraction failure, or candidate-version failure leaves
  the installed pair untouched.
- Concurrent updater invocations must not share a staging path; create-new and
  private directory semantics make the loser fail cleanly.
- A cancellation/kill during download leaves only a private temp directory; it
  must not remove or mutate the installed pair.
- The advisory gate is read-only and does not modify the lockfile.

## 9. Compatibility and migration

- Release builds no longer honor `EGRESS_UPDATE_BASE_URL`/`file://`; fixture
  tests and local workflows must enable `update-fixtures` explicitly.
- The updater’s user-visible flow (discover, download, verify, stage, execute,
  replace) is unchanged.
- `system-proxy inspect --json` output changes: raw `settings` is omitted/redacted;
  `redacted_settings` remains. This is a security fix and must be documented.
- No pproxy compatibility claim change.
- No version bump/tag.

## 10. Required tests

### Focused unit tests

- `update_fixtures_are_required_for_file_urls` / release rejection.
- `find_curl` prefers absolute paths; rejects a PATH-only fake.
- in-process SHA-256 matches known vectors and the sidecar.
- staging directory is private, create-new, and refuses a pre-existing symlink.
- post-verification archive replacement fails closed.
- `InspectionResult` JSON omits raw credentials.

### Integration tests

- Existing updater fixture flow with `update-fixtures`.
- Existing `eggress`/`pproxy` updater tests.
- CLI `system-proxy inspect --json` with credential-bearing env/system proxy.

### Security and negative tests

- `EGRESS_UPDATE_BASE_URL=file:///tmp/evil` is ignored in release/default.
- A symlinked archive/checksum path is rejected or safely replaced.
- A fake `sha256sum` on PATH cannot influence verification.
- JSON contains no `user:pass@`.

## 11. Required verification commands

```bash
cargo test -p eggress-cli update
cargo test -p eggress-cli --features update-fixtures update
cargo test -p eggress-system-proxy
cargo test -p eggress-cli --test cli_exit_codes
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo audit --ignore RUSTSEC-2023-0071
cargo deny check
```

`cargo deny check` may fail on `russh` until Transports M005 and on `zip` until
the tracked exception/upstream release; record the exact IDs and do not claim a
clean gate prematurely.

## 12. Documentation updates

- `docs/INSTALLATION.md` — updater test hook, trusted tool requirements, staging,
  no-sudo behavior.
- `docs/cli/EXIT_CODES.md` — only if verification failure wording changes.
- `docs/security/SECURE_CONFIGURATION.md` / `docs/SECURITY_REVIEW.md` —
  updater trust boundary and advisory gate.
- `deny.toml` comments and registry blocked-work entry for `zip`.
- CI/release process docs (`docs/CI_STATUS.md`, `docs/release/RELEASE_PROCESS.md`)
  after the gate lands.

## 13. Acceptance criteria

- Default/release builds ignore `EGRESS_UPDATE_BASE_URL` and reject `file://`.
- No integrity decision depends on a PATH-resolved `sha256sum`/`shasum`.
- `curl` resolves from a fixed trusted location or fails closed.
- Staging is private, create-new, and symlink-safe; post-verification replacement
  fails closed.
- `system-proxy inspect --json` contains no credentials.
- CI/release preflight runs an advisory gate; any `zip` exception is explicit,
  commented, and tracked in the registry.
- Existing updater contract and release policy are unchanged.

## 14. Stop conditions

Stop and report if:

- absolute curl locations are unavailable on a supported target;
- moving `zip` to dev-dependencies changes a public crate feature/dependency;
- the JSON redaction breaks a documented consumer contract that requires raw
  settings (add an explicit unsafe accessor instead of silently leaking);
- the advisory gate cannot run without network/action-install permission;
- the `zip` exception is not approved by the maintainer;
- the change would require a release/tag.

## 15. Closure evidence required

- Updater negative tests for env/file/PATH/symlink/TOCTOU.
- In-process SHA-256 vector test and updater fixture transcript.
- `system-proxy --json` redaction test.
- CI/release gate run output and `deny.toml` diff.
- Registry blocked-work entry for `zip` with the upstream dependency and
  removal condition.
- Explicit statement that no release/tag was created.

## 16. Handoff notes

- Do not add a new HTTP client dependency; use trusted curl or an existing
  workspace mechanism.
- Keep the update transaction’s existing Eggup staging/rollback authority; this
  plan hardens inputs, not the install transaction.
- The test hook is used by in-module tests; prefer `cfg(test)` plus a
  non-default feature so external fixture tests remain possible.
- Use `OpenOptions::create_new` rather than truncate; do not follow symlinks.
