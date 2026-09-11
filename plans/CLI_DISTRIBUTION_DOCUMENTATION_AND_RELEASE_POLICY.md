# CLI Distribution Documentation and Release Policy

## Status

Implementation plan for handoff. This is Phase 4 of `CLI_CLEANUP_AND_BINARY_DELIVERY_ROADMAP.md`.

## Objective

Make the repository’s user-facing installation guidance, CLI reference, pproxy migration guidance, and release policy accurately reflect the new distribution model:

- **PyPI remains the primary Python/pproxy-migration distribution channel.**
- **GitHub Release binaries become the preferred standalone CLI installation channel.**
- **crates.io remains a supported Rust/developer installation channel and stays manually published.**
- **source builds remain available for unsupported targets/custom feature sets.**

The documentation must avoid presenting these as contradictory “primary” paths. Each path serves a different user persona.

## Current documentation issues to resolve

1. The root README currently leads standalone CLI installation with `cargo install eggress-cli`, which would no longer be the preferred CLI path once prebuilt binaries exist.
2. `crates/eggress-cli/README.md` similarly describes Cargo installation as the default standalone path.
3. `docs/release/RELEASE_PROCESS.md` explicitly prohibits automated GitHub Release creation and mandatory release artifact/checksum jobs, which conflicts with the newly approved binary distribution design.
4. Current documentation spans Python installation, Rust library use, CLI use, and pproxy migration without one concise distribution model explaining which path is intended for which user.
5. Some CLI examples should be checked against the actual current parser/help output. In particular, migration-only subcommands belong under `eggress pproxy ...`; the standalone `pproxy` compatibility binary should remain flat unless its parser explicitly supports otherwise.
6. `version` and `update` need clear provenance semantics so Python users do not assume `eggress update` updates their PyPI environment.

## Documentation principles

### Persona-first installation guidance

Organize installation around intent, not implementation language:

1. **Python / replacing or migrating from pproxy:** `pip install eggress`.
2. **Standalone command-line proxy:** prebuilt binary bootstrap installer.
3. **Rust developers / custom features / unsupported binary target:** `cargo install eggress-cli` or source build.
4. **Rust library consumers:** Cargo dependency on the intended library crate.

### One preferred path per persona

Do not say both Cargo and curl are “recommended” for a normal standalone CLI user. The binary installer should be preferred for that persona once it is live.

Likewise, do not demote PyPI in the project overview. Python/pproxy compatibility is a major project target and PyPI remains the primary package-facing route for that use case.

### Truthful security language

Checksums attached to the same GitHub Release provide integrity/mismatch detection but are not independent signatures. Do not describe SHA-256 sidecars as cryptographic publisher authentication.

### Generated/help-derived truth where practical

CLI docs must be checked against actual `--help` output. Do not maintain command spellings that the parser does not accept.

## Phase A — Add a dedicated installation guide

Add:

```text
docs/INSTALLATION.md
```

This should be the canonical detailed installation document. Keep the root README concise and link into it.

Recommended structure:

### Python / pproxy migration

```bash
pip install eggress
```

Explain:

- this is the primary Python distribution;
- supported Python versions/platform wheels according to the actual Python workflow;
- optional compatibility package rules for the bounded top-level `pproxy` namespace;
- upstream `pproxy` and the opt-in top-level compatibility distribution must not collide in the same environment where that remains true.

### Standalone CLI — preferred binary install

Unix:

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash
```

State that this installs both:

```text
eggress
pproxy
```

Document default locations:

```text
root       -> /usr/local/bin
non-root   -> $HOME/.local/bin
```

Document custom/pinned install examples using the actual installer arguments that land, e.g.:

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash -s -- --version X.Y.Z
```

and local/auditable use:

```bash
curl -fsSLO https://github.com/eggstack/eggress/releases/latest/download/install.sh
less install.sh
bash install.sh
```

Do not imply piping remote code to a shell is risk-free. The auditable alternative is worth documenting succinctly.

### Windows standalone install

Document the final PowerShell command matching `install.ps1` and an auditable download-then-run variant.

State initial Windows architecture support accurately (x86_64 unless expanded by implementation).

### Cargo installation

```bash
cargo install eggress-cli --locked
```

Explain this is appropriate for:

- Rust users;
- unsupported prebuilt targets;
- users who want Cargo-managed installation provenance;
- custom builds/features.

Document the workspace MSRV from the actual repository source of truth.

### Custom feature/source builds

Retain examples for optional `ssh`, `quic`, legacy compatibility, lean builds, etc., but make it clear that canonical release binaries use the default `eggress-cli` feature set, not every opt-in feature.

### Updating

Standalone binary install:

```bash
eggress update
```

Python install:

```bash
python -m pip install --upgrade eggress
```

Cargo-managed install, if the user wishes to stay Cargo-managed:

```bash
cargo install eggress-cli --locked --force
```

Explain that `eggress update` installs canonical GitHub Release binaries and does not modify a Python environment.

