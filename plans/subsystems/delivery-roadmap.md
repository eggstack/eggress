# Delivery (CLI, Embed, Python, Release) Roadmap

Status: active; M001-M002 closed, M003 Eggup archive/pair self-update adoption blocked on versioned Eggup core/archive packages

Long-term references:

- `plans/000-long-term-specification.md` §2 (goal 6–7), §4 (invariants 9, 11)
- `plans/001-terminology-and-domain-model.md` §2 (Deployment forms)
- `plans/002-long-term-roadmap.md` (Phases 13–17, distribution/release maintenance)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

Owns delivery surfaces: native/compat CLI (`eggress-cli` installing `eggress` + `pproxy` binaries; default `full` minus `ssh`/`quic`/`pproxy-legacy`/`legacy-crypto`/`pproxy-daemon`), system-proxy backend, `eggress-embed` facade, Python bindings (`eggress-python` + `python/eggress` package + `python-pproxy-compat`), wheel/crates.io release engineering, installers, and self-update. Consumes all other subsystems; MUST NOT change their behavior as a side effect (release-engineering-only constraint).

Architecture: `architecture/cli.md`, `architecture/admin.md`, `architecture/metrics.md`, `architecture/system-proxy.md`, `architecture/embed.md`, `architecture/python-bindings.md`.

## 2. Work classification

### Invariants

- Version lockstep (4 gated places + aligned convenience pin); `cargo publish --workspace` stays nightly-only; never `--no-verify`/`--allow-dirty`; tags (`v*`) fire PyPI + binary-release workflows — never push tags casually.
- `eggress-embed::outbound` re-exports `eggress-outbound` authority; SSH cache policy split (native verified known-hosts vs compat host-key only with `ssh`+`pproxy-compat`).

### Capabilities

- CLI parity closures, `eggress version`/`update`, embed lifecycle API, Python classes/exceptions, wheel matrix, installers.

### Infrastructure

- `scripts/publish-crates.py` helper, wheel-matrix validation, installer assets.

### Polish

- CLI exit-code contract tests, diagnostics, docs (`docs/INSTALLATION.md`).

## 3. Non-goals

- Free-threaded Python; automated crates.io publication; proxy/protocol/runtime behavior changes unrelated to delivery. M003 may change self-update implementation mechanics only while preserving the established user-visible updater contract.

## 4. Current state

CLI cleanup/version/self-update, distribution docs/policy, embed/Python stabilization, PyPI matrix expansion, manual-publish simplification all complete. 1.0.10 prepared but unpublished; publication/tagging is maintainer-authorized, not planning-authorized.

A newly qualified upstream convergence path is registered as M003: Eggup Archive M001d can replace Eggress's generic shell archive extraction and bespoke two-binary rollback transaction while Eggress retains GitHub/checksum/version/CLI policy. The implementation plan is complete but blocked for final dependency cutover until compatible versioned `eggup-core` and `eggup-archive` packages are available from the registry. Permanent git/path dependencies are not acceptable for the publishable workspace.

## 5. Target architecture

One version, verified artifacts, manual-gated publication, with generic self-update archive/transaction mechanics delegated to Eggup while Eggress retains release and CLI policy. The existing release architecture is attained; updater-mechanism convergence remains gated by M003.

## 6. Dependency graph

```text
CLI surface (hard)
    +--> Embed facade (soft)
    +--> Python bindings/packaging (soft)
    +--> Release automation (operational: tags, PyPI env, crates.io manual)
    `--> M003 Eggup updater convergence [BLOCKED on versioned Eggup core/archive packages]
```

## 7. Milestones

### Milestone 1 — CLI/embed/Python surfaces

Class: capability. Objective: CLI closures, embed API, Python bindings + helpers. Exit: Phases 13–16, 29–32, 38–40 complete. Status: closed (historical, archive).

### Milestone 2 — Release-engineering closure

Class: polish/infrastructure. Objective: wheel matrix, publish helper, installers, 1.0.10 roll-forward. Exit: `PYPI_WHEEL_MATRIX_EXPANSION.md`, `MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md`, `POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md` implemented. Status: closed. Evidence: archive records + `scripts/release-preflight.sh --check-versions-only`.

### Milestone 3 — Eggup archive/pair self-update adoption

Plan: `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md`.

Class: capability/delivery convergence. Status: blocked on compatible versioned Eggup core/archive packages.

Objective: retain Eggress release discovery, target mapping, checksum, staged-version, CLI, and publication policy while replacing generic tar/zip extraction plus the bespoke `eggress`/`pproxy` backup/rollback transaction with Eggup's qualified archive extraction and multi-artifact transaction.

Hard gate: Eggup's post-M001d `eggup-core` API and `eggup-archive` must be available as registry dependencies suitable for `cargo package`; immutable revision qualification is allowed only before final dependency cutover.

Exit requires shell extraction removed from the updater success path, local generic pair rollback removed, bound-source staging, exact candidate validation preserved, rollback/recovery fault evidence, package dry-run, Rust 1.89, Linux/macOS/Windows qualification, and recorded footprint delta.

## 8. Cross-cutting requirements

Features: `eggress-cli full` excludes `ssh`/`quic`/`pproxy-legacy`/`legacy-crypto`/`pproxy-daemon` (extra check command for those paths); `eggress-embed full` includes `pproxy-legacy`; never `--all-features` (drags test-only `insecure-quic`). Python: maturin develop + pytest from root with `importlib` mode. Fuzz workspace standalone. Compat: Python `pproxy` namespace owned only by `python-pproxy-compat`.

## 9. Verification strategy

`cargo test -p eggress-cli --test cli_exit_codes`, embed feature-slice checks + OpenSSH regression for boundary changes, Python pytest (`python/tests`, `tests/compat`), `scripts/release-preflight.sh` for version/release changes, `cargo deny`/`cargo audit --ignore RUSTSEC-2023-0071` for dependency changes.

## 10. Risks and decision points

Tag-push discipline remains critical because both publish workflows fire on `v*`. M003 additionally carries a registry-package dependency gate, Windows running-image qualification risk, and default-binary footprint risk. The current whole-archive SHA-256 release evidence is sufficient for Eggup extraction because member expectations are optional; do not invent a new member manifest solely for this migration.

## 11. Completion definition

Existing delivery surfaces are qualified and 1.0.10 remains prepared/unpublished. M003 is an active convergence extension and closes only after the versioned Eggup dependency gate and updater migration qualify; it does not authorize tagging/publication.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive CLI/embed/python records | archive + Git history | — |
| 2 | closed | archive distribution records | registry control points | maintainer release action (out of planning scope) |
| 3 | blocked | `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md` | — | compatible versioned `eggup-core` + `eggup-archive` registry packages containing M001d APIs |
