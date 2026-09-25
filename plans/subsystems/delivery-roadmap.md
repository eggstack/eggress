# Delivery (CLI, Embed, Python, Release) Roadmap

Status: closed

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

- Free-threaded Python; automated crates.io publication; new runtime behavior under a delivery plan.

## 4. Current state

CLI cleanup/version/self-update, distribution docs/policy, embed/Python stabilization, PyPI matrix expansion, manual-publish simplification all complete. 1.0.10 prepared but unpublished; publication/tagging is maintainer-authorized, not planning-authorized.

## 5. Target architecture

One version, verified artifacts, manual-gated publication — attained.

## 6. Dependency graph

```text
CLI surface (hard)
    +--> Embed facade (soft)
    +--> Python bindings/packaging (soft)
    `--> Release automation (operational: tags, PyPI env, crates.io manual)
```

## 7. Milestones

### Milestone 1 — CLI/embed/Python surfaces

Class: capability. Objective: CLI closures, embed API, Python bindings + helpers. Exit: Phases 13–16, 29–32, 38–40 complete. Status: closed (historical, archive).

### Milestone 2 — Release-engineering closure

Class: polish/infrastructure. Objective: wheel matrix, publish helper, installers, 1.0.10 roll-forward. Exit: `PYPI_WHEEL_MATRIX_EXPANSION.md`, `MANUAL_CRATES_IO_PUBLISHING_SIMPLIFICATION.md`, `POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md` implemented. Status: closed. Evidence: archive records + `scripts/release-preflight.sh --check-versions-only`.

## 8. Cross-cutting requirements

Features: `eggress-cli full` excludes `ssh`/`quic`/`pproxy-legacy`/`legacy-crypto`/`pproxy-daemon` (extra check command for those paths); `eggress-embed full` includes `pproxy-legacy`; never `--all-features` (drags test-only `insecure-quic`). Python: maturin develop + pytest from root with `importlib` mode. Fuzz workspace standalone. Compat: Python `pproxy` namespace owned only by `python-pproxy-compat`.

## 9. Verification strategy

`cargo test -p eggress-cli --test cli_exit_codes`, embed feature-slice checks + OpenSSH regression for boundary changes, Python pytest (`python/tests`, `tests/compat`), `scripts/release-preflight.sh` for version/release changes, `cargo deny`/`cargo audit --ignore RUSTSEC-2023-0071` for dependency changes.

## 10. Risks and decision points

Tag-push discipline (both publish workflows fire on `v*`); TestPyPI via manual dispatch only. No active risk.

## 11. Completion definition

Delivery surfaces qualified with publication firmly manual/tag-gated — attained; 1.0.10 awaits maintainer release action.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive CLI/embed/python records | archive + Git history | — |
| 2 | closed | archive distribution records | registry control points | maintainer release action (out of planning scope) |