### Version inspection

```bash
eggress version
pproxy --version
```

Explain the two binaries are released in a version-aligned archive.

### Supported prebuilt matrix

List only assets actually produced:

- Linux x86_64;
- Linux AArch64;
- macOS Intel;
- macOS Apple Silicon;
- Windows x86_64.

Include the tested Linux glibc floor after implementation verifies it.

### Troubleshooting

At minimum:

- `$HOME/.local/bin` not in PATH;
- unsupported platform/architecture;
- checksum/version verification failure;
- destination not writable;
- `eggress update` cannot replace a package-manager-owned path;
- need for optional features not present in default binary.

## Phase B — Rewrite the root README installation section

Keep project positioning concise but make distribution roles immediately understandable.

Recommended ordering:

### Python / pproxy migration

Lead with:

```bash
pip install eggress
```

and identify it as the primary Python distribution.

### Standalone CLI

Then show the one-command binary installer prominently:

```bash
curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash
```

State that it installs both `eggress` and `pproxy`.

Link to `docs/INSTALLATION.md` for Windows, pinned versions, install directories, checksums, Cargo, and custom builds.

### Rust library

Keep the embeddable-library install path separate.

### Cargo/source alternative

Move `cargo install eggress-cli` out of the first CLI position and describe it as the developer/custom-build alternative.

Do not let installation detail overwhelm the README.

### Acceptance criteria

- a new Python user sees PyPI first for Python use;
- a new CLI-only user sees the binary installer without needing to read release docs;
- Cargo remains easy to find but is not presented as the preferred binary-user path;
- no duplicated full platform matrix in multiple README sections if it can live in `docs/INSTALLATION.md`.

## Phase C — Update `crates/eggress-cli/README.md`

This crate README should focus on the standalone executable artifact and Cargo package details.

Update it to state:

- prebuilt GitHub Release binaries are the preferred normal CLI install;
- `cargo install eggress-cli --locked` remains the Rust/developer path;
- both `eggress` and `pproxy` are installed by either path;
- canonical binary releases use the default crate features;
- custom feature builds require Cargo/source;
- `eggress version` and `eggress update` are native Eggress commands;
- pproxy compatibility retains `pproxy --version` and its flat option surface.

Link back to canonical install/release docs instead of duplicating detailed workflow mechanics.

## Phase D — Update operations/CLI reference

Review `docs/OPERATIONS.md` and any CLI reference sections against actual parser output after the cleanup plan lands.

Document:

```text
eggress version
eggress update
eggress route ...
eggress upstream test ...
eggress pproxy translate -- ...
eggress pproxy check -- ...
eggress pproxy run -- ...
```

If `--config` becomes a true global Clap option, document its legal placement and precedence exactly.

If typed enums are introduced, list their actual accepted values or rely on generated help snippets rather than prose that can drift.

Document update failure behavior at an operational level:

- no auto-sudo;
- no background update checks;
- same-version no-op;
- supported target requirement;
- both binaries updated as one release unit;
- errors before replacement preserve current install.

Do not turn OPERATIONS into release-internals documentation.

## Phase E — Correct pproxy migration documentation

Update `docs/PPROXY_MIGRATION.md` so it clearly distinguishes:

### Python migration

Primary package path remains PyPI:

```bash
pip install eggress
```

### CLI-only migration

Users replacing a `pproxy` command on a host can install the standalone release binaries and receive both:

```text
eggress
pproxy
```

The direct `pproxy` facade remains the closest command-line replacement path.

The nested native migration tooling remains:

```bash
eggress pproxy translate -- ...
eggress pproxy check -- ...
eggress pproxy run -- ...
```

Audit every example to ensure no documentation incorrectly shows nested migration subcommands directly under the standalone compatibility binary unless that syntax is genuinely supported.

Keep the canonical compatibility manifest/matrix as the authority for parity. Do not add a new duplicated compatibility table merely because install docs changed.

## Phase F — Amend `docs/release/RELEASE_PROCESS.md`

This is a required policy change, not optional polish.

The existing policy intentionally prohibited release bundle automation. Replace that rule with a narrow exception reflecting the new product decision.

### Preserve these release principles

- releases remain operator-initiated by deliberate version/tag actions;
- crates.io publication remains manual;
- no crates.io credentials/publishing are added to GitHub Actions;
- Python publishing remains its existing protected OIDC workflow;
- ordinary CI remains separate from release artifact generation;
- release workflows must not duplicate the full ordinary CI suite;
- no broad container/SBOM/signing/evidence machinery is introduced by default.

### Add this approved automation

A `v*` tag may trigger a dedicated binary release workflow that:

- validates tag/version alignment;
- builds the five canonical `eggress-cli` target archives;
- smoke-tests both executables;
- generates SHA-256 sidecars;
- attaches `install.sh` and `install.ps1`;
- creates/publishes the GitHub Release only after required binary jobs succeed.

State explicitly that this is an approved exception to the older “no automated GitHub Release/artifacts” rule.

