# CLI Cleanup and Binary Delivery Roadmap

## Status

Implementation plan for handoff. This plan is intentionally bounded to the `eggress-cli` user surface, its pproxy compatibility facade, binary distribution, self-update/version UX, and the documentation/release-policy changes required to support those features.

## Objective

Make the native `eggress` CLI easier to maintain and easier to install directly, without weakening pproxy compatibility or changing the project’s primary Python/PyPI positioning.

The intended distribution hierarchy after this work is:

1. **PyPI remains the primary project distribution and pproxy-migration channel.** `pip install eggress` remains the first-class Python path and the tag-triggered Python publishing workflow remains authoritative for Python artifacts.
2. **Prebuilt GitHub Release binaries become the preferred standalone CLI installation path.** Users who want `eggress`/`pproxy` as executables should not need Rust or Python.
3. **crates.io remains a supported developer/Rust installation path, published manually.** This work must not automate crates.io publication.
4. **Source builds remain available but are not the recommended CLI install path.**

The direct CLI install should converge on a Gregg-like bootstrap flow, for example:

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash
```

The installer should install the matched release of both executables produced by `eggress-cli`:

- `eggress`
- `pproxy`

`eggress version` and `eggress update` become native top-level subcommands. The pproxy compatibility binary keeps upstream-compatible `--version` behavior and should not acquire Eggress-native subcommands that would pollute its compatibility surface.

## Current state and reasons for change

`crates/eggress-cli/src/main.rs` currently mixes Clap schema, runtime startup, configuration loading, route diagnostics, a hand-written admin HTTP client, upstream testing, pproxy compatibility dispatch, logging, and service lifecycle logic. The crate also ships a second `pproxy` binary and shared helpers in `src/lib.rs`. This is functional but creates duplicated orchestration and makes the command surface harder to evolve safely.

The release policy currently says GitHub Actions must not build release bundles or create release artifacts. That policy predates the decision to make standalone binary installation a first-class CLI path and must be amended explicitly as part of this line of work. The exception should be narrow: tagged release binaries/checksums/installers for `eggress-cli` only. Ordinary CI must stay ordinary CI, and crates.io publication must stay manual.

Gregg provides a useful reference pattern: a release-only binary workflow, stable target-specific asset naming, SHA-256 sidecars, bootstrap installers, candidate `version` verification, and a binary-first self-update path. Eggress should reuse these ideas, not Gregg’s daemon/service-management machinery.

## Scope boundaries

In scope:

- simplify the native `eggress` CLI implementation and ownership boundaries;
- unify pproxy execution paths without changing supported pproxy syntax;
- make CLI value domains typed where appropriate;
- move upstream testing onto the production runtime connector path;
- add `eggress version`;
- add `eggress update`;
- publish prebuilt `eggress` and `pproxy` binaries for supported platforms;
- publish SHA-256 checksums and bootstrap installer assets;
- add a one-command curl installer for Linux/macOS and a corresponding PowerShell path for Windows;
- keep PyPI as the primary project-facing distribution channel;
- update README, CLI docs, release docs, and migration docs so install/update semantics are unambiguous.

Out of scope:

- changing pproxy protocol semantics or parity tiers;
- adding native Eggress copies of every pproxy flag;
- adding a daemon/service manager;
- adding automatic background update checks;
- adding package-manager taps/repos;
- adding auto-update to the `pproxy` compatibility CLI;
- automating crates.io publication;
- replacing the Python publish workflow;
- adding signing/SBOM infrastructure unless a separate security/release decision explicitly requests it.

## Target architecture

The CLI should converge toward these ownership boundaries:

```text
crates/eggress-cli/
  src/main.rs                  thin parse/dispatch/process boundary
  src/commands/
    run.rs                     native proxy startup facade
    route.rs                   route explain command
    upstream.rs                upstream diagnostics command
    pproxy.rs                  nested migration/compat commands
    update.rs                  self-update command adapter
    version.rs                 version rendering
  src/logging.rs               log format/value policy
  src/lib.rs                   only truly shared CLI/process helpers
  src/pproxy_main.rs           thin compatibility binary facade
```

Exact filenames may vary, but responsibilities must be separated. Do not invent a generic command framework or plugin architecture.

The production runtime/connector layer, not `eggress-cli`, should own protocol handshake construction. Upstream testing should call shared runtime connector APIs so the diagnostic surface cannot silently lag supported protocols.

The compatibility execution path should become:

```text
pproxy binary ---------\
                       -> shared pproxy execution facade -> runtime
