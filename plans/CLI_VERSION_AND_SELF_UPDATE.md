# CLI Version and Self-Update

## Status

Implementation plan for handoff. This is Phase 3 of `CLI_CLEANUP_AND_BINARY_DELIVERY_ROADMAP.md` and depends on the release asset contract in `CLI_BINARY_RELEASE_AND_INSTALLER.md`.

## Objective

Add two small native Eggress commands:

```text
eggress version
eggress update
```

`version` must provide a stable, script-friendly package version. `update` must replace a standalone CLI installation using verified artifacts from the exact GitHub Release produced by the binary pipeline.

The standalone `pproxy` compatibility binary keeps its existing pproxy-style `--version` surface and does **not** gain `update` or other Eggress-native subcommands.

## Design constraints

- No background update checks.
- No telemetry.
- No automatic `sudo` or privilege escalation.
- No automatic Cargo fallback in the self-update path.
- No dependency on the Gregg repository at runtime or build time.
- It is acceptable to adapt generic updater mechanics from Gregg into Eggress, but copied code must be reduced to Eggress’s two-binary release contract and maintained locally.
- Update must treat `eggress` and `pproxy` as one release unit.
- A failed update must leave the prior installation usable.
- Checksum and staged executable identity/version must be verified before replacement begins.

## Phase A — Add `eggress version`

Add a top-level native subcommand:

```text
eggress version
```

Output contract:

```text
eggress X.Y.Z
```

Use the package/workspace build version (`env!("CARGO_PKG_VERSION")` or a single equivalent build constant). Do not query the network.

Retain Clap’s normal `eggress --version` behavior unless there is a strong reason to remove it. Prefer making both forms report the same semantic version even if formatting differs slightly because Clap prefixes the package name automatically.

Do not add verbose build metadata, git SHA, feature inventory, or JSON in this phase. Those can be separate features if a real operational need appears.

### Tests

- `eggress version` exits 0 and prints exactly one stable version line.
- reported semver equals `CARGO_PKG_VERSION`.
- `eggress --version` remains functional.
- release workflow can grep/parse the output deterministically.

### Acceptance criteria

- `version` performs no runtime initialization or network access;
- output contains no timestamps or nondeterministic state;
- the implementation has one version source.

## Phase B — Define release discovery semantics

`eggress update` should use **GitHub Releases as the binary update authority**, not crates.io.

Rationale:

- the updater installs GitHub-built binary archives, so the release tag is the most direct authority for the artifact being installed;
- crates.io remains a supported manual Rust/developer channel but is not the preferred standalone CLI distribution;
- PyPI remains the primary Python/pproxy-migration distribution, while release preflight guarantees the tag/workspace/Python versions are aligned.

For the default update:

1. resolve the latest non-draft, non-prerelease GitHub Release for `eggstack/eggress` using the GitHub `releases/latest` endpoint or its stable redirect semantics;
2. parse the `vX.Y.Z` tag;
3. compare to the installed `CARGO_PKG_VERSION` using semver semantics;
4. if current >= latest, report already current and make no changes;
5. if latest is newer, fetch the exact target archive and checksum from that release.

Do not use a floating branch or `main` artifact for updates.

Pre-release update channels are out of scope. A stable installation must never automatically jump to a prerelease.

## Phase C — Target and asset mapping

Use exactly the supported target mapping from `CLI_BINARY_RELEASE_AND_INSTALLER.md`:

```text
x86_64-unknown-linux-gnu
aarch64-unknown-linux-gnu
x86_64-apple-darwin
aarch64-apple-darwin
x86_64-pc-windows-msvc
```

Construct the exact archive/checksum names from one shared helper used by updater tests and, where practical, packaging/release validation.

Conceptually:

```rust
fn release_archive_name(target: &Target) -> &'static/owned str
fn release_checksum_name(target: &Target) -> String
fn release_asset_urls(version: &Version, target: &Target) -> AssetUrls
```

Do not copy target strings independently into multiple command handlers.

Unsupported hosts should fail with an actionable message such as:

```text
no prebuilt Eggress release is available for <target>; update this installation with Cargo/source instead
```

No silent fallback to compiling from source.

## Phase D — Download implementation

Gregg’s updater intentionally shells out to `curl`, with internal code owning target/version/checksum/replacement logic. Eggress can reuse that approach if the dependency/portability review confirms it is preferable to adding a large HTTP client dependency to `eggress-cli`.

Preferred decision rule:

1. If an existing lightweight Eggress HTTP/control-plane dependency can safely perform HTTPS downloads without materially increasing the release binary, reuse it.
2. Otherwise, use a bounded `curl` subprocess exactly as the bootstrap installer does, with explicit discovery, timeout, redirect following, non-success failure, and stderr capture.
3. Do **not** add a heavyweight HTTP client simply for self-update without measuring the binary/dependency cost.