### Clarify release channels

The release process should state:

```text
PyPI             canonical Python distribution / pproxy migration package
GitHub Releases  canonical prebuilt standalone CLI binaries
crates.io        canonical Rust crate source distribution, published manually
```

All use the same repository version/tag invariant.

### Revised operator sequence

A suitable high-level release sequence is:

1. update lockstep versions and release notes;
2. run proportionate local verification and Cargo dry-runs;
3. manually publish Rust crates as required by dependency order;
4. tag `vX.Y.Z` and push the tag;
5. tag triggers both the existing Python publish workflow and the new binary release workflow;
6. verify PyPI publication;
7. verify GitHub Release archives/installers/checksums;
8. smoke the public installer against the new release;
9. roll forward with a new version if immutable published artifacts are defective.

Do not require one workflow to republish the other channel.

### Update the “Prohibited automation” section

Retain prohibitions on:

- crates.io auto-publish;
- release workflows duplicating ordinary CI;
- unrelated mandatory evidence bundles;
- automatic service deployment;
- publishing noncanonical extra artifacts without an explicit project decision.

Remove/qualify prohibitions that directly conflict with the approved CLI binary workflow:

- automated GitHub Release creation is now allowed for the canonical CLI binary workflow;
- checksum/release artifact jobs are allowed for these canonical binaries;
- installer attachment is allowed.

The text must not simultaneously approve and prohibit the same workflow.

## Phase G — Add release/install documentation invariants

Where practical, add lightweight automated checks to catch obvious drift.

Examples:

- ensure README references `packaging/install.sh`/release URL that actually exists;
- ensure documented target strings match the release workflow target list;
- ensure `eggress version` and `pproxy --version` are exercised in release smoke;
- grep/check that direct pproxy migration examples use only syntax accepted by `pproxy --help`;
- maintain one canonical target list in script/data if this can be done without introducing generation machinery more complex than the problem.

Do not build a documentation generator framework. A few focused tests/scripts are sufficient.

## Phase H — Review ancillary documentation

Search the repository for stale installation/release statements and update only relevant occurrences.

Search terms should include:

```text
cargo install eggress-cli
pip install eggress
GitHub Release
release bundle
--version
pproxy translate
pproxy check
eggress pproxy
```

Likely files include:

- `README.md`;
- `crates/eggress-cli/README.md`;
- `docs/OPERATIONS.md`;
- `docs/PPROXY_MIGRATION.md`;
- `docs/release/RELEASE_PROCESS.md`;
- release/CLI skills under `.skills/` if they encode release behavior;
- contributor/development docs that instruct maintainers how to install the CLI.

Do not churn historical plan files merely to rewrite old assumptions. Plans are historical records unless explicitly marked as current policy.

## Phase I — Validate public examples

Before closing this work, build/install a release candidate and execute the documented commands exactly.

At minimum validate:

```bash
# Python
python -m pip install eggress

# Unix binary bootstrap against a test/draft or local fixture release
bash packaging/install.sh --version X.Y.Z

eggress version
eggress --help
pproxy --version
pproxy --help

eggress pproxy check -- -l socks5://127.0.0.1:1080

# Cargo alternative
cargo install eggress-cli --locked --version X.Y.Z --root <temp>
```

Validate the Windows installer/commands on Windows CI or a Windows test host.

Do not publish a production tag solely for documentation testing.

## Final acceptance criteria

This plan is complete when:

1. `docs/INSTALLATION.md` exists and is the detailed canonical install/update guide.
2. the root README presents PyPI first for Python/pproxy migration and the GitHub binary installer first for standalone CLI use.
3. `crates/eggress-cli/README.md` presents binary installation as the normal CLI path while retaining Cargo/custom-feature guidance.
4. documentation states that canonical binary archives contain both `eggress` and `pproxy` at one version.
5. documentation accurately lists only the prebuilt target matrix that the workflow produces.
6. documentation explains that release binaries use default CLI features and custom optional features require Cargo/source builds unless that policy changes.
7. `eggress version` and `eggress update` are documented with accurate provenance semantics.
8. Python users are explicitly told that `eggress update` does not update their Python environment.
9. pproxy migration examples distinguish the flat standalone `pproxy` facade from `eggress pproxy translate/check/run` migration tools.
10. stale/invalid examples such as unsupported direct `pproxy <nested-subcommand>` forms are removed or corrected.
11. `docs/release/RELEASE_PROCESS.md` explicitly authorizes the narrow tag-triggered CLI binary artifact/GitHub Release workflow.
12. the same release document continues to prohibit automatic crates.io publication and unrelated release overengineering.
13. release docs explain that one version/tag is shared across PyPI, GitHub binary releases, and manually published Rust crates.
14. checksum documentation does not overclaim independent signature/provenance guarantees.
15. README/docs commands have been exercised against the implementation or covered by focused drift checks.
16. historical plans are not mass-edited merely to make old decisions look current.