`eggress pproxy run` --/
```

while `translate` and `check` remain migration/developer tools.

## Release artifact contract

Prefer a stable, unsurprising target matrix matching current Python wheel coverage and existing cross-platform claims:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Linux release builds should use a documented glibc portability floor rather than whatever happens to be installed on the current GitHub runner. Reusing Gregg’s cargo-zigbuild/Zig approach is acceptable if it integrates cleanly with Eggress dependencies; otherwise the implementation plan must document the chosen equivalent.

Because `eggress-cli` produces two executables that should remain version-aligned, prefer a release archive per target rather than independent mutable installs:

```text
eggress-<target>.tar.gz       # Unix targets; contains eggress + pproxy
eggress-<target>.zip          # Windows; contains eggress.exe + pproxy.exe
<archive>.sha256
install.sh
install.ps1
```

A flat per-binary asset scheme is acceptable only if update/install remains atomic enough that `eggress` and `pproxy` cannot normally end up at different release versions.

The Git tag `vX.Y.Z` supplies the version namespace. Asset filenames should therefore remain stable within each release and should not duplicate the version unless a concrete tooling need requires it.

## Release version invariant

Before binary assets are attached to a tag, a release preflight must verify:

- tag is exactly `v<workspace-version>`;
- the root Cargo workspace version matches the Python package version used by the canonical `eggress` PyPI artifact;
- any other lockstep release manifests required by the existing release policy match;
- both `eggress version` and `pproxy --version` report the expected release version;
- the checkout being built is the tagged commit.

This is important because the CLI updater will trust GitHub Releases while Python users trust PyPI. The release process must prevent those two channels from advertising divergent versions for the same tag.

## Work phases

### Phase 1 — CLI ownership and maintenance cleanup

Implement `CLI_COMMAND_SURFACE_AND_RUNTIME_BOUNDARY_CLEANUP.md`.

Primary outcomes:

- split the native CLI monolith into bounded command modules;
- remove duplicated configuration argument ownership;
- use typed Clap enums for closed value domains;
- remove CLI-local protocol handshake construction;
- centralize pproxy execution orchestration;
- rationalize exit-code ownership and misleading compatibility state names.

This phase should preserve existing behavior unless a behavior is explicitly identified as erroneous.

### Phase 2 — Release binaries and bootstrap installer

Implement `CLI_BINARY_RELEASE_AND_INSTALLER.md`.

Primary outcomes:

- tag-triggered release-only binary workflow;
- target-specific archives containing both binaries;
- SHA-256 verification;
- `install.sh` and `install.ps1` release assets;
- one-command binary-first install documented as the preferred standalone CLI path;
- no changes to crates.io publication automation (it stays manual).

### Phase 3 — `version` and self-update

Implement `CLI_VERSION_AND_SELF_UPDATE.md`.

Primary outcomes:

- `eggress version` with deterministic output;
- `eggress update` using exact tagged GitHub Release assets;
- checksum and candidate-version verification before replacement;
- safe replacement of both installed binaries;
- clear handling when an installation is not writable;
- no internal sudo escalation.

### Phase 4 — Documentation and release-policy integration

Implement `CLI_DISTRIBUTION_DOCUMENTATION_AND_RELEASE_POLICY.md`.

Primary outcomes:

- PyPI remains visibly primary for Python/pproxy migration users;
- standalone CLI docs lead with the binary installer;
- Cargo is documented as a Rust/developer alternative;
- release documentation explicitly allows the narrow binary artifact workflow;
- install/update/version behavior and platform support are tested against docs.

## Ordering and dependencies

Phase 1 and Phase 2 can begin independently, but Phase 3 depends on the release asset contract established in Phase 2. Phase 4 should be completed with the implementation rather than deferred indefinitely; final documentation text must reflect the actual artifact names and behavior that landed.

Do not merge a public `eggress update` command until at least one release pipeline path can produce the exact assets that the updater expects.

## Verification strategy

Keep verification proportionate. This work does not justify re-expanding ordinary CI into a full release certification system.

Required checks should include:

- existing workspace format/clippy/test gates relevant to modified crates;
- focused CLI parser/process tests;
- pproxy compatibility tests for the unchanged compatibility surface;
- release-workflow smoke checks for `eggress version`, `eggress --help`, `pproxy --version`, and `pproxy --help` on native runners;
- installer tests with mocked/local release assets where practical;
- checksum mismatch negative test;
- candidate-version mismatch negative test;
- update no-op when already current;
- update failure leaves existing executables intact;
- documentation commands match the actual CLI help output.

Do not require release-only matrix builds on every ordinary branch push.

## Global acceptance criteria

This roadmap is complete when all of the following are true:

1. `eggress` retains its current proxy functionality and pproxy compatibility behavior.
2. `pproxy` remains a flat compatibility binary and does not gain Eggress-native subcommands.
3. `eggress-cli` command implementation no longer owns protocol handshake logic that duplicates the production runtime connector path.
4. `eggress pproxy run` and the standalone `pproxy` binary delegate to one shared compatibility execution implementation.
5. `eggress version` prints a stable machine-testable release version.
6. `eggress update` can safely update a binary-installed release using GitHub Release assets and verified SHA-256 metadata.
7. The updater never silently replaces an executable when checksum or reported candidate version does not match expectations.
8. A documented one-command curl install works on supported Linux/macOS targets without requiring Rust or Python.
9. A documented PowerShell install works for supported Windows x86_64.
10. Release assets contain both `eggress` and `pproxy` at the same version.
11. Tagged binary releases are built only in a release-specific workflow; ordinary CI stays lightweight.
12. PyPI remains the primary documented Python/pproxy-migration channel and its existing publication flow is not replaced.
13. crates.io publication remains manual.
14. `docs/release/RELEASE_PROCESS.md` is amended so its automation policy no longer contradicts the new binary artifact pipeline.
15. README and crate-level CLI documentation distinguish Python installation, standalone binary installation, Cargo installation, source builds, `version`, and `update` without presenting competing “preferred” paths for the same user persona.

## Non-goals / anti-scope-creep guardrails

Do not add telemetry, update daemons, scheduled update checks, shell completion generation, package repositories, GUI installers, service management, or generic release orchestration as part of this roadmap. If implementation uncovers a need for one of those, document it separately rather than broadening this line of work.