If `curl` is required for `eggress update`, make that explicit in the command error and docs. The one-curl bootstrap path already assumes curl on Linux/macOS; Windows may use platform-native download APIs or invoke PowerShell functionality through the Windows-specific path if that is simpler and well-tested.

The update command should not shell-evaluate downloaded content. It only downloads data files/executables.

### Required network behavior

- finite connection/overall timeout;
- redirect support for GitHub release URLs;
- distinguish not-found/missing asset from generic transport failure;
- no credentials in URLs/logs;
- user-agent identifying Eggress/version is acceptable but not required;
- no retry loop larger than a small bounded retry for transient transport errors.

## Phase E — Stage and verify the release archive

Create a private temporary directory and download:

```text
eggress-<target>.<archive>
eggress-<target>.<archive>.sha256
```

Verification order is mandatory:

1. verify SHA-256 of the complete archive;
2. extract into the temporary directory;
3. locate both staged executables;
4. execute staged `eggress version`;
5. execute staged `pproxy --version`;
6. parse each reported version;
7. require both versions to equal the release tag exactly;
8. require both versions to agree with one another;
9. only then enter replacement logic.

A checksum mismatch, malformed checksum, extraction failure, missing binary, executable failure, or candidate-version mismatch is a hard error. Never fall back to another install source after integrity/identity verification fails.

For the compatibility binary, preserve its existing version string format (currently compatible with a prefix such as `eggress-pproxy-compat X.Y.Z`). The verifier should parse the version deliberately rather than requiring the binary to pretend its public name is `pproxy` if that would change compatibility behavior.

## Phase F — Resolve installation paths safely

The updater must determine the actual running `eggress` executable with `std::env::current_exe()` and derive the sibling binary directory from it.

Expected normal layouts include:

```text
/usr/local/bin/eggress
/usr/local/bin/pproxy
```

and:

```text
$HOME/.local/bin/eggress
$HOME/.local/bin/pproxy
```

Cargo-installed binaries may also reside together in a Cargo bin directory. The updater may update a writable Cargo-installed pair using release binaries, but documentation should note that `eggress update` installs canonical GitHub release binaries and therefore changes the subsequent update provenance away from `cargo install` semantics.

Do not scan arbitrary PATH entries trying to guess which `pproxy` belongs to Eggress. Derive the expected sibling path from the running executable directory and verify the sibling’s version/identity before replacing it.

If no matching sibling Eggress `pproxy` exists:

- do not replace only `eggress` silently;
- fail with an actionable repair/install message, or provide an explicitly tested recovery path that installs the missing sibling as part of the same transaction.

Prefer the recovery path only if it remains simpler than failing closed.

## Phase G — Transactional two-binary replacement

Updating two executables introduces an atomicity problem. Treat the pair as one logical transaction.

### Unix

Use same-filesystem staging/backups and atomic rename semantics where practical:

1. confirm destination directory is writable;
2. place candidate temporary files in the destination filesystem if necessary for atomic rename;
3. preserve backups of existing `eggress` and `pproxy`;
4. replace the sibling `pproxy` and running `eggress` in an order compatible with the operating system;
5. if any replacement before commit fails, restore prior files;
6. after successful replacement, remove backups.

Use established safe-replacement mechanics rather than hand-rolling unsafe file truncation. Gregg’s use of `self-replace` is a relevant reference; reuse/adapt that dependency if it materially simplifies cross-platform correctness and passes the project’s dependency review.

### Windows

Windows running-image semantics require explicit handling. Do not assume Unix rename behavior works for the currently executing `.exe`.

Adapt the tested staged/self-replacement approach from Gregg or the `self-replace` crate. The design must handle both files:

- stage the new `pproxy.exe` and `eggress.exe`;
- replace the sibling safely;
- schedule/perform running `eggress.exe` replacement using a supported Windows self-replace strategy;
- preserve rollback or leave a deterministic recoverable state if the second step fails.

A tiny internal replacement helper or hidden internal mode is acceptable if required by Windows, but it must not become a public command surface. Do not add a separate user-facing updater executable unless testing demonstrates it is necessary.

### Permissions

Never invoke `sudo`, UAC elevation, or credential prompts automatically.

If destination files/directories are not writable, fail before mutating either binary and show a concise remediation, e.g. rerun installation/update from an appropriately privileged shell or reinstall to a user-writable directory.

### Acceptance criteria

- an update cannot intentionally leave `eggress` new and `pproxy` old on a normal successful path;
- failure before commit leaves old binaries intact;
- Windows running-executable replacement is covered by a Windows-specific test/smoke path;
- no internal privilege escalation exists.

## Phase H — CLI command behavior and output

