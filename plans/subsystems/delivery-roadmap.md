# Delivery (CLI, Embed, Python, Release) Roadmap

Status: closed; M001-M004 closed; M004 carries a tracked `zip 6.0.0` upstream exception (owner and review date in `deny.toml`)

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

CLI cleanup/version/self-update, distribution docs/policy, embed/Python stabilization, PyPI matrix expansion, manual-publish simplification all complete. 1.0.10 is published as `v1.0.10` (2026-09-24); the next version bump/publication is maintainer-authorized, not planning-authorized.

A newly qualified upstream convergence path was registered as M003: Eggup Archive M001d can replace Eggress's generic shell archive extraction and bespoke two-binary rollback transaction while Eggress retains GitHub/checksum/version/CLI policy. The implementation plan is complete and landed: Eggup M009 published compatible versioned `eggup-core 0.1.2` and `eggup-archive 0.1.2` to the registry on 2026-09-28, and M003 adopted them with hosted updater qualification (closure `plans/closure/delivery/003-status.md`). Permanent git/path dependencies are not acceptable for the publishable workspace.

A 2026-11-16 security audit found that release updater code still honors `EGRESS_UPDATE_BASE_URL`/`file://`, discovers `curl` and `sha256sum`/`shasum` through `PATH`, and stages in a predictable `/tmp/eggress-update-<pid>` directory. The same audit found `system-proxy inspect --json` serializes raw credential-bearing settings and that CI/release preflight has no advisory gate. M004 hardens those surfaces and tracks the `zip 6.0.0` advisory as an upstream `eggup-archive` blocker.

## 5. Target architecture

One version, verified artifacts, manual-gated publication, with generic self-update archive/transaction mechanics delegated to Eggup while Eggress retains release and CLI policy. The existing release architecture is attained; updater-mechanism convergence landed as M003.

## 6. Dependency graph

```text
CLI surface (hard)
    +--> Embed facade (soft)
    +--> Python bindings/packaging (soft)
    +--> Release automation (operational: tags, PyPI env, crates.io manual)
    +--> M003 Eggup updater convergence [CLOSED with registry 0.1.2 pair]
    `--> M004 updater trust/advisory gate [implemented; zip upstream blocker tracked as a deny.toml exception]
```

## 7. Milestones

### Milestone 1 — CLI/embed/Python surfaces

Class: capability. Objective: CLI closures, embed API, Python bindings + helpers. Exit: Phases 13–16, 29–32, 38–40 complete. Status: closed (historical, archive).

### Milestone 2 — Release-engineering closure

Class: polish/infrastructure. Objective: wheel matrix, publish helper, installers, 1.0.10 roll-forward. Exit: `PYPI_WHEEL_MATRIX_EXPANSION.md`, `MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md`, `POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md` implemented. Status: closed. Evidence: archive records + `scripts/release-preflight.sh --check-versions-only`.

### Milestone 3 — Eggup archive/pair self-update adoption

Plan: `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md`.

Class: capability/delivery convergence. Status: closed — Eggup M009 published `eggup-core 0.1.2` + `eggup-archive 0.1.2` on 2026-09-28 and this milestone adopted them; see `plans/closure/delivery/003-status.md`.

Objective: retain Eggress release discovery, target mapping, checksum, staged-version, CLI, and publication policy while replacing generic tar/zip extraction plus the bespoke `eggress`/`pproxy` backup/rollback transaction with Eggup's qualified archive extraction and multi-artifact transaction.

Hard gate (satisfied 2026-09-28): Eggup's post-M001d `eggup-core` API and `eggup-archive` are available as registry dependencies suitable for `cargo package` (`eggup-core 0.1.2`, `eggup-archive 0.1.2`); immutable revision qualification is allowed only before final dependency cutover.

Exit requires shell extraction removed from the updater success path, local generic pair rollback removed, bound-source staging, exact candidate validation preserved, rollback/recovery fault evidence, package dry-run, Rust 1.89, Linux/macOS/Windows qualification, and recorded footprint delta.

### Milestone 4 — Updater trust, advisory gate, and credential-output hardening

Plan: `plans/implementation/delivery/004-updater-trust-and-advisory-gate.md`.

Class: invariant / infrastructure. Status: closed 2026-11-16 — see `plans/closure/delivery/004-status.md`; the `zip 6.0.0` advisories remain an explicit, commented gate exception (owner/review date recorded) pending an upstream `eggup-archive` release.

Objective: gate the updater test hook out of release builds; replace PATH-resolved integrity tools with trusted absolute paths and in-process SHA-256; use private staging; redact `system-proxy inspect --json`; add a CI/release advisory gate with a narrow tracked `zip` exception.

Exit: default builds reject `EGRESS_UPDATE_BASE_URL`/`file://`; no PATH hash authority; staging is private/create-new/symlink-safe; JSON has no credentials; the gate runs; the `zip` blocker is recorded in the registry with a removal condition. No release/tag.

## 8. Cross-cutting requirements

Features: `eggress-cli full` excludes `ssh`/`quic`/`pproxy-legacy`/`legacy-crypto`/`pproxy-daemon` (extra check command for those paths); `eggress-embed full` includes `pproxy-legacy`; never `--all-features` (drags test-only `insecure-quic`). Python: maturin develop + pytest from root with `importlib` mode. Fuzz workspace standalone. Compat: Python `pproxy` namespace owned only by `python-pproxy-compat`.

## 9. Verification strategy

`cargo test -p eggress-cli --test cli_exit_codes`, embed feature-slice checks + OpenSSH regression for boundary changes, Python pytest (`python/tests`, `tests/compat`), `scripts/release-preflight.sh` for version/release changes, `cargo deny`/`cargo audit --ignore RUSTSEC-2023-0071` for dependency changes. M004 additionally requires updater negative tests (env/file/PATH/symlink/TOCTOU), in-process SHA-256 vectors, and `system-proxy --json` redaction tests.

## 10. Risks and decision points

Tag-push discipline remains critical because both publish workflows fire on `v*`. M003 carries Windows running-image qualification risk and default-binary footprint risk now that its registry-package dependency gate is closed. The current whole-archive SHA-256 release evidence is sufficient for Eggup extraction because member expectations are optional; do not invent a new member manifest solely for this migration. M004's `zip 6.0.0` advisory cannot be removed until `eggup-archive` publishes a `zip >= 9` dependency; the vulnerable `extract()` APIs are not used by Eggup's bounded extractor, so only a narrow, commented gate exception is acceptable in the interim.

## 11. Completion definition

Existing delivery surfaces are qualified and 1.0.10 is published as `v1.0.10` (2026-09-24). M003 landed the updater convergence with the versioned Eggup dependency gate closed 2026-09-28 and hosted qualification green. M004 is complete when the updater trust/output/advisory controls are closed, with the `zip` upstream dependency either fixed or explicitly accepted and tracked.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive CLI/embed/python records | archive + Git history | — |
| 2 | closed | archive distribution records | registry control points | maintainer release action (out of planning scope) |
| 3 | closed | `plans/implementation/delivery/003-eggup-archive-pair-self-update-adoption.md` | `plans/closure/delivery/003-status.md` | — (registry 0.1.2 pair adopted; updater lanes green) |
| 4 | closed | `plans/implementation/delivery/004-updater-trust-and-advisory-gate.md` | `plans/closure/delivery/004-status.md` | Four `zip 6.0.0` advisories remain as a narrow, commented `deny.toml` exception with an owner and review date, pending an upstream `eggup-archive` release with `zip >= 9.0.0` |