Add:

```text
eggress update
```

Keep the initial public interface intentionally small. Do not add channel selectors, force/downgrade flags, automatic cron behavior, or update configuration files.

Suggested successful output classes:

```text
eggress X.Y.Z is already the latest stable version
updated eggress X.Y.Z -> A.B.C
```

Progress such as downloading/verifying should go to stderr; the final outcome may go to stdout. Avoid progress bars or terminal UI dependencies.

Use typed update errors for at least:

- unsupported target;
- release discovery failure;
- invalid release tag/version;
- download failure;
- checksum retrieval/parse failure;
- checksum mismatch;
- archive extraction failure;
- candidate identity/version mismatch;
- installation/sibling mismatch;
- permission failure;
- replacement/rollback failure.

Map update failures to the shared CLI runtime/external-dependency exit contract established by the cleanup plan. Do not introduce arbitrary new numeric exit codes in the update module.

## Phase I — Reuse from Gregg without importing Gregg architecture

Review and selectively adapt these Gregg concepts:

```text
crates/gregg-update/src/version.rs
crates/gregg-update/src/target.rs
crates/gregg-update/src/verify.rs
crates/gregg-update/src/stage.rs
crates/gregg-update/src/exec.rs
```

Useful proven ideas include:

- stable version parsing/comparison;
- bounded command execution;
- target mapping;
- checksum verification;
- candidate executable validation;
- `self-replace`-based replacement;
- private temporary staging;
- no internal sudo.

Do not copy:

- crates.io-as-authority behavior;
- Cargo fallback behavior;
- daemon restart/service policy;
- Gregg program-specific outcome types/messages;
- abstractions whose only purpose is sharing between `gregg` and `greggd`.

Prefer a compact Eggress-local module tree under `crates/eggress-cli/src/update/` unless implementation size/independent testability clearly justifies a small internal crate. Do not create a new publishable crate merely to mirror Gregg’s layout.

## Phase J — Tests

### Unit tests

- semver comparison including already-current/newer/current-newer cases;
- prerelease does not become default latest candidate;
- OS/arch target mapping;
- archive/checksum URL construction;
- checksum parser and mismatch;
- candidate `eggress version` parsing;
- candidate compatibility-binary version parsing;
- sibling-path derivation;
- unsupported target behavior.

### Integration/process tests

Use local fixtures/mock endpoints rather than live GitHub where practical:

- update no-op when release equals current;
- download + verify + stage happy path without mutating the test runner executable;
- missing checksum fails closed;
- checksum mismatch fails closed;
- archive contains only one of the two binaries -> fail;
- candidate versions disagree -> fail;
- candidate version differs from release tag -> fail;
- destination not writable -> no mutation;
- injected second-file replacement failure -> rollback/recoverable old pair;
- Windows replacement smoke on a Windows runner;
- Unix replacement smoke using copied fixture executables in a temp directory or a purpose-built test helper.

Do not make normal unit tests depend on GitHub availability.

### Release integration test

The binary release workflow should test the actual produced archive with the same staged version-verification helper logic where practical. This prevents installer/updater asset assumptions from drifting away from release output.

## Documentation dependency

Do not advertise `eggress update` before the release asset pipeline is active. Land user-facing docs through `CLI_DISTRIBUTION_DOCUMENTATION_AND_RELEASE_POLICY.md` in the same release that activates the binary pipeline.

Document clearly that:

- the command updates the standalone executables from GitHub Releases;
- it does not update the PyPI package installed in a Python environment;
- Python users should update with their Python package manager;
- package-manager/source-managed installations may prefer their original manager rather than self-update.

## Final acceptance criteria

This plan is complete when:

1. `eggress version` prints `eggress X.Y.Z` deterministically and exits 0.
2. `eggress --version` remains functional.
3. the standalone `pproxy` binary retains its existing compatibility-oriented version option and does not gain an `update` subcommand.
4. `eggress update` discovers only the latest stable GitHub Release by default.
5. update target/archive naming exactly matches the release pipeline contract.
6. archive checksum is verified before extraction/replacement.
7. both staged executables are executed and version-verified before replacement.
8. both staged versions equal the exact release tag and one another.
9. update replaces `eggress` and `pproxy` as one logical release unit.
10. replacement failures leave the previous installation intact or deterministically recoverable and are tested.
11. Windows running-executable replacement is implemented with a supported staged/self-replace mechanism rather than assumed Unix semantics.
12. unsupported targets and permission problems fail before mutation with actionable messages.
13. the updater never invokes sudo/UAC automatically.
14. there is no implicit Cargo/source fallback and no background update behavior.
15. normal tests do not depend on live GitHub availability.
16. release workflow artifacts are verified using the same asset/version assumptions consumed by the updater.
